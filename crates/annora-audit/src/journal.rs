use crate::entry::{AuditEntry, sha256_token_hash};
use annora_core::{
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

impl From<JournalError> for annora_core::AnnoraError {
    fn from(err: JournalError) -> Self {
        annora_core::AnnoraError::Internal(err.to_string())
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainVerification {
    pub total_entries: u64,
    pub valid_entries: u64,
    pub broken_at: Option<EntryId>,
}

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
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)? {
                if let Some(entry_data) = entries_table
                    .get(id_data.value())
                    .map_err(JournalError::Storage)?
                {
                    let entry: AuditEntry = serde_json::from_slice(entry_data.value())
                        .map_err(JournalError::Serialization)?;
                    results.push(entry);
                }
            }
        }
        Ok(results)
    }

    pub fn append(&self, mut entry: AuditEntry) -> Result<EntryId> {
        use ed25519_dalek::Signer;

        let mut last_hash_lock = self.last_hash.lock().unwrap();
        let mut seq_lock = self.seq.lock().unwrap();
        entry.prev_hash = last_hash_lock.clone();

        // Sign the entry (excluding signature field itself)
        entry.signature = vec![];
        let bytes_to_sign = serde_json::to_vec(&entry).map_err(JournalError::Serialization)?;
        let signature = self.signing_key.sign(&bytes_to_sign);
        entry.signature = signature.to_bytes().to_vec();

        let id = entry.id.clone();
        let id_bytes = id.0.as_bytes().to_vec();
        let entry_bytes = serde_json::to_vec(&entry).map_err(JournalError::Serialization)?;

        // Compute new hash
        let mut hasher = Sha256::new();
        hasher.update(&entry_bytes);
        let new_hash = hasher.finalize().to_vec();
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
            annora_core::AnnoraError::Internal(format!(
                "flag de réversion corrompu pour {} (signature illisible)",
                id.0
            ))
        })?;
        verifying_key
            .verify(&Self::reversal_flag_message(id), &sig)
            .map_err(|_| {
                annora_core::AnnoraError::Internal(format!(
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
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)? {
                if let Some(entry_data) = entries_table
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
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)? {
                if let Some(entry_data) = entries_table
                    .get(id_data.value())
                    .map_err(JournalError::Storage)?
                {
                    let entry: AuditEntry = serde_json::from_slice(entry_data.value())
                        .map_err(JournalError::Serialization)?;
                    results.push(entry);
                }
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

    pub fn verify_chain(&self) -> Result<ChainVerification> {
        use ed25519_dalek::{Signature as DalekSignature, Verifier};

        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn
            .open_table(ENTRIES_TABLE)
            .map_err(JournalError::Table)?;
        let order_table = read_txn
            .open_table(ENTRY_ORDER_TABLE)
            .map_err(JournalError::Table)?;

        let seq = *self.seq.lock().unwrap();
        let verifying_key = self.signing_key.verifying_key();
        let mut expected_prev_hash = vec![0u8; 32];
        let mut valid_count = 0u64;

        for i in 0..seq {
            let id_data = match order_table.get(i).map_err(JournalError::Storage)? {
                Some(data) => data,
                None => {
                    return Ok(ChainVerification {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: None,
                    });
                }
            };
            let entry_data = match entries_table
                .get(id_data.value())
                .map_err(JournalError::Storage)?
            {
                Some(data) => data,
                None => {
                    return Ok(ChainVerification {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: None,
                    });
                }
            };
            let mut entry: AuditEntry =
                serde_json::from_slice(entry_data.value()).map_err(JournalError::Serialization)?;

            // Verify prev_hash
            if entry.prev_hash != expected_prev_hash {
                return Ok(ChainVerification {
                    total_entries: seq,
                    valid_entries: valid_count,
                    broken_at: Some(entry.id),
                });
            }

            // Verify signature
            let recorded_sig = entry.signature.clone();
            entry.signature = vec![];
            let unsigned_bytes = serde_json::to_vec(&entry).map_err(JournalError::Serialization)?;
            if let Ok(dalek_sig) = DalekSignature::from_slice(&recorded_sig) {
                if verifying_key.verify(&unsigned_bytes, &dalek_sig).is_err() {
                    return Ok(ChainVerification {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: Some(entry.id),
                    });
                }
            } else {
                return Ok(ChainVerification {
                    total_entries: seq,
                    valid_entries: valid_count,
                    broken_at: Some(entry.id),
                });
            }

            // Compute hash for next block
            entry.signature = recorded_sig;
            let entry_bytes = serde_json::to_vec(&entry).map_err(JournalError::Serialization)?;
            let mut hasher = Sha256::new();
            hasher.update(&entry_bytes);
            expected_prev_hash = hasher.finalize().to_vec();
            valid_count += 1;
        }

        // F2 : vérifie les flags `reversed` (table annexe signée séparément).
        // Un flag à signature invalide = altération => chaîne rompue sur
        // l'entrée concernée (fail-closed, jamais silencieuse).
        {
            use ed25519_dalek::{Signature as DalekSignature, Verifier};
            let verifying_key = self.signing_key.verifying_key();
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
                        annora_core::AnnoraError::Internal(format!(
                            "flag de réversion illisible (entry_id={})",
                            flag.entry_id
                        ))
                    })?;
                let msg = format!("reversed:{id_uuid}").into_bytes();
                let ok = DalekSignature::from_slice(&flag.signature)
                    .ok()
                    .is_some_and(|sig| verifying_key.verify(&msg, &sig).is_ok());
                if !ok {
                    return Ok(ChainVerification {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: Some(EntryId(id_uuid)),
                    });
                }
            }
        }

        Ok(ChainVerification {
            total_entries: seq,
            valid_entries: valid_count,
            broken_at: None,
        })
    }

    pub fn revert(&self, id: &EntryId) -> Result<AuditEntry> {
        let entry = self.get(id)?.ok_or_else(|| {
            annora_core::AnnoraError::Internal(format!("Entry {} not found", id.0))
        })?;

        if !entry.action.reversible {
            return Err(annora_core::AnnoraError::CapabilityDenied(format!(
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
            return Err(annora_core::AnnoraError::Internal(format!(
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
    ///   ses propres actions). `annora-core` ne définit pas de scope
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
            annora_core::AnnoraError::CapabilityDenied(
                "revert_with_cap refuse : clé racine non épinglée (construire le journal via new_with_root)".to_string(),
            )
        })?;
        // F3/F5 : le token d'autorisation lui-même ne doit pas être révoqué.
        let (_, nonce, _) = token_binding_facts(cap);
        if let Some(n) = nonce {
            if self.is_token_revoked(&n)? {
                return Err(annora_core::AnnoraError::CapabilityDenied(format!(
                    "token d'autorisation révoqué (nonce/jti {n})"
                )));
            }
        }
        let entry = self.get(id)?.ok_or_else(|| {
            annora_core::AnnoraError::Internal(format!("Entry {} not found", id.0))
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
            return Err(annora_core::AnnoraError::CapabilityDenied(
                "revert exige une capability Audit/Admin (FullSystem Read ou Admin)".to_string(),
            ));
        }
        if &entry.actor != caller && !is_admin {
            return Err(annora_core::AnnoraError::CapabilityDenied(
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
            return Err(annora_core::AnnoraError::CapabilityDenied(
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
            annora_core::AnnoraError::CapabilityDenied(format!("token serialization: {e}"))
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
    use annora_core::{ActorId, Timestamp, UserId};
    use ed25519_dalek::SigningKey;
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
        use annora_core::{
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
        use annora_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
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
                annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
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
        use annora_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
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
                annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
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
        use annora_core::{CapabilityAction, CapabilityAuthority, CapabilityScope};
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
                annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
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
}
