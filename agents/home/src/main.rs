use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SmartHomeCommand {
    pub room: String,
    pub device: String,
    pub command: String,
    pub value: serde_json::Value,
}

fn main() {
    println!("Annora Home Automation Agent initialized.");
}

pub fn check_home_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Network, CapabilityAction::Read),
        (CapabilityScope::Network, CapabilityAction::Write),
        (CapabilityScope::Sensor, CapabilityAction::Read),
    ]
}
