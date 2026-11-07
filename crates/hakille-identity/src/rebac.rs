//! Zanzibar-style Relationship-Based Access Control (ReBAC) Engine (F09).
//! Relation tuples: <entity_type>:<entity_id>#<relation>@<subject_type>:<subject_id>

use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RelationTuple {
    pub entity_type: String,
    pub entity_id: String,
    pub relation: String,
    pub subject_type: String,
    pub subject_id: String,
}

impl std::fmt::Display for RelationTuple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}#{}@{}:{}",
            self.entity_type, self.entity_id, self.relation, self.subject_type, self.subject_id
        )
    }
}

impl RelationTuple {
    pub fn new(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        relation: impl Into<String>,
        subject_type: impl Into<String>,
        subject_id: impl Into<String>,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            relation: relation.into(),
            subject_type: subject_type.into(),
            subject_id: subject_id.into(),
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        // e.g. "team:alpha#member@user:alice"
        let (entity_part, rest) = s.split_once('#').ok_or("Missing '#' separator")?;
        let (relation, subject_part) = rest.split_once('@').ok_or("Missing '@' separator")?;
        let (e_type, e_id) = entity_part.split_once(':').ok_or("Missing entity ':' separator")?;
        let (s_type, s_id) = subject_part.split_once(':').ok_or("Missing subject ':' separator")?;

        Ok(Self::new(e_type, e_id, relation, s_type, s_id))
    }
}

#[derive(Default, Debug, Clone)]
pub struct RebacEngine {
    tuples: HashSet<RelationTuple>,
}

