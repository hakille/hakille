use annora_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SituationalState {
    pub location_context: String,
    pub present_speakers: HashSet<String>,
    pub active_application: Option<String>,
    pub ambient_noise_level_db: f32,
    pub user_present: bool,
    pub user_attentive: bool,
    pub third_parties_present: bool,
    pub people_count: usize,
    pub lighting_condition: String,
    pub user_distance_m: f32,
    pub in_meeting: bool,
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
                user_present: true,
                user_attentive: true,
                third_parties_present: false,
                people_count: 1,
                lighting_condition: "Daylight".to_string(),
                user_distance_m: 0.6,
                in_meeting: false,
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
        self.current_state
            .present_speakers
            .insert(speaker_id.into());
        if self.current_state.present_speakers.len() > 1 {
            self.current_state.third_parties_present = true;
        }
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn clear_absent_speakers(&mut self) {
        self.current_state.present_speakers.clear();
        self.current_state.third_parties_present = self.current_state.people_count > 1;
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn update_noise_level(&mut self, db: f32) {
        self.current_state.ambient_noise_level_db = db;
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn update_visual_presence(
        &mut self,
        present: bool,
        attentive: bool,
        people_count: usize,
        distance_m: f32,
        lighting: impl Into<String>,
    ) {
        self.current_state.user_present = present;
        self.current_state.user_attentive = attentive;
        self.current_state.people_count = people_count;
        self.current_state.third_parties_present =
            people_count > 1 || self.current_state.present_speakers.len() > 1;
        self.current_state.user_distance_m = distance_m;
        self.current_state.lighting_condition = lighting.into();
        self.current_state.last_updated = Timestamp::now();
    }

    pub fn set_meeting_status(&mut self, in_meeting: bool) {
        self.current_state.in_meeting = in_meeting;
        self.current_state.last_updated = Timestamp::now();
    }

    /// Determines if the current environment is strictly private
    /// (user is present, no third parties detected via camera or diarization, not in meeting)
    pub fn is_private_environment(&self) -> bool {
        self.current_state.user_present
            && !self.current_state.third_parties_present
            && !self.current_state.in_meeting
            && self.current_state.present_speakers.len() <= 1
    }

    /// Determines whether audio output should be suppressed / whispered / moved to HUD
    pub fn should_suppress_loud_audio(&self) -> bool {
        self.current_state.in_meeting
            || self.current_state.third_parties_present
            || self.current_state.ambient_noise_level_db > 68.0
    }
}
