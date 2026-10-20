use hakille_core::{Timestamp, TripletId};
use hakille_memory::KnowledgeGraph;
use hakille_memory::triplet::Triplet;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VectorClock {
    pub clock: HashMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            clock: HashMap::new(),
        }
    }

    pub fn tick(&mut self, node_id: &str) {
        let entry = self.clock.entry(node_id.to_string()).or_insert(0);
        *entry += 1;
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (node, &remote_counter) in &other.clock {
            let local_counter = self.clock.entry(node.clone()).or_insert(0);
            *local_counter = (*local_counter).max(remote_counter);
        }
    }

    pub fn get(&self, node_id: &str) -> u64 {
        self.clock.get(node_id).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtSync {
    pub elements: HashMap<TripletId, Triplet>,
    pub tombstones: HashSet<TripletId>,
    /// Horodatage de chaque tombstone (défaut vide pour compatibilité avec
    /// les états sérialisés avant cette version).
    #[serde(default)]
    pub tombstone_times: HashMap<TripletId, Timestamp>,
    pub vector_clock: VectorClock,
}

impl CrdtSync {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
            tombstone_times: HashMap::new(),
            vector_clock: VectorClock::new(),
        }
    }

    pub fn insert_triplet(&mut self, triplet: Triplet, node_id: &str) {
        self.vector_clock.tick(node_id);
        // Une réinsertion explicite ressuscite : la tombstone est levée.
        self.tombstones.remove(&triplet.id);
        self.tombstone_times.remove(&triplet.id);
        self.elements.insert(triplet.id.clone(), triplet);
    }

    pub fn remove_triplet(&mut self, id: TripletId, node_id: &str) {
        self.vector_clock.tick(node_id);
        self.tombstones.insert(id.clone());
        self.tombstone_times
            .entry(id)
            .or_insert_with(Timestamp::now);
    }

    /// Fusion LWW (Last-Writer-Wins) sur `last_activated` :
    ///
    /// - même ID des deux côtés ⇒ garde celui avec `last_activated` le plus
    ///   récent (comparaison `DateTime<Utc>`, égalité ⇒ **first-wins
    ///   documenté** : l'existant local est conservé pour la déterminisme et
    ///   la commutativité) ;
    /// - ID inconnu localement ⇒ insertion (sauf si tombstone locale, qui
    ///   garde la précédence — la suppression gagne sur la création
    ///   concurrente) ;
    /// - union des tombstones (avec conservation du plus ancien horodatage
    ///   pour un GC équitable) + fusion des horloges vectorielles.
    pub fn merge(&mut self, remote: &CrdtSync) {
        // 1. Union LWW des éléments.
        for (id, remote_triplet) in &remote.elements {
            // Tombstone locale ⇒ la suppression gagne (pas de résurrection
            // par un état distant antérieur à la suppression).
            if self.tombstones.contains(id) || remote.tombstones.contains(id) {
                continue;
            }
            match self.elements.get(id) {
                None => {
                    self.elements.insert(id.clone(), remote_triplet.clone());
                }
                Some(local) => {
                    if remote_triplet.metadata.last_activated.0 > local.metadata.last_activated.0 {
                        self.elements.insert(id.clone(), remote_triplet.clone());
                    }
                    // Sinon first-wins : on garde l'existant (y compris à
                    // égalité stricte, pour rester commutatif).
                }
            }
        }

        // 2. Union des tombstones (+ horodatages : garde le plus ancien).
        for tombstone in &remote.tombstones {
            self.tombstones.insert(tombstone.clone());
            // Le distant a peut-être un horodatage, sinon `now` conservateur.
            let remote_time = remote
                .tombstone_times
                .get(tombstone)
                .cloned()
                .unwrap_or_else(Timestamp::now);
            self.tombstone_times
                .entry(tombstone.clone())
                .and_modify(|local_time| {
                    if remote_time.0 < local_time.0 {
                        *local_time = remote_time.clone();
                    }
                })
                .or_insert(remote_time);
            // La tombstone fait disparaître l'élément des actifs.
            self.elements.remove(tombstone);
        }
        // Les tombstones locales s'appliquent aussi aux éléments distants
        // qui viendraient d'être insérés ci-dessus par erreur d'ordre.
        for tomb in self.tombstones.clone() {
            self.elements.remove(&tomb);
        }

        // 3. Merge vector clocks
        self.vector_clock.merge(&remote.vector_clock);
    }

    pub fn active_triplets(&self) -> Vec<&Triplet> {
        self.elements
            .iter()
            .filter(|(id, _)| !self.tombstones.contains(id))
            .map(|(_, triplet)| triplet)
            .collect()
    }

    /// Horodatage d'une tombstone (`None` si pas de tombstone ou état ancien
    /// sans horodatage).
    pub fn tombstone_time(&self, id: &TripletId) -> Option<&Timestamp> {
        self.tombstone_times.get(id)
    }

    /// Ramasse-miettes : oublie les tombstones (et leurs éléments associés)
    /// strictement plus anciennes que `cutoff`, rend le nombre d'entrées
    /// purgées. Les tombstones sans horodatage (états pré-LWW) sont
    /// conservées par prudence (jamais purgées aveuglément).
    pub fn gc(&mut self, cutoff: Timestamp) -> usize {
        let old: Vec<TripletId> = self
            .tombstone_times
            .iter()
            .filter(|(_, t)| t.0 < cutoff.0)
            .map(|(id, _)| id.clone())
            .collect();
        let n = old.len();
        for id in old {
            self.tombstones.remove(&id);
            self.tombstone_times.remove(&id);
            self.elements.remove(&id);
        }
        n
    }

    /// Variante pratique : `max_age` (ex. `chrono::Duration::days(30)`).
    pub fn gc_older_than(&mut self, max_age: chrono::Duration) -> usize {
        let cutoff = Timestamp(chrono::Utc::now() - max_age);
        self.gc(cutoff)
    }
}

