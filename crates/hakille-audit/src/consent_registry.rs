use hakille_core::{
    ActorId, CapabilityAction, CapabilityScope, Ed25519Signature, Ed25519VerifyingKey,
    HakilleError, KeyPair, Result, Timestamp, VerifyEd25519,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// An explicit cryptographic record of authorized capability permissions (§20i).
/// Distinct from individual action execution entries; provides an instant, tamper-evident
/// overview of "Which powers have I granted, to whom, and since when?".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub grant_id: String,
    pub subject_actor: ActorId,
    pub granted_scopes: Vec<CapabilityScope>,
    pub granted_actions: Vec<CapabilityAction>,
    pub purpose: String,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
    pub revocation_reason: Option<String>,
}

impl ConsentRecord {
    pub fn is_currently_active(&self) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if let Some(ref until) = self.valid_until {
            let now = chrono::Utc::now();
            if now > until.0 {
                return false;
            }
        }
        true
    }

    /// Le consentement couvre un couple (scope, action) : les deux doivent
    /// être explicitement accordés ET le grant actif. Vérifier le scope seul
    /// laisserait passer p. ex. un `Write` sur un scope accordé en `Read`.
    pub fn covers_scope(&self, scope: &CapabilityScope, action: &CapabilityAction) -> bool {
        self.is_currently_active()
            && self.granted_scopes.contains(scope)
            && self.granted_actions.contains(action)
    }
}

/// Consent Registry (§20i).
/// Dedicated registry answering in one query "What permissions did I grant?",
/// without requiring forensic reconstruction across millions of action log entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsentRegistry {
    records: Vec<ConsentRecord>,
    /// F5 : révocations par token (nonce/jti → motif). `#[serde(default)]` :
    /// les JSON antérieurs sans ce champ se relisent (registre vide).
    #[serde(default)]
    revocations: RevocationStore,
}

/// F5 : révocation par token (nonce/jti). `HashMap` persistée via le
/// `ConsentRegistry` (sérialisée avec lui) : `register` (émission) /
/// `revoke` (compromis, motif obligatoire) / `check` (fail-closed) /
/// `is_revoked` / `reason`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevocationStore {
    /// Tokens émis suivis (nonce/jti). Optionnel mais recommandé : permet
    /// de distinguer "jamais émis" de "émis puis révoqué" dans les audits.
    #[serde(default)]
    issued: HashSet<String>,
    /// Tokens révoqués : jti → motif (non vide exigé).
    #[serde(default)]
    revoked: HashMap<String, String>,
}

impl RevocationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un token émis (nonce/jti). Idempotent. `jti` vide refusé.
    pub fn register(&mut self, jti: impl Into<String>) -> Result<()> {
        let id = jti.into();
        if id.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "jti vide refusé (register)".to_string(),
            ));
        }
        self.issued.insert(id);
        Ok(())
    }

    /// Révoque un token (motif non vide exigé). Idempotent (motif écrasé).
    /// Retourne `true` si c'est une nouvelle révocation, `false` si déjà révoqué.
    pub fn revoke(&mut self, jti: &str, reason: impl Into<String>) -> Result<bool> {
        let reason = reason.into();
        if jti.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "jti vide refusé (revoke)".to_string(),
            ));
        }
        if reason.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "motif de révocation vide refusé".to_string(),
            ));
        }
        let fresh = !self.revoked.contains_key(jti);
        self.revoked.insert(jti.to_string(), reason);
        Ok(fresh)
    }

    /// Fail-closed : `Err` si révoqué, `Ok(())` sinon.
    pub fn check(&self, jti: &str) -> Result<()> {
        if let Some(reason) = self.revoked.get(jti) {
            return Err(HakilleError::CapabilityDenied(format!(
                "token révoqué (jti {jti}) : {reason}"
            )));
        }
        Ok(())
    }

    /// `true` si le jti est révoqué.
    pub fn is_revoked(&self, jti: &str) -> bool {
        self.revoked.contains_key(jti)
    }

    /// Motif de révocation (`None` = non révoqué).
    pub fn reason(&self, jti: &str) -> Option<&str> {
        self.revoked.get(jti).map(String::as_str)
    }

    /// Nombre de tokens révoqués.
    pub fn len(&self) -> usize {
        self.revoked.len()
    }

    /// `true` si aucune révocation.
    pub fn is_empty(&self) -> bool {
        self.revoked.is_empty()
    }

    /// Liste des jti révoqués (triés, pour audit déterministe).
    pub fn revoked_jtis(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.revoked.keys().map(String::as_str).collect();
        out.sort_unstable();
        out
    }
}

