use crate::{AuditEntry, AuditJournal, ReasoningTrace};
use hakille_core::{ActorId, EntryId, Result, TripletId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub entry: AuditEntry,
    pub reasoning: ReasoningTrace,
    pub graph_context: Vec<TripletId>,
    pub human_readable: String,
}

pub fn explain(journal: &AuditJournal, entry_id: &EntryId) -> Result<Explanation> {
    let entry = journal.get(entry_id)?.ok_or_else(|| {
        hakille_core::HakilleError::Internal(format!("Entry {:?} not found", entry_id))
    })?;

    Ok(Explanation {
        reasoning: entry.reasoning_trace.clone(),
        graph_context: entry.graph_nodes_used.clone(),
        human_readable: format!("Action: {}", entry.action.description),
        entry,
    })
}

/// Recherche toutes les entrées d'un acteur (index secondaire mémoire,
/// reconstruit au load ; repli scan si index vide mais journal non vide).
pub fn search_by_actor(journal: &AuditJournal, actor: &ActorId) -> Result<Vec<AuditEntry>> {
    let indexed = journal.entries_by_actor(actor)?;
    if !indexed.is_empty() {
        return Ok(indexed);
    }
    // Repli : scan ordonné (couvre le cas d'un journal dont l'index n'a pas
    // encore été reconstruit, ou acteur réellement absent).
    let all = journal.all_entries()?;
    let filtered: Vec<AuditEntry> = all.into_iter().filter(|e| &e.actor == actor).collect();
    Ok(filtered)
}

/// Recherche toutes les entrées d'un type d'action (index + repli scan).
pub fn search_by_action_type(journal: &AuditJournal, action_type: &str) -> Result<Vec<AuditEntry>> {
    let indexed = journal.entries_by_action(action_type)?;
    if !indexed.is_empty() {
        return Ok(indexed);
    }
    Ok(journal
        .all_entries()?
        .into_iter()
        .filter(|e| e.action.action_type == action_type)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{ActionRecord, ReasoningTrace};
    use hakille_core::{Timestamp, UserId};

    fn make_entry(actor: ActorId, action_type: &str) -> AuditEntry {
        AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor,
            action: ActionRecord {
                action_type: action_type.into(),
                description: format!("did {action_type}"),
                parameters: serde_json::json!({}),
                reversible: false,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 1,
            },
            graph_nodes_used: vec![],
            confidence: 1.0,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        }
    }

    #[test]
    fn test_search_by_actor_and_action_type() {
        let db_path =
            std::env::temp_dir().join(format!("test_journal_query_{}.redb", uuid::Uuid::now_v7()));
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[11u8; 32]);
        let journal =
            AuditJournal::new(db_path.to_str().unwrap(), signing_key).expect("journal creation");

        let alice = ActorId::User(UserId(uuid::Uuid::now_v7()));
        let bob = ActorId::Agent("mail-agent".to_string());
        journal
            .append(make_entry(alice.clone(), "send_email"))
            .unwrap();
        journal
            .append(make_entry(bob.clone(), "send_email"))
            .unwrap();
        journal
            .append(make_entry(alice.clone(), "read_file"))
            .unwrap();

        let alice_entries = search_by_actor(&journal, &alice).unwrap();
        assert_eq!(alice_entries.len(), 2);
        assert!(alice_entries.iter().all(|e| e.actor == alice));

        let bob_entries = search_by_actor(&journal, &bob).unwrap();
        assert_eq!(bob_entries.len(), 1);

        let unknown = search_by_actor(&journal, &ActorId::Kernel).unwrap();
        assert!(unknown.is_empty());

        let emails = search_by_action_type(&journal, "send_email").unwrap();
        assert_eq!(emails.len(), 2);
        let reads = search_by_action_type(&journal, "read_file").unwrap();
        assert_eq!(reads.len(), 1);
        let missing = search_by_action_type(&journal, "nope").unwrap();
        assert!(missing.is_empty());

        let _ = std::fs::remove_file(db_path);
    }
}
