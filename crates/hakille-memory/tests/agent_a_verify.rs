// Vérification temporaire Agent A (graphe RAM) — FICHIER ÉPHÉMÈRE, supprimé
// après exécution. Mirroir comportemental (API publique uniquement) des tests
// unitaires ajoutés dans graph.rs / lib.rs / triplet.rs, qui ne peuvent pas
// compiler dans le harness `lib test` tant que les modules d'autres agents
// (query_lang.rs, vector/mod.rs, cfg(test)) n'auront pas ajouté le champ
// `node_type` à leurs littéraux `Triplet`.
use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
use hakille_memory::{
    ClusterStore, ClusterVectorStore, EdgeStore, EdgeVectorStore, GraphStorage, KnowledgeGraph,
    MemoryManager, NodeType, NodeValue, TriplePattern, Triplet, TripletMetadata, TripletSource,
    VectorIndex,
};
use std::sync::Arc;

fn sample(
    subject: &NodeId,
    predicate: &PredicateId,
    object: NodeValue,
    weight: f64,
    confidence: f64,
) -> Triplet {
    Triplet {
        id: TripletId(uuid::Uuid::now_v7()),
        subject: subject.clone(),
        predicate: predicate.clone(),
        object,
        metadata: TripletMetadata {
            weight,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence,
            context: hakille_memory::ContextPartition::Default,
            trust_level: TrustLevel::User,
            decay_rate: 0.01,
        },
        node_type: Some(NodeType::Fact),
    }
}

