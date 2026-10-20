use crate::entry::{AuditEntry, sha256_token_hash};
use hakille_core::{
    ActorId, CapabilityAction, CapabilityScope, CapabilityToken, EntryId, PinnedRootPublicKey,
    Result, Timestamp, token_binding_facts, verify_capability_with_root_key,
};
use redb::{Database, TableDefinition};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

const ENTRIES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("entries");
const ENTRY_ORDER_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("entry_order");
const META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("metadata");
/// F2 : table annexe des marquages `reversed` (jamais de mutation in-place
/// des entrées). Clé = id d'entrée (bytes UUID), valeur = JSON signé.
const FLAGS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("flags");
/// F3/F5 : révocations de tokens par nonce/jti. Clé = jti, valeur = motif.
const REVOCATIONS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("revocations");

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Database transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("Database commit error: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("Database table error: {0}")]
    Table(#[from] redb::TableError),
    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("Entry not found: {0}")]
    NotFound(String),
}

impl From<JournalError> for hakille_core::HakilleError {
    fn from(err: JournalError) -> Self {
        hakille_core::HakilleError::Internal(err.to_string())
    }
}

pub struct AuditJournal {
    db: Database,
    signing_key: ed25519_dalek::SigningKey,
    last_hash: Mutex<Vec<u8>>,
    seq: Mutex<u64>,
    /// Index secondaire acteur → ids (mémoire, reconstruit au load).
    actor_index: Mutex<HashMap<ActorId, Vec<EntryId>>>,
    /// Index secondaire type d'action → ids (mémoire, reconstruit au load).
    action_index: Mutex<HashMap<String, Vec<EntryId>>>,
    /// F3 : clé publique racine Biscuit épinglée pour `revert_with_cap`.
    /// `None` (construit via `new`) = pinning absent => `revert_with_cap`
    /// refuse fail-closed (utiliser `new_with_root` en production).
    pinned_root: Option<PinnedRootPublicKey>,
}

/// Marqueur de réversion signé stocké dans `FLAGS_TABLE` (F2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ReversalFlag {
    entry_id: String,
    reversed: bool,
    /// Ed25519 (clé du journal) sur `b"reversed:<uuid>"`.
    signature: Vec<u8>,
}

/// Identifiant d'entrée des API d'audit signées (`append_signed`) — alias
/// de [`EntryId`] pour ne pas casser les APIs existantes.
pub type AuditEntryId = EntryId;

/// Erreur des API d'audit signées (`append_signed`, `verify_chain`).
///
/// Ne contient JAMAIS de secret (ni clé de signature, ni token, ni
/// payload) : uniquement des ids, des index et des messages d'état.
#[derive(Debug, Error)]
pub enum AuditError {
    /// Erreur bas niveau du journal, boxée pour garder `AuditError` petit
    /// (redb embarque des variantes volumineuses).
    #[error("{0}")]
    Journal(Box<JournalError>),
    #[error("corruption du journal d'audit : {0}")]
    Corruption(String),
    #[error("entrée d'audit invalide : {0}")]
    InvalidInput(String),
}

impl From<JournalError> for AuditError {
    fn from(err: JournalError) -> Self {
        Self::Journal(Box::new(err))
    }
}

impl From<AuditError> for hakille_core::HakilleError {
    fn from(err: AuditError) -> Self {
        match err {
            AuditError::Journal(e) => (*e).into(),
            other => hakille_core::HakilleError::Internal(other.to_string()),
        }
    }
}

/// Nature de la corruption détectée par [`AuditJournal::verify_chain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChainCorruption {
    /// Signature Ed25519 absente, illisible ou invalide.
    InvalidSignature,
    /// Rupture de chaîne : `prev_hash` ne correspond pas au hash de l'entrée
    /// précédente (ou à la genèse `[0u8; 32]` pour la première entrée).
    ChainBreak,
    /// Hash incohérent : `prev_hash` malformé (≠ 32 octets) ou ancre
    /// `last_hash` de la metadata désynchronisée du dernier maillon.
    HashMismatch,
    /// Entrée absente (index de séquence sans id, ou id sans données).
    MissingEntry,
    /// Octets d'entrée illisibles (JSON invalide / sérialisation impossible).
    MalformedEntry,
}

/// Rapport de [`AuditJournal::verify_chain`] : verdict + première entrée
/// corrompue (index 0-based, id) et nature de la corruption.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainReport {
    pub total_entries: u64,
    pub valid_entries: u64,
    /// Première entrée corrompue (id), `None` si chaîne saine.
    pub broken_at: Option<EntryId>,
    /// Index séquentiel 0-based de la première entrée corrompue.
    pub corrupted_index: Option<u64>,
    /// Nature de la corruption, `None` si chaîne saine.
    pub corruption: Option<ChainCorruption>,
}

impl ChainReport {
    /// `true` si toutes les entrées sont vérifiées et correctement liées.
    pub fn is_valid(&self) -> bool {
        self.corruption.is_none() && self.broken_at.is_none()
    }

    fn healthy(total_entries: u64, valid_entries: u64) -> Self {
        Self {
            total_entries,
            valid_entries,
            broken_at: None,
            corrupted_index: None,
            corruption: None,
        }
    }

    fn corrupted(
        total_entries: u64,
        valid_entries: u64,
        broken_at: Option<EntryId>,
        corrupted_index: u64,
        corruption: ChainCorruption,
    ) -> Self {
        Self {
            total_entries,
            valid_entries,
            broken_at,
            corrupted_index: Some(corrupted_index),
            corruption: Some(corruption),
        }
    }
}

/// Ancien nom public du rapport — conservé en alias pour compatibilité
/// source ; [`ChainReport`] en est le successeur enrichi (mêmes champs
/// `total_entries`, `valid_entries`, `broken_at`).
pub type ChainVerification = ChainReport;

impl AuditJournal {
    pub fn new(db_path: &str, signing_key: ed25519_dalek::SigningKey) -> Result<Self> {
        Self::new_inner(db_path, signing_key, None)
    }

    /// F3 : construction avec clé racine épinglée (production obligatoire).
    /// `revert_with_cap` vérifie alors les capabilities contre CETTE clé
    /// (`verify_capability_with_root_key`), jamais en mode non-épinglé.
    pub fn new_with_root(
        db_path: &str,
        signing_key: ed25519_dalek::SigningKey,
        root_pub: PinnedRootPublicKey,
    ) -> Result<Self> {
        Self::new_inner(db_path, signing_key, Some(root_pub))
    }