impl Default for CrdtSync {
    fn default() -> Self {
        Self::new()
    }
}

/// Pont graphe ↔ CRDT **sans réseau** (conversion locale uniquement, mémoire
/// bout-en-bout : `Triplet` <-> élément CRDT).
///
/// - [`knowledge_graph_to_crdt`] : photographie tous les triplets du graphe
///   RAM dans un `CrdtSync` frais via `insert_triplet` (avec le `node_id`
///   donné, horloge vectorielle avancée). Le graphe n'a pas de tombstones :
///   seules les présences sont converties, rien n'est supprimé.
/// - [`apply_crdt_to_graph`] : rejoue les triplets **actifs** du CRDT dans le
///   graphe via `KnowledgeGraph::insert` (routage contradiction natif : un
///   `NeedsUserResolution` est sauté sans écraser, l'existant est conservé).
///   Rend le nombre de triplets appliqués (les doublons idempotents comptent
///   comme appliqués car déjà présents ; les conflits `NeedsUserResolution`
///   ne comptent pas).
///
/// Aucun I/O, aucun socket : la synchro réseau reste hors périmètre (voir
/// `docs/MEMORY_END_TO_END.md`, limites honnêtes).
pub fn knowledge_graph_to_crdt(graph: &KnowledgeGraph, node_id: &str) -> CrdtSync {
    let mut crdt = CrdtSync::new();
    for t in graph.all_triplets() {
        crdt.insert_triplet(t.clone(), node_id);
    }
    crdt
}

