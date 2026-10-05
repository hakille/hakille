use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextPartition {
    Default,
    Professional,
    Personal,
    Shared,
    Custom(String),
}

pub struct ContextManager {
    pub active_partition: ContextPartition,
}

impl ContextManager {
    pub fn activate_partition(&mut self, partition: ContextPartition) {
        self.active_partition = partition;
    }
}
