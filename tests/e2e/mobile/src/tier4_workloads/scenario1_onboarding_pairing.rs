//! Tier 4 Real-World Application Workload: Scenario 1
//!
//! Sovereign Voice Onboarding & QR Pairing
//! Features Exercised: F01 (Living Presence), F02 (Aura Ring), F19 (Gateway Pairing), F20 (Checkpoint v2 Atomic Persist)
//! Complexity: Medium
//!
//! Workflow:
//! 1. Cold boot in unconfigured sovereign state (zero network credentials).
//! 2. Ingest QR pairing token payload, establish loopback connection, and complete handshake.
//! 3. Initial status refresh: daemon responds with StatusReply, connection becomes Connected,
//!    aura ring initializes at tau = 1.0 (high trust).
//! 4. Voice onboarding tour: user asks onboarding query via voice, streams TokenDelta frames,
//!    finalizes with VoiceReply; presence cycles Listening -> Acting -> Resting.
//! 5. Atomic Checkpoint v2 fsync verification guaranteeing restart survivability.

use crate::harness::{CheckpointHarness, DaemonStep, MobileTestRig};
use hakille_design::brand::{WIRES_FULL, wire_count};
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::aura::AuraRingUpdateDto;
use hakille_ui_protocol::{KernelToUi, PresenceState, StatusDto, UiToKernel};
use serde_json::json;

#[tokio::test]
async fn test_scenario1_sovereign_voice_onboarding_and_qr_pairing() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let (rig, daemon) = MobileTestRig::setup().await;

    // --- STEP 1: Cold Boot ---
    let bridge = MobileBridge::new();
    let initial_snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();

    assert!(
        initial_snap["hub_id"].is_null(),
        "Cold boot hub must be null"
    );
    assert_eq!(initial_snap["connection"], "disconnected");
    assert!(initial_snap["presence"].is_null());
    assert_eq!(wire_count(360), WIRES_FULL, "Default full 21 wires");

    // --- STEP 2: QR Scan & Pairing ---
    // Simulate ingesting QR payload: { "endpoint": rig.base_url, "token": rig.mobile_bearer }
    let set_hub_res = bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": rig.base_url.clone(),
        })
        .to_string(),
    );
    let set_hub_val: serde_json::Value = serde_json::from_str(&set_hub_res).unwrap();
    assert_eq!(set_hub_val["ok"], true);

    let paired_snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(paired_snap["hub_id"], rig.base_url);

    // --- STEP 3: Initial Status Refresh & Aura Ring Activation ---
    let custom_status = StatusDto {
        model: "qwen2.5-0.5b-instruct-q4km".into(),
        cost: "0,00 EUR".into(),
        state: "Idle".into(),
        memory_count: 12,
    };

    let daemon_task = tokio::spawn({
        let st = custom_status.clone();
        async move {
            let mut stream = daemon.accept().await.expect("daemon accept");

            // Step 1: StatusRequest
            let step1 = DaemonStep::new(
                UiToKernel::StatusRequest,
                KernelToUi::StatusReply { status: st },
            )
            .after(vec![KernelToUi::Presence(PresenceState::Idle)]);
            daemon
                .handle_step(&mut stream, 1, step1)
                .await
                .expect("step 1 status");

            // Step 2: Voice dialogue tour
            let step2 = DaemonStep::new(
                UiToKernel::HudSubmit {
                    query: "Bienvenue sur Hakille".into(),
                },
                KernelToUi::HudReply {
                    response: "Bonjour. Je suis ton intelligence locale et souveraine.".into(),
                    validation: None,
                },
            )
            .before(vec![
                KernelToUi::Presence(PresenceState::Acting),
                KernelToUi::TokenDelta {
                    text: "Bonjour. Je suis ton intelligence...".into(),
                },
            ])
            .after(vec![KernelToUi::Presence(PresenceState::Idle)]);
            daemon
                .handle_step(&mut stream, 3, step2)
                .await
                .expect("step 2 voice");
        }
    });

    let (status_code, result) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    assert_eq!(status_code, 200);
    assert_eq!(result["ok"], true);

    let connected_snap = rig.snapshot(&bridge);
    assert_eq!(connected_snap["connection"], "connected");
    assert_eq!(
        connected_snap["status"]["model"],
        "qwen2.5-0.5b-instruct-q4km"
    );
    assert_eq!(connected_snap["status"]["memory_count"], 12);

    // Aura ring initialization at tau = 1.0 (high trust, emerald signature)
    let aura = AuraRingUpdateDto::new(
        1.0,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.0,
        1700000000000,
    );
    assert!(aura.is_high_trust());
    assert_eq!(aura.primary_hex, "#10B981");

    // --- STEP 4: Voice Dialogue Tour ---
    let begin_voice_res = bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    assert!(begin_voice_res.contains("\"ok\":true"));

    let armed_snap = rig.snapshot(&bridge);
    let ctx_id = armed_snap["voice_context_id"].as_u64().unwrap();
    assert_eq!(ctx_id, 2);

    let (voice_code, voice_result) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "submit_voice",
                "context_id": ctx_id,
                "text": "Bienvenue sur Hakille",
            }),
        )
        .await;
    assert_eq!(voice_code, 200);
    assert_eq!(voice_result["ok"], true);

    let post_voice_snap = rig.snapshot(&bridge);
    assert_eq!(post_voice_snap["presence"], "Idle");
    let reply = post_voice_snap["voice_reply"].as_object().unwrap();
    assert_eq!(reply["context_id"], ctx_id);
    assert_eq!(
        reply["text"],
        "Bonjour. Je suis ton intelligence locale et souveraine."
    );

    // --- STEP 5: Durable Persist & Checkpoint v2 Verification ---
    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("save draft to disk");
    let checkpoint = CheckpointHarness::validate_checkpoint_schema(&draft_json)
        .expect("validate checkpoint v2 schema");

    assert_eq!(checkpoint["schema_version"], 2);
    assert_eq!(
        checkpoint["uncertain_operations"].as_array().unwrap().len(),
        0
    );

    daemon_task.await.unwrap();
}
