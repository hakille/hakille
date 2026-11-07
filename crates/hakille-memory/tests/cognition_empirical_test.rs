//! Tests empiriques et adversariaux pour `hakille-memory` (Jalon M1).
//!
//! Vérifie :
//! 1. La demi-vie exacte de 5 minutes (COGNITIVE_DECAY_RATE = 0.00231 s⁻¹).
//! 2. L'absence absolue de tombstone sous le cutoff (preserve_history = false / purge physique).
//! 3. La résilience stricte aux confiances invalides (< 0.0, > 1.0, NaN, ±Infinity) et poids invalides.
//! 4. La persistance redb sans fantômes via persist_with_gc.

use chrono::{Duration, Utc};
use hakille_core::{Timestamp, TrustLevel};
use hakille_memory::GraphStorage;
use hakille_memory::cognition::{
    ALL_COGNITIVE_PREDICATES, COGNITIVE_DECAY_RATE, DEFAULT_COGNITIVE_CUTOFF_WEIGHT,
    DEFAULT_COGNITIVE_TRUST, PREDICATE_COGNITION_ATTENTION_LEVEL, PREDICATE_COGNITION_EMOTION,
    PREDICATE_COGNITION_ESTIMATED_AGE, PREDICATE_COGNITION_FATIGUE_INDEX,
    PREDICATE_COGNITION_POSTURE, SUBJECT_ACTIVE_USER, active_user_node_id, build_cognitive_triplet,
    build_cognitive_triplet_with_weight, cognitive_predicate_id, create_attention_level_triplet,
    create_emotion_triplet, create_estimated_age_triplet, create_fatigue_index_triplet,
    create_posture_triplet, is_cognitive_predicate, prune_cognitive_signals,
};
use hakille_memory::context_partition::ContextPartition;
use hakille_memory::graph::KnowledgeGraph;
use hakille_memory::storage::edge::EdgeStore;
use hakille_memory::triplet::{NodeType, NodeValue, TripletSource};
use std::sync::Arc;

// ============================================================================
// 1. Tests Empiriques de Demi-Vie (5 minutes = 300 s)
// ============================================================================

#[test]
fn test_half_life_exact_mathematical_decay() {
    let lambda = COGNITIVE_DECAY_RATE; // 0.00231
    assert_eq!(lambda, 0.00231);

    // Demi-vie théorique : T_1/2 = ln(2) / λ
    let theoretical_half_life = 2.0_f64.ln() / lambda;
    // 0.69314718 / 0.00231 ≈ 300.0637 secondes ≈ 5 minutes et 0.06 secondes !
    assert!(
        (theoretical_half_life - 300.0).abs() < 0.1,
        "La demi-vie théorique doit être de 300 s ± 0.1 s : got {theoretical_half_life}"
    );

    // À t = 300 s (5 min) : W(300) = W0 * e^(-0.00231 * 300) = W0 * 0.500073...
    let decay_factor_1 = (-lambda * 300.0).exp();
    assert!(
        (decay_factor_1 - 0.5).abs() < 0.001,
        "Le facteur après 5 min doit être 0.500 ± 0.001 : got {decay_factor_1}"
    );

    // À t = 600 s (10 min, 2 demi-vies) : W(600) = W0 * 0.25007...
    let decay_factor_2 = (-lambda * 600.0).exp();
    assert!(
        (decay_factor_2 - 0.25).abs() < 0.001,
        "Le facteur après 10 min doit être 0.250 ± 0.001 : got {decay_factor_2}"
    );

    // À t = 1200 s (20 min, 4 demi-vies) : W(1200) = W0 * 0.06253...
    let decay_factor_4 = (-lambda * 1200.0).exp();
    assert!(
        (decay_factor_4 - 0.0625).abs() < 0.001,
        "Le facteur après 20 min doit être 0.0625 ± 0.001 : got {decay_factor_4}"
    );

    // À t = 1300 s (21.6 min) : W(1300) = e^(-0.00231 * 1300) = e^(-3.003) ≈ 0.0496 < 0.05 (cutoff)
    let decay_factor_cutoff = (-lambda * 1300.0).exp();
    assert!(
        decay_factor_cutoff < DEFAULT_COGNITIVE_CUTOFF_WEIGHT,
        "Après ~21.6 min le signal doit passer sous le cutoff 0.05 : got {decay_factor_cutoff}"
    );
}

