//! Snapshot stable — équivalent canonique de `ShellSnapshot` sans Slint.

use annora_ui::{CostCounter, EphemeralHud, HudState, PresenceIndicator, PresenceState, SilenceBudgetTracker};
use serde::{Deserialize, Serialize};

/// Tokens design — source unique `tokens/` (miroir `annora-design::generated`).
pub const BG_HEX: &str = "#0B0C0F";
pub const TEXT_HEX: &str = "#ECEEF2";
pub const CORAL_HEX: &str = "#FF7A2F";
pub const BORDER_HEX: &str = "#262A33";
pub const HUD_WIDTH: u32 = 720;
pub const HUD_HEIGHT: u32 = 64;
pub const HUD_TOP_OFFSET: i32 = 120;
pub const HUD_BLUR: u32 = 30;
pub const HUD_RADIUS: u32 = 12;
pub const EVAPORATE_MS: u32 = 1500;

/// Destination de navigation (0 Journal, 1 Mémoire, 2 Pouvoirs, 3 Réglages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Destination {
    Journal = 0,
    Memoire = 1,
    Pouvoirs = 2,
    Reglages = 3,
}

impl Destination {
    pub fn from_u8(v: u8) -> Self {
        match v.min(3) {
            0 => Self::Journal,
            1 => Self::Memoire,
            2 => Self::Pouvoirs,
            _ => Self::Reglages,
        }
    }
}

/// Snapshot immuable consommé par le renderer (winit/wgpu/software).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StableSnapshot {
    pub presence_state: u8,
    pub presence_pulse: f32,
    pub presence_attention: bool,
    pub hud_query: String,
    pub hud_response: String,
    pub hud_streaming: bool,
    pub hud_validation: bool,
    pub hud_validation_text: String,
    pub hud_validation_cost: String,
    pub cost_total_cents: u64,
    pub cost_budget_cents: u64,
    pub cost_level: u8,
    pub silence_remaining: u32,
    pub silence_quota: u32,
    pub high_contrast: bool,
    pub light_mode: bool,
    pub presence_a11y_label: String,
}

impl StableSnapshot {
    pub fn from_states(
        presence: &PresenceIndicator,
        hud: &EphemeralHud,
        cost: &CostCounter,
        silence: &SilenceBudgetTracker,
        now_ms: u64,
        pending_actions: usize,
    ) -> Self {
        let (state, pulse) = match presence.state() {
            PresenceState::Idle => (0u8, 0.0),
            PresenceState::Listening => (1u8, presence.pulse_factor(now_ms)),
            PresenceState::Acting => (2u8, presence.pulse_factor(now_ms)),
            PresenceState::Speaking => (3u8, presence.pulse_factor(now_ms)),
            PresenceState::DecisionRequired => (4u8, 0.0),
            PresenceState::Urgent => (5u8, presence.pulse_factor(now_ms)),
        };
        let (q, r, streaming, v, vt, vc) = match hud.state() {
            HudState::Hidden => (String::new(), String::new(), false, false, String::new(), String::new()),
            HudState::Prompting { query, .. } => (query.clone(), String::new(), false, false, String::new(), String::new()),
            HudState::StreamingResponse { query, response_text, is_final } => {
                (query.clone(), response_text.clone(), !is_final, false, String::new(), String::new())
            }
            HudState::ValidationRequired { description, cost_cents, .. } => (
                String::new(), String::new(), false, true, description.clone(),
                cost_cents.map(|c| format!("{:.2} EUR", c as f64 / 100.0)).unwrap_or_default(),
            ),
            HudState::Evaporating { last_text, .. } => {
                (String::new(), last_text.clone(), false, false, String::new(), String::new())
            }
        };
        let level = if cost.is_budget_exceeded() { 2 } else if cost.is_near_budget_limit() { 1 } else { 0 };
        let presence_a11y_label = match presence.state() {
            PresenceState::Idle => "Akora : veille ambiante".to_string(),
            PresenceState::Listening => "Akora : écoute active".to_string(),
            PresenceState::Acting => "Akora : action en cours — activer pour ouvrir le journal".to_string(),
            PresenceState::Speaking => "Akora : parole en cours".to_string(),
            PresenceState::DecisionRequired => "Akora : décision requise — activer pour voir".to_string(),
            PresenceState::Urgent => "Akora : urgence".to_string(),
        };
        Self {
            presence_state: state,
            presence_pulse: pulse,
            presence_attention: pending_actions > 0,
            hud_query: q,
            hud_response: r,
            hud_streaming: streaming,
            hud_validation: v,
            hud_validation_text: vt,
            hud_validation_cost: vc,
            cost_total_cents: cost.state().total_cents,
            cost_budget_cents: cost.state().daily_budget_cents,
            cost_level: level,
            silence_remaining: silence.credits_remaining(),
            silence_quota: silence.daily_quota(),
            high_contrast: false,
            light_mode: false,
            presence_a11y_label,
        }
    }

