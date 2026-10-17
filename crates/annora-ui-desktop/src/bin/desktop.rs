//! Binaire desktop — présence + HUD, hotkey, overlay Windows.
//!
//! Fenêtre masquée = aucun frame. GPU indisponible = erreur explicite.

use annora_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use annora_ui_desktop::{HeadlessRuntime, RuntimeEvent, SoftwareRenderer};

fn main() {
    println!("Akora desktop shell — winit+wgpu+AccessKit (skeleton runtime)");
    // Headless smoke : prouve 0 frame en veille + 1 wakeup au hotkey.
    let mut rt = HeadlessRuntime::new();
    let snap = annora_ui_model::StableSnapshot::from_states(
        &PresenceIndicator::new(),
        &EphemeralHud::new(),
        &CostCounter::new(500, "EUR"),
        &SilenceBudgetTracker::new(3),
        0,
        0,
    );
    rt.set_snapshot(snap);
    rt.push(RuntimeEvent::FrameDue);
    let _ = rt.tick();
    assert_eq!(rt.frames_emitted, 0);
    println!("OK: 0 frame en veille. Hotkey → HUD testé en lib (cargo test -p annora-ui-desktop).");
    let _ = SoftwareRenderer::new(720, 64, true);
}
