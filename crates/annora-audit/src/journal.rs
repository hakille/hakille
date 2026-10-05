use crate::entry::AuditEntry;
use annora_core::{EntryId, Result, Timestamp};
use redb::{Database, TableDefinition};
use std::sync::Mutex;
use sha2::{Sha256, Digest};
use thiserror::Error;

const ENTRIES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("entries");
const ENTRY_ORDER_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("entry_order");
const META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("metadata");

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainVerification {
    pub total_entries: u64,
    pub valid_entries: u64,
    pub broken_at: Option<EntryId>,
}

impl AuditJournal {
    pub fn new(db_path: &str, signing_key: ed25519_dalek::SigningKey) -> Result<Self> {
        let db = Database::create(db_path).map_err(JournalError::Database)?;
        let write_txn = db.begin_write().map_err(JournalError::Transaction)?;
        {
            write_txn.open_table(ENTRIES_TABLE).map_err(JournalError::Table)?;
            write_txn.open_table(ENTRY_ORDER_TABLE).map_err(JournalError::Table)?;
            write_txn.open_table(META_TABLE).map_err(JournalError::Table)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;

        let (seq, last_hash_bytes) = {
            let read_txn = db.begin_read().map_err(JournalError::Transaction)?;
            let meta_table = read_txn.open_table(META_TABLE).map_err(JournalError::Table)?;
            let seq = if let Some(v) = meta_table.get(b"seq".as_slice()).map_err(JournalError::Storage)? {
                let mut b = [0u8; 8];
                b.copy_from_slice(v.value());
                u64::from_be_bytes(b)
            } else {
                0
            };
            let hash = if let Some(v) = meta_table.get(b"last_hash".as_slice()).map_err(JournalError::Storage)? {
                v.value().to_vec()
            } else {
                vec![0u8; 32]
            };
            (seq, hash)
        };

        Ok(Self {
            db,
            signing_key,
            last_hash: Mutex::new(last_hash_bytes),
            seq: Mutex::new(seq),
        })
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
            let mut entries_table = write_txn.open_table(ENTRIES_TABLE).map_err(JournalError::Table)?;
            entries_table.insert(id_bytes.as_slice(), entry_bytes.as_slice()).map_err(JournalError::Storage)?;

            let mut order_table = write_txn.open_table(ENTRY_ORDER_TABLE).map_err(JournalError::Table)?;
            order_table.insert(cur_seq, id_bytes.as_slice()).map_err(JournalError::Storage)?;

            let mut meta_table = write_txn.open_table(META_TABLE).map_err(JournalError::Table)?;
            meta_table.insert(b"seq".as_slice(), seq_lock.to_be_bytes().as_slice()).map_err(JournalError::Storage)?;
            meta_table.insert(b"last_hash".as_slice(), new_hash.as_slice()).map_err(JournalError::Storage)?;
        }
        write_txn.commit().map_err(JournalError::Commit)?;

        Ok(id)
    }

    pub fn get(&self, id: &EntryId) -> Result<Option<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let table = read_txn.open_table(ENTRIES_TABLE).map_err(JournalError::Table)?;
        let id_bytes = id.0.as_bytes().to_vec();
        
        if let Some(entry_data) = table.get(id_bytes.as_slice()).map_err(JournalError::Storage)? {
            let entry = serde_json::from_slice(entry_data.value()).map_err(JournalError::Serialization)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    pub fn list_range(&self, from: Timestamp, to: Timestamp) -> Result<Vec<AuditEntry>> {
        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn.open_table(ENTRIES_TABLE).map_err(JournalError::Table)?;
        let order_table = read_txn.open_table(ENTRY_ORDER_TABLE).map_err(JournalError::Table)?;
        
        let seq = *self.seq.lock().unwrap();
        let mut results = Vec::new();
        for i in 0..seq {
            if let Some(id_data) = order_table.get(i).map_err(JournalError::Storage)? {
                if let Some(entry_data) = entries_table.get(id_data.value()).map_err(JournalError::Storage)? {
                    let entry: AuditEntry = serde_json::from_slice(entry_data.value()).map_err(JournalError::Serialization)?;
                    if entry.timestamp >= from && entry.timestamp <= to {
                        results.push(entry);
                    }
                }
            }
        }
        Ok(results)
    }

    pub fn verify_chain(&self) -> Result<ChainVerification> {
        use ed25519_dalek::{Verifier, Signature as DalekSignature};

        let read_txn = self.db.begin_read().map_err(JournalError::Transaction)?;
        let entries_table = read_txn.open_table(ENTRIES_TABLE).map_err(JournalError::Table)?;
        let order_table = read_txn.open_table(ENTRY_ORDER_TABLE).map_err(JournalError::Table)?;

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
            let entry_data = match entries_table.get(id_data.value()).map_err(JournalError::Storage)? {
                Some(data) => data,
                None => {
                    return Ok(ChainVerification {
                        total_entries: seq,
                        valid_entries: valid_count,
                        broken_at: None,
                    });
                }
            };
            let mut entry: AuditEntry = serde_json::from_slice(entry_data.value()).map_err(JournalError::Serialization)?;

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

        Ok(ChainVerification {
            total_entries: seq,
            valid_entries: valid_count,
            broken_at: None,
        })
    }

    pub fn revert(&self, id: &EntryId) -> Result<AuditEntry> {
        let entry = self.get(id)?
            .ok_or_else(|| annora_core::AnnoraError::Internal(format!("Entry {} not found", id.0)))?;

        if !entry.action.reversible {
            return Err(annora_core::AnnoraError::CapabilityDenied(
                format!("Action {} is marked non-reversible", id.0)
            ));
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
        };

        self.append(compensation_entry.clone())?;
        Ok(compensation_entry)
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
        }
    }

    #[test]
    fn test_audit_journal_append_verify_and_revert() {
        let db_path = std::env::temp_dir().join(format!("test_journal_{}.redb", uuid::Uuid::now_v7()));
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
        let db_path = std::env::temp_dir().join(format!("test_journal_tamper_{}.redb", uuid::Uuid::now_v7()));
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
            entries_table.insert(id2.0.as_bytes().as_slice(), tampered_bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();

        // Verification must now detect tampering and report broken_at == Some(id2)
        let ver_tampered = journal.verify_chain().unwrap();
        assert_eq!(ver_tampered.valid_entries, 1);
        assert_eq!(ver_tampered.broken_at, Some(id2));

        let _ = std::fs::remove_file(db_path);
    }
}

