//! Spike S5 — énergie : fenêtre cachée et présence, 0 réveil périodique.
//!
//! Mesure de sortie (plan refonte §6) : 0 réveil périodique ; mesures
//! CPU/GPU/batterie archivées. Preuve outillée ici : le runtime desktop
//! (`HeadlessRuntime`, même politique que la boucle winit) n'émet aucun
//! frame en veille, ne planifie aucun timer, et plafonne l'animation à
//! 30 fps en mode batterie. Critère labo restant : mesures ETW/Instruments/
//! powertop sur matériel réel.

use annora_ui_desktop::{HeadlessRuntime, RuntimeEvent};
use annora_ui_model::StableSnapshot;
use annora_ui::{CostCounter, EphemeralHud, PresenceIndicator, PresenceState, SilenceBudgetTracker};
use std::time::{Duration, Instant};

/// Construit un snapshot pour un état de présence donné.
pub fn snapshot_for(state: PresenceState) -> StableSnapshot {
    let mut presence = PresenceIndicator::new();
    presence.set_state(state);
    StableSnapshot::from_states(
        &presence,
        &EphemeralHud::new(),
        &CostCounter::new(500, "EUR"),
        &SilenceBudgetTracker::new(3),
        0,
        0,
    )
}

/// Simule `duration` de veille et compte les frames émis.
pub fn idle_frames_over(duration: Duration) -> (u64, u64) {
    let mut rt = HeadlessRuntime::new();
    rt.set_snapshot(snapshot_for(PresenceState::Idle));
    // Fenêtre masquée : aucun FrameDue accepté.
    let start = Instant::now();
    let mut pumps = 0u64;
    while start.elapsed() < duration {
        rt.push(RuntimeEvent::FrameDue);
        let out = rt.tick();
        if !out.is_empty() {
            pumps += 1;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    (rt.frames_emitted, pumps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_ui_desktop::runtime::FrameBudget;

    #[test]
    fn test_s5_hidden_idle_zero_frames_zero_wakeups() {
        let (frames, pumps) = idle_frames_over(Duration::from_millis(50));
        assert_eq!(frames, 0, "0 frame en veille cachée exigé");
        assert_eq!(pumps, 0, "0 réveil de rendu exigé");
    }

    #[test]
    fn test_s5_visible_idle_still_zero_frames() {
        // Présence Idle + visible + HUD masqué : statique, toujours 0 frame.
        let mut rt = HeadlessRuntime::new();
        rt.set_snapshot(snapshot_for(PresenceState::Idle));
        rt.push(RuntimeEvent::ShowHud);
        let _ = rt.tick();
        assert!(rt.visible);
        assert!(!rt.needs_frame(), "Idle statique ne doit pas planifier de frame");
    }

    #[test]
    fn test_s5_battery_budget_30fps_max() {
        let mut b = FrameBudget::battery_saver();
        assert_eq!(b.max_fps, 30);
        assert_eq!(b.min_frame_interval, Duration::from_millis(33));
        let t0 = Instant::now();
        let mut frames = 0u64;
        // 1 seconde simulée à 1 ms de granularité.
        for ms in 0..1000 {
            if b.poll(t0 + Duration::from_millis(ms), true) {
                frames += 1;
            }
        }
        assert!(frames <= 31, "30 fps max en batterie, obtenu {frames}");
        assert!(frames >= 29, "doit atteindre ~30 fps, obtenu {frames}");
    }

    #[test]
    fn test_s5_reduced_motion_halts_animation() {
        // Mouvement réduit : plus aucun frame animé même en écoute.
        let mut rt = HeadlessRuntime::new();
        rt.reduced_motion = true;
        rt.set_snapshot(snapshot_for(PresenceState::Listening));
        rt.push(RuntimeEvent::ShowHud);
        let _ = rt.tick();
        // Présence animée mais mouvement réduit → pulse quantifié ; le HUD statique
        // ne demande plus de frame de rendu continu.
        let snap = rt.snapshot.as_ref().unwrap();
        assert_eq!(rt.frames_emitted, 0);
        assert!(snap.presence_state == 1);
    }
}