    pub fn with_high_contrast(mut self, enabled: bool) -> Self {
        self.high_contrast = enabled;
        self
    }

    pub fn with_light_mode(mut self, enabled: bool) -> Self {
        self.light_mode = enabled;
        self
    }

    pub fn presence_label(&self) -> &'static str {
        match self.presence_state {
            0 => "Veille ambiante",
            1 => "Écoute active",
            2 => "Action en cours — toucher pour journal",
            3 => "Parole en cours",
            4 => "Décision requise — toucher pour voir",
            _ => "Urgence",
        }
    }

    /// Dashboard texte — fallback sans GPU, même info que la fenêtre.
    pub fn render_text_dashboard(&self) -> String {
        let cost_level = match self.cost_level {
            2 => "DÉPASSÉ",
            1 => "PROCHE LIMITE (≥80%)",
            _ => "nominal",
        };
        let hud_block = if self.hud_validation {
            format!(
                "[VALIDATION #FF7A2F] {}\n  coût : {}\n  [Entrée: valider] [Échap: annuler]",
                self.hud_validation_text, self.hud_validation_cost
            )
        } else if self.hud_streaming || !self.hud_response.is_empty() {
            format!("> {}\n→ {}", self.hud_query, self.hud_response)
        } else if !self.hud_query.is_empty() {
            format!("> {}█", self.hud_query)
        } else {
            "[HUD masqué — Ctrl+Alt+Space pour invoquer (alias Alt+Space)]".to_string()
        };
        format!(
            "Akora — {} (pulse {:.0}%){}\n{}\nCoût System2 : {:.2} EUR / {:.2} EUR [{}]\nSilence : {}/{} crédits | Contraste : {}",
            self.presence_label(),
            self.presence_pulse * 100.0,
            if self.presence_attention { " [● attention]" } else { "" },
            hud_block,
            self.cost_total_cents as f64 / 100.0,
            self.cost_budget_cents as f64 / 100.0,
            cost_level,
            self.silence_remaining,
            self.silence_quota,
            if self.high_contrast { "fort" } else { "standard" },
        )
    }

    pub fn to_compositor_json(&self) -> serde_json::Value {
        serde_json::json!({
            "theme": {"bg": BG_HEX, "text": TEXT_HEX, "coral": CORAL_HEX, "border": BORDER_HEX},
            "presence": {"state": self.presence_state, "pulse": self.presence_pulse, "attention": self.presence_attention, "label": self.presence_label()},
            "hud": {"query": self.hud_query, "response": self.hud_response, "streaming": self.hud_streaming,
                    "validation": self.hud_validation, "validation_text": self.hud_validation_text, "validation_cost": self.hud_validation_cost},
            "cost": {"total_cents": self.cost_total_cents, "budget_cents": self.cost_budget_cents, "level": self.cost_level},
            "silence": {"remaining": self.silence_remaining, "quota": self.silence_quota},
            "window": {"width": HUD_WIDTH, "height": HUD_HEIGHT, "top_offset": HUD_TOP_OFFSET,
                       "blur": HUD_BLUR, "radius": HUD_RADIUS},
        })
    }
}

/// Préférences additives (mouvement réduit + destination).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellPrefs {
    pub reduced_motion: bool,
    pub destination: u8,
}

impl Default for ShellPrefs {
    fn default() -> Self {
        Self { reduced_motion: false, destination: 0 }
    }
}

