//! Module Cognition : Schéma sémantique RDF et cycle de vie des signaux cognitifs.
//!
//! Ce module formalise les prédicats RDF standardisés pour les inférences cognitives
//! et biométriques de l'utilisateur actif ("user:active"), calibrés avec une demi-vie
//! de 5 minutes (décroissance exponentielle λ = 0.00231 s⁻¹) et une politique de purge
//! physique sans tombstone pour respecter la souveraineté et l'anti-surveillance.

use crate::context_partition::ContextPartition;
use crate::decay::{DecayEngine, DecayReport};
use crate::graph::KnowledgeGraph;
use crate::triplet::{NodeType, NodeValue, Triplet, TripletMetadata, TripletSource};
use hakille_core::{HakilleError, NodeId, PredicateId, Result, Timestamp, TrustLevel};

// -----------------------------------------------------------------------------
// Constantes RDF Standardisées
// -----------------------------------------------------------------------------

/// Identifiant nominal du sujet utilisateur actif dans la session courante.
pub const SUBJECT_ACTIVE_USER: &str = "user:active";

/// Prédicat RDF : État émotionnel consolidé (ex. "neutral", "focused", "fatigued").
pub const PREDICATE_COGNITION_EMOTION: &str = "cognition:emotion";

/// Prédicat RDF : Âge estimé de l'utilisateur (valeur numérique f64).
pub const PREDICATE_COGNITION_ESTIMATED_AGE: &str = "cognition:estimated_age";

/// Prédicat RDF : Posture corporelle 3D (ex. "Upright", "Slouched", "Standing").
pub const PREDICATE_COGNITION_POSTURE: &str = "cognition:posture";

/// Prédicat RDF : Niveau d'attention cognitif (ex. "high", "medium", "low").
pub const PREDICATE_COGNITION_ATTENTION_LEVEL: &str = "cognition:attention_level";

/// Prédicat RDF : Indice de fatigue cumulée (PERCLOS, valeur entre 0.0 et 1.0).
pub const PREDICATE_COGNITION_FATIGUE_INDEX: &str = "cognition:fatigue_index";

/// Liste de l'ensemble des prédicats cognitifs standardisés.
pub const ALL_COGNITIVE_PREDICATES: [&str; 5] = [
    PREDICATE_COGNITION_EMOTION,
    PREDICATE_COGNITION_ESTIMATED_AGE,
    PREDICATE_COGNITION_POSTURE,
    PREDICATE_COGNITION_ATTENTION_LEVEL,
    PREDICATE_COGNITION_FATIGUE_INDEX,
];

/// Taux de décroissance calibré pour les signaux cognitifs (λ = 0.00231 s⁻¹).
/// Demi-vie T_1/2 = ln(2) / λ ≈ 300.06 secondes (exactement 5 minutes).
pub const COGNITIVE_DECAY_RATE: f64 = 0.00231;

/// Seuil de coupure par défaut au-dessous duquel le signal est physiquement purgé.
pub const DEFAULT_COGNITIVE_CUTOFF_WEIGHT: f64 = 0.05;

// -----------------------------------------------------------------------------
// Résolution d'Identifiants Déterministes
// -----------------------------------------------------------------------------

/// Retourne le `NodeId` déterministe pour l'utilisateur actif (`"user:active"`).
pub fn active_user_node_id() -> NodeId {
    KnowledgeGraph::subject_id_for_name(SUBJECT_ACTIVE_USER)
}

/// Retourne le `PredicateId` déterministe pour un libellé de prédicat donné.
pub fn cognitive_predicate_id(predicate: &str) -> PredicateId {
    KnowledgeGraph::predicate_id_for_name(predicate)
}

/// Indique si un libellé de prédicat correspond à un prédicat cognitif standard.
pub fn is_cognitive_predicate(predicate: &str) -> bool {
    ALL_COGNITIVE_PREDICATES.contains(&predicate) || predicate.starts_with("cognition:")
}

// -----------------------------------------------------------------------------
// Helpers de Création de Triplets Cognitifs
// -----------------------------------------------------------------------------

/// Niveau de confiance par défaut pour les signaux cognitifs inférés.
pub const DEFAULT_COGNITIVE_TRUST: TrustLevel = TrustLevel::Guest;

/// Constructeur générique de triplet cognitif pour `user:active`.
///
/// Attributs imposés par la politique de souveraineté et d'anti-surveillance :
/// - Sujet : `active_user_node_id()` ("user:active")
/// - Partition de contexte : `ContextPartition::Personal`
/// - Source : `TripletSource::Inferred`
/// - Niveau de confiance : `TrustLevel::Guest`
/// - Taux de décroissance : `COGNITIVE_DECAY_RATE` (0.00231 s⁻¹)
/// - Nature de nœud : `NodeType::Fact`
pub fn build_cognitive_triplet(
    predicate: &str,
    object: NodeValue,
    confidence: f64,
) -> Result<Triplet> {
    build_cognitive_triplet_with_weight(predicate, object, confidence, 1.0)
}

