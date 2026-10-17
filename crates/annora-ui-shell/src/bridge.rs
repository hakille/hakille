//! Rust → Slint property bridge. Pure, testable, no event loop here.

use annora_ui::{
    AcrylicOverlayWindowSpec, CostCounter, EphemeralHud, HudState, InteractionContext,
    OutputChannel, PresenceIndicator, PresenceState, SilenceBudgetTracker,
};
use serde_json::Value;

/// Design tokens — single source of truth (vision §13).
pub const BG_HEX: &str = "#0B0C0F";
pub const TEXT_HEX: &str = "#ECEEF2";
pub const CORAL_HEX: &str = "#FF7A2F";
pub const BORDER_HEX: &str = "#262A33";
pub const HUD_WIDTH: u32 = 720;
pub const HUD_TOP_OFFSET: i32 = 120;
pub const HUD_BLUR: u32 = 30;
pub const HUD_RADIUS: u32 = 12;
pub const EVAPORATE_MS: u32 = 1500;

/// Slint-ready snapshot of the whole ambient shell.
#[derive(Debug, Clone)]
pub struct ShellSnapshot {
    pub presence_state: u8, // 0 Idle, 1 Listening, 2 Acting
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
    /// 0 normal, 1 near >=80%, 2 exceeded
    pub cost_level: u8,
    pub silence_remaining: u32,
    pub silence_quota: u32,
    /// Accessibilité §21g : fort contraste (texte #FFFFFF, bordures #FFFFFF).
    pub high_contrast: bool,
    /// Thème clair complet (spec §3) : fonds #F6F4F0, braise #C2410C.
    pub light_mode: bool,
    /// Libellé lecteur d'écran du point de présence.
    pub presence_a11y_label: String,
}

impl ShellSnapshot {
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
            HudState::Hidden => (
                String::new(),
                String::new(),
                false,
                false,
                String::new(),
                String::new(),
            ),
            HudState::Prompting { query, .. } => (
                query.clone(),
                String::new(),
                false,
                false,
                String::new(),
                String::new(),
            ),
            HudState::StreamingResponse {
                query,
                response_text,
                is_final,
            } => (
                query.clone(),
                response_text.clone(),
                !is_final,
                false,
                String::new(),
                String::new(),
            ),
            HudState::ValidationRequired {
                description,
                cost_cents,
                ..
            } => (
                String::new(),
                String::new(),
                false,
                true,
                description.clone(),
                cost_cents
                    .map(|c| format!("{:.2} EUR", c as f64 / 100.0))
                    .unwrap_or_default(),
            ),
            HudState::Evaporating { last_text, .. } => (
                String::new(),
                last_text.clone(),
                false,
                false,
                String::new(),
                String::new(),
            ),
        };
        let level = if cost.is_budget_exceeded() {
            2
        } else if cost.is_near_budget_limit() {
            1
        } else {
            0
        };
        let presence_a11y_label = match presence.state() {
            PresenceState::Idle => "Akora : veille ambiante".to_string(),
            PresenceState::Listening => "Akora : écoute active".to_string(),
            PresenceState::Acting => {
                "Akora : action en cours — activer pour ouvrir le journal".to_string()
            }
            PresenceState::Speaking => "Akora : parole en cours".to_string(),
            PresenceState::DecisionRequired => {
                "Akora : décision requise — activer pour voir".to_string()
            }
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

    /// Active le fort contraste (AccessibilityProfile.high_contrast).
    pub fn with_high_contrast(mut self, enabled: bool) -> Self {
        self.high_contrast = enabled;
        self
    }

    /// Bascule en thème clair complet (spec §3).
    pub fn with_light_mode(mut self, enabled: bool) -> Self {
        self.light_mode = enabled;
        self
    }

    /// Human-readable honest labels for the shell (used by Slint + tests).
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

    /// Dashboard texte intégral — fallback Redox orbital / Web canvas / logs.
    /// Même information que la fenêtre Slint, sans GPU : lisible au clavier,
    /// compatible lecteur d'écran, copiable dans un ticket.
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
            if self.presence_attention {
                " [● attention]"
            } else {
                ""
            },
            hud_block,
            self.cost_total_cents as f64 / 100.0,
            self.cost_budget_cents as f64 / 100.0,
            cost_level,
            self.silence_remaining,
            self.silence_quota,
            if self.high_contrast {
                "fort"
            } else {
                "standard"
            },
        )
    }

    pub fn to_compositor_json(&self, hud_spec: &AcrylicOverlayWindowSpec) -> Value {
        serde_json::json!({
            "theme": {"bg": BG_HEX, "text": TEXT_HEX, "coral": CORAL_HEX, "border": BORDER_HEX},
            "presence": {"state": self.presence_state, "pulse": self.presence_pulse, "attention": self.presence_attention, "label": self.presence_label()},
            "hud": {"query": self.hud_query, "response": self.hud_response, "streaming": self.hud_streaming,
                    "validation": self.hud_validation, "validation_text": self.hud_validation_text, "validation_cost": self.hud_validation_cost},
            "cost": {"total_cents": self.cost_total_cents, "budget_cents": self.cost_budget_cents, "level": self.cost_level},
            "silence": {"remaining": self.silence_remaining, "quota": self.silence_quota},
            "window": {"width": hud_spec.width, "height": hud_spec.height, "top_offset": hud_spec.pos_y_offset,
                       "opacity": hud_spec.acrylic_opacity, "blur": hud_spec.blur_radius, "radius": hud_spec.corner_radius,
                       "visible": hud_spec.is_visible},
        })
    }
}

