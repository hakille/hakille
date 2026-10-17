//! Démo lançable du shell Slint : `cargo run -p annora-ui-shell --features render --bin shell-demo`.
//!
//! Sans `render`, ce binaire refuse de se lancer avec un message clair
//! (le bridge headless reste testé en CI rapide).

#[cfg(feature = "render")]
fn main() -> Result<(), String> {
    use annora_ui::{
        CostCounter, EphemeralHud, PresenceIndicator, PresenceState, SilenceBudgetTracker,
    };
    use annora_ui_shell::{ShellPlatform, ShellSnapshot};

    let mut presence = PresenceIndicator::new();
    presence.set_state(PresenceState::Listening);
    let mut hud = EphemeralHud::new();
    hud.open_prompt();
    hud.insert_text("Transférer 25 EUR pour déjeuner");
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    let platform = ShellPlatform::auto();
    println!(
        "Annora shell-demo — {} | hotkey {} | {}",
        platform.tray_backend(),
        platform.hotkey(),
        platform.overlay_backend()
    );

    let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 1000, 0);
    annora_ui_shell::render::run_window(snap)
}

#[cfg(not(feature = "render"))]
fn main() {
    eprintln!("shell-demo nécessite --features render (Slint natif, ADR-007).");
    eprintln!("Essai headless: cargo test -p annora-ui-shell");
    std::process::exit(2);
}