/// Constructeur générique de triplet cognitif avec poids initial spécifique.
pub fn build_cognitive_triplet_with_weight(
    predicate: &str,
    object: NodeValue,
    confidence: f64,
    weight: f64,
) -> Result<Triplet> {
    if !(0.0..=1.0).contains(&confidence) {
        return Err(HakilleError::Internal(format!(
            "build_cognitive_triplet : confidence hors [0, 1] : {confidence}"
        )));
    }
    if !(0.0..=1.0).contains(&weight) {
        return Err(HakilleError::Internal(format!(
            "build_cognitive_triplet : weight hors [0, 1] : {weight}"
        )));
    }

    let subject = active_user_node_id();
    let predicate_id = cognitive_predicate_id(predicate);
    let triplet_id = KnowledgeGraph::deterministic_triplet_id(&subject, &predicate_id, &object);

    let metadata = TripletMetadata {
        weight,
        created_at: Timestamp::now(),
        last_activated: Timestamp::now(),
        source: TripletSource::Inferred,
        confidence,
        context: ContextPartition::Personal,
        trust_level: DEFAULT_COGNITIVE_TRUST,
        decay_rate: COGNITIVE_DECAY_RATE,
    };
    metadata.validate()?;

    let triplet = Triplet {
        id: triplet_id,
        subject,
        predicate: predicate_id,
        object,
        metadata,
        node_type: Some(NodeType::Fact),
    };

    Ok(triplet)
}

/// Helper spécifique pour l'état émotionnel (`cognition:emotion`).
pub fn create_emotion_triplet(emotion: &str, confidence: f64) -> Result<Triplet> {
    build_cognitive_triplet(
        PREDICATE_COGNITION_EMOTION,
        NodeValue::Text(emotion.to_string()),
        confidence,
    )
}

/// Helper spécifique pour l'âge estimé (`cognition:estimated_age`).
pub fn create_estimated_age_triplet(age: f64, confidence: f64) -> Result<Triplet> {
    build_cognitive_triplet(
        PREDICATE_COGNITION_ESTIMATED_AGE,
        NodeValue::Number(age),
        confidence,
    )
}

/// Helper spécifique pour la posture physique (`cognition:posture`).
pub fn create_posture_triplet(posture: &str, confidence: f64) -> Result<Triplet> {
    build_cognitive_triplet(
        PREDICATE_COGNITION_POSTURE,
        NodeValue::Text(posture.to_string()),
        confidence,
    )
}

/// Helper spécifique pour le niveau d'attention (`cognition:attention_level`).
pub fn create_attention_level_triplet(level: &str, confidence: f64) -> Result<Triplet> {
    build_cognitive_triplet(
        PREDICATE_COGNITION_ATTENTION_LEVEL,
        NodeValue::Text(level.to_string()),
        confidence,
    )
}

/// Helper spécifique pour l'indice de fatigue (`cognition:fatigue_index`).
pub fn create_fatigue_index_triplet(fatigue_index: f64, confidence: f64) -> Result<Triplet> {
    build_cognitive_triplet(
        PREDICATE_COGNITION_FATIGUE_INDEX,
        NodeValue::Number(fatigue_index),
        confidence,
    )
}

// -----------------------------------------------------------------------------
// Politique de Purge Physique Anti-Surveillance
// -----------------------------------------------------------------------------

