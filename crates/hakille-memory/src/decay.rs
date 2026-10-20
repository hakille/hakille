use crate::graph::KnowledgeGraph;
use crate::triplet::{Triplet, TripletSource};
use hakille_core::{Timestamp, TrustLevel};

pub struct DecayEngine {
    pub rate: f64,
}

pub struct DecayReport {
    pub nodes_decayed: u64,
    pub nodes_removed: u64,
    /// Triplets fanés mais conservés (poids ramené à `cutoff`, histoire préservée).
    pub nodes_faded: u64,
    pub min_weight: f64,
    pub avg_weight: f64,
}

impl DecayEngine {
    /// Construit un moteur avec un taux de repli (`fallback`) utilisé quand
    /// `metadata.decay_rate == 0` (triplet sans taux propre).
    pub fn new(rate: f64) -> Self {
        Self {
            rate: rate.max(0.0),
        }
    }

    /// Taux effectif d'un triplet : son `decay_rate` propre, ou le repli
    /// `self.rate` quand il vaut exactement 0.
    pub fn effective_rate(&self, triplet: &Triplet) -> f64 {
        if triplet.metadata.decay_rate == 0.0 {
            self.rate.max(0.0)
        } else {
            triplet.metadata.decay_rate.max(0.0)
        }
    }

    /// Exclusion de dégradation : on ne dégrade jamais un triplet
    /// `source == System` ni `trust_level == System` (faits système stables).
    pub fn should_exclude(triplet: &Triplet) -> bool {
        matches!(triplet.metadata.source, TripletSource::System)
            || triplet.metadata.trust_level == TrustLevel::System
    }

    /// Passe instance (avec `self.rate` comme repli).
    pub fn apply(&self, graph: &mut KnowledgeGraph, cutoff_weight: f64) -> DecayReport {
        self.apply_with_history(graph, cutoff_weight, true)
    }

    /// Passe instance avec choix d'historique.
    pub fn apply_with_history(
        &self,
        graph: &mut KnowledgeGraph,
        cutoff_weight: f64,
        preserve_history: bool,
    ) -> DecayReport {
        Self::apply_inner(graph, cutoff_weight, preserve_history, self.rate)
    }

    /// Vraie décroissance exponentielle (COGNITION.md §5) :
    /// `W(t) = W0 · e^(−λ·(t − t0))`.
    ///
    /// - `W0` = poids courant du triplet, `λ` = taux effectif
    ///   (`metadata.decay_rate` propre au triplet, repli `0.0` ici —
    ///   voir [`Self::effective_rate`] / [`Self::new`] pour le repli
    ///   configurable),
    /// - `t − t0` = secondes écoulées depuis `metadata.last_activated`
    ///   (horloge murale UTC, jamais négative : dates futures ⇒ 0).
    ///
    /// `preserve_history = true` (défaut via [`Self::apply_decay`]) : aucun
    /// triplet n'est supprimé. Ceux qui tombent **strictement sous**
    /// `cutoff_weight` sont fanés — poids ramené à `cutoff_weight` — et
    /// conservés comme *tombstone logique* (un poids `== cutoff` signale un
    /// souvenir fané, interrogeable pour l'historique). `nodes_removed` vaut
    /// alors 0 et les fanés sont comptés dans `nodes_faded`.
    ///
    /// `preserve_history = false` : comportement destructif historique
    /// (suppression via `graph.remove`, comptée dans `nodes_removed`).
    ///
    /// Exclusions : les triplets [`Self::should_exclude`] (source ou trust
    /// `System`) sont ignorés. Après chaque décroissance, `last_activated`
    /// est recalé sur `now` (corrige le double-comptage : un second passage
    /// immédiat ne dégrade plus).
    pub fn apply_decay(graph: &mut KnowledgeGraph, cutoff_weight: f64) -> DecayReport {
        Self::apply_decay_with_history(graph, cutoff_weight, true)
    }

    pub fn apply_decay_with_history(
        graph: &mut KnowledgeGraph,
        cutoff_weight: f64,
        preserve_history: bool,
    ) -> DecayReport {
        // Compat historique : aucun repli configuré (taux 0 ⇒ pas de
        // décroissance pour les triplets sans taux propre).
        Self::apply_inner(graph, cutoff_weight, preserve_history, 0.0)
    }

