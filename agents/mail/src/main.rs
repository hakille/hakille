use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MailMessage {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}

fn main() {
    println!("Annora Mail Agent initialized with scoped capabilities.");
}

pub fn check_mail_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Network, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Read),
    ]
}
