use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp};
use hakille_perception::{CameraPipeline, LightingCondition, UserPresence, WorldModel};
use hakille_substrate::{CameraConfig, CameraFacing, CameraFrame, ImageFormat, Substrate};
use hakille_substrate_windows::WindowsSubstrate;
use hakille_ui::{
    ChannelSelector, DeviceType, EphemeralHud, HudState, InitiativeOrigin, InteractionContext,
    OutputChannel, PlatformAdapter, PresenceState, SilenceBudgetTracker, UrgencyLevel,
};

#[tokio::test]
async fn test_ambient_multimodal_perception_and_interface_pipeline() {
    // =========================================================================
    // 1. Substrate Camera & Sensor Layer
    // =========================================================================
    let substrate = WindowsSubstrate::new();
    let authority = CapabilityAuthority::new();
    let camera_cap = authority
        .create_root_token(
            vec![CapabilityScope::Camera, CapabilityScope::Sensor],
            vec![CapabilityAction::Read],
            Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        )
        .expect("Root token creation failed");

    let camera_config = CameraConfig {
        width: 1280,
        height: 720,
        fps: 30,
        facing: CameraFacing::Front,
    };
    // Honnêteté substrate : le backend MediaFoundation Windows est branché ;
    // s'il est indisponible (pas de device, permission refusée) l'erreur
    // nomme le backend, jamais de frames synthétiques. Les autres OS
    // retournent encore "camera not implemented". Si un backend réel répond,
    // la branche Ok capture une vraie frame.
    match substrate
        .start_camera_capture(camera_config.clone(), &camera_cap)
        .await
    {
        Ok(handle) => {
            let raw_frame = substrate
                .capture_camera_frame(handle, &camera_cap)
                .await
                .expect("Camera frame capture failed");
            assert!(raw_frame.width > 0);
            assert!(!raw_frame.data.is_empty());
        }
        Err(hakille_core::HakilleError::SubstrateError(msg)) => {
            assert!(
                msg.contains("camera not implemented") || msg.contains("camera unavailable"),
                "erreur honnête nommant le backend, obtenue : {msg}"
            );
            assert!(
                !camera_config.is_available(),
                "is_available doit refléter l'absence de backend"
            );
        }
        Err(other) => panic!("erreur caméra inattendue : {other:?}"),
    }

    // =========================================================================
    // 2. Multimodal Perception Pipeline (Zero Raw Pixel Leak Guarantee)
    // =========================================================================
    // Fixture de test explicite (pas une frame substrate) : le pipeline réel
    // mesure cette image et le hash cryptographique d'attestation est vérifié.
    let fixture = CameraFrame {
        width: 64,
        height: 48,
        data: (0..64 * 48 * 3).map(|i| (i % 251) as u8).collect(),
        format: ImageFormat::Rgb8,
        timestamp_ms: 0,
    };
    let mut camera_pipeline = CameraPipeline::new(camera_config);
    let mut visual_context = camera_pipeline.process_frame(&fixture);

    // Frame hash must be computed for cryptographic audit without persisting raw pixels (§6)
    assert_ne!(visual_context.privacy_preserving_frame_hash, [0u8; 32]);
    assert_eq!(camera_pipeline.frames_processed(), 1);

    // =========================================================================
    // 3. Situational WorldModel Updates
    // =========================================================================
    let mut world_model = WorldModel::new("SovereignWorkstation");
    // Pas de caméra matérielle : contexte posé explicitement (utilisateur
    // présent, plein jour, 1 personne) pour la suite du pipeline.
    visual_context.user_presence = UserPresence::Present;
    visual_context.estimated_distance_m = 0.8;
    visual_context.user_gaze_focused_on_device = true;
    visual_context.people_count = 1;
    visual_context.lighting = LightingCondition::Daylight;
    camera_pipeline.update_world_model(&visual_context, &mut world_model);

    // By default single user in daylight/adequate room is private
    assert!(world_model.state().user_present);
    assert_eq!(world_model.state().ambient_noise_level_db, 35.0);
    assert!(world_model.is_private_environment());

    // =========================================================================
    // 4. Dynamic Channel Selector Arbitration (§21a)
    // =========================================================================
    // Scenario A: Private room, user looking at desktop screen -> Ephemeral HUD
    let ctx_desktop = InteractionContext {
        ambient_noise_db: world_model.state().ambient_noise_level_db,
        third_parties_present: world_model.state().third_parties_present,
        in_meeting: world_model.state().in_meeting,
        user_attentive_to_screen: world_model.state().user_attentive,
        user_wearing_headphones: false,
        device_type: DeviceType::Desktop,
        urgency: UrgencyLevel::Normal,
        is_night_time: false,
    };
    let decision_a = ChannelSelector::arbitrate(&ctx_desktop);
    assert_eq!(decision_a.channel, OutputChannel::EphemeralHud);
    assert!(
        decision_a
            .rationale
            .contains("Spotlight-like ephemeral HUD")
    );

    // Scenario B: Colleague enters room (camera/diarization detects multiple people)
    world_model.update_visual_presence(true, false, 2, 1.8, LightingCondition::Daylight.as_str());
    assert!(world_model.state().third_parties_present);
    assert!(!world_model.is_private_environment());

    let ctx_crowded = InteractionContext {
        ambient_noise_db: 45.0,
        third_parties_present: world_model.state().third_parties_present,
        in_meeting: false,
        user_attentive_to_screen: false,
        user_wearing_headphones: false,
        device_type: DeviceType::Desktop,
        urgency: UrgencyLevel::Normal,
        is_night_time: false,
    };
    let decision_b = ChannelSelector::arbitrate(&ctx_crowded);
    // Must protect privacy by suppressing voice output
    assert_eq!(decision_b.channel, OutputChannel::EphemeralHud);
    assert!(
        decision_b
            .rationale
            .contains("Third parties present: silent ephemeral HUD")
    );

    // Scenario C: Wearable with low urgency task completion -> Haptic vibration
    let ctx_wearable = InteractionContext {
        ambient_noise_db: 50.0,
        third_parties_present: false,
        in_meeting: false,
        user_attentive_to_screen: false,
        user_wearing_headphones: false,
        device_type: DeviceType::Wearable,
        urgency: UrgencyLevel::Low,
        is_night_time: false,
    };
    let decision_c = ChannelSelector::arbitrate(&ctx_wearable);
    assert_eq!(decision_c.channel, OutputChannel::Haptic);

    // =========================================================================
    // 5. Ephemeral HUD & Braise #FF7A2F Action Validation (§13)
    // =========================================================================
    let mut hud = EphemeralHud::new();
    assert_eq!(*hud.state(), HudState::Hidden);

    // Global hotkey invocation
    hud.open_prompt();
    for c in "Transférer 50 EUR".chars() {
        hud.insert_char(c);
    }
    assert_eq!(
        *hud.state(),
        HudState::Prompting {
            query: "Transférer 50 EUR".to_string(),
            cursor_pos: "Transférer 50 EUR".len(),
        }
    );

    // Braise #FF7A2F Action Confirmation
    hud.request_validation(
        "act-wallet-50",
        "Paiement 50.00 EUR vers compte externe",
        Some(5000),
        "Financial",
    );
    let hud_ascii = hud.render_ascii();
    assert!(hud_ascii.contains("#FF7A2F BRAISE"));
    assert!(hud_ascii.contains("50.00 EUR"));

    // Smooth auto-evaporation
    hud.start_evaporation();
    assert!(matches!(*hud.state(), HudState::Evaporating { .. }));

    // =========================================================================
    // 6. Silence Budget Tracker (§21d)
    // =========================================================================
    let mut silence_budget = SilenceBudgetTracker::new(3);
    assert_eq!(silence_budget.credits_remaining(), 3);

    // User reminder = 0 credits consumed
    assert!(silence_budget.request_initiative(
        "cal-reminder",
        InitiativeOrigin::UserScheduled,
        0.5,
        "Réunion imminente"
    ));
    assert_eq!(silence_budget.credits_remaining(), 3);

    // Inferred suggestions consume 1 credit each
    assert!(silence_budget.request_initiative(
        "proactive-1",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Recommander repos"
    ));
    assert_eq!(silence_budget.credits_remaining(), 2);

    assert!(silence_budget.request_initiative(
        "proactive-2",
        InitiativeOrigin::SystemInferred,
        0.95,
        "Sauvegarder git"
    ));
    assert_eq!(silence_budget.credits_remaining(), 1);

    assert!(silence_budget.request_initiative(
        "proactive-3",
        InitiativeOrigin::SystemInferred,
        0.88,
        "Classer documents"
    ));
    assert_eq!(silence_budget.credits_remaining(), 0);

    // Budget exhausted -> blocked
    assert!(!silence_budget.request_initiative(
        "proactive-4",
        InitiativeOrigin::SystemInferred,
        0.99,
        "Autre suggestion"
    ));

    // =========================================================================
    // 7. Multi-OS Platform Adapter Verification
    // =========================================================================
    let adapter = PlatformAdapter::auto_detect();
    let tray_spec = adapter.create_tray_spec(PresenceState::Listening, 0);
    assert_eq!(tray_spec.pulse_period_ms, 4000);

    let acting_spec = adapter.create_tray_spec(PresenceState::Acting, 1);
    assert_eq!(acting_spec.pulse_period_ms, 1200);
    assert!(acting_spec.has_attention_badge);
    assert_eq!(acting_spec.status_color.r, 244);
    assert_eq!(acting_spec.status_color.g, 235);
    assert_eq!(acting_spec.status_color.b, 221); // lueur #F4EBDD, anneau fin
}