    fn apply_inner(
        graph: &mut KnowledgeGraph,
        cutoff_weight: f64,
        preserve_history: bool,
        fallback_rate: f64,
    ) -> DecayReport {
        let now = chrono::Utc::now();
        let fallback = fallback_rate.max(0.0);
        let mut decayed: u64 = 0;
        let mut to_fade = Vec::new();

        for id in graph.ids() {
            if let Some(t) = graph.get_mut(&id) {
                if Self::should_exclude(t) {
                    continue;
                }
                let dt_secs = (now - t.metadata.last_activated.0)
                    .num_milliseconds()
                    .max(0) as f64
                    / 1000.0;
                let lambda = if t.metadata.decay_rate == 0.0 {
                    fallback
                } else {
                    t.metadata.decay_rate.max(0.0)
                };
                let before = t.metadata.weight;
                let after = before * (-lambda * dt_secs).exp();
                // Mise à jour en place + recalage de référence sur `now` :
                // la décroissance est mesurée depuis la dernière activation,
                // et on ré-ancre t0 pour éviter tout double-comptage.
                t.metadata.weight = after;
                t.metadata.last_activated = Timestamp(now);
                if (before - after).abs() > 1e-12 {
                    decayed += 1;
                }
                if after < cutoff_weight {
                    to_fade.push(id);
                }
            }
        }

        let (removed, faded) = if preserve_history {
            // Pas de suppression : fane + tombstone logique.
            for id in &to_fade {
                if let Some(t) = graph.get_mut(id) {
                    t.metadata.weight = cutoff_weight;
                }
            }
            (0u64, to_fade.len() as u64)
        } else {
            let removed = to_fade.len() as u64;
            for id in to_fade {
                graph.remove(&id);
            }
            (removed, 0u64)
        };

        let remaining: Vec<f64> = graph
            .all_triplets()
            .iter()
            .map(|t| t.metadata.weight)
            .collect();
        let (min_weight, avg_weight) = if remaining.is_empty() {
            (0.0, 0.0)
        } else {
            let min = remaining.iter().cloned().fold(f64::INFINITY, f64::min);
            let avg = remaining.iter().sum::<f64>() / remaining.len() as f64;
            (min, avg)
        };

        DecayReport {
            nodes_decayed: decayed,
            nodes_removed: removed,
            nodes_faded: faded,
            min_weight,
            avg_weight,
        }
    }

    /// Tombstone logique : un triplet fané (poids `<= cutoff`) est conservé
    /// pour l'historique au lieu d'être supprimé.
    pub fn is_faded(weight: f64, cutoff_weight: f64) -> bool {
        weight <= cutoff_weight
    }

    /// Candidats tombstones : triplets fanés (poids `<= cutoff`), à propager
    /// vers le CRDT (`tombstones`) après `N` cycles sans réactivation.
    pub fn faded_candidates(
        graph: &KnowledgeGraph,
        cutoff_weight: f64,
    ) -> Vec<hakille_core::TripletId> {
        graph
            .all_triplets()
            .into_iter()
            .filter(|t| Self::is_faded(t.metadata.weight, cutoff_weight))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Oubli unifié : supprime les triplets fanés plus vieux que `max_age`
    /// (dernier accès `last_activated`). Rend le nombre de purgés.
    /// Les non-fanés ne sont jamais purgés ici (histoire préservée).
    pub fn gc_faded_older_than(
        graph: &mut KnowledgeGraph,
        cutoff_weight: f64,
        max_age: chrono::Duration,
    ) -> usize {
        let cutoff = chrono::Utc::now() - max_age;
        let old: Vec<hakille_core::TripletId> = graph
            .all_triplets()
            .into_iter()
            .filter(|t| {
                Self::is_faded(t.metadata.weight, cutoff_weight)
                    && t.metadata.last_activated.0 < cutoff
            })
            .map(|t| t.id.clone())
            .collect();
        let n = old.len();
        for id in old {
            graph.remove(&id);
        }
        n
    }

    /// Variante 30 jours (politique d'oubli unifiée : fané + 30j sans accès ⇒ GC).
    pub fn gc_faded_older_than_30d(graph: &mut KnowledgeGraph, cutoff_weight: f64) -> usize {
        Self::gc_faded_older_than(graph, cutoff_weight, chrono::Duration::days(30))
    }
}

/// Âge maximal d'un souvenir fané avant GC (30 jours, politique unifiée).
pub const FORGET_GC_MAX_AGE_DAYS: i64 = 30;
/// Cycles de decay consécutifs fané avant candidature tombstone (défaut 3).
pub const FADED_TOMBSTONE_CYCLES: u32 = 3;

/// Compteur de cycles fanés : un triplet resté fané `N` passages de suite
/// (sans réactivation) devient candidat tombstone CRDT. `record` s'appelle
/// après chaque `apply_decay` ; `tombstone_candidates` rend les IDs ≥ seuil.
/// Une réactivation (poids > cutoff ou disparition) remet le compteur à zéro.
#[derive(Debug, Default)]
pub struct ForgettingTracker {
    faded_cycles: std::collections::HashMap<hakille_core::TripletId, u32>,
}

impl ForgettingTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un passage : incrémente les fanés, oublie les réactivés.
    pub fn record(&mut self, graph: &KnowledgeGraph, cutoff_weight: f64) {
        let mut seen = std::collections::HashSet::new();
        for t in graph.all_triplets() {
            if DecayEngine::is_faded(t.metadata.weight, cutoff_weight) {
                *self.faded_cycles.entry(t.id.clone()).or_insert(0) += 1;
                seen.insert(t.id.clone());
            }
        }
        self.faded_cycles.retain(|id, _| seen.contains(id));
    }

