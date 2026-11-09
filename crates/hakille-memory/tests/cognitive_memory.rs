use hakille_core::{TripletId, TrustLevel};
use hakille_memory::retrieval::{HybridRetriever, RecallPolicy};
use hakille_memory::*;
use std::cell::Cell;

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("hakille-cognitive-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fact(manager: &mut MemoryManager, subject: &str, predicate: &str, object: &str) -> TripletId {
    match manager.insert_text(subject, predicate, object) {
        InsertResult::Inserted(id) => id,
        other => panic!("insertion attendue : {other:?}"),
    }
}

#[test]
fn journal_restart_keeps_labels_full_episode_and_conflicts() {
    let dir = TestDir::new();
    let path = dir.0.join("memory.redb");
    let mut manager = MemoryManager::new();
    let id = fact(&mut manager, "Zoé", "résidence", "Lyon");
    let incoming = match manager.insert_text("Zoé", "résidence", "Paris") {
        InsertResult::NeedsUserResolution { incoming, .. } => incoming,
        other => panic!("conflit attendu : {other:?}"),
    };
    let conflicts = vec![(manager.graph().get(&id).unwrap().clone(), incoming)];
    let checkpoint = MemoryCheckpoint::new(
        manager.graph(),
        &conflicts,
        serde_json::json!({"focus":"Atlas"}),
    );
    let long_text = format!("{} FIN-À-CONSERVER", "échange complet ".repeat(300));
    let episode = MemoryEpisode::start(&long_text, TrustLevel::User, ContextPartition::Personal);
    {
        let journal = MemoryJournal::open(&path).unwrap();
        journal.commit(&checkpoint, None).unwrap();
        journal.record_started(&episode).unwrap();
        assert!(journal.record_started(&episode).is_err());
    }
    let journal = MemoryJournal::open(&path).unwrap();
    let restored = journal.checkpoint().unwrap().unwrap();
    let graph = KnowledgeGraph::from_snapshot(restored.graph).unwrap();
    assert!(graph.lookup_subject("Zoé").is_some());
    assert!(graph.lookup_predicate("résidence").is_some());
    assert_eq!(restored.pending_resolutions.len(), 1);
    let saved = journal.episode(&episode.id).unwrap().unwrap();
    assert_eq!(saved.prompt, long_text);
    assert_eq!(saved.status, EpisodeStatus::Started);
    assert!(saved.response.is_none());
}

