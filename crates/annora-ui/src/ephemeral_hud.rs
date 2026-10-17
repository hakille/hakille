use crate::platform_adapter::PlatformKind;
use crate::presence::UiTheme;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudState {
    /// Completely hidden, zero screen real estate consumed
    Hidden,
    /// Ephemeral single-line input bar invoked via global hotkey (spec §8/§29 : Ctrl+Alt+Space, alias Alt+Space)
    Prompting { query: String, cursor_pos: usize },
    /// Live streaming response appearing inline
    StreamingResponse {
        query: String,
        response_text: String,
        is_final: bool,
    },
    /// Action requiring user validation chip — rendered with Braise #FF7A2F accent!
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
    custom_hotkey: Option<String>,
}

impl EphemeralHud {
    /// Raccourci global par défaut, spec §8/§29 : ⌃⌥Espace modifiable dès
    /// l'onboarding. macOS → "Cmd+Space", autres → "Ctrl+Alt+Space".
    /// "Alt+Space" reste accepté comme alias (pas de casse utilisateur).
    pub fn default_hotkey_for(platform: PlatformKind) -> &'static str {
        match platform {
            PlatformKind::MacOS => "Cmd+Space",
            _ => "Ctrl+Alt+Space",
        }
    }

    /// "Alt+Space" est un alias historique accepté partout, jamais le défaut.
    pub fn is_hotkey_alias_accepted(hotkey: &str) -> bool {
        matches!(hotkey, "Alt+Space" | "Cmd+Space" | "Ctrl+Alt+Space")
    }

    pub fn new() -> Self {
        Self {
            state: HudState::Hidden,
            theme: UiTheme::default(),
            auto_evaporate_delay_ms: 1500,
            hotkey_binding: Self::default_hotkey_for(PlatformKind::Windows).to_string(),
            custom_hotkey: None,
        }
    }

    pub fn with_platform(platform: PlatformKind) -> Self {
        let mut hud = Self::new();
        hud.hotkey_binding = Self::default_hotkey_for(platform).to_string();
        hud
    }

    pub fn state(&self) -> &HudState {
        &self.state
    }

    pub fn theme(&self) -> &UiTheme {
        &self.theme
    }

    pub fn hotkey(&self) -> &str {
        self.custom_hotkey
            .as_deref()
            .unwrap_or(&self.hotkey_binding)
    }

    /// Modifiable dès l'onboarding (spec §8/§29).
    pub fn set_custom_hotkey(&mut self, hotkey: impl Into<String>) {
        self.custom_hotkey = Some(hotkey.into());
    }

    pub fn clear_custom_hotkey(&mut self) {
        self.custom_hotkey = None;
    }

    pub fn custom_hotkey(&self) -> Option<&str> {
        self.custom_hotkey.as_deref()
    }

    pub fn open_prompt(&mut self) {
        self.state = HudState::Prompting {
            query: String::new(),
            cursor_pos: 0,
        };
    }

    pub fn insert_char(&mut self, c: char) {
        if let HudState::Prompting {
            ref mut query,
            ref mut cursor_pos,
        } = self.state
        {
            let byte_len = c.len_utf8();
            query.insert(*cursor_pos, c);
            *cursor_pos += byte_len;
        }
    }

    pub fn backspace(&mut self) {
        if let HudState::Prompting {
            ref mut query,
            ref mut cursor_pos,
        } = self.state
        {
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

    pub fn move_cursor_left(&mut self) {
        if let HudState::Prompting {
            ref query,
            ref mut cursor_pos,
        } = self.state
        {
            if *cursor_pos > 0 {
                let prev_boundary = query[..*cursor_pos]
                    .char_indices()
                    .last()
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                *cursor_pos = prev_boundary;
            }
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let HudState::Prompting {
            ref query,
            ref mut cursor_pos,
        } = self.state
        {
            if *cursor_pos < query.len() {
                let next_boundary = query[*cursor_pos..]
                    .char_indices()
                    .nth(1)
                    .map(|(idx, _)| *cursor_pos + idx)
                    .unwrap_or(query.len());
                *cursor_pos = next_boundary;
            }
        }
    }

    pub fn delete(&mut self) {
        if let HudState::Prompting {
            ref mut query,
            ref cursor_pos,
        } = self.state
        {
            if *cursor_pos < query.len() {
                query.remove(*cursor_pos);
            }
        }
    }

    pub fn clear(&mut self) {
        if let HudState::Prompting {
            ref mut query,
            ref mut cursor_pos,
        } = self.state
        {
            query.clear();
            *cursor_pos = 0;
        }
    }

    pub fn set_streaming(
        &mut self,
        query: impl Into<String>,
        partial: impl Into<String>,
        is_final: bool,
    ) {
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

    /// Fait avancer l'auto-évaporation (1500ms → 0 puis Hidden).
    /// À appeler chaque frame avec le temps écoulé ; sans tick, le HUD
    /// resterait figé en `Evaporating` au lieu de disparaître.
    pub fn tick(&mut self, elapsed_ms: u32) {
        if let HudState::Evaporating { remaining_ms, .. } = &mut self.state {
            if elapsed_ms >= *remaining_ms {
                self.state = HudState::Hidden;
            } else {
                *remaining_ms -= elapsed_ms;
            }
        }
    }

    pub fn dismiss(&mut self) {
        self.state = HudState::Hidden;
    }

    /// Renders an ASCII text-based representation of the ephemeral bar
    pub fn render_ascii(&self) -> String {
        match &self.state {
            HudState::Hidden => "[HUD: Inactive (Listening in background)]".to_string(),
            HudState::Prompting { query, .. } => {
                format!(
                    "╭─ [Akora HUD • {}] ──────────────────────────╮\n│ > {}█\n╰─────────────────────────────────────────────────────╯",
                    self.hotkey(),
                    query
                )
            }
            HudState::StreamingResponse {
                query,
                response_text,
                is_final,
            } => {
                let badge = if *is_final {
                    "✓ Final"
                } else {
                    "● Streaming"
                };
                format!(
                    "╭─ [Akora HUD • {}] ──────────────────────────╮\n│ > {}\n│ → {}\n╰─────────────────────────────────────────────────────╯",
                    badge, query, response_text
                )
            }
            HudState::ValidationRequired {
                action_id,
                description,
                cost_cents,
                risk_level,
            } => {
                let cost_str = cost_cents
                    .map(|c| format!(" | Cost: {:.2} EUR", c as f64 / 100.0))
                    .unwrap_or_default();
                format!(
                    "╭─ [Annora VALIDATION REQUIRED #FF7A2F BRAISE] ─────────╮\n│ Action [{}]: {}\n│ Risk: {}{}\n│ [Enter] Confirmer  |  [Esc] Annuler\n╰─────────────────────────────────────────────────────╯",
                    action_id, description, risk_level, cost_str
                )
            }
            HudState::Evaporating {
                last_text,
                remaining_ms,
            } => {
                format!(
                    "╭─ [Akora HUD • Evaporating in {}ms] ─────────────╮\n│ → {}\n╰─────────────────────────────────────────────────────╯",
                    remaining_ms, last_text
                )
            }
        }
    }

    /// Computes the exact acrylic overlay window specification for the current HUD state
    pub fn overlay_spec(&self) -> AcrylicOverlayWindowSpec {
        let (height, opacity) = match &self.state {
            HudState::Hidden => (0, 0.0),
            HudState::Prompting { .. } => (64, 0.88),
            HudState::StreamingResponse { .. } => (240, 0.92),
            HudState::ValidationRequired { .. } => (280, 0.95),
            HudState::Evaporating { remaining_ms, .. } => {
                let frac =
                    (*remaining_ms as f32 / self.auto_evaporate_delay_ms as f32).clamp(0.0, 1.0);
                ((240.0 * frac) as u32, 0.85 * frac)
            }
        };

        AcrylicOverlayWindowSpec {
            width: 720,
            height,
            pos_y_offset: 120,
            acrylic_tint_hex: "#121419".to_string(),
            acrylic_opacity: opacity,
            blur_radius: 30,
            corner_radius: 12,
            border_hex: "#2D2D2D".to_string(),
            coral_accent_hex: "#FF7A2F".to_string(),
            is_topmost: true,
            is_visible: self.state != HudState::Hidden,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        for c in text.chars() {
            self.insert_char(c);
        }
    }

    pub fn submit_prompt(&mut self) -> Option<String> {
        if let HudState::Prompting { ref query, .. } = self.state {
            let q = query.trim().to_string();
            if !q.is_empty() {
                self.state = HudState::StreamingResponse {
                    query: q.clone(),
                    response_text: "En attente de réponse neuronale...".to_string(),
                    is_final: false,
                };
                return Some(q);
            }
        }
        None
    }

    pub fn confirm_validation(&mut self, expected_action_id: &str) -> bool {
        if let HudState::ValidationRequired { ref action_id, .. } = self.state {
            if action_id == expected_action_id {
                self.start_evaporation();
                return true;
            }
        }
        false
    }

    pub fn cancel_validation(&mut self) {
        self.dismiss();
    }

    /// JSON payload for OS compositors (Windows DWM overlay, macOS NSPanel, Wayland wlr-layer-shell)
    pub fn to_compositor_payload(&self) -> serde_json::Value {
        let spec = self.overlay_spec();
        serde_json::json!({
            "state": format!("{:?}", self.state),
            "theme": {
                "background_hex": spec.acrylic_tint_hex,
                "text_hex": "#ECEEF2",
                "validation_accent_coral": spec.coral_accent_hex,
                "opacity": spec.acrylic_opacity,
                "blur_radius": spec.blur_radius,
                "corner_radius": spec.corner_radius,
            },
            "window": {
                "width": spec.width,
                "height": spec.height,
                "top_offset": spec.pos_y_offset,
                "is_topmost": spec.is_topmost,
            },
            "hotkey": self.hotkey(),
            "is_visible": spec.is_visible,
        })
    }
}

/// Specifications for Windows DirectComposition / Acrylic translucent overlay window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcrylicOverlayWindowSpec {
    pub width: u32,
    pub height: u32,
    pub pos_y_offset: i32,
    pub acrylic_tint_hex: String,
    pub acrylic_opacity: f32,
    pub blur_radius: u32,
    pub corner_radius: u32,
    pub border_hex: String,
    pub coral_accent_hex: String,
    pub is_topmost: bool,
    pub is_visible: bool,
}

impl AcrylicOverlayWindowSpec {
    pub fn centered_default() -> Self {
        Self {
            width: 720,
            height: 64,
            pos_y_offset: 120,
            acrylic_tint_hex: "#121419".to_string(),
            acrylic_opacity: 0.88,
            blur_radius: 30,
            corner_radius: 12,
            border_hex: "#2D2D2D".to_string(),
            coral_accent_hex: "#FF7A2F".to_string(),
            is_topmost: true,
            is_visible: true,
        }
    }
}

impl Default for AcrylicOverlayWindowSpec {
    fn default() -> Self {
        Self::centered_default()
    }
}

impl Default for EphemeralHud {
    fn default() -> Self {
        Self::new()
    }
}