#[test]
fn test_empirical_decay_progression_in_graph() {
    let mut graph = KnowledgeGraph::new();

    // Création de 5 signaux cognitifs distincts
    let emo = create_emotion_triplet("focused", 0.9).unwrap();
    let age = create_estimated_age_triplet(28.0, 0.8).unwrap();
    let post = create_posture_triplet("Upright", 0.85).unwrap();
    let att = create_attention_level_triplet("high", 0.95).unwrap();
    let fat = create_fatigue_index_triplet(0.05, 0.9).unwrap();

    let id_emo = emo.id.clone();
    let id_age = age.id.clone();
    let id_post = post.id.clone();
    let id_att = att.id.clone();
    let id_fat = fat.id.clone();

    graph.insert(emo);
    graph.insert(age);
    graph.insert(post);
    graph.insert(att);
    graph.insert(fat);

    assert_eq!(graph.len(), 5);

    // Simuler un recul temporel de 300 secondes (5 minutes) pour id_emo
    let now = Utc::now();
    let past_5m = now - Duration::seconds(300);
    graph.get_mut(&id_emo).unwrap().metadata.last_activated = Timestamp(past_5m);

    // Simuler un recul temporel de 600 secondes (10 minutes) pour id_age
    let past_10m = now - Duration::seconds(600);
    graph.get_mut(&id_age).unwrap().metadata.last_activated = Timestamp(past_10m);

    // id_post, id_att, id_fat restent à now (0 s écoulées)

    // Exécution de la décroissance cognitive
    let report = prune_cognitive_signals(&mut graph, DEFAULT_COGNITIVE_CUTOFF_WEIGHT);

    // 2 nœuds ont décru, 0 supprimés (leurs poids restent > 0.05)
    assert_eq!(report.nodes_decayed, 2);
    assert_eq!(report.nodes_removed, 0);
    assert_eq!(report.nodes_faded, 0);

    // Vérifier les poids résiduels empiriques
    let w_emo = graph.get(&id_emo).unwrap().metadata.weight;
    let w_age = graph.get(&id_age).unwrap().metadata.weight;

    assert!(
        (w_emo - 0.50007).abs() < 0.005,
        "Poids après 5 min doit valoir ~0.50 : got {w_emo}"
    );
    assert!(
        (w_age - 0.25007).abs() < 0.005,
        "Poids après 10 min doit valoir ~0.25 : got {w_age}"
    );

    // Vérifier que les triplets non vieillis ont conservé leur poids initial de 1.0
    assert_eq!(graph.get(&id_post).unwrap().metadata.weight, 1.0);
    assert_eq!(graph.get(&id_att).unwrap().metadata.weight, 1.0);
    assert_eq!(graph.get(&id_fat).unwrap().metadata.weight, 1.0);
}

// ============================================================================
// 2. Tests Adversariaux : Purge Physique Sans Tombstone (preserve_history = false)
// ============================================================================

