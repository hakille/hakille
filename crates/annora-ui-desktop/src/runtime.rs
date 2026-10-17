//! Runtime desktop — winit + boucle pilotée événements, 0 frame en veille.
//!
//! S1 : hotkey → visible p95 ≤ 100 ms, 0 frame fenêtre masquée.
//! S5 : 0 réveil périodique, animation ≤30fps batterie, arrêt si réduit.
//!
//! `HeadlessRuntime` est la référence testable (même politique que `WinitRuntime`).

use annora_ui_model::{StableSnapshot, UiEvent};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeEvent {
    HotkeyPressed,
    ShowHud,
    HideHud,
    UiEvent(UiEvent),
    FrameDue,
    GpuLost,
    DpiChanged(f32),
}

#[derive(Debug, Clone)]
pub struct FrameBudget {
    /// Intervalle min entre frames animées (30fps batterie = 33ms).
    pub min_frame_interval: Duration,
    pub max_fps: u32,
    pub last_frame: Option<Instant>,
}

impl FrameBudget {
    pub fn full() -> Self {
        Self {
            min_frame_interval: Duration::from_millis(8),
            max_fps: 120,
            last_frame: None,
        }
    }
    pub fn battery_saver() -> Self {
        Self {
            min_frame_interval: Duration::from_millis(33),
            max_fps: 30,
            last_frame: None,
        }
    }
    /// `true` ssi un nouveau frame est autorisé maintenant.
    pub fn poll(&mut self, now: Instant, needs_frame: bool) -> bool {
        if !needs_frame {
            return false;
        }
        match self.last_frame {
            None => {
                self.last_frame = Some(now);
                true
            }
            Some(last) if now.duration_since(last) >= self.min_frame_interval => {
                self.last_frame = Some(now);
                true
            }
            _ => false,
        }
    }
}

/// Runtime headless — politique identique au runtime winit réel.
pub struct HeadlessRuntime {
    pub visible: bool,
    pub snapshot: Option<StableSnapshot>,
    pub queue: VecDeque<RuntimeEvent>,
    pub budget: FrameBudget,
    pub frames_emitted: u64,
    pub wakeups: u64,
    pub reduced_motion: bool,
}

impl HeadlessRuntime {
    pub fn new() -> Self {
        Self {
            visible: false,
            snapshot: None,
            queue: VecDeque::new(),
            budget: FrameBudget::full(),
            frames_emitted: 0,
            wakeups: 0,
            reduced_motion: false,
        }
    }

    pub fn push(&mut self, ev: RuntimeEvent) {
        // Fenêtre masquée : aucun frame planifié (S1/S5).
        if !self.visible && matches!(ev, RuntimeEvent::FrameDue) {
            return;
        }
        self.queue.push_back(ev);
    }

    /// `true` ssi un frame est nécessaire (visible + animé ou HUD vivant).
    pub fn needs_frame(&self) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(s) = &self.snapshot {
            if s.hud_streaming || s.hud_validation {
                return true;
            }
            if !self.reduced_motion && s.presence_pulse > 0.01 && s.presence_state != 0 {
                return true;
            }
        }
        // Événement en attente → un frame pour le consommer.
        !self.queue.is_empty()
    }

    pub fn tick(&mut self) -> Vec<RuntimeEvent> {
        let mut out = Vec::new();
        while let Some(ev) = self.queue.pop_front() {
            match ev {
                RuntimeEvent::HotkeyPressed => {
                    self.visible = !self.visible;
                    out.push(if self.visible {
                        RuntimeEvent::ShowHud
                    } else {
                        RuntimeEvent::HideHud
                    });
                    // Hotkey → visible doit produire au plus 1 frame (p95 ≤100ms mesuré en intégration).
                    if self.visible {
                        self.wakeups += 1;
                    }
                }
                RuntimeEvent::ShowHud => {
                    self.visible = true;
                }
                RuntimeEvent::HideHud => {
                    self.visible = false;
                }
                RuntimeEvent::FrameDue => {
                    let now = Instant::now();
                    if self.budget.poll(now, self.needs_frame()) {
                        self.frames_emitted += 1;
                        out.push(RuntimeEvent::FrameDue);
                    }
                }
                other => out.push(other),
            }
        }
        out
    }

    pub fn set_snapshot(&mut self, snap: StableSnapshot) {
        self.snapshot = Some(snap);
    }
}

impl Default for HeadlessRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration fenêtre winit — HUD 720×64 transparent topmost.
#[derive(Debug, Clone)]
pub struct WinitConfig {
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    pub decorations: bool,
    pub topmost: bool,
    pub visible: bool,
}

impl Default for WinitConfig {
    fn default() -> Self {
        Self {
            width: 720,
            height: 64,
            transparent: true,
            decorations: false,
            topmost: true,
            visible: false,
        }
    }
}

impl WinitConfig {
    pub fn window_attributes(&self) -> winit::window::WindowAttributes {
        use winit::window::WindowLevel;
        winit::window::Window::default_attributes()
            .with_title("Akora")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.width as f64,
                self.height as f64,
            ))
            .with_transparent(self.transparent)
            .with_decorations(self.decorations)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_visible(self.visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_ui::{
        CostCounter, EphemeralHud, PresenceIndicator, PresenceState, SilenceBudgetTracker,
    };

    fn snap_idle() -> StableSnapshot {
        StableSnapshot::from_states(
            &PresenceIndicator::new(),
            &EphemeralHud::new(),
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            0,
        )
    }

    #[test]
    fn hidden_window_emits_zero_frames() {
        let mut r = HeadlessRuntime::new();
        r.set_snapshot(snap_idle());
        assert!(!r.needs_frame());
        r.push(RuntimeEvent::FrameDue);
        assert!(r.queue.is_empty());
        let out = r.tick();
        assert!(out.is_empty());
        assert_eq!(r.frames_emitted, 0);
    }

    #[test]
    fn hotkey_toggles_and_counts_one_wakeup() {
        let mut r = HeadlessRuntime::new();
        r.set_snapshot(snap_idle());
        r.push(RuntimeEvent::HotkeyPressed);
        let out = r.tick();
        assert!(r.visible);
        assert!(out.contains(&RuntimeEvent::ShowHud));
        assert_eq!(r.wakeups, 1);
    }

    #[test]
    fn frame_budget_throttles_to_30fps_battery() {
        let mut b = FrameBudget::battery_saver();
        let t0 = Instant::now();
        assert!(b.poll(t0, true));
        assert!(!b.poll(t0, true));
        assert!(b.poll(t0 + Duration::from_millis(40), true));
    }

    #[test]
    fn quiescence_idle_hidden() {
        // S4 quiescence : Idle + Hidden → pas de frame.
        let mut r = HeadlessRuntime::new();
        let mut presence = PresenceIndicator::new();
        presence.set_state(PresenceState::Idle);
        let snap = StableSnapshot::from_states(
            &presence,
            &EphemeralHud::new(),
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            0,
        );
        r.set_snapshot(snap);
        assert!(!r.needs_frame());
    }
}
