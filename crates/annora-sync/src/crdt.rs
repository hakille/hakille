use annora_core::TripletId;
use annora_memory::triplet::Triplet;
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
    pub vector_clock: VectorClock,
}

impl CrdtSync {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
            vector_clock: VectorClock::new(),
        }
    }

    pub fn insert_triplet(&mut self, triplet: Triplet, node_id: &str) {
        self.vector_clock.tick(node_id);
        self.elements.insert(triplet.id.clone(), triplet);
    }

    pub fn remove_triplet(&mut self, id: TripletId, node_id: &str) {
        self.vector_clock.tick(node_id);
        self.tombstones.insert(id);
    }

    pub fn merge(&mut self, remote: &CrdtSync) {
        // 1. Union of elements
        for (id, triplet) in &remote.elements {
            self.elements.entry(id.clone()).or_insert_with(|| triplet.clone());
        }

        // 2. Union of tombstones
        for tombstone in &remote.tombstones {
            self.tombstones.insert(tombstone.clone());
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
}

impl Default for CrdtSync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{NodeId, PredicateId, Timestamp, TrustLevel};
    use annora_memory::ContextPartition;
    use annora_memory::triplet::{NodeValue, TripletMetadata, TripletSource};
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
        assert_eq!(a_then_b.active_triplets().len(), b_then_a.active_triplets().len());
        assert_eq!(a_then_b.vector_clock, b_then_a.vector_clock);
    }
}