#[test]
fn test_purge_physical_zero_tombstone_guarantee() {
    let mut graph = KnowledgeGraph::new();

    // Insérer un signal cognitif
    let emo = create_emotion_triplet("neutral", 0.9).unwrap();
    let id_emo = emo.id.clone();
    graph.insert(emo);

    // Insérer un fait permanent non-cognitif (Partition Default, source User)
    let non_cog = hakille_memory::triplet::Triplet {
        id: hakille_core::TripletId(uuid::Uuid::now_v7()),
        subject: hakille_core::NodeId(uuid::Uuid::now_v7()),
        predicate: hakille_core::PredicateId(uuid::Uuid::now_v7()),
        object: NodeValue::Text("Fait permanent".into()),
        metadata: hakille_memory::triplet::TripletMetadata {
            weight: 0.02, // Poids très faible < 0.05, mais NON cognitif
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence: 0.9,
            context: ContextPartition::Default,
            trust_level: TrustLevel::User,
            decay_rate: 0.0,
        },
        node_type: Some(NodeType::Fact),
    };
    let id_non_cog = non_cog.id.clone();
    graph.insert(non_cog);

    assert_eq!(graph.len(), 2);

    // Vieillir le signal cognitif de 1500 secondes (~25 minutes)
    let past_25m = Utc::now() - Duration::seconds(1500);
    graph.get_mut(&id_emo).unwrap().metadata.last_activated = Timestamp(past_25m);

    // Purge cognitive avec cutoff 0.05
    let report = prune_cognitive_signals(&mut graph, 0.05);

    // Assertions strictes d'anti-surveillance et de purge physique :
    assert_eq!(
        report.nodes_removed, 1,
        "Le nœud cognitif doit être supprimé"
    );
    assert_eq!(
        report.nodes_faded, 0,
        "STRICTEMENT AUCUN TOMBSTONE ne doit être conservé !"
    );

    // Vérifier l'absence absolue dans le graphe RAM
    assert!(
        !graph.contains(&id_emo),
        "graph.contains(id_emo) doit être faux"
    );
    assert!(
        graph.get(&id_emo).is_none(),
        "graph.get(id_emo) doit être None"
    );

    // Vérifier qu'aucun résidu n'existe dans la table d'indexation
    let active_user = active_user_node_id();
    let remaining_for_user = graph.related(&active_user);
    assert!(
        remaining_for_user.is_empty(),
        "Aucun triplet pour active_user ne doit subsister dans l'index"
    );

    // Vérifier que le triplet non-cognitif n'a PAS été touché
    assert!(graph.contains(&id_non_cog));
    assert_eq!(graph.len(), 1);
}

