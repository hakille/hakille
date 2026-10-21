//! Conformance du shell ambient : snapshot, dashboard, honnêteté.
//!
//! La coquille affiche l'état réel (présence, HUD, coût, silence) et ne
//! ment jamais sur l'inférence. Baseline renderer-independent (ADR-009).

use hakille_ui::{
    ChannelSelector, CostCounter, EphemeralHud, InteractionContext, OnboardingManager,
    OutputChannel, PresenceIndicator, PresenceState, SilenceBudgetTracker,
};
use hakille_ui_shell::{
    ChannelDebugView, JournalRow, ShellSnapshot, filter_journal, honest_engine_rows, page_journal,
    reference_agent_cards,
};

#[test]
fn test_shell_snapshot_dashboard_and_honesty() {
    // 1. Snapshot : Acting + validation → attention + chip Coral + coût.
    let mut presence = PresenceIndicator::new();
    presence.set_state(PresenceState::Acting);
    let mut hud = EphemeralHud::new();
    hud.request_validation("wallet-spend-9", "Envoi 25.00 EUR", Some(2500), "Financial");
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    let snap = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 150, 2);

    assert_eq!(snap.presence_state, 2);
    assert!(snap.presence_attention);
    assert!(snap.hud_validation);
    assert!(snap.hud_validation_cost.contains("25.00"));
    assert!(snap.presence_a11y_label.contains("journal"));

    // Dashboard texte : même info que le renderer, lisible sans GPU.
    let dash = snap.render_text_dashboard();
    assert!(dash.contains("Action en cours"));
    assert!(dash.contains("#FF7A2F"));
    assert!(dash.contains("3/3"));

    // Compositor JSON : tokens + géométrie futuriste.
    let spec = hud.overlay_spec();
    assert_eq!(spec.width, 720);
    assert_eq!(spec.pos_y_offset, 120);
    let payload = snap.to_compositor_json(&spec);
    assert_eq!(payload["theme"]["coral"], "#FF7A2F");
    assert_eq!(payload["theme"]["bg"], "#0B0C0F");

    // 2. Honnêteté : les statuts sont dérivés d'artefacts réellement présents.
    // Fixture temporaire complète : chaque moteur RÉEL doit citer son
    // mécanisme, Mimi reste SIMULÉ (projection, pas de codec neural) et
    // UI-TARS reste INSPECTÉ (pas de forward VLM).
    let models_dir = std::env::temp_dir().join(format!(
        "hakille-conformance-shell-models-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&models_dir);
    std::fs::create_dir_all(&models_dir).unwrap();
    for name in [
        "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        "tokenizer.json",
        "ui-tars-2b-sft-q4_k_m.gguf",
        "mimi_candle.safetensors",
        "sensevoice_small.int8.onnx",
        "tokens.txt",
        "campplus_zh_en_16k_advanced.onnx",
        "all-minilm-l6-v2.onnx",
        "minilm_tokenizer.json",
        "silero_vad.onnx",
    ] {
        std::fs::write(models_dir.join(name), b"fixture").unwrap();
    }
    let rows = honest_engine_rows(&models_dir, 2.1, 10);
    assert_eq!(rows.len(), 7);
    for row in &rows {
        match row.status {
            hakille_ui_shell::InferenceStatus::Real => assert!(
                row.detail.contains("ort")
                    || row.detail.contains("Candle")
                    || row.detail.contains("RVQ")
                    || row.detail.contains("formantique")
                    || row.detail.contains("vision classique"),
                "moteur RÉEL sans preuve citée : {}",
                row.name
            ),
            _ => assert!(
                row.detail.contains("pas de forward"),
                "moteur non-RÉEL sans limite explicite : {}",
                row.name
            ),
        }
    }
    std::fs::remove_dir_all(&models_dir).ok();
    assert_eq!(reference_agent_cards().len(), 5);

    // 3. Canal : réunion → HUD ; journal filtrable et paginé.
    let view = ChannelDebugView::from_context(&InteractionContext {
        in_meeting: true,
        ..Default::default()
    });
    assert_eq!(view.channel, "EphemeralHud");

    let rows = vec![];
    let filtered = filter_journal(&rows, "test");
    assert!(filtered.is_empty());
    let _page: Vec<&JournalRow> = page_journal(&filtered, 0, 50);
}

#[test]
fn test_shell_onboarding_blocks_without_ack() {
    let mut m = OnboardingManager::new();
    assert!(!m.is_ready());
    m.inject_platform(
        "Native",
        "Windows",
        "11",
        &["Filesystem".to_string()],
        &["FullSystem".to_string()],
    );
    assert!(
        m.manifest()
            .guaranteed_limits
            .iter()
            .any(|l| l.title == "Plateforme hôte détectée")
    );
    m.acknowledge_limits();
    assert!(m.is_ready());

    // Defer existe : faible urgence + inattentif → reporté, pas d'interruption.
    let d = ChannelSelector::arbitrate(&InteractionContext {
        urgency: hakille_ui::UrgencyLevel::Low,
        user_attentive_to_screen: false,
        device_type: hakille_ui::DeviceType::Desktop,
        ..Default::default()
    });
    assert_eq!(d.channel, OutputChannel::Defer);
}
