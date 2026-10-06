use annora_inference::GgufModel;
use annora_substrate::{Key, MouseButton, ScrollDirection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuiIntention {
    Click {
        x: f32, // normalized 0.0 to 1.0
        y: f32,
        button: MouseButton,
    },
    DoubleClick {
        x: f32,
        y: f32,
    },
    Type {
        text: String,
    },
    Hotkey {
        keys: Vec<Key>,
    },
    Scroll {
        direction: ScrollDirection,
        amount: i32,
    },
    Drag {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    },
    Wait {
        duration_ms: u64,
    },
    Terminate {
        success: bool,
        summary: String,
    },
}

pub struct UiTarsAgent {
    pub agent_name: String,
    pub model_metadata: Option<GgufModel>,
}

impl UiTarsAgent {
    pub fn new() -> Self {
        Self {
            agent_name: "UI-TARS-2B-SFT".to_string(),
            model_metadata: None,
        }
    }

    pub fn with_gguf_file(path: impl AsRef<Path>) -> annora_core::Result<Self> {
        let model = GgufModel::read_from_file(path)?;
        Ok(Self::with_gguf_model(model))
    }

    pub fn with_gguf_model(model: GgufModel) -> Self {
        Self {
            agent_name: model.name().unwrap_or("UI-TARS-2B-SFT").to_string(),
            model_metadata: Some(model),
        }
    }

    pub fn is_model_connected(&self) -> bool {
        self.model_metadata.is_some()
    }

    pub fn tensor_count(&self) -> usize {
        self.model_metadata.as_ref().map(|m| m.tensor_count()).unwrap_or(0)
    }

    pub fn predict_intent_from_task(&self, task_description: &str) -> Vec<GuiIntention> {
        let task_lower = task_description.to_lowercase();
        if task_lower.contains("click") {
            // Coordinate extraction if specified (e.g. click at 200, 400)
            let mut x = 0.5f32;
            let mut y = 0.5f32;
            if task_lower.contains("top") {
                y = 0.15;
            } else if task_lower.contains("bottom") {
                y = 0.85;
            }
            if task_lower.contains("left") {
                x = 0.15;
            } else if task_lower.contains("right") {
                x = 0.85;
            }

            vec![GuiIntention::Click {
                x,
                y,
                button: MouseButton::Left,
            }]
        } else if task_lower.contains("type") || task_lower.contains("write") {
            let text = if let Some(idx) = task_description.find(':') {
                task_description[idx + 1..].trim().to_string()
            } else {
                "Hello from Annora GUI Agent".to_string()
            };
            vec![GuiIntention::Type { text }]
        } else if task_lower.contains("scroll") {
            let direction = if task_lower.contains("up") {
                ScrollDirection::Up
            } else {
                ScrollDirection::Down
            };
            vec![GuiIntention::Scroll {
                direction,
                amount: 3,
            }]
        } else {
            vec![GuiIntention::Wait { duration_ms: 100 }]
        }
    }
}

impl Default for UiTarsAgent {
    fn default() -> Self {
        Self::new()
    }
}
