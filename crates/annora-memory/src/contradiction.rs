use crate::triplet::Triplet;
use annora_core::TrustLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContradictionType {
    DirectConflict,
    TemporalConflict,
    ConfidenceConflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Resolution {
    KeepExisting,
    ReplaceWithNew,
    AskUser { reason: String },
}

/// Seuils configurables de détection / résolution.
///
/// Défauts historiques (v1) : `min_conf = 0.5`, `gap = 0.4`,
/// `replace_margin = 0.2`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContradictionPolicy {
    /// Confiance minimale des deux côtés pour un conflit direct/temporel.
    pub min_conf: f64,
    /// Écart de confiance déclenchant un conflit de confiance (même S/P/O).
    pub gap: f64,
    /// Marge de confiance exigée pour remplacer sans demander (même trust).
    pub replace_margin: f64,
}

impl Default for ContradictionPolicy {
    fn default() -> Self {
        Self {
            min_conf: 0.5,
            gap: 0.4,
            replace_margin: 0.2,
        }
    }
}

impl ContradictionPolicy {
    pub fn new(min_conf: f64, gap: f64, replace_margin: f64) -> Self {
        Self {
            min_conf,
            gap,
            replace_margin,
        }
    }

    /// Heuristique documentée (spec §8, boîte « contradictions ») :
    /// - même sujet + même prédicat + objets différents + confiance min ≥
    ///   `min_conf` → conflit direct (« deux choses ne vont pas ensemble ») ;
    /// - objets = horodatages différents → conflit temporel ;
    /// - mêmes S/P/O mais écart de confiance > `gap` → conflit de confiance
    ///   (deux sources pas d'accord sur la certitude).
    /// En deçà des seuils : pas de contradiction (bruit, doublon, incertain).
    pub fn detect(&self, existing: &Triplet, incoming: &Triplet) -> Option<ContradictionType> {
        if existing.subject != incoming.subject || existing.predicate != incoming.predicate {
            return None;
        }
        if existing.object != incoming.object {
            let temporal = matches!(
                (&existing.object, &incoming.object),
                (
                    crate::triplet::NodeValue::Timestamp(_),
                    crate::triplet::NodeValue::Timestamp(_)
                )
            );
            let min_conf = existing
                .metadata
                .confidence
                .min(incoming.metadata.confidence);
            if min_conf < self.min_conf {
                return None;
            }
            return Some(if temporal {
                ContradictionType::TemporalConflict
            } else {
                ContradictionType::DirectConflict
            });
        }
        let gap = (existing.metadata.confidence - incoming.metadata.confidence).abs();
        if gap > self.gap {
            return Some(ContradictionType::ConfidenceConflict);
        }
        None
    }

    /// Résolution : l'entrant nettement plus sûr remplace ; sinon on demande à
    /// l'utilisateur avec la raison (jamais d'écrasement silencieux).
    ///
    /// Prise en compte du `trust_level` (`System > User > Guest > Untrusted`,
    /// voir [`trust_rank`]) :
    /// - entrant strictement plus fiable que l'existant : il suffit qu'il soit
    ///   au moins aussi confiant (`>=`, sans marge) pour remplacer ;
    /// - existant strictement plus fiable (typ. fait `System`) : jamais de
    ///   remplacement silencieux, même avec une confiance supérieure → `AskUser`
    ///   (protège les faits système) ;
    /// - même niveau : règle historique (`incoming >= existing + replace_margin`).
    pub fn resolve(&self, existing: &Triplet, incoming: &Triplet) -> Resolution {
        match self.detect(existing, incoming) {
            None => Resolution::KeepExisting,
            Some(ContradictionType::DirectConflict | ContradictionType::TemporalConflict) => {
                let rank_old = trust_rank(&existing.metadata.trust_level);
                let rank_new = trust_rank(&incoming.metadata.trust_level);
                if rank_new > rank_old {
                    if incoming.metadata.confidence >= existing.metadata.confidence {
                        Resolution::ReplaceWithNew
                    } else {
                        Resolution::AskUser {
                            reason: "Source entrante plus fiable mais moins confiante : arbitrage requis.".to_string(),
                        }
                    }
                } else if rank_old > rank_new {
                    Resolution::AskUser {
                        reason: "Le fait existant vient d'une source plus fiable : pas d'écrasement silencieux.".to_string(),
                    }
                } else if incoming.metadata.confidence
                    >= existing.metadata.confidence + self.replace_margin
                {
                    Resolution::ReplaceWithNew
                } else {
                    Resolution::AskUser {
                        reason: "Deux faits se contredisent sans vainqueur net.".to_string(),
                    }
                }
            }
            Some(ContradictionType::ConfidenceConflict) => Resolution::AskUser {
                reason: "Même fait, certitudes incompatibles entre sources.".to_string(),
            },
        }
    }
}

