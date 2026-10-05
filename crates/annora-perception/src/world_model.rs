use annora_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SituationalState {
    pub location_context: String,
    pub present_speakers: HashSet<String>,
    pub active_application: Option<String>,
    pub ambient_noise_level_db: f32,
    pub last_updated: Timestamp,
}

pub struct WorldModel {
    current_state: SituationalState,
}

impl WorldModel {
    pub fn new(initial_location: impl Into<String>) -> Self {
        Self {
            current_state: SituationalState {
                location_context: initial_location.into(),
                present_speakers: HashSet::new(),
                active_application: None,
                ambient_noise_level_db: 35.0,
                last_updated: Timestamp::now(),
            },
        }
    }

    pub fn state(&self) -> &SituationalState {
        &self.current_state
    }

    pub fn update_active_application(&mut self, app_name: impl Into<String>) {
        self.current_state.active_application = Some(app_name.into());
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn record_speaker_presence(&mut self, speaker_id: impl Into<String>) {
        self.current_state.present_speakers.insert(speaker_id.into());
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn clear_absent_speakers(&mut self) {
        self.current_state.present_speakers.clear();
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn update_noise_level(&mut self, db: f32) {
        self.current_state.ambient_noise_level_db = db;
        self.current_state.last_updated = Timestamp::now();
    }
}
