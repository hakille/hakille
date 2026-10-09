use annora_inference::GgufModel;
use annora_substrate::{InputAction, Key, MouseButton, ScrollDirection};
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

    /// Converts normalized GUI intention into physical Substrate input action
    pub fn to_input_action(&self, intention: &GuiIntention, screen_w: u32, screen_h: u32) -> Option<InputAction> {
        match intention {
            GuiIntention::Click { x, y, button } => {
                let px = (x * screen_w as f32) as i32;
                let py = (y * screen_h as f32) as i32;
                Some(InputAction::Click { x: px, y: py, button: *button })
            }
            GuiIntention::DoubleClick { x, y } => {
                let px = (x * screen_w as f32) as i32;
                let py = (y * screen_h as f32) as i32;
                Some(InputAction::DoubleClick { x: px, y: py })
            }
            GuiIntention::Type { text } => {
                Some(InputAction::Type { text: text.clone() })
            }
            GuiIntention::Hotkey { keys } => {
                Some(InputAction::Hotkey { keys: keys.clone() })
            }
            GuiIntention::Scroll { direction, amount } => {
                Some(InputAction::Scroll { direction: *direction, amount: *amount })
            }
            GuiIntention::Drag { from_x, from_y, to_x, to_y } => {
                let fx = (from_x * screen_w as f32) as i32;
                let fy = (from_y * screen_h as f32) as i32;
                let tx = (to_x * screen_w as f32) as i32;
                let ty = (to_y * screen_h as f32) as i32;
                Some(InputAction::Drag { from_x: fx, from_y: fy, to_x: tx, to_y: ty })
            }
            GuiIntention::Wait { .. } | GuiIntention::Terminate { .. } => None,
        }
    }

    /// Executes GUI intention natively on the host OS via Substrate
    pub async fn execute_intention(
        &self,
        substrate: &impl annora_substrate::Substrate,
        intention: &GuiIntention,
        cap: &annora_core::CapabilityToken,
        screen_w: u32,
        screen_h: u32,
    ) -> annora_core::Result<String> {
        if let Some(action) = self.to_input_action(intention, screen_w, screen_h) {
            substrate.inject_input(action, cap).await?;
            Ok(format!("Executed native GUI action: {:?}", intention))
        } else {
            match intention {
                GuiIntention::Wait { duration_ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*duration_ms)).await;
                    Ok(format!("Waited {}ms", duration_ms))
                }
                GuiIntention::Terminate { success, summary } => {
                    Ok(format!("Terminated (success={}): {}", success, summary))
                }
                _ => Ok("No-op".to_string()),
            }
        }
    }

    /// Predicts GUI action plan using real loaded GGUF neural model if connected
    pub fn predict_intentions_neural(&self, task_description: &str, screen_w: u32, screen_h: u32) -> Vec<GuiIntention> {
        if let Some(ref m) = self.model_metadata {
            let engine = annora_inference::GgufInferenceEngine::new(m.clone());
            if let Ok(plan) = engine.generate_gui_action_plan(task_description, screen_w, screen_h) {
                let mut intentions = Vec::new();
                for step in plan {
                    if step.starts_with("click(") {
                        let mut x = 0.5f32;
                        let mut y = 0.5f32;
                        let inner = step.trim_start_matches("click(").trim_end_matches(')');
                        for part in inner.split(',') {
                            let part = part.trim();
                            if let Some(rest) = part.strip_prefix("x=") {
                                if let Ok(v) = rest.parse::<f32>() { x = v; }
                            } else if let Some(rest) = part.strip_prefix("y=") {
                                if let Ok(v) = rest.parse::<f32>() { y = v; }
                            }
                        }
                        intentions.push(GuiIntention::Click { x, y, button: MouseButton::Left });
                    } else if step.starts_with("type(") {
                        let text = step.trim_start_matches("type(text=\"").trim_end_matches("\")").to_string();
                        intentions.push(GuiIntention::Type { text });
                    } else if step.starts_with("scroll(") {
                        let direction = if step.contains("direction=\"up\"") {
                            ScrollDirection::Up
                        } else if step.contains("direction=\"left\"") {
                            ScrollDirection::Left
                        } else if step.contains("direction=\"right\"") {
                            ScrollDirection::Right
                        } else {
                            ScrollDirection::Down
                        };
                        intentions.push(GuiIntention::Scroll { direction, amount: 3 });
                    } else {
                        intentions.push(GuiIntention::Wait { duration_ms: 100 });
                    }
                }
                if !intentions.is_empty() {
                    return intentions;
                }
            }
        }
        self.predict_intent_from_task(task_description)
    }
}

impl Default for UiTarsAgent {
    fn default() -> Self {
        Self::new()
    }
}

