use annora_core::Timestamp;
use annora_substrate::{CameraConfig, CameraFrame};
use crate::world_model::WorldModel;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserPresence {
    /// User is present and actively focused in front of the device
    Present,
    /// Nobody detected in the field of view
    Absent,
    /// User is physically present but looking away / distracted
    Distracted,
    /// Multiple people detected in the field of view (triggers privacy protection)
    MultiplePeopleDetected { count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingCondition {
    Daylight,
    IndoorAdequate,
    Dim,
    Dark,
}

impl LightingCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daylight => "Daylight",
            Self::IndoorAdequate => "IndoorAdequate",
            Self::Dim => "Dim",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualSceneContext {
    pub user_presence: UserPresence,
    pub estimated_distance_m: f32,
    pub user_gaze_focused_on_device: bool,
    pub people_count: usize,
    pub lighting: LightingCondition,
    /// SHA-256 digest of frame data for cryptographic attestation.
    /// RAW PIXEL DATA NEVER LEAVES LOCAL PROCESSING MEMORY (§6).
    pub privacy_preserving_frame_hash: [u8; 32],
    pub timestamp: Timestamp,
}

pub struct CameraPipeline {
    config: CameraConfig,
    frames_processed: u64,
}

impl CameraPipeline {
    pub fn new(config: CameraConfig) -> Self {
        Self {
            config,
            frames_processed: 0,
        }
    }

    pub fn config(&self) -> &CameraConfig {
        &self.config
    }

    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Process a raw camera frame from Substrate.
    /// Guarantees that raw pixels are processed entirely in ephemeral RAM and discarded.
    pub fn process_frame(&mut self, frame: &CameraFrame) -> VisualSceneContext {
        self.frames_processed += 1;

        // 1. Cryptographic frame attestation hash
        let mut hasher = Sha256::new();
        hasher.update(&frame.data);
        let frame_hash: [u8; 32] = hasher.finalize().into();

        if frame.data.is_empty() || frame.width == 0 || frame.height == 0 {
            return VisualSceneContext {
                user_presence: UserPresence::Absent,
                estimated_distance_m: 0.0,
                user_gaze_focused_on_device: false,
                people_count: 0,
                lighting: LightingCondition::Dark,
                privacy_preserving_frame_hash: frame_hash,
                timestamp: Timestamp::now(),
            };
        }

        // 2. Luminance and lighting assessment
        let step = match frame.format {
            annora_substrate::ImageFormat::Rgb8 | annora_substrate::ImageFormat::Bgra8 => 3,
            annora_substrate::ImageFormat::Rgba8 => 4,
        };

        // Sample pixels sparsely for high performance and low thermal footprint
        let mut sum_lum = 0.0f64;
        let mut sample_count = 0usize;
        let mut min_val = 255u8;
        let mut max_val = 0u8;

        for chunk in frame.data.chunks(step * 16) {
            if chunk.len() >= 3 {
                // Perceived luminance formula: 0.299*R + 0.587*G + 0.114*B
                let lum = (0.299 * chunk[0] as f64) + (0.587 * chunk[1] as f64) + (0.114 * chunk[2] as f64);
                sum_lum += lum;
                sample_count += 1;
                min_val = min_val.min(chunk[0]);
                max_val = max_val.max(chunk[0]);
            }
        }

        let mean_luminance = if sample_count > 0 {
            sum_lum / sample_count as f64
        } else {
            0.0
        };

        let lighting = if mean_luminance < 25.0 {
            LightingCondition::Dark
        } else if mean_luminance < 75.0 {
            LightingCondition::Dim
        } else if mean_luminance < 190.0 {
            LightingCondition::IndoorAdequate
        } else {
            LightingCondition::Daylight
        };

        // 3. Contrast & spatial gradient analysis for presence
        let dynamic_range = max_val.saturating_sub(min_val);
        let is_black_or_uniform = dynamic_range < 12;

        let (user_presence, people_count, gaze_focused, distance_m) = if is_black_or_uniform {
            (UserPresence::Absent, 0, false, 0.0)
        } else {
            // Heuristic spatial distribution of edges:
            // Check central third vs lateral thirds
            let stride = (frame.width * step as u32) as usize;
            let center_y_start = (frame.height / 3) as usize;
            let center_y_end = (frame.height * 2 / 3) as usize;
            let center_x_start = (frame.width / 4) as usize * step;
            let center_x_end = (frame.width * 3 / 4) as usize * step;

            let mut center_energy = 0u64;
            let mut peripheral_energy = 0u64;

            for y in (0..frame.height as usize).step_by(8) {
                let row_offset = y * stride;
                let in_center_y = y >= center_y_start && y <= center_y_end;

                for x in (0..frame.width as usize * step).step_by(step * 8) {
                    if row_offset + x + 1 < frame.data.len() {
                        let diff = (frame.data[row_offset + x] as i32 - frame.data[row_offset + x + 1] as i32).abs() as u64;
                        let in_center_x = x >= center_x_start && x <= center_x_end;
                        if in_center_y && in_center_x {
                            center_energy += diff;
                        } else {
                            peripheral_energy += diff;
                        }
                    }
                }
            }

            let has_central_subject = center_energy > 50;
            let has_peripheral_subjects = peripheral_energy > (center_energy * 2);

            if has_peripheral_subjects && has_central_subject {
                (UserPresence::MultiplePeopleDetected { count: 2 }, 2, false, 1.2)
            } else if has_central_subject {
                // Determine whether user is looking at device (center energy high and balanced)
                let attentive = center_energy > peripheral_energy;
                let presence = if attentive {
                    UserPresence::Present
                } else {
                    UserPresence::Distracted
                };
                (presence, 1, attentive, 0.65)
            } else {
                (UserPresence::Absent, 0, false, 0.0)
            }
        };

        VisualSceneContext {
            user_presence,
            estimated_distance_m: distance_m,
            user_gaze_focused_on_device: gaze_focused,
            people_count,
            lighting,
            privacy_preserving_frame_hash: frame_hash,
            timestamp: Timestamp::now(),
        }
    }

    /// Feed the resulting visual perception into the system's WorldModel
    pub fn update_world_model(&self, ctx: &VisualSceneContext, world_model: &mut WorldModel) {
        let present = match ctx.user_presence {
            UserPresence::Present | UserPresence::Distracted | UserPresence::MultiplePeopleDetected { .. } => true,
            UserPresence::Absent => false,
        };
        world_model.update_visual_presence(
            present,
            ctx.user_gaze_focused_on_device,
            ctx.people_count,
            ctx.estimated_distance_m,
            ctx.lighting.as_str(),
        );
    }
}
