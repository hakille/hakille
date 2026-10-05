use annora_substrate::{Key, MouseButton, ScrollDirection};
use serde::{Deserialize, Serialize};

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
}

impl UiTarsAgent {
    pub fn new() -> Self {
        Self {
            agent_name: "UI-TARS-7B-DPO".to_string(),
        }
    }

    pub fn predict_intent_from_task(&self, task_description: &str) -> Vec<GuiIntention> {
        let task_lower = task_description.to_lowercase();
        if task_lower.contains("click") {
            vec![GuiIntention::Click {
                x: 0.5,
                y: 0.5,
                button: MouseButton::Left,
            }]
        } else if task_lower.contains("type") || task_lower.contains("write") {
            vec![GuiIntention::Type {
                text: "Hello from Annora GUI Agent".to_string(),
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
