use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub location: Option<String>,
}

fn main() {
    println!("Annora Calendar Agent initialized with local graph synchronization.");
}

pub fn check_calendar_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Filesystem, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Write),
    ]
}