fn temp_edge(name: &str) -> (EdgeStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("hakille-verify-{name}-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = redb::Database::create(dir.join("t.redb")).unwrap();
    (EdgeStore::new(Arc::new(db)), dir)
}

#[test]
fn verify_index_query_and_duplicate_idempotent() {
    let mut g = KnowledgeGraph::new();
    let s1 = NodeId(uuid::Uuid::now_v7());
    let p1 = PredicateId(uuid::Uuid::now_v7());
    let p2 = PredicateId(uuid::Uuid::now_v7());
    let s2 = NodeId(uuid::Uuid::now_v7());
    let t1 = sample(&s1, &p1, NodeValue::Text("x".into()), 0.5, 0.9);
    let id1 = t1.id.clone();
    assert!(matches!(g.insert(t1), hakille_memory::InsertResult::Inserted(id) if id == id1));
    g.insert(sample(&s1, &p2, NodeValue::Text("y".into()), 0.6, 0.9));
    g.insert(sample(&s2, &p1, NodeValue::Text("z".into()), 0.7, 0.9));
    assert_eq!(g.len(), 3);
    let pat = |s, p| TriplePattern {
        subject: s,
        predicate: p,
        object: None,
    };
    assert_eq!(g.query(&pat(Some(s1.clone()), None)).len(), 2);
    assert_eq!(g.query(&pat(None, Some(p1.clone()))).len(), 2);
    assert_eq!(g.query(&pat(Some(s1.clone()), Some(p1.clone()))).len(), 1);
    // Doublon exact, nouvel ID ⇒ idempotent.
    let dup = sample(&s1, &p1, NodeValue::Text("x".into()), 0.5, 0.9);
    assert!(matches!(g.insert(dup), hakille_memory::InsertResult::Inserted(id) if id == id1));
    assert_eq!(g.len(), 3);
    assert!(g.remove(&id1));
    assert_eq!(g.query(&pat(Some(s1.clone()), Some(p1.clone()))).len(), 0);
}

#[test]
fn verify_activate_and_traverse() {
    let mut g = KnowledgeGraph::new();
    let s = NodeId(uuid::Uuid::now_v7());
    let p = PredicateId(uuid::Uuid::now_v7());
    let mut t = sample(&s, &p, NodeValue::Text("souv".into()), 0.5, 0.9);
    t.metadata.last_activated = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
    let before = t.metadata.last_activated.clone();
    let id = t.id.clone();
    g.insert(t);
    g.activate(&id);
    let got = g.get(&id).unwrap();
    assert!((got.metadata.weight - 0.55).abs() < 1e-12);
    assert!(got.metadata.last_activated.0 >= before.0);

    // Traverse déterministe sans doublons (cycle A→B→A).
    let a = NodeId(uuid::Uuid::now_v7());
    let b = NodeId(uuid::Uuid::now_v7());
    let pp = PredicateId(uuid::Uuid::now_v7());
    let t1 = sample(&a, &pp, NodeValue::Entity(b.clone()), 0.9, 0.9);
    let t2 = sample(&b, &pp, NodeValue::Entity(a.clone()), 0.3, 0.9);
    g.insert(t1);
    g.insert(t2);
    let r1 = g.traverse_from(&a, 5);
    assert_eq!(r1.len(), 2);
    assert!(r1[0].metadata.weight >= r1[1].metadata.weight);
    let r2 = g.traverse_from(&a, 5);
    assert_eq!(
        r1.iter().map(|t| &t.id).collect::<Vec<_>>(),
        r2.iter().map(|t| &t.id).collect::<Vec<_>>()
    );
}

#[test]
fn verify_metadata_validation_and_node_type() {
    let meta = TripletMetadata::new(
        0.5,
        0.9,
        TripletSource::User,
        hakille_memory::ContextPartition::Default,
        TrustLevel::User,
        0.01,
    )
    .unwrap();
    assert!(meta.validate().is_ok());
    assert!(
        TripletMetadata::new(
            1.5,
            0.9,
            TripletSource::User,
            hakille_memory::ContextPartition::Default,
            TrustLevel::User,
            0.01
        )
        .is_err()
    );
    let t = Triplet::new(
        TripletId(uuid::Uuid::now_v7()),
        NodeId(uuid::Uuid::now_v7()),
        PredicateId(uuid::Uuid::now_v7()),
        NodeValue::Text("f".into()),
        meta,
    )
    .unwrap();
    assert_eq!(t.node_type_or_default(), NodeType::Fact);
    let r = hakille_memory::InsertResult::Inserted(t.id.clone());
    assert_eq!(r, hakille_memory::InsertResult::Inserted(t.id.clone()));
    // Snapshot public.
    let snap = hakille_memory::GraphSnapshot {
        triplets: vec![],
        subject_names: vec![],
        predicate_names: vec![],
        subject_labels: vec![],
        predicate_labels: vec![],
    };
    assert!(snap.triplets.is_empty());
}

#[test]
fn verify_reexports_present() {
    // Les ré-exports storage/vector existent et s'utilisent.
    let _c = ClusterStore::new();
    let mut v = EdgeVectorStore::new();
    assert!(v.is_empty());
    let _cv = ClusterVectorStore::new();
    let id = TripletId(uuid::Uuid::now_v7());
    v.insert(id.clone(), vec![1.0, 0.0]).unwrap();
    let hits: Vec<(TripletId, f32)> = VectorIndex::search(&v, &[1.0, 0.0], 1).unwrap();
    assert_eq!(hits[0].0, id);
    let _brute: Option<hakille_memory::BruteForceVectorStore> = None;
}

#[tokio::test]
async fn verify_hydrate_registries_and_persist_gc() {
    let (edge, dir) = temp_edge("hg");
    let mut src = KnowledgeGraph::new();
    let alice = src.resolve_subject("Alice");
    let aime = src.resolve_predicate("aime");
    src.insert(sample(
        &alice,
        &aime,
        NodeValue::Text("café".into()),
        0.8,
        0.9,
    ));
    src.persist_to(&edge).await.unwrap();

    let mut seeded = KnowledgeGraph::new();
    seeded.resolve_subject("Alice");
    seeded.hydrate_from(&edge).await.unwrap();
    assert_eq!(seeded.len(), 1);
    assert_eq!(seeded.subject_label(&alice), Some("Alice"));
    assert_eq!(seeded.lookup_subject("alice"), Some(alice.clone()));

    let mut fresh = KnowledgeGraph::new();
    fresh.hydrate_from(&edge).await.unwrap();
    assert_eq!(
        fresh.subject_label(&alice),
        Some(alice.0.to_string().as_str())
    );

    // GC : fantôme supprimé de redb.
    let mut g2 = KnowledgeGraph::new();
    let (x, px) = (
        NodeId(uuid::Uuid::now_v7()),
        PredicateId(uuid::Uuid::now_v7()),
    );
    let (y, py) = (
        NodeId(uuid::Uuid::now_v7()),
        PredicateId(uuid::Uuid::now_v7()),
    );
    let keep = sample(&x, &px, NodeValue::Text("gardé".into()), 0.8, 0.9);
    let ghost = sample(&y, &py, NodeValue::Text("fant".into()), 0.8, 0.9);
    let gid = ghost.id.clone();
    g2.insert(keep);
    g2.insert(ghost);
    g2.persist_to(&edge).await.unwrap();
    g2.remove(&gid);
    g2.persist_to(&edge).await.unwrap();
    assert_eq!(edge.all_triplets().await.unwrap().len(), 3); // 1 ancien + 2 (fantôme survivant)
    g2.persist_to_with_gc(&edge).await.unwrap();
    let rest = edge.all_triplets().await.unwrap();
    assert!(rest.iter().all(|t| t.id != gid));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn verify_memory_manager_facade() {
    let dir = std::env::temp_dir().join(format!("hakille-verify-mm-{}", uuid::Uuid::now_v7()));
    let path = dir.join("m.redb");
    let mut m = MemoryManager::open(&path).await.unwrap();
    assert!(m.is_persistent());
    let id = match m.insert_text("Alice", "aime", "le café") {
        hakille_memory::InsertResult::Inserted(id) => id,
        other => panic!("insert attendu: {:?}", other),
    };
    match m.insert_text("Alice", "aime", "le café") {
        hakille_memory::InsertResult::Inserted(dup) => assert_eq!(dup, id),
        other => panic!("idempotent attendu: {:?}", other),
    }
    assert_eq!(m.recall_text("café").len(), 1);
    assert_eq!(m.vector_store().len(), 0);
    m.apply_decay(0.01);
    m.persist().await.unwrap();
    m.flush().unwrap();
    m.close().await.unwrap();
    let m2 = MemoryManager::open(&path).await.unwrap();
    assert_eq!(m2.graph().len(), 1);
    let ram = MemoryManager::new();
    assert!(ram.persist().await.is_err());
    std::fs::remove_dir_all(&dir).ok();
}
