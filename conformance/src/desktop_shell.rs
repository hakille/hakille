//! Conformance e2e — shell desktop natif (winit+wgpu+AccessKit).
//!
//! Vérifie la parité bout-en-bout du nouveau shell :
//! snapshot → scène (fingerprint stable ×2 thèmes), accessibilité complète,
//! quiescence (0 frame en veille), kernel fail-closed, i18n FR/EN/AR/JA,
//! parcours complet demande → confirmation → journal → annulation.

#[cfg(test)]
mod tests {
    use annora_ui::{CostCounter, EphemeralHud, PresenceIndicator, PresenceState, SilenceBudgetTracker};
    use annora_ui_desktop::a11y::{audit_interactive_labels, build_shell_tree};
    use annora_ui_desktop::kernel_bridge::{BridgeError, KernelBridge, KernelStub};
    use annora_ui_desktop::renderer::{hud_scene, SoftwareRenderer, WgpuRenderer};
    use annora_ui_desktop::scene::SceneRenderer;
    use annora_ui_desktop::runtime::RuntimeEvent;
    use annora_ui_desktop::text::SimpleTextEngine;
    use annora_ui_desktop::text::TextEngine;
    use annora_ui_desktop::theme::Theme;
    use annora_ui_desktop::{HeadlessRuntime, Locale, Strings};
    use annora_ui_model::{JournalRow, PowerRow, StableSnapshot, UiEvent};

    fn snapshot() -> StableSnapshot {
        let mut presence = PresenceIndicator::new();
        presence.set_state(PresenceState::DecisionRequired);
        let mut hud = EphemeralHud::new();
        hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        StableSnapshot::from_states(
            &presence,
            &hud,
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            1,
        )
    }

    fn rows() -> Vec<JournalRow> {
        vec![JournalRow {
            id: "j-1".into(),
            time: "14:32".into(),
            actor: "Akora".into(),
            action: "Envoi du récapitulatif à Awa".into(),
            reason: "Demandé".into(),
            confidence_pct: 92,
            cost: "0.00 EUR".into(),
            verified: true,
            can_revert: true,
        }]
    }

    #[test]
    fn desktop_scene_fingerprint_stable_per_theme() {
        let snap = snapshot();
        let dark_a = hud_scene(&snap, true).fingerprint();
        let dark_b = hud_scene(&snap, true).fingerprint();
        let light = hud_scene(&snap, false).fingerprint();
        assert_eq!(dark_a, dark_b, "scène sombre non déterministe");
        assert_ne!(dark_a, light, "les thèmes doivent produire des scènes distinctes");
    }

    #[test]
    fn desktop_software_render_parity_golden() {
        let snap = snapshot();
        let scene = hud_scene(&snap, true);
        let mut a = SoftwareRenderer::new(720, 64, true);
        let mut b = SoftwareRenderer::new(720, 64, true);
        let pa = a.submit(&scene).unwrap();
        let pb = b.submit(&scene).unwrap();
        assert_eq!(pa, pb, "pixels logiciels doivent être déterministes");
        assert_eq!(pa.len(), 720 * 64 * 4);
    }

    #[test]
    fn desktop_gpu_degradation_is_explicit() {
        let mut w = WgpuRenderer::new(720, 64, true);
        let status = w.try_init_gpu();
        assert!(
            !matches!(status, annora_ui_desktop::GpuStatus::Failed(_)),
            "headless doit dégrader explicitement vers le logiciel"
        );
        let snap = snapshot();
        let scene = hud_scene(&snap, true);
        assert!(w.render(&scene).is_ok(), "fallback logiciel doit rendre");
    }

