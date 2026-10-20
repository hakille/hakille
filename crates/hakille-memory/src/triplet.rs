use crate::context_partition::ContextPartition;
use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeValue {
    Entity(NodeId),
    Text(String),
    Number(f64),
    Boolean(bool),
    Timestamp(Timestamp),
    Embedding(Vec<f32>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TripletSource {
    User,
    System,
    Inferred,
    External(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TripletMetadata {
    pub weight: f64,
    pub created_at: Timestamp,
    pub last_activated: Timestamp,
    pub source: TripletSource,
    pub confidence: f64,
    pub context: ContextPartition,
    pub trust_level: TrustLevel,
    pub decay_rate: f64,
}

impl TripletMetadata {
    /// Construit des métadonnées en validant les invariants numériques.
    ///
    /// `created_at` / `last_activated` sont initialisés à maintenant ;
    /// `weight` et `confidence` doivent appartenir à `[0, 1]`,
    /// `decay_rate` doit être fini et `>= 0.0` (taux par seconde).
    /// Toute violation rend `Err` (jamais de clamp silencieux : un appelant
    /// qui veut borner le fait explicitement avant d'appeler).
    pub fn new(
        weight: f64,
        confidence: f64,
        source: TripletSource,
        context: ContextPartition,
        trust_level: TrustLevel,
        decay_rate: f64,
    ) -> hakille_core::Result<Self> {
        let meta = Self {
            weight,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source,
            confidence,
            context,
            trust_level,
            decay_rate,
        };
        meta.validate()?;
        Ok(meta)
    }

    /// Valide les invariants numériques : `weight` et `confidence` dans
    /// `[0, 1]` (bornes incluses, `NaN`/`±∞` rejetés), `decay_rate` fini et
    /// `>= 0.0` (`NaN` rejeté).
    pub fn validate(&self) -> hakille_core::Result<()> {
        for (name, value) in [("weight", self.weight), ("confidence", self.confidence)] {
            // `contains` rend faux pour NaN/±∞ hors intervalle : rejeté.
            if !(0.0..=1.0).contains(&value) {
                return Err(hakille_core::HakilleError::Internal(format!(
                    "TripletMetadata::{name} hors [0,1] : {value}"
                )));
            }
        }
        if !self.decay_rate.is_finite() || self.decay_rate < 0.0 {
            return Err(hakille_core::HakilleError::Internal(format!(
                "TripletMetadata::decay_rate doit être fini et >= 0.0 : {}",
                self.decay_rate
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Triplet {
    pub id: TripletId,
    pub subject: NodeId,
    pub predicate: PredicateId,
    pub object: NodeValue,
    pub metadata: TripletMetadata,
    /// Nature du nœud sujet/objet tel que typé à l'ingestion.
    ///
    /// `None` = donnée historique (antérieure au typage) : interpréter comme
    /// [`NodeType::Fact`] via [`Self::node_type_or_default`]. `#[serde(default)]`
    /// garantit la compatibilité de lecture des snapshots JSON écrits avant
    /// l'introduction du champ. Nota : les blobs `bincode` (tier edge redb)
    /// écrits avant ce champ ne se décodent plus — ils sont réécrits au
    /// prochain `persist` (format v1, pas de migration prévue à cette échelle).
    #[serde(default)]
    pub node_type: Option<NodeType>,
}

impl Triplet {
    /// Constructeur validé : refuse les métadonnées hors invariants
    /// ([`TripletMetadata::validate`]) et type le triplet comme
    /// [`NodeType::Fact`] par défaut (spécialiser ensuite via
    /// [`Self::with_node_type`]).
    pub fn new(
        id: TripletId,
        subject: NodeId,
        predicate: PredicateId,
        object: NodeValue,
        metadata: TripletMetadata,
    ) -> hakille_core::Result<Self> {
        metadata.validate()?;
        Ok(Self {
            id,
            subject,
            predicate,
            object,
            metadata,
            node_type: Some(NodeType::Fact),
        })
    }

    /// Spécialise la nature du nœud (builder, ne valide rien de plus).
    pub fn with_node_type(mut self, node_type: NodeType) -> Self {
        self.node_type = Some(node_type);
        self
    }

    /// Nature effective du nœud : le type stocké, ou [`NodeType::Fact`] pour
    /// les triplets historiques non typés (`node_type == None`).
    pub fn node_type_or_default(&self) -> NodeType {
        self.node_type.clone().unwrap_or(NodeType::Fact)
    }

    /// Valide les invariants du triplet (délègue aux métadonnées).
    pub fn validate(&self) -> hakille_core::Result<()> {
        self.metadata.validate()
    }
}

#[derive(Debug, Clone)]
pub struct TriplePattern {
    pub subject: Option<NodeId>,
    pub predicate: Option<PredicateId>,
    pub object: Option<NodeValue>,
}

/// Nature d'un nœud du graphe, renseignée à l'ingestion et exposée pour le
/// routage cognitif (partitions, politique de décroissance, affichage).
///
/// - [`NodeType::Fact`] : fait brut (« Alice aime le café ») — défaut pour
///   tout triplet construit via [`Triplet::new`] et pour les données
///   historiques non typées.
/// - [`NodeType::Entity`] : le sujet désigne une entité suivie (personne,
///   lieu, projet) plutôt qu'un simple libellé.
/// - [`NodeType::Objective`] : objectif / but poursuivi (lié au module
///   `objective`).
/// - [`NodeType::Preference`] : préférence utilisateur (goûts, réglages) —
///   poids fort, décroissance lente recommandée.
/// - [`NodeType::Skill`] : savoir-faire / capacité (« sait réserver un vol »).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Fact,
    Entity,
    Objective,
    Preference,
    Skill,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // API inter-crates figée (kernel/daemon) : boxer `incoming` casserait les appelants.
pub enum InsertResult {
    Inserted(TripletId),
    ContradictionResolved {
        old: TripletId,
        new: TripletId,
    },
    NeedsUserResolution {
        existing: Triplet,
        incoming: Triplet,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_metadata(weight: f64, confidence: f64) -> TripletMetadata {
        TripletMetadata {
            weight,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence,
            context: ContextPartition::Default,
            trust_level: TrustLevel::User,
            decay_rate: 0.01,
        }
    }

    fn sample_triplet(weight: f64, confidence: f64) -> Triplet {
        Triplet {
            id: TripletId(uuid::Uuid::now_v7()),
            subject: NodeId(uuid::Uuid::now_v7()),
            predicate: PredicateId(uuid::Uuid::now_v7()),
            object: NodeValue::Text("fait".to_string()),
            metadata: valid_metadata(weight, confidence),
            node_type: Some(NodeType::Fact),
        }
    }

    #[test]
    fn test_metadata_validation_rejects_out_of_range() {
        assert!(valid_metadata(0.0, 0.0).validate().is_ok());
        assert!(valid_metadata(1.0, 1.0).validate().is_ok());
        assert!(valid_metadata(-0.1, 0.5).validate().is_err());
        assert!(valid_metadata(1.5, 0.5).validate().is_err());
        assert!(valid_metadata(0.5, -0.2).validate().is_err());
        assert!(valid_metadata(0.5, 2.0).validate().is_err());
        assert!(valid_metadata(f64::NAN, 0.5).validate().is_err());
        assert!(valid_metadata(0.5, f64::INFINITY).validate().is_err());
        let mut bad_decay = valid_metadata(0.5, 0.5);
        bad_decay.decay_rate = -0.01;
        assert!(bad_decay.validate().is_err());
        let mut nan_decay = valid_metadata(0.5, 0.5);
        nan_decay.decay_rate = f64::NAN;
        assert!(nan_decay.validate().is_err());
        assert!(
            TripletMetadata::new(
                0.5,
                0.9,
                TripletSource::User,
                ContextPartition::Default,
                TrustLevel::User,
                0.01
            )
            .is_ok()
        );
        assert!(
            TripletMetadata::new(
                1.2,
                0.9,
                TripletSource::User,
                ContextPartition::Default,
                TrustLevel::User,
                0.01
            )
            .is_err()
        );
    }

    #[test]
    fn test_triplet_new_defaults_node_type_to_fact() {
        let t = Triplet::new(
            TripletId(uuid::Uuid::now_v7()),
            NodeId(uuid::Uuid::now_v7()),
            PredicateId(uuid::Uuid::now_v7()),
            NodeValue::Text("fait".to_string()),
            valid_metadata(0.5, 0.9),
        )
        .unwrap();
        assert_eq!(t.node_type, Some(NodeType::Fact));
        assert_eq!(t.node_type_or_default(), NodeType::Fact);
        let skilled = t.with_node_type(NodeType::Skill);
        assert_eq!(skilled.node_type_or_default(), NodeType::Skill);
    }

    #[test]
    fn test_node_type_or_default_maps_legacy_none_to_fact() {
        let mut t = sample_triplet(0.5, 0.9);
        t.node_type = None;
        assert_eq!(t.node_type_or_default(), NodeType::Fact);
        assert!(t.validate().is_ok());
        let mut bad = sample_triplet(0.5, 0.9);
        bad.metadata.weight = 42.0;
        assert!(bad.validate().is_err());
        assert!(
            Triplet::new(
                bad.id.clone(),
                bad.subject.clone(),
                bad.predicate.clone(),
                bad.object.clone(),
                bad.metadata.clone(),
            )
            .is_err()
        );
    }

    #[test]
    fn test_insert_result_is_comparable() {
        let id = TripletId(uuid::Uuid::now_v7());
        assert_eq!(
            InsertResult::Inserted(id.clone()),
            InsertResult::Inserted(id)
        );
    }
}