#[tokio::test]
async fn test_redb_persistence_with_gc_purges_physically_on_disk() {
    let temp_dir = std::env::temp_dir().join(format!("hakille-gc-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("cognitive_gc.redb");

    let db = Arc::new(redb::Database::create(&db_path).unwrap());
    let edge = EdgeStore::new(db);

    let mut graph = KnowledgeGraph::new();

    // 1. Ajouter un signal cognitif et persister
    let emo = create_emotion_triplet("fatigued", 0.95).unwrap();
    let id_emo = emo.id.clone();
    graph.insert(emo);

    graph.persist_to(&edge).await.unwrap();
    let stored = edge.all_triplets().await.unwrap();
    assert!(stored.iter().any(|t| t.id == id_emo));

    // 2. Décroissance et purge en RAM
    let past = Utc::now() - Duration::seconds(1500);
    graph.get_mut(&id_emo).unwrap().metadata.last_activated = Timestamp(past);
    let report = prune_cognitive_signals(&mut graph, 0.05);
    assert_eq!(report.nodes_removed, 1);
    assert!(!graph.contains(&id_emo));

    // 3. Persist avec GC sur le disque redb
    graph.persist_to_with_gc(&edge).await.unwrap();

    // 4. Vérifier que redb ne contient plus le triplet (purge physique disque confirmée !)
    let stored_after_gc = edge.all_triplets().await.unwrap();
    assert!(
        !stored_after_gc.iter().any(|t| t.id == id_emo),
        "Le triplet cognitif purgé en RAM doit être physiquement effacé de redb par persist_to_with_gc !"
    );

    // Nettoyage
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// 3. Tests Adversariaux : Résilience aux Confiances et Poids Invalides
// ============================================================================

#[test]
fn test_adversarial_invalid_confidence_values() {
    let invalid_confidences = [
        -0.000001,
        -0.5,
        -1.0,
        -100.0,
        -f64::INFINITY,
        -f64::MIN_POSITIVE,
        1.000001,
        1.05,
        2.0,
        100.0,
        f64::INFINITY,
        f64::NAN,
    ];

    for &conf in &invalid_confidences {
        let res = build_cognitive_triplet(
            PREDICATE_COGNITION_EMOTION,
            NodeValue::Text("test".into()),
            conf,
        );
        assert!(
            res.is_err(),
            "build_cognitive_triplet doit rejeter la confiance invalide {conf}"
        );

        let res_emo = create_emotion_triplet("test", conf);
        assert!(
            res_emo.is_err(),
            "create_emotion_triplet doit rejeter {conf}"
        );

        let res_age = create_estimated_age_triplet(30.0, conf);
        assert!(
            res_age.is_err(),
            "create_estimated_age_triplet doit rejeter {conf}"
        );

        let res_post = create_posture_triplet("Upright", conf);
        assert!(
            res_post.is_err(),
            "create_posture_triplet doit rejeter {conf}"
        );

        let res_att = create_attention_level_triplet("high", conf);
        assert!(
            res_att.is_err(),
            "create_attention_level_triplet doit rejeter {conf}"
        );

        let res_fat = create_fatigue_index_triplet(0.1, conf);
        assert!(
            res_fat.is_err(),
            "create_fatigue_index_triplet doit rejeter {conf}"
        );
    }

    // Valeurs limites valides [0.0, 1.0]
    let valid_confidences = [0.0, 0.000001, 0.5, 0.999999, 1.0];
    for &conf in &valid_confidences {
        let res = create_emotion_triplet("neutral", conf);
        assert!(
            res.is_ok(),
            "create_emotion_triplet doit accepter la confiance valide {conf}"
        );
        assert_eq!(res.unwrap().metadata.confidence, conf);
    }
}

#[test]
fn test_adversarial_invalid_weight_values() {
    let invalid_weights = [
        -0.000001,
        -1.0,
        -f64::INFINITY,
        1.000001,
        2.0,
        f64::INFINITY,
        f64::NAN,
    ];

    for &w in &invalid_weights {
        let res = build_cognitive_triplet_with_weight(
            PREDICATE_COGNITION_EMOTION,
            NodeValue::Text("test".into()),
            0.8,
            w,
        );
        assert!(
            res.is_err(),
            "build_cognitive_triplet_with_weight doit rejeter le poids invalide {w}"
        );
    }

    // Poids valides [0.0, 1.0]
    let valid_weights = [0.0, 0.05, 0.5, 1.0];
    for &w in &valid_weights {
        let res = build_cognitive_triplet_with_weight(
            PREDICATE_COGNITION_EMOTION,
            NodeValue::Text("test".into()),
            0.8,
            w,
        );
        assert!(res.is_ok(), "Poids valide {w} doit être accepté");
        assert_eq!(res.unwrap().metadata.weight, w);
    }
}

// ============================================================================
// 4. Tests de Conformité du Schéma Cognitif
// ============================================================================

#[test]
fn test_cognitive_schema_constants_and_types() {
    assert_eq!(SUBJECT_ACTIVE_USER, "user:active");
    assert_eq!(ALL_COGNITIVE_PREDICATES.len(), 5);
    assert_eq!(PREDICATE_COGNITION_EMOTION, "cognition:emotion");
    assert_eq!(PREDICATE_COGNITION_ESTIMATED_AGE, "cognition:estimated_age");
    assert_eq!(PREDICATE_COGNITION_POSTURE, "cognition:posture");
    assert_eq!(
        PREDICATE_COGNITION_ATTENTION_LEVEL,
        "cognition:attention_level"
    );
    assert_eq!(PREDICATE_COGNITION_FATIGUE_INDEX, "cognition:fatigue_index");

    for pred in ALL_COGNITIVE_PREDICATES {
        assert!(is_cognitive_predicate(pred));
        let id1 = cognitive_predicate_id(pred);
        let id2 = cognitive_predicate_id(pred);
        assert_eq!(id1, id2, "PredicateId doit être déterministe");
    }

    assert_eq!(DEFAULT_COGNITIVE_TRUST, TrustLevel::Guest);
}
