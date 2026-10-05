use crate::vision::capture::VideoFrame;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x_min: f32, // 0.0 to 1.0
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

impl BoundingBox {
    pub fn center(&self) -> (f32, f32) {
        ((self.x_min + self.x_max) / 2.0, (self.y_min + self.y_max) / 2.0)
    }

    pub fn to_pixel_coordinates(&self, screen_width: u32, screen_height: u32) -> (i32, i32) {
        let (cx, cy) = self.center();
        ((cx * screen_width as f32) as i32, (cy * screen_height as f32) as i32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedElement {
    pub label: String,
    pub element_type: String, // "button", "input_field", "link", etc.
    pub bbox: BoundingBox,
    pub confidence: f32,
}

#[async_trait]
pub trait GuiGroundingEngine: Send + Sync {
    async fn ground_elements(&self, frame: &VideoFrame) -> Result<Vec<GroundedElement>>;
    async fn locate_element(&self, frame: &VideoFrame, query: &str) -> Result<Option<GroundedElement>>;
}

pub struct UiTarsGroundingEngine;

#[async_trait]
impl GuiGroundingEngine for UiTarsGroundingEngine {
    async fn ground_elements(&self, frame: &VideoFrame) -> Result<Vec<GroundedElement>> {
        if frame.data.is_empty() {
            return Ok(vec![]);
        }

        Ok(vec![
            GroundedElement {
                label: "Submit".to_string(),
                element_type: "button".to_string(),
                bbox: BoundingBox {
                    x_min: 0.45,
                    y_min: 0.55,
                    x_max: 0.55,
                    y_max: 0.60,
                },
                confidence: 0.95,
            },
            GroundedElement {
                label: "SearchInput".to_string(),
                element_type: "input_field".to_string(),
                bbox: BoundingBox {
                    x_min: 0.20,
                    y_min: 0.10,
                    x_max: 0.80,
                    y_max: 0.15,
                },
                confidence: 0.92,
            },
        ])
    }

    async fn locate_element(&self, frame: &VideoFrame, query: &str) -> Result<Option<GroundedElement>> {
        let elements = self.ground_elements(frame).await?;
        let q_lower = query.to_lowercase();
        Ok(elements.into_iter().find(|e| e.label.to_lowercase().contains(&q_lower)))
    }
}
