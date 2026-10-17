//! Galerie `akora-gallery` (spec §28) : chaque composant dans tous ses
//! états, thèmes, langues et tailles de texte. Base de régression visuelle.
//! `cargo run -p annora-ui-shell --features render --bin gallery`

#[cfg(feature = "render")]
fn main() -> Result<(), String> {
    use annora_ui::{
        CostCounter, EphemeralHud, PresenceIndicator, PresenceState, SilenceBudgetTracker,
    };
    use annora_ui_shell::{ShellSnapshot, reference_agent_cards};

    // Matrice d'états : 6 présences × (hud caché, prompt, validation).
    let states = [
        PresenceState::Idle,
        PresenceState::Listening,
        PresenceState::Acting,
        PresenceState::Speaking,
        PresenceState::DecisionRequired,
        PresenceState::Urgent,
    ];
    for state in states {
        let mut presence = PresenceIndicator::new();
        presence.set_state(state);
        let mut hud = EphemeralHud::new();
        if state == PresenceState::DecisionRequired {
            hud.request_validation("gallery-1", "Exemple 12,00 EUR", Some(1200), "Test");
        }
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 1000, 1);
        let dash = snap.render_text_dashboard();
        println!("--- {:?} ---\n{dash}\n", state);
    }
    println!("Cartes agents : {}", reference_agent_cards().len());
    println!("Rendu Slint : même matrice via AnnoraWindow (voir render::run_window).");
    Ok(())
}

#[cfg(not(feature = "render"))]
fn main() {
    eprintln!("gallery nécessite --features render (Slint natif, ADR-007).");
    std::process::exit(2);
}