/// Rang de fiabilité : `System (3) > User (2) > Guest (1) > Untrusted (0)`.
pub fn trust_rank(level: &TrustLevel) -> u8 {
    match level {
        TrustLevel::System => 3,
        TrustLevel::User => 2,
        TrustLevel::Guest => 1,
        TrustLevel::Untrusted => 0,
    }
}

/// Heuristique v1 avec seuils par défaut (compatibilité historique).
pub fn detect_contradiction(existing: &Triplet, incoming: &Triplet) -> Option<ContradictionType> {
    ContradictionPolicy::default().detect(existing, incoming)
}

/// Boîte « contradictions » (spec §8) : balaye le graphe et rend les paires
/// qui « ne vont pas ensemble », avec leur nature. O(n²) assumé pour un
/// graphe local ; le shell n'affiche que le compte + les premières paires.
pub fn scan_contradictions(triplets: &[Triplet]) -> Vec<(usize, usize, &'static str)> {
    scan_contradictions_with_policy(triplets, &ContradictionPolicy::default())
}

/// Variante à seuils injectés.
pub fn scan_contradictions_with_policy(
    triplets: &[Triplet],
    policy: &ContradictionPolicy,
) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    for (i, a) in triplets.iter().enumerate() {
        for (j, b) in triplets.iter().enumerate().skip(i + 1) {
            let kind = match policy.detect(a, b) {
                Some(ContradictionType::DirectConflict) => "conflit direct",
                Some(ContradictionType::TemporalConflict) => "conflit temporel",
                Some(ContradictionType::ConfidenceConflict) => "confiance incompatible",
                None => continue,
            };
            out.push((i, j, kind));
        }
    }
    out
}

/// Résolution avec seuils par défaut (compatibilité historique).
pub fn resolve(existing: &Triplet, incoming: &Triplet) -> Resolution {
    ContradictionPolicy::default().resolve(existing, incoming)
}

/// Contradiction en attente d'arbitrage (jamais d'écrasement silencieux :
/// les cas `AskUser` sont mis en file au lieu d'être perdus).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingResolution {
    pub id: String,
    pub incoming: Triplet,
    pub existing: Triplet,
}

impl PendingResolution {
    pub fn new(id: impl Into<String>, existing: Triplet, incoming: Triplet) -> Self {
        Self {
            id: id.into(),
            incoming,
            existing,
        }
    }
}

/// File d'attente FIFO des arbitrages en suspens.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingQueue {
    queue: Vec<PendingResolution>,
}

impl PendingQueue {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub fn push(&mut self, pending: PendingResolution) {
        self.queue.push(pending);
    }

