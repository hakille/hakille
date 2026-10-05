use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BargeInEvent {
    pub triggered_at: Timestamp,
    pub interrupted_turn_id: String,
    pub cutoff_playback_seconds: f32,
    pub user_speech_energy: f32,
    pub remaining_unspoken_text: Option<String>,
}

pub struct BargeInDetector {
    is_system_speaking: bool,
    consecutive_speech_frames: usize,
    speech_threshold: f32,
    trigger_frame_count: usize,
    current_playback_time: f32,
    active_turn_id: Option<String>,
    full_turn_text: Option<String>,
}

impl BargeInDetector {
    pub fn new(speech_threshold: f32, trigger_frame_count: usize) -> Self {
        Self {
            is_system_speaking: false,
            consecutive_speech_frames: 0,
            speech_threshold,
            trigger_frame_count,
            current_playback_time: 0.0,
            active_turn_id: None,
            full_turn_text: None,
        }
    }

    pub fn start_system_speech(&mut self, turn_id: impl Into<String>, text: impl Into<String>) {
        self.is_system_speaking = true;
        self.consecutive_speech_frames = 0;
        self.current_playback_time = 0.0;
        self.active_turn_id = Some(turn_id.into());
        self.full_turn_text = Some(text.into());
    }

    pub fn stop_system_speech(&mut self) {
        self.is_system_speaking = false;
        self.consecutive_speech_frames = 0;
        self.active_turn_id = None;
        self.full_turn_text = None;
    }

    pub fn update_playback_time(&mut self, delta_seconds: f32) {
        if self.is_system_speaking {
            self.current_playback_time += delta_seconds;
        }
    }

    pub fn check_user_audio(&mut self, user_speech_probability: f32, energy_rms: f32) -> Option<BargeInEvent> {
        if !self.is_system_speaking {
            return None;
        }

        if user_speech_probability >= self.speech_threshold && energy_rms > 0.01 {
            self.consecutive_speech_frames += 1;
        } else {
            self.consecutive_speech_frames = 0;
        }

        if self.consecutive_speech_frames >= self.trigger_frame_count {
            let event = BargeInEvent {
                triggered_at: Timestamp::now(),
                interrupted_turn_id: self.active_turn_id.clone().unwrap_or_default(),
                cutoff_playback_seconds: self.current_playback_time,
                user_speech_energy: energy_rms,
                remaining_unspoken_text: self.full_turn_text.clone(),
            };
            self.stop_system_speech();
            Some(event)
        } else {
            None
        }
    }

    pub fn is_system_speaking(&self) -> bool {
        self.is_system_speaking
    }
}