    fn new_inner(
        db_path: &str,
        signing_key: ed25519_dalek::SigningKey,
        pinned_root: Option<PinnedRootPublicKey>,
    ) -> Result<Self> {
        let db = Database::create(db_path).map_err(JournalError::Database)?;
        let write_txn = db.begin_write().map_err(JournalError::Transaction)?;
        {
            write_txn
                .open_table(ENTRIES_TABLE)
                .map_err(JournalError::Table)?;
            write_txn
                .open_table(ENTRY_ORDER_TABLE)
                .map_err(JournalError::Table)?;
            write_txn
                .open_table(META_TABLE)
                .map_err(JournalError::Table)?;
            write_txn
                .open_table(FLAGS_TABLE)
                .map_err(JournalError::Table)?;
            write_txn
                .open_table(REVOCATIONS_TABLE)
                .map_err(JournalError::Table)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;

        let (seq, last_hash_bytes) = {
            let read_txn = db.begin_read().map_err(JournalError::Transaction)?;
            let meta_table = read_txn
                .open_table(META_TABLE)
                .map_err(JournalError::Table)?;
            let seq = if let Some(v) = meta_table
                .get(b"seq".as_slice())
                .map_err(JournalError::Storage)?
            {
                let mut b = [0u8; 8];
                b.copy_from_slice(v.value());
                u64::from_be_bytes(b)
            } else {
                0
            };
            let hash = if let Some(v) = meta_table
                .get(b"last_hash".as_slice())
                .map_err(JournalError::Storage)?
            {
                v.value().to_vec()
            } else {
                vec![0u8; 32]
            };
            (seq, hash)
        };

        let journal = Self {
            db,
            signing_key,
            last_hash: Mutex::new(last_hash_bytes),
            seq: Mutex::new(seq),
            actor_index: Mutex::new(HashMap::new()),
            action_index: Mutex::new(HashMap::new()),
            pinned_root,
        };
        // Reconstruit les index secondaires au chargement (pas de migration redb).
        journal.rebuild_indexes()?;
        Ok(journal)
    }

    /// Reconstruit les index secondaires depuis le contenu persistant.
    /// Appelé au `new` (ici différé : `rebuild_indexes` explicite pour
    /// éviter d'emprunter `self` avant sa création complète).
    fn index_entry(
        actor_index: &mut HashMap<ActorId, Vec<EntryId>>,
        action_index: &mut HashMap<String, Vec<EntryId>>,
        entry: &AuditEntry,
    ) {
        actor_index
            .entry(entry.actor.clone())
            .or_default()
            .push(entry.id.clone());
        action_index
            .entry(entry.action.action_type.clone())
            .or_default()
            .push(entry.id.clone());
    }

    /// Reconstruit les index mémoire en scannant la DB (sans migration redb).
    pub fn rebuild_indexes(&self) -> Result<()> {
        let entries = self.all_entries_uncached()?;
        let mut actor_idx: HashMap<ActorId, Vec<EntryId>> = HashMap::new();
        let mut action_idx: HashMap<String, Vec<EntryId>> = HashMap::new();
        for e in &entries {
            Self::index_entry(&mut actor_idx, &mut action_idx, e);
        }
        *self.actor_index.lock().unwrap() = actor_idx;
        *self.action_index.lock().unwrap() = action_idx;
        Ok(())
    }

    /// Lecture ordonnée sans toucher aux index (usage interne rebuild).
    fn all_entries_uncached(&self) -> Result<Vec<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let order_table = read_txn
            .open_table(ENTRY_ORDER_TABLE)
            .map_err(JournalError::Table)?;
        let seq = *self.seq.lock().unwrap();
        let mut results = Vec::new();
        for i in 0..seq {
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)?
                && let Some(entry_data) = entries_table
                    .get(id_data.value())
                    .map_err(JournalError::Storage)?
            {
                let entry: AuditEntry = serde_json::from_slice(entry_data.value())
                    .map_err(JournalError::Serialization)?;
                results.push(entry);
            }
        }
        Ok(results)
    }

    /// Octets canoniques signés : JSON stable de l'entrée SANS le champ
    /// `signature` (l'ordre des champs suit l'ordre de déclaration de
    /// [`AuditEntry`], donc la sérialisation est déterministe).
    fn signing_bytes(entry: &AuditEntry) -> std::result::Result<Vec<u8>, serde_json::Error> {
        let mut unsigned = entry.clone();
        unsigned.signature = Vec::new();
        serde_json::to_vec(&unsigned)
    }

    /// Hash de chaînage `prev_hash` : SHA-256 des octets stockés de l'entrée
    /// SIGNÉE complète (signature incluse).
    fn chain_hash(entry: &AuditEntry) -> std::result::Result<Vec<u8>, serde_json::Error> {
        let bytes = serde_json::to_vec(entry)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hasher.finalize().to_vec())
    }

    pub fn append(&self, entry: AuditEntry) -> Result<EntryId> {
        self.append_inner(entry).map_err(|e| (*e).into())
    }

    /// Cœur d'écriture : chaînage (`prev_hash`), signature Ed25519 du contenu
    /// canonique, persistance redb et mise à jour des index. Le chaînage et
    /// la signature sont atomiques vis-à-vis des appels concurrents (verrous
    /// `last_hash` + `seq` tenus pendant toute l'opération).
    ///
    /// Erreur boxée (`Box<JournalError>`) : `JournalError` embarque des
    /// variantes redb volumineuses, le résultat doit rester petit.
    fn append_inner(
        &self,
        mut entry: AuditEntry,
    ) -> std::result::Result<EntryId, Box<JournalError>> {
        use ed25519_dalek::Signer;

        let mut last_hash_lock = self.last_hash.lock().unwrap();
        let mut seq_lock = self.seq.lock().unwrap();
        entry.prev_hash = last_hash_lock.clone();

        // Sign the entry (excluding signature field itself)
        entry.signature = Vec::new();
        let bytes_to_sign = Self::signing_bytes(&entry).map_err(JournalError::Serialization)?;
        let signature = self.signing_key.sign(&bytes_to_sign);
        entry.signature = signature.to_bytes().to_vec();

        let id = entry.id.clone();
        let id_bytes = id.0.as_bytes().to_vec();
        let entry_bytes = serde_json::to_vec(&entry).map_err(JournalError::Serialization)?;

        // Compute new hash
        let new_hash = Self::chain_hash(&entry).map_err(JournalError::Serialization)?;
        *last_hash_lock = new_hash.clone();

        let cur_seq = *seq_lock;
        *seq_lock += 1;

        let write_txn = self.db.begin_write().map_err(JournalError::Transaction)?;
        {
            let mut entries_table = write_txn
                .open_table(ENTRIES_TABLE)
                .map_err(JournalError::Table)?;
            entries_table
                .insert(id_bytes.as_slice(), entry_bytes.as_slice())
                .map_err(JournalError::Storage)?;

            let mut order_table = write_txn
                .open_table(ENTRY_ORDER_TABLE)
                .map_err(JournalError::Table)?;
            order_table
                .insert(cur_seq, id_bytes.as_slice())
                .map_err(JournalError::Storage)?;

            let mut meta_table = write_txn
                .open_table(META_TABLE)
                .map_err(JournalError::Table)?;
            meta_table
                .insert(b"seq".as_slice(), seq_lock.to_be_bytes().as_slice())
                .map_err(JournalError::Storage)?;
            meta_table
                .insert(b"last_hash".as_slice(), new_hash.as_slice())
                .map_err(JournalError::Storage)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;

        // Maintient les index secondaires en mémoire.
        {
            let mut actor_idx = self.actor_index.lock().unwrap();
            let mut action_idx = self.action_index.lock().unwrap();
            // Re-lit l'entrée signée telle que stockée pour indexer l'id exact.
            // Ici `entry` (avec signature/prev_hash) suffit.
            Self::index_entry(&mut actor_idx, &mut action_idx, &entry);
        }

        Ok(id)
    }

    /// Écrit une entrée de tour AUTOMATIQUEMENT signée (Ed25519) et chaînée.
    ///
    /// - `prev_hash` = SHA-256 de la dernière entrée signée (genèse
    ///   `[0u8; 32]` si le journal est vide) ;
    /// - signature Ed25519 sur le contenu canonique (JSON stable, champ
    ///   `signature` vidé) ;
    /// - `actor` = [`ActorId::Kernel`] (entrée système écrite par le démon),
    ///   action non réversible, confiance 1.0 ;
    /// - retourne l'[`AuditEntryId`] de l'entrée écrite.
    ///
    /// Aucun secret (clé de signature, token) n'est journalisé.
    pub fn append_signed(
        &mut self,
        action: &str,
        payload: serde_json::Value,
    ) -> std::result::Result<AuditEntryId, AuditError> {
        let action = action.trim();
        if action.is_empty() {
            return Err(AuditError::InvalidInput(
                "action vide : entrée d'audit refusée".to_string(),
            ));
        }
        let entry = AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: ActorId::Kernel,
            action: crate::entry::ActionRecord {
                action_type: action.to_string(),
                description: action.to_string(),
                parameters: payload,
                reversible: false,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: crate::entry::ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 0,
            },
            graph_nodes_used: vec![],
            confidence: 1.0,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };
        let id = self.append_inner(entry).map_err(AuditError::Journal)?;
        Ok(id)
    }

    pub fn get(&self, id: &EntryId) -> Result<Option<AuditEntry>> {
        let raw = self.get_raw(id)?;
        match raw {
            None => Ok(None),
            Some(mut entry) => {
                // Overlay F2 : le statut `reversed` vient de FLAGS_TABLE,
                // jamais d'une mutation de l'entrée stockée.
                if self.is_reversed(id)? {
                    entry.action.reversed = true;
                }
                Ok(Some(entry))
            }
        }
    }

    /// Lecture brute SANS overlay (usage interne : `verify_chain` doit voir
    /// les octets stockés exacts pour détecter toute mutation).
    fn get_raw(&self, id: &EntryId) -> Result<Option<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let id_bytes = id.0.as_bytes().to_vec();

        if let Some(entry_data) = table
            .get(id_bytes.as_slice())
            .map_err(JournalError::Storage)?
        {
            let entry =
                serde_json::from_slice(entry_data.value()).map_err(JournalError::Serialization)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// Message signé par les flags : `b"reversed:<uuid>"`.
    fn reversal_flag_message(id: &EntryId) -> Vec<u8> {
        format!("reversed:{}", id.0).into_bytes()
    }

    /// `true` si un flag `reversed` valide (signature Ed25519 du journal)
    /// existe pour `id`. Un flag à signature invalide => `Err` fail-closed
    /// (altération des flags détectée comme une rupture de chaîne).
    pub fn is_reversed(&self, id: &EntryId) -> Result<bool> {
        use ed25519_dalek::{Signature as DalekSignature, Verifier};
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let table = read_txn
            .open_table(FLAGS_TABLE)
            .map_err(JournalError::Table)?;
        let key = id.0.as_bytes().to_vec();
        let Some(flag_data) = table.get(key.as_slice()).map_err(JournalError::Storage)? else {
            return Ok(false);
        };
        let flag: ReversalFlag =
            serde_json::from_slice(flag_data.value()).map_err(JournalError::Serialization)?;
        if !flag.reversed {
            return Ok(false);
        }
        let verifying_key = self.signing_key.verifying_key();
        let sig = DalekSignature::from_slice(&flag.signature).map_err(|_| {
            hakille_core::HakilleError::Internal(format!(
                "flag de réversion corrompu pour {} (signature illisible)",
                id.0
            ))
        })?;
        verifying_key
            .verify(&Self::reversal_flag_message(id), &sig)
            .map_err(|_| {
                hakille_core::HakilleError::Internal(format!(
                    "flag de réversion invalide pour {} (altération détectée)",
                    id.0
                ))
            })?;
        Ok(true)
    }

    pub fn list_range(&self, from: Timestamp, to: Timestamp) -> Result<Vec<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let order_table = read_txn
            .open_table(ENTRY_ORDER_TABLE)
            .map_err(JournalError::Table)?;

        let seq = *self.seq.lock().unwrap();
        let mut results = Vec::new();
        for i in 0..seq {
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)?
                && let Some(entry_data) = entries_table
                    .get(id_data.value())
                    .map_err(JournalError::Storage)?
            {
                let entry: AuditEntry = serde_json::from_slice(entry_data.value())
                    .map_err(JournalError::Serialization)?;
                if entry.timestamp >= from && entry.timestamp <= to {
                    results.push(entry);
                }
            }
        }
        drop(order_table);
        drop(entries_table);
        drop(read_txn);
        // Overlay F2 après fermeture du txn de lecture (pas de mutation stockée).
        for entry in results.iter_mut() {
            if self.is_reversed(&entry.id)? {
                entry.action.reversed = true;
            }
        }
        Ok(results)
    }

    pub fn all_entries(&self) -> Result<Vec<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let order_table = read_txn
            .open_table(ENTRY_ORDER_TABLE)
            .map_err(JournalError::Table)?;

        let seq = *self.seq.lock().unwrap();
        let mut results = Vec::new();
        for i in 0..seq {
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)?
                && let Some(entry_data) = entries_table
                    .get(id_data.value())
                    .map_err(JournalError::Storage)?
            {
                let entry: AuditEntry = serde_json::from_slice(entry_data.value())
                    .map_err(JournalError::Serialization)?;
                results.push(entry);
            }
        }
        drop(order_table);
        drop(entries_table);
        drop(read_txn);
        // Overlay F2 (jamais de mutation des octets stockés).
        for entry in results.iter_mut() {
            if self.is_reversed(&entry.id)? {
                entry.action.reversed = true;
            }
        }
        Ok(results)
    }

    /// Vérifie l'intégrité complète du journal :
    /// - signatures Ed25519 de chaque entrée (contenu canonique) ;
    /// - liens `prev_hash` depuis la genèse `[0u8; 32]` ;
    /// - ancre `last_hash` de la metadata synchronisée avec le dernier
    ///   maillon (détecte une tête de chaîne falsifiée) ;
    /// - flags de réversion `reversed` (table annexe signée séparément).
    ///
    /// En cas de corruption, retourne un [`ChainReport`] avec l'index
    /// 0-based (`corrupted_index`), l'id (`broken_at`) et la nature
    /// (`corruption`). Seules les erreurs de stockage/sérialisation
    /// remontent en `Err` ; une entrée illisible est rapportée comme
    /// [`ChainCorruption::MalformedEntry`].
    pub fn verify_chain(&self) -> std::result::Result<ChainReport, AuditError> {
        use ed25519_dalek::{Signature as DalekSignature, Verifier};

        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let order_table = read_txn
            .open_table(ENTRY_ORDER_TABLE)
            .map_err(JournalError::Table)?;
        let meta_table = read_txn
            .open_table(META_TABLE)
            .map_err(JournalError::Table)?;

        let seq = *self.seq.lock().unwrap();
        let verifying_key = self.signing_key.verifying_key();
        let mut expected_prev_hash = vec![0u8; 32];
        let mut valid_count = 0u64;
        let mut last_id: Option<EntryId> = None;

        for i in 0..seq {
            let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)? else {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    None,
                    i,
                    ChainCorruption::MissingEntry,
                ));
            };
            let id_bytes = id_data.value();
            let known_id = uuid::Uuid::from_slice(id_bytes).ok().map(EntryId);
            let Some(entry_data) = entries_table.get(id_bytes).map_err(JournalError::Storage)?
            else {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    known_id,
                    i,
                    ChainCorruption::MissingEntry,
                ));
            };
            let Ok(entry) = serde_json::from_slice::<AuditEntry>(entry_data.value()) else {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    known_id,
                    i,
                    ChainCorruption::MalformedEntry,
                ));
            };

            // Hash incohérent : un `prev_hash` doit être un SHA-256 (32 octets).
            if entry.prev_hash.len() != 32 {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    Some(entry.id),
                    i,
                    ChainCorruption::HashMismatch,
                ));
            }

            // Rupture de chaîne : le lien vers l'entrée précédente est faux.
            if entry.prev_hash != expected_prev_hash {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    Some(entry.id),
                    i,
                    ChainCorruption::ChainBreak,
                ));
            }

            // Signature invalide : Ed25519 sur le contenu canonique.
            let recorded_sig = entry.signature.clone();
            let Ok(unsigned_bytes) = Self::signing_bytes(&entry) else {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    Some(entry.id),
                    i,
                    ChainCorruption::MalformedEntry,
                ));
            };
            let sig_ok = DalekSignature::from_slice(&recorded_sig)
                .ok()
                .is_some_and(|sig| verifying_key.verify(&unsigned_bytes, &sig).is_ok());
            if !sig_ok {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    Some(entry.id),
                    i,
                    ChainCorruption::InvalidSignature,
                ));
            }

            // Hash du maillon courant pour lier l'entrée suivante.
            let Ok(hash) = Self::chain_hash(&entry) else {
                return Ok(ChainReport::corrupted(
                    seq,
                    valid_count,
                    Some(entry.id),
                    i,
                    ChainCorruption::MalformedEntry,
                ));
            };
            expected_prev_hash = hash;
            last_id = Some(entry.id);
            valid_count += 1;
        }

        // Ancre de tête : `last_hash` (metadata) doit correspondre au hash
        // recalculé du dernier maillon. Absente = pas de contrôle (compat
        // journaux anciens), présente et différente = hash incohérent.
        if seq > 0
            && let Some(v) = meta_table
                .get(b"last_hash".as_slice())
                .map_err(JournalError::Storage)?
            && v.value() != expected_prev_hash.as_slice()
        {
            return Ok(ChainReport::corrupted(
                seq,
                valid_count,
                last_id,
                seq - 1,
                ChainCorruption::HashMismatch,
            ));
        }

        // F2 : vérifie les flags `reversed` (table annexe signée séparément).
        // Un flag à signature invalide = altération => chaîne rompue sur
        // l'entrée concernée (fail-closed, jamais silencieuse).
        {
            let flags_table = read_txn
                .open_table(FLAGS_TABLE)
                .map_err(JournalError::Table)?;
            let range = flags_table
                .range::<&[u8]>(..)
                .map_err(JournalError::Storage)?;
            for item in range {
                let (k, v) = item.map_err(JournalError::Storage)?;
                let flag: ReversalFlag =
                    serde_json::from_slice(v.value()).map_err(JournalError::Serialization)?;
                let id_str = String::from_utf8_lossy(k.value()).into_owned();
                let id_uuid = uuid::Uuid::parse_str(&flag.entry_id)
                    .or_else(|_| uuid::Uuid::parse_str(&id_str))
                    .map_err(|_| {
                        AuditError::Corruption(format!(
                            "flag de réversion illisible (entry_id={})",
                            flag.entry_id
                        ))
                    })?;
                let msg = format!("reversed:{id_uuid}").into_bytes();
                let ok = DalekSignature::from_slice(&flag.signature)
                    .ok()
                    .is_some_and(|sig| verifying_key.verify(&msg, &sig).is_ok());
                if !ok {
                    let corrupted_index = (0..seq).find(|&i| {
                        order_table
                            .get(i)
                            .ok()
                            .flatten()
                            .is_some_and(|d| d.value() == id_uuid.as_bytes())
                    });
                    return Ok(ChainReport {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: Some(EntryId(id_uuid)),
                        corrupted_index,
                        corruption: Some(ChainCorruption::InvalidSignature),
                    });
                }
            }
        }

        Ok(ChainReport::healthy(seq, valid_count))
    }

    pub fn revert(&self, id: &EntryId) -> Result<AuditEntry> {
        let entry = self.get(id)?.ok_or_else(|| {
            hakille_core::HakilleError::Internal(format!("Entry {} not found", id.0))
        })?;

        if !entry.action.reversible {
            return Err(hakille_core::HakilleError::CapabilityDenied(format!(
                "Action {} is marked non-reversible",
                id.0
            )));
        }

        let compensation_entry = AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: entry.actor.clone(),
            action: crate::entry::ActionRecord {
                action_type: format!("revert_{}", entry.action.action_type),
                description: format!("Rollback of action: {}", entry.action.description),
                parameters: entry.action.parameters.clone(),
                reversible: false,
                reversed: true,
            },
            capability_used: entry.capability_used.clone(),
            reasoning_trace: crate::entry::ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 0,
            },
            graph_nodes_used: vec![],
            confidence: 1.0,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };

        self.append(compensation_entry.clone())?;
        Ok(compensation_entry)
    }

    /// F2 : marque l'entrée comme révoquée SANS MUTER les octets stockés.
    ///
    /// L'ancien code réécrivait l'entrée + re-signait toute la chaîne aval,
    /// ce qui effaçait la preuve de mutation (`verify_chain` restait verte
    /// après altération : fail-open). Désormais :
    /// - l'entrée originale est INTANGIBLE (tout `write` direct = signature
    ///   invalide = `verify_chain.broken_at = Some(id)`) ;
    /// - le statut `reversed` vit dans la table annexe `flags`, signé
    ///   séparément (Ed25519 du journal sur `b"reversed:<uuid>"`) ;
    /// - la révocation métier passe TOUJOURS par `revert()` (compensation
    ///   append-only) + ce flag. Idempotent. Fail-closed si introuvable.
    pub fn mark_reversed(&self, id: &EntryId) -> Result<()> {
        use ed25519_dalek::Signer;
        // Fail-closed : l'entrée doit exister (lecture brute, pas d'overlay).
        let exists = self.get_raw(id)?.is_some();
        if !exists {
            return Err(hakille_core::HakilleError::Internal(format!(
                "Entry {} not found",
                id.0
            )));
        }
        if self.is_reversed(id)? {
            return Ok(());
        }
        let sig = self.signing_key.sign(&Self::reversal_flag_message(id));
        let flag = ReversalFlag {
            entry_id: id.0.to_string(),
            reversed: true,
            signature: sig.to_bytes().to_vec(),
        };
        let flag_bytes = serde_json::to_vec(&flag).map_err(JournalError::Serialization)?;
        let key = id.0.as_bytes().to_vec();
        let write_txn = self.db.begin_write().map_err(JournalError::Transaction)?;
        {
            let mut table = write_txn
                .open_table(FLAGS_TABLE)
                .map_err(JournalError::Table)?;
            table
                .insert(key.as_slice(), flag_bytes.as_slice())
                .map_err(JournalError::Storage)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;
        Ok(())
    }

    /// Révocation sous capability ÉPINGLÉE (F3) : vérifie le token contre la
    /// clé racine du journal (`new_with_root`), jamais en mode non-épinglé.
    ///
    /// - Exige un scope Audit/Admin : `FullSystem + Admin` (admin, peut
    ///   révoquer pour autrui) ou `FullSystem + Read` (auditeur, seulement
    ///   ses propres actions). `hakille-core` ne définit pas de scope
    ///   `Audit` dédié : `FullSystem` joue ce rôle (fail-closed via Biscuit).
    /// - Exige `caller == acteur original` OU `Admin` (cap Admin valide).
    /// - Refuse si le token présente un nonce/jti révoqué (F3/F5).
    /// - Compense via `revert()` (append-only) + flag `mark_reversed`
    ///   (table annexe signée, JAMAIS de mutation in-place).
    ///
    /// Sans clé racine épinglée (`new` legacy), refuse fail-closed avec un
    /// message explicite (passer à `new_with_root`).
    pub fn revert_with_cap(
        &self,
        id: &EntryId,
        cap: &CapabilityToken,
        caller: &ActorId,
    ) -> Result<AuditEntry> {
        let root = self.pinned_root.as_ref().ok_or_else(|| {
            hakille_core::HakilleError::CapabilityDenied(
                "revert_with_cap refuse : clé racine non épinglée (construire le journal via new_with_root)".to_string(),
            )
        })?;
        // F3/F5 : le token d'autorisation lui-même ne doit pas être révoqué.
        let (_, nonce, _) = token_binding_facts(cap);
        if let Some(n) = nonce
            && self.is_token_revoked(&n)?
        {
            return Err(hakille_core::HakilleError::CapabilityDenied(format!(
                "token d'autorisation révoqué (nonce/jti {n})"
            )));
        }
        let entry = self.get(id)?.ok_or_else(|| {
            hakille_core::HakilleError::Internal(format!("Entry {} not found", id.0))
        })?;
        let is_admin = verify_capability_with_root_key(
            cap,
            root,
            CapabilityScope::FullSystem,
            CapabilityAction::Admin,
        )
        .is_ok();
        let is_auditor = verify_capability_with_root_key(
            cap,
            root,
            CapabilityScope::FullSystem,
            CapabilityAction::Read,
        )
        .is_ok();
        if !is_admin && !is_auditor {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "revert exige une capability Audit/Admin (FullSystem Read ou Admin)".to_string(),
            ));
        }
        if &entry.actor != caller && !is_admin {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "seul l'acteur original ou un Admin peut révoquer cette entrée".to_string(),
            ));
        }
        let compensation = self.revert(id)?;
        self.mark_reversed(id)?;
        Ok(compensation)
    }

    /// F3/F5 : révoque un token par son nonce/jti (`jti → reason`).
    /// Idempotent (ré-révocation = motif écrasé). Persiste en redb.
    pub fn revoke_token(&self, jti: &str, reason: &str) -> Result<()> {
        if jti.trim().is_empty() {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "jti vide refusé".to_string(),
            ));
        }
        let write_txn = self.db.begin_write().map_err(JournalError::Transaction)?;
        {
            let mut table = write_txn
                .open_table(REVOCATIONS_TABLE)
                .map_err(JournalError::Table)?;
            table
                .insert(jti.as_bytes(), reason.as_bytes())
                .map_err(JournalError::Storage)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;
        Ok(())
    }

    /// `true` si le nonce/jti est révoqué.
    pub fn is_token_revoked(&self, jti: &str) -> Result<bool> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let table = read_txn
            .open_table(REVOCATIONS_TABLE)
            .map_err(JournalError::Table)?;
        Ok(table
            .get(jti.as_bytes())
            .map_err(JournalError::Storage)?
            .is_some())
    }

    /// Motif de révocation (`None` = non révoqué).
    pub fn revocation_reason(&self, jti: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let table = read_txn
            .open_table(REVOCATIONS_TABLE)
            .map_err(JournalError::Table)?;
        match table.get(jti.as_bytes()).map_err(JournalError::Storage)? {
            None => Ok(None),
            Some(v) => Ok(Some(String::from_utf8_lossy(v.value()).into_owned())),
        }
    }

    /// F4 : append avec liaison token — calcule SHA256 des octets du token
    /// et le stocke dans `entry.token_hash` AVANT signature (la signature
    /// couvre donc la liaison : anti-substitution de token).
    pub fn append_with_token_hash(
        &self,
        mut entry: AuditEntry,
        token: &CapabilityToken,
    ) -> Result<EntryId> {
        let bytes = token.biscuit.to_vec().map_err(|e| {
            hakille_core::HakilleError::CapabilityDenied(format!("token serialization: {e}"))
        })?;
        entry.token_hash = Some(sha256_token_hash(&bytes));
        self.append(entry)
    }

    /// Accès indexé par acteur (HashMap mémoire, reconstruit au load).
    pub fn entries_by_actor(&self, actor: &ActorId) -> Result<Vec<AuditEntry>> {
        let ids: Vec<EntryId> = self
            .actor_index
            .lock()
            .unwrap()
            .get(actor)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(e) = self.get(&id)? {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Accès indexé par type d'action (HashMap mémoire, reconstruit au load).
    pub fn entries_by_action(&self, action_type: &str) -> Result<Vec<AuditEntry>> {
        let ids: Vec<EntryId> = self
            .action_index
            .lock()
            .unwrap()
            .get(action_type)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(e) = self.get(&id)? {
                out.push(e);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use hakille_core::{ActorId, Timestamp, UserId};
    use uuid::Uuid;

    fn make_test_entry(reversible: bool) -> AuditEntry {
        AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: ActorId::User(UserId(Uuid::now_v7())),
            action: crate::entry::ActionRecord {
                action_type: "send_email".into(),
                description: "Sent email to Bob".into(),
                parameters: serde_json::json!({"recipient": "bob@example.com"}),
                reversible,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: crate::entry::ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 15,
            },
            graph_nodes_used: vec![],
            confidence: 0.95,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        }
    }

    #[test]
    fn test_audit_journal_append_verify_and_revert() {
        let db_path =
            std::env::temp_dir().join(format!("test_journal_{}.redb", uuid::Uuid::now_v7()));
        let db_str = db_path.to_str().unwrap();

        let signing_key = SigningKey::from_bytes(&[42u8; 32]);

        let journal = AuditJournal::new(db_str, signing_key).expect("journal creation failed");

        // Append 3 entries
        let e1 = make_test_entry(true);
        let id1 = journal.append(e1).unwrap();

        let e2 = make_test_entry(false);
        let id2 = journal.append(e2).unwrap();

        let e3 = make_test_entry(true);
        let _id3 = journal.append(e3).unwrap();

        // Verify chain integrity
        let ver = journal.verify_chain().expect("verification should pass");
        assert_eq!(ver.total_entries, 3);
        assert_eq!(ver.valid_entries, 3);
        assert_eq!(ver.broken_at, None);

        // Revert reversible action e1
        let reverted = journal.revert(&id1).expect("revert should succeed");
        assert_eq!(reverted.action.action_type, "revert_send_email");

        // Verify chain again (now 4 entries, all cryptographically intact)
        let ver2 = journal.verify_chain().expect("verification should pass");
        assert_eq!(ver2.total_entries, 4);
        assert_eq!(ver2.valid_entries, 4);
        assert_eq!(ver2.broken_at, None);

        // Attempting to revert non-reversible e2 must fail
        assert!(journal.revert(&id2).is_err());

        // Cleanup temp file
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_audit_journal_tampering_detection() {
        let db_path =
            std::env::temp_dir().join(format!("test_journal_tamper_{}.redb", uuid::Uuid::now_v7()));
        let db_str = db_path.to_str().unwrap();
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let journal = AuditJournal::new(db_str, signing_key).expect("journal creation failed");

        let e1 = make_test_entry(true);
        let _id1 = journal.append(e1).unwrap();
        let e2 = make_test_entry(true);
        let id2 = journal.append(e2).unwrap();

        // Chain is valid initially
        let ver = journal.verify_chain().unwrap();
        assert_eq!(ver.valid_entries, 2);
        assert_eq!(ver.broken_at, None);

        // Tamper directly with entry 2 in the database
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).unwrap();
            let mut tampered_entry = journal.get(&id2).unwrap().unwrap();
            tampered_entry.action.description = "MALICIOUS MODIFICATION".to_string();
            let tampered_bytes = serde_json::to_vec(&tampered_entry).unwrap();
            entries_table
                .insert(id2.0.as_bytes().as_slice(), tampered_bytes.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();

        // Verification must now detect tampering and report broken_at == Some(id2)
        let ver_tampered = journal.verify_chain().unwrap();
        assert_eq!(ver_tampered.valid_entries, 1);
        assert_eq!(ver_tampered.broken_at, Some(id2));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_audit_journal_perf_append_200_and_verify() {
        let db_path = std::env::temp_dir().join(format!(
            "test_journal_perf_timing_{}.redb",
            uuid::Uuid::now_v7()
        ));
        let db_str = db_path.to_str().unwrap();
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let journal = AuditJournal::new(db_str, signing_key).expect("journal creation failed");

        let start = std::time::Instant::now();
        for _ in 0..200 {
            journal.append(make_test_entry(true)).unwrap();
        }
        assert!(
            start.elapsed().as_secs() < 5,
            "budget perf append dépassé : {:?}",
            start.elapsed()
        );

        let vstart = std::time::Instant::now();
        let ver = journal.verify_chain().unwrap();
        assert_eq!(ver.total_entries, 200);
        assert_eq!(ver.valid_entries, 200);
        assert_eq!(ver.broken_at, None);
        assert!(
            vstart.elapsed().as_secs() < 10,
            "budget perf verify dépassé : {:?}",
            vstart.elapsed()
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_revert_with_cap_and_indexes() {
        use hakille_core::{
            CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp as Ts,
        };
        let db_path =
            std::env::temp_dir().join(format!("test_journal_cap_{}.redb", uuid::Uuid::now_v7()));
        // F3 : journal épinglé sur la racine de l'autorité (pinning obligatoire).
        let authority = CapabilityAuthority::new();
        let journal = AuditJournal::new_with_root(
            db_path.to_str().unwrap(),
            SigningKey::from_bytes(&[7u8; 32]),
            authority.root_public_key(),
        )
        .unwrap();

        let actor = ActorId::User(UserId(Uuid::now_v7()));
        let mut e = make_test_entry(true);
        e.actor = actor.clone();
        e.action.action_type = "pay".to_string();
        let id = journal.append(e).unwrap();
        journal.rebuild_indexes().unwrap();
        assert_eq!(journal.entries_by_actor(&actor).unwrap().len(), 1);
        assert_eq!(journal.entries_by_action("pay").unwrap().len(), 1);
        assert!(journal.entries_by_action("nope").unwrap().is_empty());

        let admin_cap = authority
            .create_root_token(
                vec![CapabilityScope::FullSystem],
                vec![CapabilityAction::Admin, CapabilityAction::Read],
                Ts(chrono::Utc::now() + chrono::Duration::seconds(3600)),
            )
            .unwrap();
        // Admin révoque pour autrui : ok, flag `reversed` visible en lecture.
        let other = ActorId::User(UserId(Uuid::now_v7()));
        let comp = journal
            .revert_with_cap(&id, &admin_cap, &other)
            .expect("admin revert");
        assert_eq!(comp.action.action_type, "revert_pay");
        assert!(journal.get(&id).unwrap().unwrap().action.reversed);
        assert!(journal.is_reversed(&id).unwrap());
        // Les octets stockés restent intacts (pas de mutation in-place).
        assert!(!journal.get_raw(&id).unwrap().unwrap().action.reversed);
        // Chaîne toujours valide après flag + compensation.
        let ver = journal.verify_chain().unwrap();
        assert_eq!(ver.broken_at, None);

        // Auditeur (Read seul) ne peut révoquer que ses propres entrées.
        let read_cap = authority
            .create_root_token(
                vec![CapabilityScope::FullSystem],
                vec![CapabilityAction::Read],
                Ts(chrono::Utc::now() + chrono::Duration::seconds(3600)),
            )
            .unwrap();
        let mut e2 = make_test_entry(true);
        e2.actor = actor.clone();
        e2.action.action_type = "pay2".to_string();
        let id2 = journal.append(e2).unwrap();
        assert!(journal.revert_with_cap(&id2, &read_cap, &actor).is_ok());
        let mut e3 = make_test_entry(true);
        e3.action.action_type = "pay3".to_string();
        let id3 = journal.append(e3).unwrap();
        assert!(journal.revert_with_cap(&id3, &read_cap, &actor).is_err());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_mark_reversed_flag_then_mutation_breaks_chain() {
        // F2 : le flag ne casse PAS la chaîne, mais toute mutation SI.
        let db_path =
            std::env::temp_dir().join(format!("test_journal_flags_{}.redb", uuid::Uuid::now_v7()));
        let journal = AuditJournal::new(
            db_path.to_str().unwrap(),
            SigningKey::from_bytes(&[9u8; 32]),
        )
        .unwrap();
        let id = journal.append(make_test_entry(true)).unwrap();
        journal.mark_reversed(&id).unwrap();
        // Flag visible en lecture, chaîne intacte.
        assert!(journal.get(&id).unwrap().unwrap().action.reversed);
        let ver = journal.verify_chain().unwrap();
        assert_eq!(ver.broken_at, None);
        assert_eq!(ver.valid_entries, 1);
        // Mutation directe des octets stockés => broken_at = Some(id).
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).unwrap();
            let mut raw = journal.get_raw(&id).unwrap().unwrap();
            raw.action.description = "MUTATION IN-PLACE".to_string();
            let bytes = serde_json::to_vec(&raw).unwrap();
            entries_table
                .insert(id.0.as_bytes().as_slice(), bytes.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();
        let broken = journal.verify_chain().unwrap();
        assert_eq!(broken.broken_at, Some(id));
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_revert_with_cap_requires_pinning() {
        // F3 : sans `new_with_root`, revert_with_cap refuse fail-closed
        // (aucune vérification non-épinglée en prod).
        use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
        let db_path =
            std::env::temp_dir().join(format!("test_journal_nopin_{}.redb", uuid::Uuid::now_v7()));
        let journal = AuditJournal::new(
            db_path.to_str().unwrap(),
            SigningKey::from_bytes(&[7u8; 32]),
        )
        .unwrap();
        let authority = CapabilityAuthority::new();
        let id = journal.append(make_test_entry(true)).unwrap();
        let cap = authority
            .create_root_token(
                vec![CapabilityScope::FullSystem],
                vec![CapabilityAction::Admin],
                hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
            )
            .unwrap();
        let caller = ActorId::User(UserId(Uuid::now_v7()));
        let err = journal
            .revert_with_cap(&id, &cap, &caller)
            .expect_err("pin manquant");
        assert!(err.to_string().contains("non épinglée"));
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_revert_with_cap_rejects_revoked_token() {
        // F3/F5 : un token d'autorisation révoqué (nonce/jti) est refusé.
        use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
        let db_path = std::env::temp_dir().join(format!(
            "test_journal_revoked_{}.redb",
            uuid::Uuid::now_v7()
        ));
        let authority = CapabilityAuthority::new();
        let journal = AuditJournal::new_with_root(
            db_path.to_str().unwrap(),
            SigningKey::from_bytes(&[7u8; 32]),
            authority.root_public_key(),
        )
        .unwrap();
        let actor = ActorId::User(UserId(Uuid::now_v7()));
        let mut e = make_test_entry(true);
        e.actor = actor.clone();
        let id = journal.append(e).unwrap();
        let cap = authority
            .create_root_token_with_binding(
                vec![CapabilityScope::FullSystem],
                vec![CapabilityAction::Admin],
                hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
                None,
                Some("jti-revoque-42"),
                None,
            )
            .unwrap();
        journal.revoke_token("jti-revoque-42", "compromis").unwrap();
        assert!(journal.is_token_revoked("jti-revoque-42").unwrap());
        assert_eq!(
            journal
                .revocation_reason("jti-revoque-42")
                .unwrap()
                .as_deref(),
            Some("compromis")
        );
        assert!(journal.revert_with_cap(&id, &cap, &actor).is_err());
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_append_with_token_hash_binds_token() {
        // F4 : le hash SHA256 du token est stocké AVANT signature.
        use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
        let db_path = std::env::temp_dir().join(format!(
            "test_journal_tokhash_{}.redb",
            uuid::Uuid::now_v7()
        ));
        let journal = AuditJournal::new(
            db_path.to_str().unwrap(),
            SigningKey::from_bytes(&[7u8; 32]),
        )
        .unwrap();
        let authority = CapabilityAuthority::new();
        let tok = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
            )
            .unwrap();
        let id = journal
            .append_with_token_hash(make_test_entry(true), &tok)
            .unwrap();
        let stored = journal.get(&id).unwrap().unwrap();
        let expected = crate::entry::sha256_token_hash(&tok.biscuit.to_vec().unwrap());
        assert_eq!(stored.token_hash, Some(expected));
        // Chaîne valide avec le nouveau champ (signature couvre token_hash).
        assert_eq!(journal.verify_chain().unwrap().broken_at, None);
        // Compat serde : un JSON legacy sans token_hash se relit (None).
        let legacy = serde_json::json!({
            "id": stored.id, "timestamp": stored.timestamp, "actor": stored.actor,
            "action": stored.action, "capability_used": stored.capability_used,
            "reasoning_trace": stored.reasoning_trace, "graph_nodes_used": [],
            "confidence": 1.0, "cost": null, "signature": [], "prev_hash": []
        });
        let back: AuditEntry = serde_json::from_value(legacy).unwrap();
        assert_eq!(back.token_hash, None);
        let _ = std::fs::remove_file(db_path);
    }

    /// Clé de test fixe, JETABLE, jamais utilisée hors tests (pas un secret).
    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[42u8; 32])
    }

    fn temp_journal() -> (std::path::PathBuf, AuditJournal) {
        let db_path =
            std::env::temp_dir().join(format!("test_journal_signed_{}.redb", uuid::Uuid::now_v7()));
        let journal = AuditJournal::new(db_path.to_str().unwrap(), test_key())
            .expect("journal creation failed");
        (db_path, journal)
    }

    #[test]
    fn test_append_signed_genesis_and_verify_empty() {
        let (db_path, mut journal) = temp_journal();

        // Genèse : chaîne vide = valide, rien à corrompre.
        let empty = journal.verify_chain().unwrap();
        assert_eq!(empty.total_entries, 0);
        assert_eq!(empty.valid_entries, 0);
        assert_eq!(empty.corrupted_index, None);
        assert_eq!(empty.corruption, None);
        assert!(empty.is_valid());

        // Première entrée signée : prev_hash = genèse [0u8; 32].
        let id = journal
            .append_signed("CognitiveTurn", serde_json::json!({ "text": "bonjour" }))
            .unwrap();
        let entry = journal.get(&id).unwrap().unwrap();
        assert_eq!(entry.actor, ActorId::Kernel);
        assert_eq!(entry.action.action_type, "CognitiveTurn");
        assert_eq!(entry.action.parameters["text"], "bonjour");
        assert_eq!(entry.prev_hash, vec![0u8; 32], "lien de genèse");
        assert_eq!(entry.signature.len(), 64, "signature Ed25519");

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.total_entries, 1);
        assert_eq!(report.valid_entries, 1);
        assert_eq!(report.broken_at, None);
        assert!(report.is_valid());

        // Entrée invalide refusée sans écriture.
        assert!(journal.append_signed("   ", serde_json::json!({})).is_err());
        assert_eq!(journal.verify_chain().unwrap().total_entries, 1);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_append_signed_three_entries_chain_links() {
        let (db_path, mut journal) = temp_journal();

        let id1 = journal
            .append_signed("Turn", serde_json::json!({ "n": 1 }))
            .unwrap();
        let id2 = journal
            .append_signed("Turn", serde_json::json!({ "n": 2 }))
            .unwrap();
        let id3 = journal
            .append_signed("Turn", serde_json::json!({ "n": 3 }))
            .unwrap();

        let e1 = journal.get(&id1).unwrap().unwrap();
        let e2 = journal.get(&id2).unwrap().unwrap();
        let e3 = journal.get(&id3).unwrap().unwrap();
        assert_eq!(e1.prev_hash, vec![0u8; 32]);
        assert_eq!(e2.prev_hash, AuditJournal::chain_hash(&e1).unwrap());
        assert_eq!(e3.prev_hash, AuditJournal::chain_hash(&e2).unwrap());
        assert_ne!(e1.signature, e2.signature);

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.total_entries, 3);
        assert_eq!(report.valid_entries, 3);
        assert_eq!(report.broken_at, None);
        assert_eq!(report.corrupted_index, None);
        assert_eq!(report.corruption, None);
        assert!(report.is_valid());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_verify_chain_detects_tampered_entry_byte() {
        let (db_path, mut journal) = temp_journal();
        let _id1 = journal
            .append_signed("Turn", serde_json::json!({ "n": 1 }))
            .unwrap();
        let id2 = journal
            .append_signed("Turn", serde_json::json!({ "n": 2 }))
            .unwrap();
        let _id3 = journal
            .append_signed("Turn", serde_json::json!({ "n": 3 }))
            .unwrap();

        // Altère un octet utile de l'entrée 2 (JSON toujours lisible).
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).unwrap();
            let mut raw = journal.get_raw(&id2).unwrap().unwrap();
            raw.action.description.push('X');
            let bytes = serde_json::to_vec(&raw).unwrap();
            entries_table
                .insert(id2.0.as_bytes().as_slice(), bytes.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.total_entries, 3);
        assert_eq!(report.valid_entries, 1, "entrées 1 valides seulement");
        assert_eq!(report.corrupted_index, Some(1), "index 0-based");
        assert_eq!(report.corruption, Some(ChainCorruption::InvalidSignature));
        assert_eq!(report.broken_at, Some(id2));
        assert!(!report.is_valid());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_verify_chain_detects_chain_break() {
        let (db_path, mut journal) = temp_journal();
        let _id1 = journal
            .append_signed("Turn", serde_json::json!({ "n": 1 }))
            .unwrap();
        let id2 = journal
            .append_signed("Turn", serde_json::json!({ "n": 2 }))
            .unwrap();

        // prev_hash de 32 octets mais faux => rupture de chaîne.
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).unwrap();
            let mut raw = journal.get_raw(&id2).unwrap().unwrap();
            raw.prev_hash = vec![7u8; 32];
            let bytes = serde_json::to_vec(&raw).unwrap();
            entries_table
                .insert(id2.0.as_bytes().as_slice(), bytes.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.valid_entries, 1);
        assert_eq!(report.corrupted_index, Some(1));
        assert_eq!(report.corruption, Some(ChainCorruption::ChainBreak));
        assert_eq!(report.broken_at, Some(id2));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_verify_chain_detects_hash_inconsistency() {
        let (db_path, mut journal) = temp_journal();
        let _id1 = journal
            .append_signed("Turn", serde_json::json!({ "n": 1 }))
            .unwrap();
        let id2 = journal
            .append_signed("Turn", serde_json::json!({ "n": 2 }))
            .unwrap();

        // prev_hash malformé : 31 octets => hash incohérent (pas un SHA-256).
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).unwrap();
            let mut raw = journal.get_raw(&id2).unwrap().unwrap();
            raw.prev_hash = vec![7u8; 31];
            let bytes = serde_json::to_vec(&raw).unwrap();
            entries_table
                .insert(id2.0.as_bytes().as_slice(), bytes.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.valid_entries, 1);
        assert_eq!(report.corrupted_index, Some(1));
        assert_eq!(report.corruption, Some(ChainCorruption::HashMismatch));
        assert_eq!(report.broken_at, Some(id2));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_verify_chain_detects_tampered_last_hash_anchor() {
        let (db_path, mut journal) = temp_journal();
        let _id1 = journal
            .append_signed("Turn", serde_json::json!({ "n": 1 }))
            .unwrap();
        let id2 = journal
            .append_signed("Turn", serde_json::json!({ "n": 2 }))
            .unwrap();

        // Tête de chaîne falsifiée dans la metadata => hash incohérent
        // sur la dernière entrée (les octets d'entrée restent signés).
        let write_txn = journal.db.begin_write().unwrap();
        {
            let mut meta_table = write_txn.open_table(META_TABLE).unwrap();
            meta_table
                .insert(b"last_hash".as_slice(), [9u8; 32].as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();

        let report = journal.verify_chain().unwrap();
        assert_eq!(report.total_entries, 2);
        assert_eq!(report.valid_entries, 2, "signatures toujours valides");
        assert_eq!(report.corrupted_index, Some(1));
        assert_eq!(report.corruption, Some(ChainCorruption::HashMismatch));
        assert_eq!(report.broken_at, Some(id2));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_signed_entries_are_deterministic() {
        // Même clé fixe + mêmes octets d'entrée => mêmes signature, hash,
        // et même rapport de vérification (aucune horloge/RNG impliqué).
        let fixed_id = EntryId(Uuid::from_u128(0x018f_2a3b_4c5d_6e7f_8091_a2b3_c4d5_e6f7));
        let fixed_ts = Timestamp(chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap());
        let make_entry = || AuditEntry {
            id: fixed_id.clone(),
            timestamp: fixed_ts.clone(),
            actor: ActorId::Kernel,
            action: crate::entry::ActionRecord {
                action_type: "Turn".to_string(),
                description: "Turn".to_string(),
                parameters: serde_json::json!({ "n": 1 }),
                reversible: false,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: crate::entry::ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 0,
            },
            graph_nodes_used: vec![],
            confidence: 1.0,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };

        let (p1, j1) = temp_journal();
        let (p2, j2) = temp_journal();
        let a = j1.append(make_entry()).unwrap();
        let b = j2.append(make_entry()).unwrap();
        assert_eq!(a, b);
        let e1 = j1.get(&a).unwrap().unwrap();
        let e2 = j2.get(&b).unwrap().unwrap();
        assert_eq!(e1.signature, e2.signature);
        assert_eq!(e1.prev_hash, e2.prev_hash);
        assert_eq!(
            AuditJournal::chain_hash(&e1).unwrap(),
            AuditJournal::chain_hash(&e2).unwrap()
        );
        assert_eq!(j1.verify_chain().unwrap(), j2.verify_chain().unwrap());

        let _ = std::fs::remove_file(p1);
        let _ = std::fs::remove_file(p2);
    }
}
