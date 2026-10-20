use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrameFormat {
    Rgb8,
    Rgba8,
    Bgra8,
    Jpeg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub data: Vec<u8>,
    pub timestamp: Timestamp,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: FrameFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            data,
            timestamp: Timestamp::now(),
        }
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}
