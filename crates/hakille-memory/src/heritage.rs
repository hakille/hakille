use crate::context_partition::ContextPartition;
use hakille_core::{HakilleError, Result, Timestamp};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Erreurs explicites du module héritage (validation + registre de clés).
#[derive(Debug, Clone, thiserror::Error)]
pub enum HeritageError {
    #[error("heritage config invalid: beneficiaries must not be empty")]
    EmptyBeneficiaries,
    #[error("heritage config invalid: items must resolve to a non-empty set")]
    EmptyItems,
    #[error("heritage config invalid: item #{index} has a blank item_id")]
    BlankItemId { index: usize },
    #[error("heritage key not found: {0}")]
    KeyNotFound(String),
    #[error("not implemented: {0}")]
    NotImplemented(String),
}

impl From<HeritageError> for HakilleError {
    fn from(e: HeritageError) -> Self {
        match &e {
            HeritageError::KeyNotFound(_) => HakilleError::StorageError(e.to_string()),
            _ => HakilleError::Internal(e.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct EncryptionKey(pub Vec<u8>);

impl EncryptionKey {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Vrai si la clé est déjà effacée (vide ou que des zéros).
    pub fn is_erased(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HeritageItemType {
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeritageItem {
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BeneficiaryConfig {
    pub user_id: String,
    pub public_key: Vec<u8>,
    pub allowed_items: Vec<HeritageItemType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeritageConfig {
    pub beneficiaries: Vec<BeneficiaryConfig>,
    pub items: Vec<HeritageItem>,
}

/// Valide une config héritage : bénéficiaires non vides + items résolvables
/// en ensemble non vide (chaque `item_id` doit être non blanc).
///
/// Retourne `Err(HeritageError)` explicite sinon.
pub fn validate_heritage_config(config: &HeritageConfig) -> std::result::Result<(), HeritageError> {
    if config.beneficiaries.is_empty() {
        return Err(HeritageError::EmptyBeneficiaries);
    }
    if config.items.is_empty() {
        return Err(HeritageError::EmptyItems);
    }
    for (index, item) in config.items.iter().enumerate() {
        if item.item_id.trim().is_empty() {
            return Err(HeritageError::BlankItemId { index });
        }
    }
    Ok(())
}

/// Paquet de transfert vers un bénéficiaire (cible future : `item_id`
/// chiffré avec `public_key` du bénéficiaire).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferPackage {
    pub item_id: String,
    pub beneficiary_user_id: String,
    pub sealed_item_id: Vec<u8>,
}

/// Prépare le transfert d'un item vers un bénéficiaire.
///
/// Valide d'abord la config ([`validate_heritage_config`]), vérifie que le
/// bénéficiaire et l'item existent, puis chiffre `item_id` avec la
/// `public_key` du bénéficiaire.
///
/// Stub honnête : aucun backend de chiffrement asymétrique n'est branché
/// dans `hakille-memory` (pas de dépendance crypto adaptée), donc après une
/// validation réussie la fonction retourne
/// `Err(HeritageError::NotImplemented(..))` explicite — jamais de faux
/// succès. Les erreurs de validation / bénéficiaire ou item introuvable
/// restent prioritaires et sont retournées avant le stub.
pub fn prepare_transfer(
    config: &HeritageConfig,
    item_id: &str,
    beneficiary_user_id: &str,
) -> std::result::Result<TransferPackage, HeritageError> {
    validate_heritage_config(config)?;
    let beneficiary = config
        .beneficiaries
        .iter()
        .find(|b| b.user_id == beneficiary_user_id)
        .ok_or_else(|| {
            HeritageError::KeyNotFound(format!("beneficiary '{beneficiary_user_id}'"))
        })?;
    if !config.items.iter().any(|i| i.item_id == item_id) {
        return Err(HeritageError::KeyNotFound(format!("item '{item_id}'")));
    }
    let _ = beneficiary.public_key.len();
    Err(HeritageError::NotImplemented(
        "chiffrement bénéficiaire (item_id avec public_key) non branché : \
         aucun backend crypto asymétrique dans hakille-memory"
            .to_string(),
    ))
}

/// Gestionnaire d'héritage : pile de configs persistée en JSON.
///
/// Chaque `configure_heritage` empile la config et réécrit le fichier.
/// Les doublons consécutifs identiques sont dédupliqués (no-op) pour éviter
/// une croissance illimitée de la pile.
pub struct HeritageManager {
    config_file: PathBuf,
    stack: Vec<HeritageConfig>,
}

impl HeritageManager {
    pub fn new(config_file: impl Into<PathBuf>) -> Self {
        Self {
            config_file: config_file.into(),
            stack: Vec::new(),
        }
    }

    /// Chemin du fichier JSON sous-jacent.
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    /// Nombre de configs empilées.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Effacement cryptographique : remplit les octets de la clé avec des 0.
    ///
    /// Retourne `true` si des octets non nuls ont été effacés, `false` si la
    /// clé était déjà vide ou déjà à zéro. Le `Vec` est conservé (longueur
    /// inchangée) mais son contenu est neutralisé ; l'appelant doit ensuite
    /// abandonner la clé (`drop`).
    pub fn crypto_erase(key: &mut EncryptionKey) -> bool {
        if key.0.is_empty() || key.is_erased() {
            return false;
        }
        for b in key.0.iter_mut() {
            // `write_volatile` évite que l'optimiseur supprime l'effacement.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        // Barrière mémoire pour éviter toute réorganisation.
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
        true
    }

    /// Empile une config bénéficiaires et la persiste en JSON.
    ///
    /// Valide d'abord (`beneficiaries` non vide + `items` résolvables en
    /// non-vide) et refuse avec `Err` explicite sinon — sans rien stocker.
    /// Une config identique au sommet de pile est dédupliquée (no-op, sans
    /// réécriture) pour borner la pile.
    pub fn configure_heritage(&mut self, config: HeritageConfig) -> Result<()> {
        validate_heritage_config(&config).map_err(HakilleError::from)?;
        if self.stack.last() == Some(&config) {
            return Ok(());
        }
        self.stack.push(config);
        self.save()
    }

    /// Config courante (sommet de pile).
    pub fn current(&self) -> Option<&HeritageConfig> {
        self.stack.last()
    }

    /// Persiste la pile complète en JSON, écriture atomique (tmp + rename).
    ///
    /// Écrit d'abord dans `<fichier>.tmp` puis renomme atomiquement, pour ne
    /// jamais laisser un fichier tronqué en cas de crash à mi-écriture.
    pub fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.stack)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        let mut tmp_os = self.config_file.clone().into_os_string();
        tmp_os.push(".tmp");
        let tmp = PathBuf::from(tmp_os);
        std::fs::write(&tmp, bytes).map_err(|e| HakilleError::StorageError(e.to_string()))?;
        std::fs::rename(&tmp, &self.config_file)
            .map_err(|e| HakilleError::StorageError(e.to_string()))
    }

    /// Charge la pile depuis le fichier JSON (fichier absent ⇒ pile vide).
    pub fn load_from_file(config_file: impl Into<PathBuf>) -> Result<Self> {
        let config_file = config_file.into();
        let stack: Vec<HeritageConfig> = match std::fs::read(&config_file) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| HakilleError::SerializationError(e.to_string()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(HakilleError::StorageError(e.to_string())),
        };
        Ok(Self { config_file, stack })
    }
}

/// Entrée scellée du registre de clés (mémoire uniquement, jamais persistée).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEntry {
    pub key_id: String,
    pub sealed: Vec<u8>,
    pub partition: ContextPartition,
    pub created_at: Timestamp,
}

/// Preuve d'effacement rendue par `KeyRegistry::destroy` / `crypto_erase`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseProof {
    pub key_id: String,
    pub destroyed_at: Timestamp,
    pub wiped_bytes: usize,
}

/// Registre de clés héritage en mémoire.
#[derive(Debug, Default)]
pub struct KeyRegistry {
    entries: Vec<KeyEntry>,
}

impl KeyRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Enregistre une clé scellée et rend son `key_id` (UUID v7).
    pub fn register(&mut self, partition: ContextPartition, sealed: Vec<u8>) -> String {
        let key_id = uuid::Uuid::now_v7().to_string();
        self.entries.push(KeyEntry {
            key_id: key_id.clone(),
            sealed,
            partition,
            created_at: Timestamp::now(),
        });
        key_id
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key_id: &str) -> bool {
        self.entries.iter().any(|e| e.key_id == key_id)
    }

    pub fn get(&self, key_id: &str) -> Option<&KeyEntry> {
        self.entries.iter().find(|e| e.key_id == key_id)
    }

    /// Rotation de clé : génère une nouvelle entrée (nouveau `key_id`) avec
    /// `new_sealed` pour la même partition que `key_id`, et rend le nouvel ID.
    ///
    /// Stub documenté : seule la rotation au niveau du registre est faite
    /// (l'ancienne entrée est conservée pour déchiffrement de l'existant).
    /// La ré-encryption réelle des triplets au repos avec la nouvelle clé
    /// n'est PAS implémentée ici (voir module `crypto_erase` : périmètre
    /// équipe crypto/stockage, `storage/edge.rs`).
    pub fn rotate_key(
        &mut self,
        key_id: &str,
        new_sealed: Vec<u8>,
    ) -> std::result::Result<String, HeritageError> {
        let partition = self
            .entries
            .iter()
            .find(|e| e.key_id == key_id)
            .map(|e| e.partition.clone())
            .ok_or_else(|| HeritageError::KeyNotFound(key_id.to_string()))?;
        let new_id = uuid::Uuid::now_v7().to_string();
        self.entries.push(KeyEntry {
            key_id: new_id.clone(),
            sealed: new_sealed,
            partition,
            created_at: Timestamp::now(),
        });
        Ok(new_id)
    }

    /// Détruit une clé : `zeroize` des octets scellés + retrait du registre.
    ///
    /// Rend `EraseProof { key_id, destroyed_at, wiped_bytes }`.
    /// Un second `destroy` du même `key_id` rend `Err(KeyNotFound)`.
    pub fn destroy(&mut self, key_id: &str) -> std::result::Result<EraseProof, HeritageError> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.key_id == key_id)
            .ok_or_else(|| HeritageError::KeyNotFound(key_id.to_string()))?;
        let mut entry = self.entries.remove(pos);
        let wiped_bytes = entry.sealed.len();
        // Zeroize sans dépendance : `write_volatile` évite que l'optimiseur
        // supprime l'effacement, suivi d'une barrière mémoire.
        for b in entry.sealed.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
        Ok(EraseProof {
            key_id: entry.key_id,
            destroyed_at: Timestamp::now(),
            wiped_bytes,
        })
    }
}

/// Effacement cryptographique d'une clé du registre : `destroy` + preuve.
///
/// TODO (hors périmètre ÉQUIPE DATA) : le chiffrement du graphe lui-même
/// (chiffrement au repos des triplets + rotation de clé) n'est PAS implémenté
/// ici — seul l'effacement des clés scellées l'est. Contradiction assumée
/// avec SECURITY_AND_GOVERNANCE:14 (qui exige le chiffrement) : à traiter par
/// l'équipe crypto/stockage (voir `storage/edge.rs`).
pub fn crypto_erase(
    registry: &mut KeyRegistry,
    key_id: &str,
) -> std::result::Result<EraseProof, HeritageError> {
    registry.destroy(key_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> HeritageConfig {
        HeritageConfig {
            beneficiaries: vec![BeneficiaryConfig {
                user_id: "aya".to_string(),
                public_key: vec![1, 2, 3],
                allowed_items: vec![HeritageItemType::All],
            }],
            items: vec![HeritageItem {
                item_id: "vault-1".to_string(),
            }],
        }
    }

    fn other_config() -> HeritageConfig {
        HeritageConfig {
            beneficiaries: vec![BeneficiaryConfig {
                user_id: "ibra".to_string(),
                public_key: vec![4, 5, 6],
                allowed_items: vec![HeritageItemType::All],
            }],
            items: vec![HeritageItem {
                item_id: "vault-2".to_string(),
            }],
        }
    }

    #[test]
    fn test_crypto_erase_zeroizes() {
        let mut key = EncryptionKey::new(vec![1, 2, 3, 4]);
        assert!(HeritageManager::crypto_erase(&mut key));
        assert_eq!(key.0, vec![0, 0, 0, 0]);
        // Deuxième effacement : déjà à zéro ⇒ false.
        assert!(!HeritageManager::crypto_erase(&mut key));

        let mut empty = EncryptionKey::new(vec![]);
        assert!(!HeritageManager::crypto_erase(&mut empty));
    }

    #[test]
    fn test_configure_heritage_persists_json_stack() {
        let dir = std::env::temp_dir().join(format!("hakille-heritage-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("heritage.json");

        let mut mgr = HeritageManager::new(&path);
        mgr.configure_heritage(sample_config()).unwrap();
        mgr.configure_heritage(other_config()).unwrap();
        assert_eq!(mgr.len(), 2);

        // Le fichier existe et contient 2 configs.
        let raw = std::fs::read(&path).unwrap();
        let back: Vec<HeritageConfig> = serde_json::from_slice(&raw).unwrap();
        assert_eq!(back.len(), 2);

        // Rechargement depuis le fichier.
        let reloaded = HeritageManager::load_from_file(&path).unwrap();
        assert_eq!(reloaded.len(), 2);
        assert!(reloaded.current().is_some());

        // Pas de fichier temporaire résiduel (écriture atomique).
        let mut tmp_os = path.clone().into_os_string();
        tmp_os.push(".tmp");
        assert!(!std::path::Path::new(&tmp_os).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_configure_dedups_identical_consecutive() {
        let dir = std::env::temp_dir().join(format!("hakille-heritage-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("heritage.json");
        let mut mgr = HeritageManager::new(&path);

        mgr.configure_heritage(sample_config()).unwrap();
        // Doublon identique consécutif ⇒ no-op (pile bornée).
        mgr.configure_heritage(sample_config()).unwrap();
        assert_eq!(mgr.len(), 1);

        // Config différente ⇒ empilée.
        mgr.configure_heritage(other_config()).unwrap();
        assert_eq!(mgr.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_key_registry_cycle_register_destroy_proof() {
        use crate::ContextPartition;

        let mut registry = KeyRegistry::new();
        let key_id = registry.register(ContextPartition::Personal, vec![9, 8, 7, 6]);
        assert!(registry.contains(&key_id));
        assert_eq!(registry.len(), 1);

        let proof = registry.destroy(&key_id).unwrap();
        assert_eq!(proof.key_id, key_id);
        assert_eq!(proof.wiped_bytes, 4);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_key_registry_double_destroy_is_error() {
        use crate::ContextPartition;

        let mut registry = KeyRegistry::new();
        let key_id = registry.register(ContextPartition::Default, vec![1, 2, 3]);
        registry.destroy(&key_id).unwrap();
        let err = registry.destroy(&key_id).unwrap_err();
        assert!(
            matches!(err, HeritageError::KeyNotFound(_)),
            "attendu KeyNotFound, obtenu: {err}"
        );

        // `crypto_erase` libre = `destroy` + preuve ; clé inconnue ⇒ Err.
        let err = crypto_erase(&mut registry, "cle-inexistante").unwrap_err();
        assert!(matches!(err, HeritageError::KeyNotFound(_)));
    }

    #[test]
    fn test_configure_heritage_rejects_invalid() {
        let dir = std::env::temp_dir().join(format!("hakille-heritage-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("heritage.json");
        let mut mgr = HeritageManager::new(&path);

        // Bénéficiaires vides ⇒ refus, rien stocké.
        let no_benef = HeritageConfig {
            beneficiaries: vec![],
            items: vec![HeritageItem {
                item_id: "vault-1".to_string(),
            }],
        };
        assert!(mgr.configure_heritage(no_benef).is_err());
        assert!(mgr.is_empty());

        // Items vides ⇒ refus, rien stocké.
        let no_items = HeritageConfig {
            beneficiaries: vec![BeneficiaryConfig {
                user_id: "aya".to_string(),
                public_key: vec![1, 2, 3],
                allowed_items: vec![HeritageItemType::All],
            }],
            items: vec![],
        };
        assert!(mgr.configure_heritage(no_items).is_err());
        assert!(mgr.is_empty());

        // `item_id` blanc (non résolvable) ⇒ refus, rien stocké.
        let blank_item = HeritageConfig {
            beneficiaries: vec![BeneficiaryConfig {
                user_id: "aya".to_string(),
                public_key: vec![1, 2, 3],
                allowed_items: vec![HeritageItemType::All],
            }],
            items: vec![HeritageItem {
                item_id: "   ".to_string(),
            }],
        };
        assert!(mgr.configure_heritage(blank_item).is_err());
        assert!(mgr.is_empty());

        // Config valide ⇒ acceptée et stockée.
        mgr.configure_heritage(sample_config()).unwrap();
        assert_eq!(mgr.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_rotate_key_generates_new_entry() {
        use crate::ContextPartition;

        let mut registry = KeyRegistry::new();
        let old = registry.register(ContextPartition::Personal, vec![1, 1, 1]);
        let new = registry.rotate_key(&old, vec![2, 2, 2]).unwrap();
        assert_ne!(old, new);
        assert_eq!(registry.len(), 2);
        assert!(registry.contains(&old));
        assert!(registry.contains(&new));
        let entry = registry.get(&new).unwrap();
        assert_eq!(entry.sealed, vec![2, 2, 2]);
        assert_eq!(entry.partition, ContextPartition::Personal);

        // Clé inconnue ⇒ Err.
        assert!(matches!(
            registry.rotate_key("introuvable", vec![0]).unwrap_err(),
            HeritageError::KeyNotFound(_)
        ));
    }

    #[test]
    fn test_prepare_transfer_validates_then_reports_not_implemented() {
        let cfg = sample_config();
        // Config valide + bénéficiaire + item connus ⇒ stub honnête, pas de
        // faux succès.
        let err = prepare_transfer(&cfg, "vault-1", "aya").unwrap_err();
        assert!(
            matches!(err, HeritageError::NotImplemented(_)),
            "attendu NotImplemented, obtenu: {err}"
        );

        // Bénéficiaire inconnu ⇒ erreur avant le stub.
        let err = prepare_transfer(&cfg, "vault-1", "inconnu").unwrap_err();
        assert!(matches!(err, HeritageError::KeyNotFound(_)));

        // Item inconnu ⇒ erreur avant le stub.
        let err = prepare_transfer(&cfg, "vault-x", "aya").unwrap_err();
        assert!(matches!(err, HeritageError::KeyNotFound(_)));

        // Config invalide ⇒ erreur de validation prioritaire.
        let bad = HeritageConfig {
            beneficiaries: vec![],
            items: vec![],
        };
        let err = prepare_transfer(&bad, "vault-1", "aya").unwrap_err();
        assert!(matches!(err, HeritageError::EmptyBeneficiaries));
    }
}