    #[test]
    fn desktop_a11y_zero_interactive_without_label() {
        let snap = snapshot();
        let powers = vec![
            PowerRow { group: "Voir".into(), scope: "Camera".into(), enabled: true, policy_locked: false },
            PowerRow { group: "Agir".into(), scope: "Gui".into(), enabled: true, policy_locked: true },
        ];
        let update = build_shell_tree(&snap, &rows(), &powers);
        assert!(audit_interactive_labels(&update).is_empty());
        // La validation porte le coût dans son label.
        let valid = update.nodes.iter().find(|(i, _)| i.0 == 22).unwrap();
        assert!(valid.1.label().unwrap().contains("25.00"));
    }

    #[test]
    fn desktop_quiescence_and_hotkey_lifecycle() {
        let mut rt = HeadlessRuntime::new();
        rt.set_snapshot(StableSnapshot::from_states(
            &PresenceIndicator::new(),
            &EphemeralHud::new(),
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            0,
        ));
        rt.push(RuntimeEvent::FrameDue);
        assert!(rt.tick().is_empty());
        assert_eq!(rt.frames_emitted, 0);
        rt.push(RuntimeEvent::HotkeyPressed);
        let out = rt.tick();
        assert!(rt.visible);
        assert!(out.contains(&RuntimeEvent::ShowHud));
        assert_eq!(rt.wakeups, 1);
    }

    #[test]
    fn desktop_kernel_fail_closed_without_daemon() {
        struct Refuse;
        impl KernelStub for Refuse {
            fn submit(
                &mut self,
                _msg: annora_ui_protocol::UiMessage,
            ) -> Result<annora_ui_protocol::KernelToUi, BridgeError> {
                Err(BridgeError::DaemonRequired("a11y gate".into()))
            }
        }
        let mut bridge = KernelBridge::new(Refuse);
        // Toute action sensible est refusée ; les événements locaux restent locaux.
        assert!(bridge.dispatch(UiEvent::CardApprove { card_id: 1 }).is_err());
        assert!(bridge.dispatch(UiEvent::PresenceTapped).unwrap().is_none());
        assert!(bridge.outbox.is_empty());
    }

    #[test]
    fn desktop_i18n_all_locales_no_tofu() {
        let engine = SimpleTextEngine::new();
        for locale in [Locale::Fr, Locale::En, Locale::Ar, Locale::Ja] {
            let strings = Strings::new(locale);
            for key in ["hud.prompt", "hud.validate", "journal.title", "presence.decision"] {
                let text = strings.get(key);
                assert!(!text.is_empty(), "{locale:?}/{key} vide");
                assert!(!text.contains("〈manquant〉"), "{locale:?}/{key} manquant");
                let run = engine.shape(&text, 16.0, None);
                assert!(run.width_px > 0.0);
                assert_eq!(run.is_rtl, locale.is_rtl(), "RTL incorrect pour {locale:?}");
            }
        }
    }

    #[test]
    fn desktop_theme_contrast_wcag_aa() {
        let dark = Theme::dark();
        assert!(dark.contrast_ratio(dark.text1, dark.bg) >= 4.5);
        let light = Theme::light();
        assert!(light.contrast_ratio(light.text1, light.bg) >= 4.5);
        let hc = Theme::dark().with_high_contrast(true);
        assert!(hc.contrast_ratio(hc.text1, hc.bg) >= 7.0);
    }

    #[test]
    fn desktop_e2e_authorized_journey() {
        use annora_ui_desktop::RealDaemonBridge;
        let mut bridge = KernelBridge::new(RealDaemonBridge::local_authorized());
        bridge.kernel.stage_card(3, "Envoi 25.00 EUR");
        let approved = bridge.dispatch(UiEvent::CardApprove { card_id: 3 }).unwrap().unwrap();
        assert!(matches!(approved, annora_ui_model::KernelEvent::JournalAppended { .. }));
        let undo = bridge.dispatch(UiEvent::JournalRevert { index: 1 }).unwrap().unwrap();
        assert!(matches!(undo, annora_ui_model::KernelEvent::JournalAppended { .. }));
        assert!(bridge.dispatch(UiEvent::HudCancelled).unwrap().is_none());
        assert_eq!(bridge.outbox.len(), 2);
    }
}
