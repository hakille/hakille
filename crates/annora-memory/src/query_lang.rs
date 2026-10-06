use crate::context_partition::ContextPartition;
use crate::graph::KnowledgeGraph;
use crate::triplet::{NodeValue, Triplet};
use annora_core::{NodeId, PredicateId};
use serde::{Deserialize, Serialize};

/// Filter criteria for object values in a declarative graph query (§31b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectFilter {
    Any,
    ExactEntity(NodeId),
    ContainsText(String),
    NumericRange { min: f64, max: f64 },
    Boolean(bool),
}

/// Declarative Graph Query specification (§31b).
/// Enables typed traversal and filtering without ad-hoc manual graph loops.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    pub subject: Option<NodeId>,
    pub predicate: Option<PredicateId>,
    pub object_filter: ObjectFilter,
    pub min_weight: f64,
    pub min_confidence: f64,
    pub partition: Option<ContextPartition>,
    pub limit: usize,
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self {
            subject: None,
            predicate: None,
            object_filter: ObjectFilter::Any,
            min_weight: 0.0,
            min_confidence: 0.0,
            partition: None,
            limit: 100,
        }
    }
}

impl GraphQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_subject(mut self, subject: NodeId) -> Self {
        self.subject = Some(subject);
        self
    }

    pub fn with_predicate(mut self, predicate: PredicateId) -> Self {
        self.predicate = Some(predicate);
        self
    }

    pub fn with_text_filter(mut self, substring: impl Into<String>) -> Self {
        self.object_filter = ObjectFilter::ContainsText(substring.into());
        self
    }

    pub fn with_min_weight(mut self, min_weight: f64) -> Self {
        self.min_weight = min_weight;
        self
    }

    pub fn with_min_confidence(mut self, min_confidence: f64) -> Self {
        self.min_confidence = min_confidence;
        self
    }

    pub fn with_partition(mut self, partition: ContextPartition) -> Self {
        self.partition = Some(partition);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Execute declarative query against the knowledge graph.
    pub fn execute<'a>(&self, graph: &'a KnowledgeGraph) -> Vec<&'a Triplet> {
        let all_triplets = graph.all_triplets();
        let mut results = Vec::new();

        for triplet in all_triplets {
            // Subject filter
            if let Some(ref subj) = self.subject {
                if triplet.subject != *subj {
                    continue;
                }
            }

            // Predicate filter
            if let Some(ref pred) = self.predicate {
                if triplet.predicate != *pred {
                    continue;
                }
            }

            // Object filter
            match &self.object_filter {
                ObjectFilter::Any => {}
                ObjectFilter::ExactEntity(id) => match &triplet.object {
                    NodeValue::Entity(e) if e == id => {}
                    _ => continue,
                },
                ObjectFilter::ContainsText(query) => match &triplet.object {
                    NodeValue::Text(s) if s.to_lowercase().contains(&query.to_lowercase()) => {}
                    _ => continue,
                },
                ObjectFilter::NumericRange { min, max } => match &triplet.object {
                    NodeValue::Number(n) if n >= min && n <= max => {}
                    _ => continue,
                },
                ObjectFilter::Boolean(expected) => match &triplet.object {
                    NodeValue::Boolean(b) if b == expected => {}
                    _ => continue,
                },
            }

            // Weight filter (temporal decay)
            if triplet.metadata.weight < self.min_weight {
                continue;
            }

            // Confidence filter
            if triplet.metadata.confidence < self.min_confidence {
                continue;
            }

            // Partition filter
            if let Some(ref part) = self.partition {
                if triplet.metadata.context != *part {
                    continue;
                }
            }

            results.push(triplet);
            if results.len() >= self.limit {
                break;
            }
        }

        results
    }
}

/// Query Builder DSL for fluent syntax (§31b).
pub struct GraphQueryBuilder {
    inner: GraphQuery,
}

impl GraphQueryBuilder {
    pub fn select() -> Self {
        Self {
            inner: GraphQuery::new(),
        }
    }

    pub fn where_subject(mut self, subject: NodeId) -> Self {
        self.inner.subject = Some(subject);
        self
    }

    pub fn where_predicate(mut self, predicate: PredicateId) -> Self {
        self.inner.predicate = Some(predicate);
        self
    }

    pub fn where_object_contains(mut self, text: impl Into<String>) -> Self {
        self.inner.object_filter = ObjectFilter::ContainsText(text.into());
        self
    }

    pub fn where_weight_above(mut self, weight: f64) -> Self {
        self.inner.min_weight = weight;
        self
    }

    pub fn where_confidence_above(mut self, confidence: f64) -> Self {
        self.inner.min_confidence = confidence;
        self
    }

    pub fn where_partition(mut self, partition: ContextPartition) -> Self {
        self.inner.partition = Some(partition);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.inner.limit = limit;
        self
    }

    pub fn build(self) -> GraphQuery {
        self.inner
    }
}
