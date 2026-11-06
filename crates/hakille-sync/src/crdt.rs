//! Fusion locale remove-wins/LWW, sans transport ni garantie de causalité par élément.
//! L'ordre LWW choisit une valeur et ne conserve pas les versions perdantes. Les
//! horloges globales ne prouvent pas qu'un pair connaît chaque suppression : le
//! ramasse-miettes temporel est donc désactivé, et un ID supprimé n'est pas réutilisé.

use hakille_core::{HakilleError, Result, Timestamp, TripletId};
use hakille_memory::KnowledgeGraph;
use hakille_memory::triplet::{NodeValue, Triplet};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_CLOCK_NODES: usize = 1024;

fn sync_error(message: &str) -> HakilleError {
    HakilleError::Internal(message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VectorClock {
    pub clock: HashMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self::default()
    }
    fn validate(&self) -> Result<()> {
        if self.clock.len() > MAX_CLOCK_NODES
            || self
                .clock
                .keys()
                .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(sync_error(
                "Horloge vectorielle invalide ou trop volumineuse",
            ));
        }
        Ok(())
    }
    pub fn tick(&mut self, node_id: &str) -> Result<()> {
        self.validate()?;
        if node_id.is_empty()
            || node_id.len() > 256
            || node_id.chars().any(char::is_control)
            || (!self.clock.contains_key(node_id) && self.clock.len() >= MAX_CLOCK_NODES)
        {
            return Err(sync_error("Identité ou capacité d'horloge invalide"));
        }
        let next = self
            .get(node_id)
            .checked_add(1)
            .ok_or_else(|| sync_error("Compteur d'horloge vectorielle épuisé"))?;
        self.clock.insert(node_id.into(), next);
        Ok(())
    }
    pub fn merge(&mut self, other: &VectorClock) -> Result<()> {
        self.validate()?;
        other.validate()?;
        if self
            .clock
            .keys()
            .chain(other.clock.keys())
            .collect::<HashSet<_>>()
            .len()
            > MAX_CLOCK_NODES
        {
            return Err(sync_error("Fusion d'horloges trop volumineuse"));
        }
        for (node, &counter) in &other.clock {
            let local = self.clock.entry(node.clone()).or_insert(0);
            *local = (*local).max(counter);
        }
        Ok(())
    }
    pub fn get(&self, node_id: &str) -> u64 {
        self.clock.get(node_id).copied().unwrap_or(0)
    }
}

