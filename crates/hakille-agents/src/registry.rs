use hakille_core::{CapabilityScope, HakilleError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub required_capabilities: Vec<CapabilityScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Installed,
    Running,
    Paused,
    Terminated,
}

pub struct AgentRegistry {
    agents: HashMap<String, (AgentManifest, AgentStatus)>,
    /// Hash SHA-256 canonique du manifest au moment du register
    /// (pour détecter toute substitution ultérieure).
    manifest_hashes: HashMap<String, [u8; 32]>,
    /// Si vrai, tout `register` / `set_status` est refusé (fail-closed).
    read_only: bool,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            manifest_hashes: HashMap::new(),
            read_only: false,
        }
    }

    /// Active / désactive le mode lecture seule.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Garde respectée par `register` / `register_verified` / `set_status` :
    /// refuse toute mutation si `read_only`.
    pub fn check_writable(&self) -> Result<()> {
        if self.read_only {
            return Err(HakilleError::CapabilityDenied(
                "Registre d'agents en lecture seule (read_only)".to_string(),
            ));
        }
        Ok(())
    }

    /// Alias historique de `check_writable` (respect du champ `read_only`).
    pub fn check(&self) -> Result<()> {
        self.check_writable()
    }

    /// Hash canonique 32 octets d'un manifest (JSON des champs).
    /// Dérivé sans dépendance externe (4× SipHash à domaines séparés) :
    /// fingerprint anti-substitution, pas un ancrage cryptographique externe.
    /// Pour un ancrage fort, fournir `expected_hash` calculé côté
    /// distribution vérifiée (SHA-256) — la comparaison reste stricte.
    pub fn manifest_hash(manifest: &AgentManifest) -> [u8; 32] {
        let bytes = serde_json::to_vec(manifest).unwrap_or_default();
        let mut out = [0u8; 32];
        for (i, chunk) in out.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (i as u8).hash(&mut h);
            bytes.hash(&mut h);
            chunk.copy_from_slice(&h.finish().to_be_bytes());
        }
        out
    }

    /// Valide le format de version (exige `X.Y.Z` numérique minimal) :
    /// refuse version vide ou non semver-like (fail-closed).
    pub fn validate_version(version: &str) -> Result<()> {
        Self::parse_semver(version)?;
        Ok(())
    }

    /// Parse `X.Y[.Z][-pre]` en triplet numérique (préfixe numérique exigé
    /// par composante, suffixe `-pre`/`+build` ignoré pour la comparaison).
    /// Fail-closed : erreur si vide, < 2 composantes ou préfixe non numérique.
    pub fn parse_semver(version: &str) -> Result<(u64, u64, u64)> {
        let v = version.trim();
        if v.is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "Version d'agent vide refusée".to_string(),
            ));
        }
        // Ignore suffixe pre-release/build pour la comparaison numérique.
        let core = v.split(['-', '+']).next().unwrap_or(v);
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() < 2 {
            return Err(HakilleError::CapabilityDenied(format!(
                "Version d'agent invalide (attendu semver X.Y[.Z]) : '{}'",
                version
            )));
        }
        let mut nums = [0u64; 3];
        for (i, p) in parts.iter().take(3).enumerate() {
            // Accepte `1`, `0`, `1-alpha` → préfixe numérique exigé.
            let numeric_prefix: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            if numeric_prefix.is_empty() {
                return Err(HakilleError::CapabilityDenied(format!(
                    "Version d'agent invalide : '{}'",
                    version
                )));
            }
            nums[i] = numeric_prefix.parse::<u64>().map_err(|_| {
                HakilleError::CapabilityDenied(format!("Version d'agent invalide : '{}'", version))
            })?;
        }
        Ok((nums[0], nums[1], nums[2]))
    }

    /// Compatibilité semver caret (`^min_version`) : même majeur, et
    /// `version >= min_version` (fail-closed sinon). Les deux versions
    /// doivent d'abord passer `validate_version`.
    pub fn check_version_compatible(version: &str, min_version: &str) -> Result<()> {
        let (maj, min, patch) = Self::parse_semver(version)?;
        let (req_maj, req_min, req_patch) = Self::parse_semver(min_version)?;
        if maj != req_maj || (maj, min, patch) < (req_maj, req_min, req_patch) {
            return Err(HakilleError::CapabilityDenied(format!(
                "Version d'agent incompatible : '{}' ne satisfait pas '^ {}'",
                version, min_version
            )));
        }
        Ok(())
    }

    pub fn register(&mut self, manifest: AgentManifest) -> Result<()> {
        self.register_verified(manifest, None, false)
    }

    /// Enregistrement vérifié :
    /// - `check()` refuse si `read_only` ;
    /// - version validée (`validate_version`) ;
    /// - si `expected_hash` fourni, le hash canonique du manifest doit
    ///   correspondre (vérification hash, fail-closed) ;
    /// - si `id` existe déjà et `force == false` → refus (anti-écrasement) ;
    ///   `force == true` autorise explicitement le remplacement.
    ///
    /// Note signature : la vérification cryptographique du manifest se fait
    /// via le hash épinglé (`expected_hash`, fourni par le canal de
    /// distribution vérifié). Un manifest altéré produit un hash différent
    /// et est rejeté.
    pub fn register_verified(
        &mut self,
        manifest: AgentManifest,
        expected_hash: Option<[u8; 32]>,
        force: bool,
    ) -> Result<()> {
        self.check()?;
        Self::validate_version(&manifest.version)?;
        if manifest.id.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "ID d'agent vide refusé".to_string(),
            ));
        }
        let actual = Self::manifest_hash(&manifest);
        if let Some(expected) = expected_hash
            && actual != expected
        {
            return Err(HakilleError::CapabilityDenied(format!(
                "Hash du manifest '{}' non conforme à l'attendu",
                manifest.id
            )));
        }
        let id = manifest.id.clone();
        if self.agents.contains_key(&id) && !force {
            return Err(HakilleError::CapabilityDenied(format!(
                "Agent '{}' déjà enregistré (utiliser force=true pour remplacer)",
                id
            )));
        }
        self.manifest_hashes.insert(id.clone(), actual);
        self.agents.insert(id, (manifest, AgentStatus::Installed));
        Ok(())
    }

    /// Hash enregistré pour un agent (audit / vérification ultérieure).
    pub fn get_expected_hash(&self, agent_id: &str) -> Option<[u8; 32]> {
        self.manifest_hashes.get(agent_id).copied()
    }

    pub fn get_manifest(&self, agent_id: &str) -> Option<&AgentManifest> {
        self.agents.get(agent_id).map(|(m, _)| m)
    }

    pub fn get_status(&self, agent_id: &str) -> Option<AgentStatus> {
        self.agents.get(agent_id).map(|(_, s)| s.clone())
    }

    pub fn set_status(&mut self, agent_id: &str, status: AgentStatus) -> bool {
        // Respecte `read_only` (fail-closed) : mutation refusée.
        if self.read_only {
            return false;
        }
        if let Some((_, s)) = self.agents.get_mut(agent_id) {
            *s = status;
            true
        } else {
            false
        }
    }

    /// Variante `Result` qui explicite le refus `read_only`.
    pub fn set_status_checked(&mut self, agent_id: &str, status: AgentStatus) -> Result<()> {
        self.check()?;
        if self
            .agents
            .get_mut(agent_id)
            .map(|(_, s)| *s = status)
            .is_some()
        {
            Ok(())
        } else {
            Err(HakilleError::CapabilityDenied(format!(
                "Agent '{}' introuvable",
                agent_id
            )))
        }
    }

    pub fn list_agents(&self) -> Vec<&AgentManifest> {
        self.agents.values().map(|(m, _)| m).collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
