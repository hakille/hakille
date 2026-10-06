use crate::presence::UiTheme;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudState {
    /// Completely hidden, zero screen real estate consumed
    Hidden,
    /// Ephemeral single-line input bar invoked via global hotkey (e.g. Alt+Space)
    Prompting {
        query: String,
        cursor_pos: usize,
    },
    /// Live streaming response appearing inline
    StreamingResponse {
        query: String,
        response_text: String,
        is_final: bool,
    },
    /// Action requiring user validation chip — rendered with Coral #FF5722 accent!
    ValidationRequired {
        action_id: String,
        description: String,
        cost_cents: Option<u64>,
        risk_level: String,
    },
    /// Response finished, fading into thin air (e.g. 1500ms auto-evaporation)
    Evaporating {
        last_text: String,
        remaining_ms: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralHud {
    state: HudState,
    theme: UiTheme,
    auto_evaporate_delay_ms: u32,
    hotkey_binding: String,
}

impl EphemeralHud {
    pub fn new() -> Self {
        Self {
            state: HudState::Hidden,
            theme: UiTheme::default(),
            auto_evaporate_delay_ms: 1500,
            hotkey_binding: "Alt+Space".to_string(),
        }
    }

    pub fn state(&self) -> &HudState {
        &self.state
    }

    pub fn theme(&self) -> &UiTheme {
        &self.theme
    }

    pub fn hotkey(&self) -> &str {
        &self.hotkey_binding
    }

    pub fn open_prompt(&mut self) {
        self.state = HudState::Prompting {
            query: String::new(),
            cursor_pos: 0,
        };
    }

    pub fn insert_char(&mut self, c: char) {
        if let HudState::Prompting { ref mut query, ref mut cursor_pos } = self.state {
            let byte_len = c.len_utf8();
            query.insert(*cursor_pos, c);
            *cursor_pos += byte_len;
        }
    }

    pub fn backspace(&mut self) {
        if let HudState::Prompting { ref mut query, ref mut cursor_pos } = self.state {
            if *cursor_pos > 0 {
                let prev_boundary = query[..*cursor_pos]
                    .char_indices()
                    .last()
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                query.remove(prev_boundary);
                *cursor_pos = prev_boundary;
            }
        }
    }


    pub fn set_streaming(&mut self, query: impl Into<String>, partial: impl Into<String>, is_final: bool) {
        self.state = HudState::StreamingResponse {
            query: query.into(),
            response_text: partial.into(),
            is_final,
        };
    }

    pub fn request_validation(
        &mut self,
        action_id: impl Into<String>,
        description: impl Into<String>,
        cost_cents: Option<u64>,
        risk_level: impl Into<String>,
    ) {
        self.state = HudState::ValidationRequired {
            action_id: action_id.into(),
            description: description.into(),
            cost_cents,
            risk_level: risk_level.into(),
        };
    }

    pub fn start_evaporation(&mut self) {
        let text = match &self.state {
            HudState::StreamingResponse { response_text, .. } => response_text.clone(),
            _ => String::new(),
        };
        self.state = HudState::Evaporating {
            last_text: text,
            remaining_ms: self.auto_evaporate_delay_ms,
        };
    }

    pub fn dismiss(&mut self) {
        self.state = HudState::Hidden;
    }

    /// Renders an ASCII text-based representation of the ephemeral bar
    pub fn render_ascii(&self) -> String {
        match &self.state {
            HudState::Hidden => "[HUD: Inactive (Listening in background)]".to_string(),
            HudState::Prompting { query, .. } => {
                format!("╭─ [Annora HUD • Alt+Space] ──────────────────────────╮\n│ > {}█\n╰─────────────────────────────────────────────────────╯", query)
            }
            HudState::StreamingResponse { query, response_text, is_final } => {
                let badge = if *is_final { "✓ Final" } else { "● Streaming" };
                format!(
                    "╭─ [Annora HUD • {}] ──────────────────────────╮\n│ > {}\n│ → {}\n╰─────────────────────────────────────────────────────╯",
                    badge, query, response_text
                )
            }
            HudState::ValidationRequired { action_id, description, cost_cents, risk_level } => {
                let cost_str = cost_cents.map(|c| format!(" | Cost: {:.2} EUR", c as f64 / 100.0)).unwrap_or_default();
                format!(
                    "╭─ [Annora VALIDATION REQUIRED #FF5722 CORAL] ─────────╮\n│ Action [{}]: {}\n│ Risk: {}{}\n│ [Enter] Confirmer  |  [Esc] Annuler\n╰─────────────────────────────────────────────────────╯",
                    action_id, description, risk_level, cost_str
                )
            }
            HudState::Evaporating { last_text, remaining_ms } => {
                format!("╭─ [Annora HUD • Evaporating in {}ms] ─────────────╮\n│ → {}\n╰─────────────────────────────────────────────────────╯", remaining_ms, last_text)
            }
        }
    }

    /// JSON payload for OS compositors (Windows DWM overlay, macOS NSPanel, Wayland wlr-layer-shell)
    pub fn to_compositor_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "state": format!("{:?}", self.state),
            "theme": {
                "background_hex": "#121212",
                "text_hex": "#E0E0E0",
                "validation_accent_coral": "#FF5722",
            },
            "hotkey": self.hotkey_binding,
            "is_visible": self.state != HudState::Hidden,
        })
    }
}

impl Default for EphemeralHud {
    fn default() -> Self {
        Self::new()
    }
}