/// Clé canonique du schéma Triplet actuel : ordre des champs struct/enum fixe,
/// aucun dictionnaire dans Triplet. Les nombres non finis sont refusés avant
/// sérialisation (JSON les convertirait sinon en null et créerait des collisions).
pub(crate) fn canonical_triplet(triplet: &Triplet) -> Result<Vec<u8>> {
    triplet.validate()?;
    match &triplet.object {
        NodeValue::Number(value) if !value.is_finite() => {
            return Err(sync_error("Nombre de triplet non fini"));
        }
        NodeValue::Embedding(values) if values.iter().any(|value| !value.is_finite()) => {
            return Err(sync_error("Embedding de triplet non fini"));
        }
        _ => {}
    }
    serde_json::to_vec(triplet).map_err(|error| HakilleError::SerializationError(error.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtSync {
    pub elements: HashMap<TripletId, Triplet>,
    pub tombstones: HashSet<TripletId>,
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
    pub fn insert_triplet(&mut self, triplet: Triplet, node_id: &str) -> Result<()> {
        canonical_triplet(&triplet)?;
        if self.tombstones.contains(&triplet.id) {
            return Err(sync_error(
                "ID supprimé non réutilisable sans génération causale ; créer un nouvel ID",
            ));
        }
        self.vector_clock.tick(node_id)?;
        self.elements.insert(triplet.id.clone(), triplet);
        Ok(())
    }
    pub fn remove_triplet(&mut self, id: TripletId, node_id: &str) -> Result<()> {
        self.vector_clock.tick(node_id)?;
        if self.tombstones.insert(id.clone()) {
            self.tombstone_times.insert(id.clone(), Timestamp::now());
        }
        self.elements.remove(&id);
        Ok(())
    }

    fn canonical_elements(&self) -> Result<HashMap<TripletId, Vec<u8>>> {
        self.vector_clock.validate()?;
        if self
            .tombstone_times
            .keys()
            .any(|id| !self.tombstones.contains(id))
        {
            return Err(sync_error("Date de suppression sans tombstone"));
        }
        self.elements
            .iter()
            .map(|(id, triplet)| {
                if id != &triplet.id {
                    return Err(sync_error("Clé de triplet incohérente"));
                }
                Ok((id.clone(), canonical_triplet(triplet)?))
            })
            .collect()
    }

    /// Fusion atomique après validation : suppression prioritaire, puis maximum
    /// de (last_activated, octets canoniques du triplet entier). Une égalité de date
    /// ne dépend plus du côté local. Les dates de tombstone inconnues restent
    /// inconnues ; jamais d'horodatage inventé pendant une fusion.
    pub fn merge(&mut self, remote: &CrdtSync) -> Result<()> {
        let local_canonical = self.canonical_elements()?;
        let remote_canonical = remote.canonical_elements()?;
        let mut merged_clock = self.vector_clock.clone();
        merged_clock.merge(&remote.vector_clock)?;
        for (id, triplet) in &remote.elements {
            if self.tombstones.contains(id) || remote.tombstones.contains(id) {
                continue;
            }
            let replace = self.elements.get(id).is_none_or(|local| {
                (&triplet.metadata.last_activated, &remote_canonical[id])
                    > (&local.metadata.last_activated, &local_canonical[id])
            });
            if replace {
                self.elements.insert(id.clone(), triplet.clone());
            }
        }
        for id in &remote.tombstones {
            let existed = self.tombstones.contains(id);
            match (
                existed,
                self.tombstone_times.get(id),
                remote.tombstone_times.get(id),
            ) {
                (false, _, Some(time)) => {
                    self.tombstone_times.insert(id.clone(), time.clone());
                }
                (true, Some(local), Some(other)) => {
                    self.tombstone_times
                        .insert(id.clone(), local.min(other).clone());
                }
                _ => {
                    self.tombstone_times.remove(id);
                }
            }
            self.tombstones.insert(id.clone());
        }
        self.elements.retain(|id, _| !self.tombstones.contains(id));
        self.vector_clock = merged_clock;
        Ok(())
    }

    pub fn active_triplets(&self) -> Vec<&Triplet> {
        self.elements
            .iter()
            .filter(|(id, _)| !self.tombstones.contains(id))
            .map(|(_, value)| value)
            .collect()
    }
    pub fn tombstone_time(&self, id: &TripletId) -> Option<&Timestamp> {
        self.tombstone_times.get(id)
    }

    /// Purge désactivée : l'âge seul ne prouve pas la réception par les pairs
    /// hors ligne. Rend toujours zéro et conserve les suppressions anti-résurrection.
    /// Une compaction future exige un protocole de convergence, absent ici.
    pub fn gc(&mut self, _cutoff: Timestamp) -> usize {
        0
    }
    pub fn gc_older_than(&mut self, _max_age: chrono::Duration) -> usize {
        0
    }
}
impl Default for CrdtSync {
    fn default() -> Self {
        Self::new()
    }
}

/// Photographie locale du graphe, sans réseau. Le graphe ne fournit pas de
/// tombstones ; cette conversion ne représente donc que les présences.
pub fn knowledge_graph_to_crdt(graph: &KnowledgeGraph, node_id: &str) -> Result<CrdtSync> {
    let mut state = CrdtSync::new();
    for triplet in graph.all_triplets() {
        state.insert_triplet(triplet.clone(), node_id)?;
    }
    Ok(state)
}

/// Rejoue seulement les triplets actifs. Les suppressions ne sont pas propagées
/// au graphe par ce pont ; un conflit métier conserve l'existant sans l'écraser.
pub fn apply_crdt_to_graph(graph: &mut KnowledgeGraph, crdt: &CrdtSync) -> usize {
    let mut applied = 0;
    for triplet in crdt.active_triplets() {
        if !matches!(
            graph.insert(triplet.clone()),
            hakille_memory::InsertResult::NeedsUserResolution { .. }
        ) {
            applied += 1;
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
        node_a.insert_triplet(t1.clone(), "node-a").unwrap();
        node_a.insert_triplet(t2.clone(), "node-a").unwrap();

        // Sync A -> B
        node_b.merge(&node_a).unwrap();
        assert_eq!(node_b.active_triplets().len(), 2);

        // Node B deletes t1 offline
        node_b.remove_triplet(t1.id.clone(), "node-b").unwrap();
        assert_eq!(node_b.active_triplets().len(), 1);

        // Node A merges B: t1 must be deleted on A too (tombstone wins)
        node_a.merge(&node_b).unwrap();
        assert_eq!(node_a.active_triplets().len(), 1);
        assert_eq!(node_a.active_triplets()[0].id, t2.id);

        // Idempotence: merging multiple times changes nothing
        let before_merge = node_a.clone();
        node_a.merge(&before_merge).unwrap();
        assert_eq!(node_a.active_triplets().len(), 1);
        assert_eq!(node_a.vector_clock, before_merge.vector_clock);
    }

    #[test]
    fn test_crdt_commutativity_and_associativity() {
        let mut a = CrdtSync::new();
        let mut b = CrdtSync::new();

        let t1 = make_test_triplet("Fact A");
        let t2 = make_test_triplet("Fact B");

        a.insert_triplet(t1, "node-a").unwrap();
        b.insert_triplet(t2, "node-b").unwrap();

        let mut a_then_b = a.clone();
        a_then_b.merge(&b).unwrap();

        let mut b_then_a = b.clone();
        b_then_a.merge(&a).unwrap();

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
        a.insert_triplet(old_t, "node-a").unwrap();
        b.insert_triplet(new_t, "node-b").unwrap();

        // A merge B ⇒ le plus récent gagne.
        a.merge(&b).unwrap();
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
        )
        .unwrap();
        d.insert_triplet(
            triplet_with_id_and_time(id.clone(), "récent", new_time),
            "node-b",
        )
        .unwrap();
        d.merge(&c).unwrap();
        let kept2 = d.elements.get(&id).unwrap();
        match &kept2.object {
            NodeValue::Text(s) => assert_eq!(s, "récent"),
            _ => panic!("attendu du texte"),
        }
    }

    #[test]
    fn test_crdt_gc_retains_tombstones_without_convergence_proof() {
        let mut node = CrdtSync::new();
        let t = make_test_triplet("éphémère");
        let id = t.id.clone();
        node.insert_triplet(t, "node-a").unwrap();
        node.remove_triplet(id.clone(), "node-a").unwrap();
        assert!(node.tombstones.contains(&id));

        // Tombstone fraîche ⇒ GC futur ne purge rien.
        let future_cutoff = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        assert_eq!(node.gc(future_cutoff), 0);
        assert!(node.tombstones.contains(&id));

        // Cutoff postérieur ⇒ purge.
        let past_cutoff = Timestamp(chrono::Utc::now() + chrono::Duration::seconds(1));
        assert_eq!(node.gc(past_cutoff), 0);
        assert!(node.tombstones.contains(&id));
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
        let crdt = super::knowledge_graph_to_crdt(&graph, "node-test").unwrap();
        assert_eq!(crdt.active_triplets().len(), 1);

        let mut graph2 = KnowledgeGraph::new();
        let applied = super::apply_crdt_to_graph(&mut graph2, &crdt);
        assert_eq!(applied, 1);
        assert_eq!(graph2.len(), 1);
    }
    #[test]
    fn equal_timestamps_converge_for_all_permutations_and_groupings() {
        let base = make_test_triplet("alpha");
        let mut variants = [base.clone(), base.clone(), base];
        variants[1].object = NodeValue::Text("middle".into());
        variants[2].object = NodeValue::Text("zulu".into());
        let mut states = Vec::new();
        for (index, triplet) in variants.iter().enumerate() {
            let mut state = CrdtSync::new();
            state
                .insert_triplet(triplet.clone(), &format!("node-{index}"))
                .unwrap();
            states.push(state);
        }
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut left = states[order[0]].clone();
            left.merge(&states[order[1]]).unwrap();
            left.merge(&states[order[2]]).unwrap();
            let mut grouped = states[order[1]].clone();
            grouped.merge(&states[order[2]]).unwrap();
            let mut right = states[order[0]].clone();
            right.merge(&grouped).unwrap();
            assert_eq!(left.elements, right.elements);
            assert_eq!(left.vector_clock, right.vector_clock);
            assert_eq!(left.elements[&variants[0].id], variants[2]);
            let before = left.clone();
            left.merge(&before).unwrap();
            assert_eq!(left.elements, before.elements);
        }
    }

    #[test]
    fn overflow_and_nonfinite_remote_values_fail_atomically() {
        let mut state = CrdtSync::new();
        let triplet = make_test_triplet("kept");
        state.insert_triplet(triplet.clone(), "node").unwrap();
        state.vector_clock.clock.insert("node".into(), u64::MAX);
        let before = state.clone();
        assert!(state.vector_clock.tick("node").is_err());
        assert!(state.remove_triplet(triplet.id.clone(), "node").is_err());
        assert!(
            state
                .insert_triplet(make_test_triplet("new"), "node")
                .is_err()
        );
        assert_eq!(state.elements, before.elements);
        assert_eq!(state.vector_clock, before.vector_clock);
        assert!(state.tombstones.is_empty());
        for object in [
            NodeValue::Number(f64::NAN),
            NodeValue::Embedding(vec![f32::INFINITY]),
        ] {
            let mut invalid = triplet.clone();
            invalid.object = object;
            let mut remote = CrdtSync::new();
            remote.elements.insert(invalid.id.clone(), invalid);
            assert!(state.merge(&remote).is_err());
            assert_eq!(state.elements, before.elements);
            assert_eq!(state.vector_clock, before.vector_clock);
        }
    }

    #[test]
    fn unknown_tombstone_dates_and_offline_replays_do_not_resurrect() {
        let triplet = make_test_triplet("offline copy");
        let id = triplet.id.clone();
        let mut stale = CrdtSync::new();
        stale.insert_triplet(triplet.clone(), "offline").unwrap();
        let mut removed = stale.clone();
        removed.remove_triplet(id.clone(), "online").unwrap();
        let before = removed.vector_clock.clone();
        assert!(removed.insert_triplet(triplet, "online").is_err());
        assert_eq!(removed.vector_clock, before);
        assert_eq!(
            removed.gc(Timestamp(chrono::Utc::now() + chrono::Duration::days(3650))),
            0
        );
        removed.merge(&stale).unwrap();
        assert!(removed.active_triplets().is_empty());
        let mut legacy = CrdtSync::new();
        legacy.tombstones.insert(id.clone()); // Ancien snapshot sans date.
        let mut a = removed.clone();
        a.merge(&legacy).unwrap();
        let mut b = legacy;
        b.merge(&removed).unwrap();
        assert_eq!(a.tombstone_times, b.tombstone_times);
        assert!(a.tombstone_time(&id).is_none());
        assert!(a.active_triplets().is_empty() && b.active_triplets().is_empty());
    }
}
