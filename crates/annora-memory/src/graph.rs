use crate::triplet::{InsertResult, TriplePattern, Triplet};
use annora_core::{NodeId, TripletId};
use std::collections::BTreeMap;

pub struct KnowledgeGraph {
    triplets: BTreeMap<TripletId, Triplet>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            triplets: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, triplet: Triplet) -> InsertResult {
        let id = triplet.id.clone();
        self.triplets.insert(id.clone(), triplet);
        InsertResult::Inserted(id)
    }

    pub fn query(&self, pattern: &TriplePattern) -> Vec<&Triplet> {
        self.triplets.values().filter(|t| {
            let s_match = pattern.subject.as_ref().map_or(true, |s| s == &t.subject);
            let p_match = pattern.predicate.as_ref().map_or(true, |p| p == &t.predicate);
            let o_match = pattern.object.as_ref().map_or(true, |o| o == &t.object);
            s_match && p_match && o_match
        }).collect()
    }

    pub fn len(&self) -> usize {
        self.triplets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.triplets.is_empty()
    }

    pub fn all_triplets(&self) -> Vec<&Triplet> {
        self.triplets.values().collect()
    }

    pub fn search_text(&self, query: &str) -> Vec<&Triplet> {
        let q = query.to_lowercase();
        self.triplets
            .values()
            .filter(|t| match &t.object {
                crate::triplet::NodeValue::Text(s) => s.to_lowercase().contains(&q),
                crate::triplet::NodeValue::Entity(_) => false,
                crate::triplet::NodeValue::Number(n) => n.to_string().contains(&q),
                crate::triplet::NodeValue::Boolean(b) => b.to_string().contains(&q),
                _ => false,
            })
            .collect()
    }

    pub fn get(&self, id: &TripletId) -> Option<&Triplet> {
        self.triplets.get(id)
    }

    pub fn traverse_from(&self, node: &NodeId, max_depth: usize) -> Vec<&Triplet> {
        let mut visited_nodes = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut current_layer = vec![node.clone()];
        visited_nodes.insert(node.clone());

        for _ in 0..max_depth {
            if current_layer.is_empty() {
                break;
            }
            let mut next_layer = Vec::new();
            for current in &current_layer {
                for triplet in self.related(current) {
                    result.push(triplet);
                    if let crate::triplet::NodeValue::Entity(ref target) = triplet.object {
                        if visited_nodes.insert(target.clone()) {
                            next_layer.push(target.clone());
                        }
                    }
                }
            }
            current_layer = next_layer;
        }

        result
    }

    pub fn related(&self, node: &NodeId) -> Vec<&Triplet> {
        self.triplets.values().filter(|t| {
            &t.subject == node || match &t.object {
                crate::triplet::NodeValue::Entity(n) => n == node,
                _ => false,
            }
        }).collect()
    }

    pub fn activate(&mut self, id: &TripletId) {
        if let Some(t) = self.triplets.get_mut(id) {
            t.metadata.last_activated = annora_core::Timestamp::now();
            t.metadata.weight = (t.metadata.weight * 1.1).min(1.0);
        }
    }

    pub fn remove(&mut self, id: &TripletId) -> bool {
        self.triplets.remove(id).is_some()
    }

    pub fn contains(&self, id: &TripletId) -> bool {
        self.triplets.contains_key(id)
    }
}

