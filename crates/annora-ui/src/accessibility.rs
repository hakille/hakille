use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryModality {
    VoiceFirst,
    TextFirst,
    VisualOnly,
    HapticAssisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityProfile {
    pub primary_modality: PrimaryModality,
    pub screen_reader_friendly: bool,
    pub high_contrast: bool,
    pub subtitles_enabled: bool,
    pub speech_rate_factor: f32,
}

impl Default for AccessibilityProfile {
    fn default() -> Self {
        Self {
            primary_modality: PrimaryModality::VoiceFirst,
            screen_reader_friendly: false,
            high_contrast: false,
            subtitles_enabled: true,
            speech_rate_factor: 1.0,
        }
    }
}

pub struct AccessibilityManager {
    profile: AccessibilityProfile,
}

impl AccessibilityManager {
    pub fn new(profile: AccessibilityProfile) -> Self {
        Self { profile }
    }

    pub fn profile(&self) -> &AccessibilityProfile {
        &self.profile
    }

    pub fn set_modality(&mut self, modality: PrimaryModality) {
        self.profile.primary_modality = modality;
    }

    pub fn set_subtitles(&mut self, enabled: bool) {
        self.profile.subtitles_enabled = enabled;
    }
}

impl Default for AccessibilityManager {
    fn default() -> Self {
        Self::new(AccessibilityProfile::default())
    }
}
