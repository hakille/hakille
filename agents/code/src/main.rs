use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CodePatch {
    pub file_path: String,
    pub original_content: String,
    pub modified_content: String,
    pub explanation: String,
}

fn main() {
    println!("Annora Code Agent initialized for workspace refactoring.");
}

pub fn check_code_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Filesystem, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Write),
    ]
}
