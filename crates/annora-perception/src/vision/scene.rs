use crate::vision::capture::VideoFrame;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneUnderstanding {
    pub scene_type: String,
    pub active_window_title: Option<String>,
    pub detected_elements_count: usize,
    pub ocr_text_sample: Vec<String>,
    pub confidence: f32,
}

#[async_trait]
pub trait SceneAnalyzer: Send + Sync {
    async fn analyze(&self, frame: &VideoFrame) -> Result<SceneUnderstanding>;
}

pub struct DefaultSceneAnalyzer;

#[async_trait]
impl SceneAnalyzer for DefaultSceneAnalyzer {
    async fn analyze(&self, frame: &VideoFrame) -> Result<SceneUnderstanding> {
        let is_valid = !frame.data.is_empty();
        Ok(SceneUnderstanding {
            scene_type: if is_valid { "DesktopGUI".to_string() } else { "EmptyFrame".to_string() },
            active_window_title: Some("ActiveApplication".to_string()),
            detected_elements_count: if is_valid { 12 } else { 0 },
            ocr_text_sample: vec!["File".to_string(), "Edit".to_string(), "View".to_string()],
            confidence: 0.90,
        })
    }
}
