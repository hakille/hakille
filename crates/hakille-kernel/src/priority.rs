use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PriorityLevel {
    SystemDefault = 0,
    GeneralUserPreference = 1,
    ExplicitUserInstruction = 2,
    Constitution = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    pub level: PriorityLevel,
    pub source: String,
    pub rule_content: String,
}

pub struct PriorityEngine;

impl Default for PriorityEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PriorityEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, directive_a: &Directive, directive_b: &Directive) -> Directive {
        if directive_a.level >= directive_b.level {
            directive_a.clone()
        } else {
            directive_b.clone()
        }
    }

    pub fn can_override(&self, caller_level: PriorityLevel, target_level: PriorityLevel) -> bool {
        caller_level > target_level
    }
}