    /// Candidats tombstones (≥ `threshold` cycles, défaut `FADED_TOMBSTONE_CYCLES`).
    pub fn tombstone_candidates(&self, threshold: u32) -> Vec<hakille_core::TripletId> {
        self.faded_cycles
            .iter()
            .filter(|&(_, &c)| c >= threshold)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.faded_cycles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faded_cycles.is_empty()
    }
}

/// Planificateur de décroissance : ne relance un passage que si
/// `interval_secs` se sont écoulées depuis le dernier run.
#[derive(Debug, Clone)]
pub struct DecayScheduler {
    pub interval_secs: u64,
    pub cutoff: f64,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
}

impl DecayScheduler {
    pub fn new(interval_secs: u64, cutoff: f64) -> Self {
        Self {
            interval_secs,
            cutoff,
            last_run: None,
        }
    }

    /// Vrai s'il faut lancer un passage maintenant (`Utc::now`).
    pub fn should_run(&self) -> bool {
        self.should_run_at(chrono::Utc::now())
    }

    /// Variante testable à horloge injectée.
    pub fn should_run_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        match self.last_run {
            None => true,
            Some(last) => (now - last).num_seconds() >= self.interval_secs as i64,
        }
    }

    /// Marque un passage à `Utc::now`.
    pub fn mark_run(&mut self) {
        self.mark_run_at(chrono::Utc::now());
    }

    /// Variante testable à horloge injectée.
    pub fn mark_run_at(&mut self, now: chrono::DateTime<chrono::Utc>) {
        self.last_run = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextPartition;
    use crate::triplet::{NodeValue, Triplet, TripletMetadata, TripletSource};
    use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};

    fn faded_candidate() -> Triplet {
        // Dernier accès il y a 1 h avec un taux agressif ⇒ poids ≈ 0 après decay.
        Triplet {
            id: TripletId(uuid::Uuid::now_v7()),
            subject: NodeId(uuid::Uuid::now_v7()),
            predicate: PredicateId(uuid::Uuid::now_v7()),
            object: NodeValue::Text("vieux souvenir".to_string()),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp(chrono::Utc::now() - chrono::Duration::hours(2)),
                last_activated: Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
                source: TripletSource::User,
                confidence: 0.9,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    fn zero_rate_candidate() -> Triplet {
        let mut t = faded_candidate();
        t.metadata.decay_rate = 0.0;
        t
    }

    fn system_candidate() -> Triplet {
        let mut t = faded_candidate();
        t.metadata.source = TripletSource::System;
        t.metadata.trust_level = TrustLevel::System;
        t
    }

    #[test]
    fn test_decay_preserves_history_by_default() {
        let mut graph = KnowledgeGraph::new();
        let t = faded_candidate();
        let id = t.id.clone();
        graph.insert(t);

        let report = DecayEngine::apply_decay(&mut graph, 0.05);
        // Rien n'est supprimé : le triplet est fané et conservé.
        assert_eq!(report.nodes_removed, 0);
        assert_eq!(report.nodes_faded, 1);
        assert!(graph.contains(&id), "l'historique doit être préservé");
        let kept = graph.get(&id).unwrap();
        assert!((kept.metadata.weight - 0.05).abs() < 1e-12);
        assert!(DecayEngine::is_faded(kept.metadata.weight, 0.05));
    }

    #[test]
    fn test_decay_destructive_when_opted_out() {
        let mut graph = KnowledgeGraph::new();
        let t = faded_candidate();
        let id = t.id.clone();
        graph.insert(t);

        let report = DecayEngine::apply_decay_with_history(&mut graph, 0.05, false);
        assert_eq!(report.nodes_removed, 1);
        assert_eq!(report.nodes_faded, 0);
        assert!(!graph.contains(&id));
    }

    #[test]
    fn test_fallback_rate_used_when_decay_rate_zero() {
        // Sans repli (API statique historique) : taux 0 ⇒ pas de décroissance.
        let mut graph = KnowledgeGraph::new();
        graph.insert(zero_rate_candidate());
        let report = DecayEngine::apply_decay(&mut graph, 0.0);
        assert_eq!(report.nodes_decayed, 0);

        // Avec repli instance : le taux `self.rate` s'applique.
        let mut graph2 = KnowledgeGraph::new();
        graph2.insert(zero_rate_candidate());
        let engine = DecayEngine::new(0.01);
        let report2 = engine.apply(&mut graph2, 0.0);
        assert_eq!(report2.nodes_decayed, 1);
        assert_eq!(engine.effective_rate(graph2.all_triplets()[0]), 0.01);

        // Le taux propre prime sur le repli.
        let engine_zero = DecayEngine::new(0.0);
        let owned = faded_candidate();
        assert!((engine_zero.effective_rate(&owned) - 0.01).abs() < 1e-12);
    }

    #[test]
    fn test_last_activated_updated_no_double_count() {
        let mut graph = KnowledgeGraph::new();
        let t = faded_candidate();
        let id = t.id.clone();
        let before_ts = t.metadata.last_activated.0;
        graph.insert(t);

        let engine = DecayEngine::new(0.0);
        engine.apply(&mut graph, 0.0);
        let after_ts = graph.get(&id).unwrap().metadata.last_activated.0;
        assert!(
            after_ts > before_ts,
            "last_activated doit être recalé sur now"
        );

        // Second passage immédiat : quasi plus rien à dégrader.
        let w1 = graph.get(&id).unwrap().metadata.weight;
        engine.apply(&mut graph, 0.0);
        let w2 = graph.get(&id).unwrap().metadata.weight;
        assert!(
            (w1 - w2).abs() < 1e-6,
            "sans recalage, le 2e passage re-dégraderait (double-comptage)"
        );
    }

    #[test]
    fn test_should_exclude_system_source_and_trust() {
        let sys = system_candidate();
        assert!(DecayEngine::should_exclude(&sys));

        let mut trust_only = faded_candidate();
        trust_only.metadata.trust_level = TrustLevel::System;
        assert!(DecayEngine::should_exclude(&trust_only));

        let mut source_only = faded_candidate();
        source_only.metadata.source = TripletSource::System;
        assert!(DecayEngine::should_exclude(&source_only));

        let user = faded_candidate();
        assert!(!DecayEngine::should_exclude(&user));

        // Exclu ⇒ poids intact après decay.
        let mut graph = KnowledgeGraph::new();
        graph.insert(system_candidate());
        let report = DecayEngine::apply_decay(&mut graph, 0.0);
        assert_eq!(report.nodes_decayed, 0);
        assert!((graph.all_triplets()[0].metadata.weight - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_decay_scheduler_should_run() {
        let mut sched = DecayScheduler::new(3600, 0.05);
        assert!(sched.should_run(), "jamais lancé ⇒ doit tourner");
        let t0 = chrono::Utc::now();
        sched.mark_run_at(t0);
        assert!(!sched.should_run_at(t0), "juste lancé ⇒ ne pas relancer");
        let later = t0 + chrono::Duration::seconds(3600);
        assert!(sched.should_run_at(later), "intervalle écoulé ⇒ relancer");
        let before = t0 + chrono::Duration::seconds(3599);
        assert!(!sched.should_run_at(before));
    }
}