impl ShellPrefs {
    pub fn with_reduced_motion(mut self, enabled: bool) -> Self {
        self.reduced_motion = enabled;
        self
    }
    pub fn with_destination(mut self, destination: u8) -> Self {
        self.destination = destination.min(3);
        self
    }
    pub fn quantized_pulse(&self, pulse: f32) -> f32 {
        if self.reduced_motion {
            if pulse >= 0.5 { 1.0 } else { 0.0 }
        } else {
            pulse.clamp(0.0, 1.0)
        }
    }
    pub fn destination_enum(&self) -> Destination {
        Destination::from_u8(self.destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StableSnapshot {
        let presence = PresenceIndicator::new();
        let hud = EphemeralHud::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0)
    }

    #[test]
    fn snapshot_tokens_and_presence() {
        let snap = sample();
        assert_eq!(snap.presence_state, 0);
        assert_eq!(BG_HEX, "#0B0C0F");
        assert_eq!(HUD_WIDTH, 720);
        assert_eq!(EVAPORATE_MS, 1500);
    }

    #[test]
    fn prefs_quantize_and_clamp() {
        let p = ShellPrefs::default().with_reduced_motion(true);
        assert_eq!(p.quantized_pulse(0.49), 0.0);
        assert_eq!(p.quantized_pulse(0.5), 1.0);
        assert_eq!(ShellPrefs::default().with_destination(9).destination, 3);
        assert_eq!(ShellPrefs::default().with_destination(2).destination_enum(), Destination::Pouvoirs);
    }

    #[test]
    fn dashboard_contains_labels() {
        let snap = sample();
        let d = snap.render_text_dashboard();
        assert!(d.contains("Veille ambiante"));
        assert!(d.contains("3/3"));
    }

    #[test]
    fn parity_with_shell_snapshot_all_fields_and_states() {
        use annora_ui::PresenceState;
        use annora_ui_shell::ShellSnapshot;

        for state in [
            PresenceState::Idle,
            PresenceState::Listening,
            PresenceState::Acting,
            PresenceState::Speaking,
            PresenceState::DecisionRequired,
            PresenceState::Urgent,
        ] {
            let mut presence = PresenceIndicator::new();
            presence.set_state(state);
            let mut hud = EphemeralHud::new();
            hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
            let cost = CostCounter::new(500, "EUR");
            let silence = SilenceBudgetTracker::new(3);

            let ours = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 1_000, 2);
            let theirs = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 1_000, 2);

            assert_eq!(ours.presence_state, theirs.presence_state, "{state:?}");
            assert_eq!(ours.presence_pulse, theirs.presence_pulse, "{state:?}");
            assert_eq!(ours.presence_attention, theirs.presence_attention, "{state:?}");
            assert_eq!(ours.hud_query, theirs.hud_query, "{state:?}");
            assert_eq!(ours.hud_response, theirs.hud_response, "{state:?}");
            assert_eq!(ours.hud_streaming, theirs.hud_streaming, "{state:?}");
            assert_eq!(ours.hud_validation, theirs.hud_validation, "{state:?}");
            assert_eq!(ours.hud_validation_text, theirs.hud_validation_text, "{state:?}");
            assert_eq!(ours.hud_validation_cost, theirs.hud_validation_cost, "{state:?}");
            assert_eq!(ours.cost_total_cents, theirs.cost_total_cents, "{state:?}");
            assert_eq!(ours.cost_budget_cents, theirs.cost_budget_cents, "{state:?}");
            assert_eq!(ours.cost_level, theirs.cost_level, "{state:?}");
            assert_eq!(ours.silence_remaining, theirs.silence_remaining, "{state:?}");
            assert_eq!(ours.silence_quota, theirs.silence_quota, "{state:?}");
            assert_eq!(ours.high_contrast, theirs.high_contrast, "{state:?}");
            assert_eq!(ours.light_mode, theirs.light_mode, "{state:?}");
            assert_eq!(ours.presence_a11y_label, theirs.presence_a11y_label, "{state:?}");
            assert_eq!(ours.presence_label(), theirs.presence_label(), "{state:?}");
            assert_eq!(
                ours.render_text_dashboard(),
                theirs.render_text_dashboard(),
                "dashboard divergent pour {state:?}"
            );
        }
    }
}