impl ConsentRegistry {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            revocations: RevocationStore::default(),
        }
    }

    /// Record a newly granted capability permission.
    ///
    /// Fail-closed :
    /// - refuse tout `grant_id` déjà présent (déduplication stricte) ;
    /// - refuse `valid_until <= now` (fenêtre temporelle vide/expirée).
    pub fn register_grant(
        &mut self,
        grant_id: impl Into<String>,
        actor: ActorId,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        purpose: impl Into<String>,
        valid_until: Option<Timestamp>,
    ) -> Result<String> {
        let id_str = grant_id.into();
        if id_str.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "grant_id vide refusé".to_string(),
            ));
        }
        if self.records.iter().any(|r| r.grant_id == id_str) {
            return Err(HakilleError::CapabilityDenied(format!(
                "grant_id '{}' déjà enregistré (déduplication)",
                id_str
            )));
        }
        if let Some(ref until) = valid_until
            && until.0 <= chrono::Utc::now()
        {
            return Err(HakilleError::CapabilityDenied(format!(
                "valid_until passé ou présent pour '{}' (doit être > now)",
                id_str
            )));
        }
        let record = ConsentRecord {
            grant_id: id_str.clone(),
            subject_actor: actor,
            granted_scopes: scopes,
            granted_actions: actions,
            purpose: purpose.into(),
            valid_from: Timestamp::now(),
            valid_until,
            revoked_at: None,
            revocation_reason: None,
        };
        self.records.push(record);
        Ok(id_str)
    }

    /// Revoke an active permission immediately.
    /// F5 : inscrit AUSSI le `grant_id` (utilisé comme jti du token lié)
    /// dans le `RevocationStore`, pour que `check()` / les chemins
    /// d'enforcement refusent tout token déjà révoqué ici.
    pub fn revoke_grant(&mut self, grant_id: &str, reason: impl Into<String>) -> bool {
        if let Some(record) = self.records.iter_mut().find(|r| r.grant_id == grant_id) {
            let reason = reason.into();
            record.revoked_at = Some(Timestamp::now());
            record.revocation_reason = Some(reason.clone());
            // Branchement F5 : le grant révoqué devient un jti révoqué.
            // (Motif garanti non vide ici via le record ; en cas d'échec
            //  interne improbable, le record reste révoqué : fail-closed.)
            let _ = self.revocations.revoke(grant_id, reason);
            true
        } else {
            false
        }
    }

    /// F5 : enregistre un token émis (nonce/jti) pour suivi de révocation.
    pub fn register_token(&mut self, jti: impl Into<String>) -> Result<()> {
        self.revocations.register(jti)
    }

    /// F5 : révoque directement un token par nonce/jti (hors grant).
    pub fn revoke_token(&mut self, jti: &str, reason: impl Into<String>) -> Result<bool> {
        self.revocations.revoke(jti, reason)
    }

    /// F5 : vérifie qu'un token (nonce/jti) n'est pas révoqué (fail-closed).
    pub fn check_token(&self, jti: &str) -> Result<()> {
        self.revocations.check(jti)
    }

    /// F5 : `true` si le nonce/jti est révoqué.
    pub fn is_token_revoked(&self, jti: &str) -> bool {
        self.revocations.is_revoked(jti)
    }

    /// Accès en lecture au store de révocation (audit, persistence).
    pub fn revocation_store(&self) -> &RevocationStore {
        &self.revocations
    }

    /// Instant query: list all currently active permissions across the entire system.
    pub fn list_active_consents(&self) -> Vec<&ConsentRecord> {
        self.records
            .iter()
            .filter(|r| r.is_currently_active())
            .collect()
    }

    /// Query active permissions for a specific agent/actor.
    pub fn get_actor_consents(&self, actor: &ActorId) -> Vec<&ConsentRecord> {
        self.records
            .iter()
            .filter(|r| &r.subject_actor == actor && r.is_currently_active())
            .collect()
    }

    /// Verify if actor has active consent for a (scope, action) pair.
    pub fn has_active_consent(
        &self,
        actor: &ActorId,
        scope: &CapabilityScope,
        action: &CapabilityAction,
    ) -> bool {
        self.records
            .iter()
            .any(|r| &r.subject_actor == actor && r.covers_scope(scope, action))
    }

    /// Persiste le registre en JSON (fichier). Pas de `redb` ici pour éviter
    /// une dépendance lourde dans ce module ; le journal d'audit reste la
    /// source horodatée, ce fichier est un snapshot de consentements.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        std::fs::write(path.as_ref(), json)
            .map_err(|e| HakilleError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Recharge un registre persisté via `save_to_file`.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let bytes =
            std::fs::read(path.as_ref()).map_err(|e| HakilleError::StorageError(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| HakilleError::SerializationError(e.to_string()))
    }

    /// Persiste le registre signé : JSON `{payload, signature}` où
    /// `payload` = registre sérialisé canonique et `signature` = Ed25519
    /// (via `hakille-core::KeyPair`) sur ces octets.
    pub fn save_signed(&self, path: impl AsRef<Path>, keypair: &KeyPair) -> Result<()> {
        let payload = serde_json::to_vec(self)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        let signature = keypair.sign(&payload);
        let file = SignedConsentFile {
            payload: serde_json::from_slice(&payload)
                .map_err(|e| HakilleError::SerializationError(e.to_string()))?,
            signature,
        };
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        std::fs::write(path.as_ref(), json)
            .map_err(|e| HakilleError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Recharge un registre persisté via `save_signed` en vérifiant la
    /// signature Ed25519 (fail-closed si invalide).
    pub fn load_signed(
        path: impl AsRef<Path>,
        verifying_key: &Ed25519VerifyingKey,
    ) -> Result<Self> {
        let bytes =
            std::fs::read(path.as_ref()).map_err(|e| HakilleError::StorageError(e.to_string()))?;
        let file: SignedConsentFile = serde_json::from_slice(&bytes)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        let payload_bytes = serde_json::to_vec(&file.payload)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        let sig = Ed25519Signature::from_slice(&file.signature.0)
            .map_err(|e| HakilleError::CryptoError(format!("signature invalide: {}", e)))?;
        verifying_key
            .verify(&payload_bytes, &sig)
            .map_err(|e| HakilleError::CryptoError(format!("signature consent invalide: {}", e)))?;
        Ok(file.payload)
    }
}

/// Enveloppe `{payload, signature}` stockée par `save_signed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedConsentFile {
    payload: ConsentRegistry,
    signature: hakille_core::Signature,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_registry_json_roundtrip() {
        let mut registry = ConsentRegistry::new();
        let actor = ActorId::Agent("mail-agent".to_string());
        registry
            .register_grant(
                "grant-mail-fs",
                actor.clone(),
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                "Lecture des courriels archivés",
                None,
            )
            .unwrap();

        let path = std::env::temp_dir().join(format!("consent_{}.json", uuid::Uuid::now_v7()));
        registry.save_to_file(&path).unwrap();
        let reloaded = ConsentRegistry::load_from_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(reloaded.has_active_consent(
            &actor,
            &CapabilityScope::Filesystem,
            &CapabilityAction::Read
        ));
        assert!(!reloaded.has_active_consent(
            &actor,
            &CapabilityScope::Filesystem,
            &CapabilityAction::Write
        ));

        // La révocation survit au roundtrip.
        let mut reloaded = reloaded;
        assert!(reloaded.revoke_grant("grant-mail-fs", "test"));
        let path2 = std::env::temp_dir().join(format!("consent_{}.json", uuid::Uuid::now_v7()));
        reloaded.save_to_file(&path2).unwrap();
        let reloaded2 = ConsentRegistry::load_from_file(&path2).unwrap();
        let _ = std::fs::remove_file(&path2);
        assert_eq!(reloaded2.list_active_consents().len(), 0);
    }

    #[test]
    fn test_consent_load_missing_file_fails() {
        assert!(ConsentRegistry::load_from_file("/chemin/inexistant/consent.json").is_err());
    }

    #[test]
    fn test_consent_dedup_and_valid_until() {
        let mut reg = ConsentRegistry::new();
        let actor = ActorId::Agent("a".to_string());
        reg.register_grant(
            "g1",
            actor.clone(),
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            "p",
            None,
        )
        .unwrap();
        // Doublon refusé.
        assert!(
            reg.register_grant(
                "g1",
                actor.clone(),
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                "p",
                None,
            )
            .is_err()
        );
        // valid_until passé refusé.
        let past = Timestamp(chrono::Utc::now() - chrono::Duration::seconds(10));
        assert!(
            reg.register_grant(
                "g2",
                actor,
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                "p",
                Some(past),
            )
            .is_err()
        );
    }

    #[test]
    fn test_consent_signed_roundtrip_and_tamper() {
        let mut reg = ConsentRegistry::new();
        let actor = ActorId::Agent("a".to_string());
        reg.register_grant(
            "g-signed",
            actor,
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Read],
            "p",
            None,
        )
        .unwrap();
        let kp = KeyPair::generate();
        let path =
            std::env::temp_dir().join(format!("consent_signed_{}.json", uuid::Uuid::now_v7()));
        reg.save_signed(&path, &kp).unwrap();
        let loaded = ConsentRegistry::load_signed(&path, &kp.verifying_key).unwrap();
        assert_eq!(loaded.list_active_consents().len(), 1);
        // Mauvaise clé → rejet.
        let other = KeyPair::generate();
        assert!(ConsentRegistry::load_signed(&path, &other.verifying_key).is_err());
        // Fichier altéré → rejet.
        let mut raw = std::fs::read(&path).unwrap();
        raw[10] = raw[10].wrapping_add(1);
        std::fs::write(&path, raw).unwrap();
        assert!(ConsentRegistry::load_signed(&path, &kp.verifying_key).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_revocation_store_register_revoke_check() {
        let mut store = RevocationStore::new();
        assert!(store.is_empty());
        // jti vide / motif vide refusés (fail-closed).
        assert!(store.register("").is_err());
        assert!(store.revoke("", "r").is_err());
        assert!(store.revoke("jti-1", "").is_err());
        store.register("jti-1").unwrap();
        assert!(store.check("jti-1").is_ok());
        assert!(store.revoke("jti-1", "compromis").unwrap());
        assert!(!store.is_empty());
        assert!(store.is_revoked("jti-1"));
        assert_eq!(store.reason("jti-1"), Some("compromis"));
        assert!(store.check("jti-1").is_err());
        // Idempotent : seconde révocation = false, motif écrasé.
        assert!(!store.revoke("jti-1", "perdu").unwrap());
        assert_eq!(store.reason("jti-1"), Some("perdu"));
        assert_eq!(store.revoked_jtis(), vec!["jti-1"]);
    }

    #[test]
    fn test_revoke_grant_feeds_revocation_store_and_persists() {
        let mut reg = ConsentRegistry::new();
        let actor = ActorId::Agent("wallet-agent".to_string());
        reg.register_grant(
            "grant-w-1",
            actor,
            vec![CapabilityScope::Wallet],
            vec![CapabilityAction::Read],
            "suivi budget",
            None,
        )
        .unwrap();
        assert!(reg.check_token("grant-w-1").is_ok());
        assert!(reg.revoke_grant("grant-w-1", "utilisateur révoque"));
        // Branché F5 : le grant_id est désormais un jti révoqué.
        assert!(reg.is_token_revoked("grant-w-1"));
        assert!(reg.check_token("grant-w-1").is_err());
        // La révocation survit au roundtrip JSON (signé et non signé).
        let path = std::env::temp_dir().join(format!("consent_rev_{}.json", uuid::Uuid::now_v7()));
        reg.save_to_file(&path).unwrap();
        let back = ConsentRegistry::load_from_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(back.is_token_revoked("grant-w-1"));
        // Compat : JSON legacy sans `revocations` se relit (vide).
        let legacy = serde_json::json!({"records": []});
        let legacy_reg: ConsentRegistry = serde_json::from_value(legacy).unwrap();
        assert!(legacy_reg.revocation_store().is_empty());
    }
}