impl RebacEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_tuple(&mut self, tuple: RelationTuple) -> bool {
        self.tuples.insert(tuple)
    }

    pub fn remove_tuple(&mut self, tuple: &RelationTuple) -> bool {
        self.tuples.remove(tuple)
    }

    pub fn tuple_count(&self) -> usize {
        self.tuples.len()
    }

    pub fn tuples(&self) -> &HashSet<RelationTuple> {
        &self.tuples
    }

    pub fn clear(&mut self) {
        self.tuples.clear();
    }

    pub fn contains_tuple(&self, tuple: &RelationTuple) -> bool {
        self.tuples.contains(tuple)
    }

    pub fn find_relations_for_subject(
        &self,
        subject_type: &str,
        subject_id: &str,
    ) -> Vec<RelationTuple> {
        self.tuples
            .iter()
            .filter(|t| t.subject_type == subject_type && t.subject_id == subject_id)
            .cloned()
            .collect()
    }

    pub fn find_relations_for_entity(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Vec<RelationTuple> {
        self.tuples
            .iter()
            .filter(|t| t.entity_type == entity_type && t.entity_id == entity_id)
            .cloned()
            .collect()
    }

    pub fn has_direct_relation(
        &self,
        entity_type: &str,
        entity_id: &str,
        relation: &str,
        subject_type: &str,
        subject_id: &str,
    ) -> bool {
        let needle = RelationTuple::new(
            entity_type,
            entity_id,
            relation,
            subject_type,
            subject_id,
        );
        self.tuples.contains(&needle)
    }

    /// Check relation with transitive traversal (downward hierarchical containment).
    /// Prevents cyclic loops via visited set with maximum depth bounding (D <= 32).
    pub fn check_relation_transitive(
        &self,
        target_entity_type: &str,
        target_entity_id: &str,
        target_relation: &str,
        subject_type: &str,
        subject_id: &str,
    ) -> bool {
        self.check_relation_transitive_bounded(
            target_entity_type,
            target_entity_id,
            target_relation,
            subject_type,
            subject_id,
            32,
        )
    }

    /// Bounded transitive relation traversal with cycle prevention.
    pub fn check_relation_transitive_bounded(
        &self,
        target_entity_type: &str,
        target_entity_id: &str,
        target_relation: &str,
        subject_type: &str,
        subject_id: &str,
        max_depth: usize,
    ) -> bool {
        // 1. Direct match check
        if self.has_direct_relation(
            target_entity_type,
            target_entity_id,
            target_relation,
            subject_type,
            subject_id,
        ) {
            return true;
        }

        // 2. Transitive graph traversal with cycle detection and depth limit
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((
            target_entity_type.to_string(),
            target_entity_id.to_string(),
            0usize,
        ));
        visited.insert((target_entity_type.to_string(), target_entity_id.to_string()));

        while let Some((curr_type, curr_id, depth)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }

            // Check if curr object grants relation directly to subject
            if self.has_direct_relation(
                &curr_type,
                &curr_id,
                target_relation,
                subject_type,
                subject_id,
            ) {
                return true;
            }

            // Check userset indirection: curr_type:curr_id#relation@userset_type:userset_id
            for t in &self.tuples {
                if t.entity_type == curr_type
                    && t.entity_id == curr_id
                    && t.relation == target_relation
                {
                    // Check if subject is member of the userset
                    if self.has_direct_relation(
                        &t.subject_type,
                        &t.subject_id,
                        "member",
                        subject_type,
                        subject_id,
                    ) {
                        return true;
                    }

                    if depth < max_depth {
                        let userset = (t.subject_type.clone(), t.subject_id.clone());
                        if !visited.contains(&userset) {
                            visited.insert(userset.clone());
                            queue.push_back((userset.0, userset.1, depth + 1));
                        }
                    }
                }
            }

            // Find child entities (e.g. parent_of relation): curr_type:curr_id#parent_of@child_type:child_id
            if depth < max_depth {
                for t in &self.tuples {
                    if t.entity_type == curr_type
                        && t.entity_id == curr_id
                        && (t.relation == "parent_of" || t.relation == "contains")
                    {
                        let next = (t.subject_type.clone(), t.subject_id.clone());
                        if !visited.contains(&next) {
                            visited.insert(next.clone());
                            queue.push_back((next.0, next.1, depth + 1));
                        }
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebac_query_helpers() {
        let mut rebac = RebacEngine::new();
        let t1 = RelationTuple::new("team", "alpha", "member", "user", "alice");
        let t2 = RelationTuple::new("team", "alpha", "lead", "user", "alice");
        let t3 = RelationTuple::new("team", "beta", "member", "user", "bob");

        rebac.add_tuple(t1.clone());
        rebac.add_tuple(t2);
        rebac.add_tuple(t3);

        assert_eq!(rebac.tuple_count(), 3);
        assert!(rebac.contains_tuple(&t1));

        let alice_rels = rebac.find_relations_for_subject("user", "alice");
        assert_eq!(alice_rels.len(), 2);

        let alpha_rels = rebac.find_relations_for_entity("team", "alpha");
        assert_eq!(alpha_rels.len(), 2);

        rebac.clear();
        assert_eq!(rebac.tuple_count(), 0);
    }

    #[test]
    fn test_rebac_parse_display_roundtrip() {
        let raw = "team:alpha#member@user:alice";
        let parsed = RelationTuple::parse(raw).expect("parse succeeds");
        assert_eq!(parsed.to_string(), raw);
    }

    #[test]
    fn test_rebac_depth_limit_enforced() {
        let mut rebac = RebacEngine::new();
        // Chain of 5 nodes: 1 -> 2 -> 3 -> 4 -> 5 -> leaf
        for i in 1..5 {
            rebac.add_tuple(RelationTuple::new("node", format!("n{}", i), "parent_of", "node", format!("n{}", i + 1)));
        }
        rebac.add_tuple(RelationTuple::new("node", "n5", "viewer", "user", "leaf"));

        // With depth limit 3, cannot reach leaf from n1
        assert!(!rebac.check_relation_transitive_bounded("node", "n1", "viewer", "user", "leaf", 3));
        // With depth limit 5, reaches leaf
        assert!(rebac.check_relation_transitive_bounded("node", "n1", "viewer", "user", "leaf", 5));
    }
}