/// Applique la décroissance exponentielle sélective aux signaux cognitifs du graphe
/// et purge physiquement (`graph.remove`) tout signal passant sous `cutoff_weight`.
///
/// Aucun tombstone logique n'est créé pour les triplets cognitifs (`preserve_history = false`).
pub fn prune_cognitive_signals(graph: &mut KnowledgeGraph, cutoff_weight: f64) -> DecayReport {
    let now = chrono::Utc::now();
    let cutoff = cutoff_weight.max(0.0);
    let mut decayed_count: u64 = 0;
    let mut to_remove = Vec::new();

    for id in graph.ids() {
        if let Some(t) = graph.get_mut(&id) {
            // Cible sélectivement les inférences personnelles cognitives
            let is_cognitive = t.metadata.context == ContextPartition::Personal
                && (t.metadata.source == TripletSource::Inferred
                    || t.metadata.decay_rate == COGNITIVE_DECAY_RATE);

            if !is_cognitive || DecayEngine::should_exclude(t) {
                continue;
            }

            let dt_secs = (now - t.metadata.last_activated.0)
                .num_milliseconds()
                .max(0) as f64
                / 1000.0;

            let lambda = t.metadata.decay_rate.max(0.0);
            let before = t.metadata.weight;
            let after = before * (-lambda * dt_secs).exp();

            t.metadata.weight = after;
            t.metadata.last_activated = Timestamp(now);

            if (before - after).abs() > 1e-12 {
                decayed_count += 1;
            }
            if after < cutoff {
                to_remove.push(id);
            }
        }
    }

    let removed_count = to_remove.len() as u64;
    for id in to_remove {
        graph.remove(&id);
    }

    let remaining: Vec<f64> = graph
        .all_triplets()
        .iter()
        .map(|t| t.metadata.weight)
        .collect();
    let (min_weight, avg_weight) = if remaining.is_empty() {
        (0.0, 0.0)
    } else {
        let min = remaining
            .iter()
            .copied()
            .fold(f64::INFINITY, |a, b| a.min(b));
        let sum: f64 = remaining.iter().sum();
        (min, sum / (remaining.len() as f64))
    };

    DecayReport {
        nodes_decayed: decayed_count,
        nodes_removed: removed_count,
        nodes_faded: 0,
        min_weight,
        avg_weight,
    }
}

// -----------------------------------------------------------------------------
// Tests Unitaires Autonomes
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predicates_and_active_user_deterministic() {
        let u1 = active_user_node_id();
        let u2 = active_user_node_id();
        assert_eq!(u1, u2);

        let p1 = cognitive_predicate_id(PREDICATE_COGNITION_EMOTION);
        let p2 = cognitive_predicate_id(PREDICATE_COGNITION_EMOTION);
        assert_eq!(p1, p2);

        assert!(is_cognitive_predicate(PREDICATE_COGNITION_EMOTION));
        assert!(is_cognitive_predicate(PREDICATE_COGNITION_FATIGUE_INDEX));
        assert!(is_cognitive_predicate("cognition:custom_signal"));
        assert!(!is_cognitive_predicate("user:name"));
    }

    #[test]
    fn test_create_cognitive_helpers() {
        let emo = create_emotion_triplet("focused", 0.95).unwrap();
        assert_eq!(emo.subject, active_user_node_id());
        assert_eq!(emo.object, NodeValue::Text("focused".to_string()));
        assert_eq!(emo.metadata.source, TripletSource::Inferred);
        assert_eq!(emo.metadata.context, ContextPartition::Personal);
        assert!((emo.metadata.decay_rate - COGNITIVE_DECAY_RATE).abs() < 1e-9);

        let age = create_estimated_age_triplet(32.5, 0.85).unwrap();
        assert_eq!(age.object, NodeValue::Number(32.5));

        let post = create_posture_triplet("Upright", 0.90).unwrap();
        assert_eq!(post.object, NodeValue::Text("Upright".to_string()));

        let att = create_attention_level_triplet("high", 0.88).unwrap();
        assert_eq!(att.object, NodeValue::Text("high".to_string()));

        let fat = create_fatigue_index_triplet(0.12, 0.92).unwrap();
        assert_eq!(fat.object, NodeValue::Number(0.12));
    }

    #[test]
    fn test_invalid_confidence_rejected() {
        assert!(create_emotion_triplet("neutral", -0.1).is_err());
        assert!(create_emotion_triplet("neutral", 1.2).is_err());
        assert!(create_emotion_triplet("neutral", f64::NAN).is_err());
    }

    #[test]
    fn test_decay_half_life_calculation() {
        let lambda = COGNITIVE_DECAY_RATE;
        let t_half = 300.0; // 5 minutes
        let factor = (-lambda * t_half).exp();
        // e^(-0.00231 * 300) = e^(-0.693) ≈ 0.50007
        assert!((factor - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_prune_cognitive_signals_purges_physically_without_tombstone() {
        let mut graph = KnowledgeGraph::new();

        let mut emo = create_emotion_triplet("focused", 0.95).unwrap();
        emo.metadata.weight = 0.04; // Déjà sous le cutoff 0.05
        emo.metadata.last_activated = Timestamp(chrono::Utc::now());

        let id = emo.id.clone();
        graph.insert(emo);
        assert!(graph.contains(&id));

        // Purge sélective cognitive
        let report = prune_cognitive_signals(&mut graph, 0.05);

        assert_eq!(report.nodes_removed, 1);
        assert_eq!(report.nodes_faded, 0); // Strictement aucun tombstone conservé !
        assert!(!graph.contains(&id));
    }
}