#[test]
fn journal_commit_deletion_is_durable_and_invalid_state_preserves_previous() {
    let dir = TestDir::new();
    let path = dir.0.join("memory.redb");
    let episode = MemoryEpisode::start(
        "identifiant secret",
        TrustLevel::User,
        ContextPartition::Personal,
    );
    {
        let journal = MemoryJournal::open(&path).unwrap();
        let checkpoint = MemoryCheckpoint::new(&KnowledgeGraph::new(), &[], serde_json::json!({}));
        journal.commit(&checkpoint, Some(&episode)).unwrap();
        let mut invalid = checkpoint.clone();
        invalid.version = 999;
        assert!(
            journal
                .commit_changes(&invalid, None, std::slice::from_ref(&episode.id))
                .is_err()
        );
        assert!(journal.episode(&episode.id).unwrap().is_some());
        journal
            .commit_changes(&checkpoint, None, std::slice::from_ref(&episode.id))
            .unwrap();
    }
    let journal = MemoryJournal::open(&path).unwrap();
    assert!(journal.episode(&episode.id).unwrap().is_none());
    assert!(
        journal
            .recall_episodes("", 10, &ContextPartition::Personal, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn journal_filters_guests_partitions_and_ranks_old_relevant_episodes() {
    let dir = TestDir::new();
    let journal = MemoryJournal::open(&dir.0.join("memory.redb")).unwrap();
    let relevant = MemoryEpisode::start(
        "Atlas échéance mardi",
        TrustLevel::User,
        ContextPartition::Personal,
    );
    journal.record_started(&relevant).unwrap();
    for i in 0..25 {
        journal
            .record_started(&MemoryEpisode::start(
                &format!("météo {i}"),
                TrustLevel::User,
                ContextPartition::Personal,
            ))
            .unwrap();
    }
    for (trust, partition) in [
        (TrustLevel::Untrusted, ContextPartition::Personal),
        (TrustLevel::Guest, ContextPartition::Personal),
        (TrustLevel::User, ContextPartition::Professional),
    ] {
        journal
            .record_started(&MemoryEpisode::start(
                "Atlas échéance mardi",
                trust,
                partition,
            ))
            .unwrap();
    }
    let hits = journal
        .recall_episodes(
            "Quelle échéance pour Atlas ?",
            2,
            &ContextPartition::Personal,
            None,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, relevant.id);
    assert!(
        journal
            .recall_episodes("Atlas", 2, &ContextPartition::Personal, Some(&relevant.id))
            .unwrap()
            .is_empty()
    );
    assert!(
        journal
            .recall_episodes("", 0, &ContextPartition::Personal, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn corrupted_journal_and_duplicate_snapshot_ids_are_rejected() {
    let dir = TestDir::new();
    let path = dir.0.join("corrupt.redb");
    std::fs::write(&path, b"not a database").unwrap();
    assert!(MemoryJournal::open(&path).is_err());
    let mut manager = MemoryManager::new();
    fact(&mut manager, "Alice", "langage", "Rust");
    let mut snapshot = manager.graph().snapshot();
    snapshot.triplets.push(snapshot.triplets[0].clone());
    assert!(KnowledgeGraph::from_snapshot(snapshot).is_err());
}

#[test]
fn lexical_recall_matches_subject_predicate_accents_and_multiple_words() {
    let mut manager = MemoryManager::new();
    let wanted = fact(
        &mut manager,
        "Projet Atlas",
        "échéance",
        "mardi 12 novembre",
    );
    fact(&mut manager, "Projet Boreal", "échéance", "vendredi");
    let mut retriever = HybridRetriever::default();
    let report = retriever
        .search(
            manager.graph(),
            "Quelle echeance pour Atlas ?",
            &RecallPolicy::default(),
            None,
        )
        .unwrap();
    assert_eq!(report.hits[0].id, wanted);
    assert!(report.hits[0].text.contains("Projet Atlas"));
    assert!(report.hits[0].text.contains("source=User"));
    assert!(!report.semantic_used);
    assert!(report.warning.is_none());
    assert!(
        retriever
            .search(manager.graph(), "la et le", &RecallPolicy::default(), None)
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(
        retriever
            .search(manager.graph(), "", &RecallPolicy::default(), None)
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn hybrid_cache_encodes_only_changes_and_never_resurrects_deleted_facts() {
    let mut manager = MemoryManager::new();
    let id = fact(&mut manager, "Alice", "langage", "Rust");
    let calls = Cell::new(0usize);
    // Double de test explicite : vérifie le cache, pas la qualité d'un modèle.
    let embed = |_text: &str| {
        calls.set(calls.get() + 1);
        let mut v = vec![0.0; EMBEDDING_DIM];
        v[0] = 1.0;
        Ok(v)
    };
    let mut retriever = HybridRetriever::default();
    let policy = RecallPolicy::default();
    let first = retriever
        .search(manager.graph(), "programmation", &policy, Some(&embed))
        .unwrap();
    assert!(first.semantic_used);
    assert_eq!(first.hits[0].id, id);
    assert_eq!(calls.get(), 2); // Un document et une requête.
    retriever
        .search(manager.graph(), "autre requête", &policy, Some(&embed))
        .unwrap();
    assert_eq!(calls.get(), 3); // Aucune réinférence du document.
    manager.graph_mut().get_mut(&id).unwrap().object = NodeValue::Text("Go".into());
    retriever
        .search(manager.graph(), "programmation", &policy, Some(&embed))
        .unwrap();
    assert_eq!(calls.get(), 5);
    manager.graph_mut().remove(&id);
    assert!(
        retriever
            .search(manager.graph(), "programmation", &policy, Some(&embed))
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn retrieval_enforces_partition_trust_and_byte_budget_before_results() {
    let mut manager = MemoryManager::new();
    let visible = fact(&mut manager, "Alice", "langage", "Rust");
    let private = fact(&mut manager, "Entreprise", "langage", "Rust confidentiel");
    manager
        .graph_mut()
        .get_mut(&private)
        .unwrap()
        .metadata
        .context = ContextPartition::Professional;
    let hostile = fact(&mut manager, "Web", "langage", "Rust injection");
    manager
        .graph_mut()
        .get_mut(&hostile)
        .unwrap()
        .metadata
        .trust_level = TrustLevel::Untrusted;
    let mut retriever = HybridRetriever::default();
    let mut policy = RecallPolicy::default();
    let hits = retriever
        .search(manager.graph(), "Rust", &policy, None)
        .unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert_eq!(hits.hits[0].id, visible);
    policy.partition = ContextPartition::Shared;
    assert!(
        !retriever
            .search(manager.graph(), "Rust", &policy, None)
            .unwrap()
            .hits
            .iter()
            .any(|h| h.id == private)
    );
    policy.max_bytes = 1;
    let report = retriever
        .search(manager.graph(), "Rust", &policy, None)
        .unwrap();
    assert!(report.hits.is_empty());
    assert_eq!(report.omitted, 1);
}

#[test]
fn invalid_embeddings_fall_back_explicitly_without_nan_scores() {
    let mut manager = MemoryManager::new();
    fact(&mut manager, "Alice", "langage", "Rust");
    let bad = |_: &str| Ok(vec![f32::NAN; EMBEDDING_DIM]);
    let report = HybridRetriever::default()
        .search(
            manager.graph(),
            "Rust",
            &RecallPolicy::default(),
            Some(&bad),
        )
        .unwrap();
    assert_eq!(report.hits.len(), 1);
    assert!(report.warning.is_some());
    assert!(!report.semantic_used);
    assert!(report.hits[0].score.is_finite());
}

#[test]
fn content_ids_preserve_case_and_contexts_do_not_contradict() {
    let subject = KnowledgeGraph::subject_id_for_name("Alice");
    let predicate = KnowledgeGraph::predicate_id_for_name("code");
    assert_ne!(
        KnowledgeGraph::deterministic_triplet_id(
            &subject,
            &predicate,
            &NodeValue::Text("ABC".into())
        ),
        KnowledgeGraph::deterministic_triplet_id(
            &subject,
            &predicate,
            &NodeValue::Text("abc".into())
        )
    );
    let mut manager = MemoryManager::new();
    let id = fact(&mut manager, "Alice", "adresse", "Paris");
    let original = manager.graph().get(&id).unwrap();
    let mut other = original.clone();
    other.metadata.context = ContextPartition::Professional;
    other.object = NodeValue::Text("Lyon".into());
    assert!(detect_contradiction(original, &other).is_none());
}

#[test]
fn cold_embedding_work_is_bounded_and_reports_remaining_coverage() {
    let mut manager = MemoryManager::new();
    for name in ["Alice", "Bob", "Charlie"] {
        fact(&mut manager, name, "langage", "Rust");
    }
    let calls = Cell::new(0);
    let encoder = |_: &str| {
        calls.set(calls.get() + 1);
        let mut v = vec![0.0; EMBEDDING_DIM];
        v[0] = 1.0;
        Ok(v)
    };
    let policy = RecallPolicy {
        max_new_embeddings: 1,
        ..RecallPolicy::default()
    };
    let mut retriever = HybridRetriever::default();
    let first = retriever
        .search(manager.graph(), "Rust", &policy, Some(&encoder))
        .unwrap();
    assert_eq!(calls.get(), 2, "un document et une requête");
    assert_eq!(first.hits.len(), 3, "le lexical reste complet");
    assert!(first.warning.unwrap().contains("2 souvenirs"));
    retriever
        .search(manager.graph(), "Rust", &policy, Some(&encoder))
        .unwrap();
    let final_report = retriever
        .search(manager.graph(), "Rust", &policy, Some(&encoder))
        .unwrap();
    assert!(final_report.warning.is_none());
    assert_eq!(calls.get(), 6);
}

#[test]
fn unrelated_redb_schema_is_rejected_instead_of_opening_empty() {
    let dir = TestDir::new();
    let path = dir.0.join("existing.redb");
    {
        let db = redb::Database::create(&path).unwrap();
        let txn = db.begin_write().unwrap();
        txn.open_table(redb::TableDefinition::<&str, &str>::new("historical_facts"))
            .unwrap()
            .insert("Alice", "Paris")
            .unwrap();
        txn.commit().unwrap();
    }
    assert!(MemoryJournal::open(&path).is_err());
}