/// ChannelSelector debug view-model for the Slint ChannelDebug panel.
#[derive(Debug, Clone)]
pub struct ChannelDebugView {
    pub channel: String,
    pub rationale: String,
    pub haptic: Option<String>,
}

impl ChannelDebugView {
    pub fn from_context(ctx: &InteractionContext) -> Self {
        let d = annora_ui::ChannelSelector::arbitrate(ctx);
        let channel = match d.channel {
            OutputChannel::VoiceNormal => "VoiceNormal",
            OutputChannel::VoiceWhisper => "VoiceWhisper",
            OutputChannel::EphemeralHud => "EphemeralHud",
            OutputChannel::Haptic => "Haptic",
            OutputChannel::AmbientPulse => "AmbientPulse",
            OutputChannel::Defer => "Defer",
        }
        .to_string();
        Self {
            channel,
            rationale: d.rationale,
            haptic: d.haptic_feedback.map(|h| format!("{h:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_tokens_and_presence() {
        let presence = PresenceIndicator::new();
        let hud = EphemeralHud::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 1000, 0);
        assert_eq!(snap.presence_state, 0);
        assert_eq!(snap.presence_pulse, 0.0);
        assert_eq!(BG_HEX, "#0B0C0F");
        assert_eq!(CORAL_HEX, "#FF7A2F");
        assert_eq!(HUD_WIDTH, 720);
        assert_eq!(EVAPORATE_MS, 1500);
        assert_eq!(snap.cost_level, 0);
        assert_eq!(snap.silence_remaining, 3);
    }

    #[test]
    fn test_snapshot_validation_chip_exposes_coral_cost() {
        let mut hud = EphemeralHud::new();
        hud.request_validation("wallet-spend-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        let presence = PresenceIndicator::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 1);
        assert!(snap.hud_validation);
        assert!(snap.hud_validation_cost.contains("25.00"));
        assert!(snap.presence_attention);
    }

    #[test]
    fn test_channel_debug_meeting_routes_to_hud() {
        let ctx = InteractionContext {
            in_meeting: true,
            ..Default::default()
        };
        let view = ChannelDebugView::from_context(&ctx);
        assert_eq!(view.channel, "EphemeralHud");
        assert!(!view.rationale.is_empty());
    }

    #[test]
    fn test_snapshot_a11y_and_dashboard() {
        let mut presence = PresenceIndicator::new();
        presence.set_state(PresenceState::Acting);
        let hud = EphemeralHud::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 150, 2)
            .with_high_contrast(true);
        assert!(snap.high_contrast);
        assert!(snap.presence_a11y_label.contains("journal"));
        let dash = snap.render_text_dashboard();
        assert!(dash.contains("Action en cours"));
        assert!(dash.contains("0.00 EUR"));
        assert!(dash.contains("3/3"));
        assert!(dash.contains("fort"));
    }

    #[test]
    fn test_dashboard_shows_validation_chip() {
        let mut hud = EphemeralHud::new();
        hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        let presence = PresenceIndicator::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
        let dash = snap.render_text_dashboard();
        assert!(dash.contains("#FF7A2F"));
        assert!(dash.contains("25.00"));
    }

    #[test]
    fn test_light_mode_defaults_off() {
        let presence = PresenceIndicator::new();
        let hud = EphemeralHud::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
        assert!(!snap.light_mode);
        assert!(!snap.high_contrast);
        let lit = snap.with_light_mode(true);
        assert!(lit.light_mode);
        let payload = lit.to_compositor_json(&hud.overlay_spec());
        assert_eq!(payload["theme"]["bg"], "#0B0C0F"); // payload porte le sombre ; Slint applique le clair
    }
}

/// Préférences shell additives (navigation + mouvement réduit).
///
/// `ShellSnapshot` et `from_states` sont intacts : ce struct porte les champs
/// que le pont headless ne doit pas connaître. Le rendu Slint les consomme
/// via `render::apply_prefs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellPrefs {
    /// Mouvement réduit (accessibilité) : le pulse présence est quantifié 0/1.
    pub reduced_motion: bool,
    /// Destination : 0 Journal, 1 Mémoire, 2 Pouvoirs, 3 Réglages.
    pub destination: u8,
}

impl Default for ShellPrefs {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            destination: 0,
        }
    }
}