    /// Rend le plus ancien en attente (FIFO), `None` si vide.
    pub fn pop(&mut self) -> Option<PendingResolution> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    pub fn peek(&self) -> Option<&PendingResolution> {
        self.queue.first()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_partition::ContextPartition;
    use crate::triplet::{NodeValue, TripletMetadata, TripletSource};
    use annora_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
    use uuid::Uuid;

    fn triplet(object: NodeValue, confidence: f64) -> Triplet {
        triplet_with_trust(object, confidence, TrustLevel::User)
    }

    fn triplet_with_trust(object: NodeValue, confidence: f64, trust_level: TrustLevel) -> Triplet {
        Triplet {
            id: TripletId(Uuid::now_v7()),
            subject: NodeId(Uuid::from_u128(1)),
            predicate: PredicateId(Uuid::from_u128(2)),
            object,
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence,
                context: ContextPartition::Personal,
                trust_level,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    #[test]
    fn test_direct_temporal_and_confidence_conflicts() {
        let a = triplet(NodeValue::Text("Paris".into()), 0.9);
        let b = triplet(NodeValue::Text("Dakar".into()), 0.8);
        assert!(matches!(
            detect_contradiction(&a, &b),
            Some(ContradictionType::DirectConflict)
        ));
        // Confiance trop basse : pas de contradiction (bruit).
        let weak = triplet(NodeValue::Text("Dakar".into()), 0.2);
        assert!(detect_contradiction(&a, &weak).is_none());
        // Horodatages différents : temporel.
        let t1 = triplet(NodeValue::Timestamp(Timestamp::now()), 0.9);
        let t2 = triplet(NodeValue::Timestamp(Timestamp::now()), 0.9);
        assert!(matches!(
            detect_contradiction(&t1, &t2),
            Some(ContradictionType::TemporalConflict)
        ));
        // Même fait, certitudes incompatibles.
        let c1 = triplet(NodeValue::Text("Paris".into()), 0.9);
        let c2 = triplet(NodeValue::Text("Paris".into()), 0.3);
        assert!(matches!(
            detect_contradiction(&c1, &c2),
            Some(ContradictionType::ConfidenceConflict)
        ));
        // Doublon : rien.
        assert!(detect_contradiction(&a, &a).is_none());
    }

    #[test]
    fn test_resolve_never_silently_overwrites() {
        let a = triplet(NodeValue::Text("Paris".into()), 0.7);
        let b = triplet(NodeValue::Text("Dakar".into()), 0.95);
        assert!(matches!(resolve(&a, &b), Resolution::ReplaceWithNew));
        let c = triplet(NodeValue::Text("Dakar".into()), 0.75);
        assert!(matches!(resolve(&a, &c), Resolution::AskUser { .. }));
        let d = triplet(NodeValue::Text("Paris".into()), 0.7);
        assert!(matches!(resolve(&a, &d), Resolution::KeepExisting));
    }

    #[test]
    fn test_policy_defaults_and_custom_thresholds() {
        let def = ContradictionPolicy::default();
        assert!((def.min_conf - 0.5).abs() < 1e-12);
        assert!((def.gap - 0.4).abs() < 1e-12);
        assert!((def.replace_margin - 0.2).abs() < 1e-12);

        // Politique stricte : min_conf 0.95 ⇒ le conflit par défaut disparaît.
        let strict = ContradictionPolicy::new(0.95, 0.4, 0.2);
        let a = triplet(NodeValue::Text("Paris".into()), 0.9);
        let b = triplet(NodeValue::Text("Dakar".into()), 0.8);
        assert!(strict.detect(&a, &b).is_none());
        assert!(def.detect(&a, &b).is_some());

        // Marge de remplacement custom : 0.5 exige un écart net.
        let picky = ContradictionPolicy::new(0.5, 0.4, 0.5);
        let old = triplet(NodeValue::Text("Paris".into()), 0.7);
        let new = triplet(NodeValue::Text("Dakar".into()), 0.95);
        assert!(matches!(
            picky.resolve(&old, &new),
            Resolution::AskUser { .. }
        ));
    }

    #[test]
    fn test_trust_rank_ordering() {
        assert!(trust_rank(&TrustLevel::System) > trust_rank(&TrustLevel::User));
        assert!(trust_rank(&TrustLevel::User) > trust_rank(&TrustLevel::Guest));
        assert!(trust_rank(&TrustLevel::Guest) > trust_rank(&TrustLevel::Untrusted));
    }

    #[test]
    fn test_resolve_honors_trust_level() {
        let policy = ContradictionPolicy::default();
        // Existant User 0.7, entrant System 0.7 (même confiance, trust supérieur)
        // ⇒ remplace (la fiabilité compense l'absence de marge).
        let old = triplet_with_trust(NodeValue::Text("Paris".into()), 0.7, TrustLevel::User);
        let sys = triplet_with_trust(NodeValue::Text("Dakar".into()), 0.7, TrustLevel::System);
        assert!(matches!(
            policy.resolve(&old, &sys),
            Resolution::ReplaceWithNew
        ));

        // Existant System 0.7, entrant Untrusted 0.95 (forte confiance mais
        // trust inférieur) ⇒ jamais d'écrasement silencieux.
        let sys_old = triplet_with_trust(NodeValue::Text("Paris".into()), 0.7, TrustLevel::System);
        let untrusted =
            triplet_with_trust(NodeValue::Text("Dakar".into()), 0.95, TrustLevel::Untrusted);
        assert!(matches!(
            policy.resolve(&sys_old, &untrusted),
            Resolution::AskUser { .. }
        ));
    }

    #[test]
    fn test_pending_queue_fifo() {
        let mut q = PendingQueue::new();
        assert!(q.is_empty());
        let a = triplet(NodeValue::Text("Paris".into()), 0.7);
        let b = triplet(NodeValue::Text("Dakar".into()), 0.75);
        q.push(PendingResolution::new("1", a.clone(), b.clone()));
        q.push(PendingResolution::new("2", b.clone(), a.clone()));
        assert_eq!(q.len(), 2);
        assert_eq!(q.peek().unwrap().id, "1");
        assert_eq!(q.pop().unwrap().id, "1");
        assert_eq!(q.pop().unwrap().id, "2");
        assert!(q.pop().is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn test_enums_serialize() {
        // Les enums doivent rester sérialisables (journalisation / IPC).
        let ct = ContradictionType::DirectConflict;
        let json = serde_json::to_string(&ct).unwrap();
        assert!(!json.is_empty());
        let r = Resolution::AskUser {
            reason: "test".to_string(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Resolution = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