pub fn apply_crdt_to_graph(graph: &mut KnowledgeGraph, crdt: &CrdtSync) -> usize {
    let mut applied = 0;
    for t in crdt.active_triplets() {
        match graph.insert(t.clone()) {
            hakille_memory::InsertResult::NeedsUserResolution { .. } => {
                // Jamais d'écrasement silencieux : conflit sauté, existant conservé.
            }
            _ => {
                applied += 1;
            }
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{NodeId, PredicateId, TrustLevel};
    use hakille_memory::ContextPartition;
    use hakille_memory::triplet::{NodeValue, TripletMetadata, TripletSource};
    use uuid::Uuid;

    fn make_test_triplet(text: &str) -> Triplet {
        Triplet {
            id: TripletId(Uuid::now_v7()),
            subject: NodeId(Uuid::now_v7()),
            predicate: PredicateId(Uuid::now_v7()),
            object: NodeValue::Text(text.to_string()),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    fn triplet_with_id_and_time(
        id: TripletId,
        text: &str,
        when: chrono::DateTime<chrono::Utc>,
    ) -> Triplet {
        Triplet {
            id,
            subject: NodeId(Uuid::now_v7()),
            predicate: PredicateId(Uuid::now_v7()),
            object: NodeValue::Text(text.to_string()),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp(when),
                last_activated: Timestamp(when),
                source: TripletSource::User,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    #[test]
    fn test_crdt_tombstone_precedence_and_idempotence() {
        let mut node_a = CrdtSync::new();
        let mut node_b = CrdtSync::new();

        let t1 = make_test_triplet("User lives in Paris");
        let t2 = make_test_triplet("User works on Rust");

        // Node A inserts t1 and t2
        node_a.insert_triplet(t1.clone(), "node-a");
        node_a.insert_triplet(t2.clone(), "node-a");

        // Sync A -> B
        node_b.merge(&node_a);
        assert_eq!(node_b.active_triplets().len(), 2);

        // Node B deletes t1 offline
        node_b.remove_triplet(t1.id.clone(), "node-b");
        assert_eq!(node_b.active_triplets().len(), 1);

        // Node A merges B: t1 must be deleted on A too (tombstone wins)
        node_a.merge(&node_b);
        assert_eq!(node_a.active_triplets().len(), 1);
        assert_eq!(node_a.active_triplets()[0].id, t2.id);

        // Idempotence: merging multiple times changes nothing
        let before_merge = node_a.clone();
        node_a.merge(&before_merge);
        assert_eq!(node_a.active_triplets().len(), 1);
        assert_eq!(node_a.vector_clock, before_merge.vector_clock);
    }

    #[test]
    fn test_crdt_commutativity_and_associativity() {
        let mut a = CrdtSync::new();
        let mut b = CrdtSync::new();

        let t1 = make_test_triplet("Fact A");
        let t2 = make_test_triplet("Fact B");

        a.insert_triplet(t1, "node-a");
        b.insert_triplet(t2, "node-b");

        let mut a_then_b = a.clone();
        a_then_b.merge(&b);

        let mut b_then_a = b.clone();
        b_then_a.merge(&a);

        // Both nodes must have the exact same active triplets count and vector clock
        assert_eq!(
            a_then_b.active_triplets().len(),
            b_then_a.active_triplets().len()
        );
        assert_eq!(a_then_b.vector_clock, b_then_a.vector_clock);
    }

    #[test]
    fn test_crdt_lww_keeps_most_recent() {
        let id = TripletId(Uuid::now_v7());
        let old_time = chrono::Utc::now() - chrono::Duration::hours(1);
        let new_time = chrono::Utc::now();
        let old_t = triplet_with_id_and_time(id.clone(), "ancien", old_time);
        let new_t = triplet_with_id_and_time(id.clone(), "récent", new_time);

        let mut a = CrdtSync::new();
        let mut b = CrdtSync::new();
        a.insert_triplet(old_t, "node-a");
        b.insert_triplet(new_t, "node-b");

        // A merge B ⇒ le plus récent gagne.
        a.merge(&b);
        let kept = a.elements.get(&id).unwrap();
        match &kept.object {
            NodeValue::Text(s) => assert_eq!(s, "récent"),
            _ => panic!("attendu du texte"),
        }

        // Ordre inverse ⇒ même résultat (LWW commutatif ici).
        let mut c = CrdtSync::new();
        let mut d = CrdtSync::new();
        c.insert_triplet(
            triplet_with_id_and_time(id.clone(), "ancien", old_time),
            "node-a",
        );
        d.insert_triplet(
            triplet_with_id_and_time(id.clone(), "récent", new_time),
            "node-b",
        );
        d.merge(&c);
        let kept2 = d.elements.get(&id).unwrap();
        match &kept2.object {
            NodeValue::Text(s) => assert_eq!(s, "récent"),
            _ => panic!("attendu du texte"),
        }
    }

    #[test]
    fn test_crdt_lww_tie_first_wins() {
        let id = TripletId(Uuid::now_v7());
        let when = chrono::Utc::now();
        let mut a = CrdtSync::new();
        let mut b = CrdtSync::new();
        a.insert_triplet(
            triplet_with_id_and_time(id.clone(), "local", when),
            "node-a",
        );
        b.insert_triplet(
            triplet_with_id_and_time(id.clone(), "distant", when),
            "node-b",
        );
        // Égalité stricte ⇒ first-wins : le local est conservé.
        a.merge(&b);
        match &a.elements.get(&id).unwrap().object {
            NodeValue::Text(s) => assert_eq!(s, "local"),
            _ => panic!("attendu du texte"),
        }
    }

    #[test]
    fn test_crdt_gc_purges_old_tombstones() {
        let mut node = CrdtSync::new();
        let t = make_test_triplet("éphémère");
        let id = t.id.clone();
        node.insert_triplet(t, "node-a");
        node.remove_triplet(id.clone(), "node-a");
        assert!(node.tombstones.contains(&id));

        // Tombstone fraîche ⇒ GC futur ne purge rien.
        let future_cutoff = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        assert_eq!(node.gc(future_cutoff), 0);
        assert!(node.tombstones.contains(&id));

        // Cutoff postérieur ⇒ purge.
        let past_cutoff = Timestamp(chrono::Utc::now() + chrono::Duration::seconds(1));
        assert_eq!(node.gc(past_cutoff), 1);
        assert!(!node.tombstones.contains(&id));
    }

    #[test]
    fn test_graph_crdt_bridge_roundtrip_no_network() {
        use hakille_memory::KnowledgeGraph;
        let mut graph = KnowledgeGraph::new();
        let s = graph.resolve_subject("Marie");
        let p = graph.resolve_predicate("aime");
        let object = NodeValue::Text("jazz".to_string());
        // `Triplet::new` (pas de littéral struct) : insensible aux champs
        // ajoutés par les autres agents (ex. `node_type`).
        let t = Triplet::new(
            KnowledgeGraph::deterministic_triplet_id(&s, &p, &object),
            s,
            p,
            object,
            TripletMetadata {
                weight: 1.0,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
        )
        .expect("métadonnées de test valides");
        graph.insert(t);
        let crdt = super::knowledge_graph_to_crdt(&graph, "node-test");
        assert_eq!(crdt.active_triplets().len(), 1);

        let mut graph2 = KnowledgeGraph::new();
        let applied = super::apply_crdt_to_graph(&mut graph2, &crdt);
        assert_eq!(applied, 1);
        assert_eq!(graph2.len(), 1);
    }
}