impl ShellPrefs {
    /// Builder : active/désactive le mouvement réduit.
    pub fn with_reduced_motion(mut self, enabled: bool) -> Self {
        self.reduced_motion = enabled;
        self
    }

    /// Builder : destination initiale (bornée 0..=3).
    pub fn with_destination(mut self, destination: u8) -> Self {
        self.destination = destination.min(3);
        self
    }

    /// Pulse Slint : quantifié 0/1 si mouvement réduit (seuil 0.5), sinon
    /// valeur brute bornée 0..1. Utilisée par `render::apply_prefs`.
    pub fn quantized_pulse(&self, pulse: f32) -> f32 {
        if self.reduced_motion {
            if pulse >= 0.5 { 1.0 } else { 0.0 }
        } else {
            pulse.clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod prefs_tests {
    use super::ShellPrefs;

    #[test]
    fn reduced_motion_quantizes_pulse_to_binary() {
        let prefs = ShellPrefs::default().with_reduced_motion(true);
        for v in [0.0, 0.1, 0.49, 0.5, 0.7, 1.0, 2.0, -1.0] {
            let q = prefs.quantized_pulse(v);
            assert!(q == 0.0 || q == 1.0, "pulse {v} -> {q}");
        }
        assert_eq!(prefs.quantized_pulse(0.49), 0.0);
        assert_eq!(prefs.quantized_pulse(0.5), 1.0);
    }

    #[test]
    fn full_motion_passes_pulse_through_clamped() {
        let prefs = ShellPrefs::default();
        assert!(!prefs.reduced_motion);
        assert_eq!(prefs.quantized_pulse(0.37), 0.37);
        assert_eq!(prefs.quantized_pulse(2.0), 1.0);
        assert_eq!(prefs.quantized_pulse(-1.0), 0.0);
    }

    #[test]
    fn destination_defaults_to_journal_and_clamps() {
        assert_eq!(ShellPrefs::default().destination, 0);
        assert_eq!(ShellPrefs::default().with_destination(2).destination, 2);
        assert_eq!(ShellPrefs::default().with_destination(9).destination, 3);
    }
}
