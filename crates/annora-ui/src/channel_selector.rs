use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    SmartSpeaker,
    Wearable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UrgencyLevel {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputChannel {
    /// Full-duplex natural vocal synthesis (default hands-free)
    VoiceNormal,
    /// Whispered low-amplitude voice synthesis (quiet rooms / late night)
    VoiceWhisper,
    /// Spotlight-like single-line ephemeral visual bar (meetings / public / focused desktop)
    EphemeralHud,
    /// Discreet tactile vibration pattern (wearable / silent notifications)
    Haptic,
    /// Subtle visual pulse on the ambient system dot without textual or auditory interruption
    AmbientPulse,
    /// Suppress and defer output until user changes context or looks at device
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HapticPattern {
    ShortPulse,
    DoubleTap,
    SubtleVibe,
    UrgentBuzz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionContext {
    pub ambient_noise_db: f32,
    pub third_parties_present: bool,
    pub in_meeting: bool,
    pub user_attentive_to_screen: bool,
    pub user_wearing_headphones: bool,
    pub device_type: DeviceType,
    pub urgency: UrgencyLevel,
    pub is_night_time: bool,
}

impl Default for InteractionContext {
    fn default() -> Self {
        Self {
            ambient_noise_db: 38.0,
            third_parties_present: false,
            in_meeting: false,
            user_attentive_to_screen: true,
            user_wearing_headphones: false,
            device_type: DeviceType::Desktop,
            urgency: UrgencyLevel::Normal,
            is_night_time: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDecision {
    pub channel: OutputChannel,
    pub rationale: String,
    pub haptic_feedback: Option<HapticPattern>,
}

pub struct ChannelSelector;

impl ChannelSelector {
    /// Arbitrate the optimal output channel in real-time according to Section 21a
    /// "Le Sélecteur de Canal dynamique — la voix n'est pas toujours la bonne réponse"
    pub fn arbitrate(ctx: &InteractionContext) -> ChannelDecision {
        // Rule 1: Critical urgency overrides standard courtesy
        if ctx.urgency == UrgencyLevel::Critical {
            if ctx.in_meeting || ctx.third_parties_present {
                return ChannelDecision {
                    channel: OutputChannel::EphemeralHud,
                    rationale: "Critical alert delivered via Ephemeral HUD to prevent public audio leak during meeting".to_string(),
                    haptic_feedback: Some(HapticPattern::UrgentBuzz),
                };
            } else {
                return ChannelDecision {
                    channel: OutputChannel::VoiceNormal,
                    rationale: "Critical alert spoken aloud with immediate priority".to_string(),
                    haptic_feedback: Some(HapticPattern::UrgentBuzz),
                };
            }
        }

        // Rule 2: In a meeting -> never speak aloud without private headphones
        if ctx.in_meeting {
            if ctx.user_wearing_headphones && ctx.urgency >= UrgencyLevel::High {
                return ChannelDecision {
                    channel: OutputChannel::VoiceWhisper,
                    rationale: "High urgency delivered softly to private headphones during meeting".to_string(),
                    haptic_feedback: Some(HapticPattern::DoubleTap),
                };
            } else if ctx.device_type == DeviceType::Wearable {
                return ChannelDecision {
                    channel: OutputChannel::Haptic,
                    rationale: "In-meeting discreet haptic notification on wearable".to_string(),
                    haptic_feedback: Some(HapticPattern::ShortPulse),
                };
            } else {
                return ChannelDecision {
                    channel: OutputChannel::EphemeralHud,
                    rationale: "In meeting: silent ephemeral single-line display".to_string(),
                    haptic_feedback: None,
                };
            }
        }

        // Rule 3: Third parties present (crowd, family, coworkers) -> protect privacy
        if ctx.third_parties_present {
            if ctx.user_wearing_headphones {
                return ChannelDecision {
                    channel: OutputChannel::VoiceNormal,
                    rationale: "Third parties present, but private headphones protect audio privacy".to_string(),
                    haptic_feedback: None,
                };
            } else if ctx.device_type == DeviceType::Wearable {
                return ChannelDecision {
                    channel: OutputChannel::Haptic,
                    rationale: "Third parties present: silent haptic delivery on wearable".to_string(),
                    haptic_feedback: Some(HapticPattern::DoubleTap),
                };
            } else {
                return ChannelDecision {
                    channel: OutputChannel::EphemeralHud,
                    rationale: "Third parties present: silent ephemeral HUD to preserve confidentiality".to_string(),
                    haptic_feedback: None,
                };
            }
        }

        // Rule 4: Ambient noise too high (> 72 dB) -> speech would be unintelligible
        if ctx.ambient_noise_db > 72.0 {
            if ctx.user_wearing_headphones {
                return ChannelDecision {
                    channel: OutputChannel::VoiceNormal,
                    rationale: "High ambient noise compensated by private headphones".to_string(),
                    haptic_feedback: None,
                };
            } else {
                return ChannelDecision {
                    channel: OutputChannel::EphemeralHud,
                    rationale: "High ambient noise (>72dB): switched to visual Ephemeral HUD".to_string(),
                    haptic_feedback: Some(HapticPattern::DoubleTap),
                };
            }
        }

        // Rule 5: Low urgency background completion
        if ctx.urgency == UrgencyLevel::Low {
            if ctx.device_type == DeviceType::Wearable {
                return ChannelDecision {
                    channel: OutputChannel::Haptic,
                    rationale: "Low-urgency task completed: micro-haptic confirmation".to_string(),
                    haptic_feedback: Some(HapticPattern::SubtleVibe),
                };
            } else {
                return ChannelDecision {
                    channel: OutputChannel::AmbientPulse,
                    rationale: "Low-urgency task: gentle pulse on ambient status indicator".to_string(),
                    haptic_feedback: None,
                };
            }
        }

        // Rule 6: Night time or ultra-quiet environment (< 32 dB)
        if ctx.is_night_time || ctx.ambient_noise_db < 32.0 {
            if !ctx.user_attentive_to_screen && ctx.device_type == DeviceType::SmartSpeaker {
                return ChannelDecision {
                    channel: OutputChannel::VoiceWhisper,
                    rationale: "Quiet night environment: soft acoustic whisper".to_string(),
                    haptic_feedback: None,
                };
            } else if ctx.user_attentive_to_screen {
                return ChannelDecision {
                    channel: OutputChannel::EphemeralHud,
                    rationale: "Quiet night environment: silent ephemeral display".to_string(),
                    haptic_feedback: None,
                };
            }
        }

        // Rule 7: User attentively looking at screen on desktop/laptop -> Ephemeral HUD is fastest
        if ctx.user_attentive_to_screen && (ctx.device_type == DeviceType::Desktop || ctx.device_type == DeviceType::Laptop) {
            return ChannelDecision {
                channel: OutputChannel::EphemeralHud,
                rationale: "User is actively engaged with screen: Spotlight-like ephemeral HUD".to_string(),
                haptic_feedback: None,
            };
        }

        // Rule 8: Smart speaker or hands-free default -> Voice
        ChannelDecision {
            channel: OutputChannel::VoiceNormal,
            rationale: "Private ambient environment: natural voice synthesis".to_string(),
            haptic_feedback: None,
        }
    }
}
