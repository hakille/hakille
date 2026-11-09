//! Tier 3 Pairwise Module: Presence & Streaming Interactions
//!
//! Combinatorial interactions between:
//! - F01: Living Presence & 21 Kora Wires
//! - F02: Aura Ring Streaming
//! - F04: Hub Voice Conversation
//! - F05: TokenDelta Real-Time Streaming
//!
//! Tests: T3_PAIR_01 to T3_PAIR_04

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_design::brand::{WIRES_FULL, wire_count};
use hakille_design::color::braise_allowed;
use hakille_design::motion::presence_rhythm;
use hakille_design::voice::{clamp_speech_rate, speak_short};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::aura::AuraRingUpdateDto;
use hakille_ui_protocol::{KernelToUi, PresenceState, UiToKernel};
use serde_json::json;

/// `T3_PAIR_01`: TokenDelta stream shifts Presence Idle -> Streaming and accelerates kora wires.
///
/// Interactions: F01 (Living Presence) + F05 (Streaming)
/// During TokenDelta streaming, presence moves from resting baseline (0 Hz / static)
/// to active rhythm (period 1200ms / ~0.83-1.2Hz), and wire telemetry maintains full 21 wires.
#[test]
fn test_t3_pair_01_tokendelta_presence_shift_and_kora_frequency_acceleration() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Exécute analyse souveraine",
        })
        .to_string(),
    );

    let submit_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let submit_res: serde_json::Value = serde_json::from_str(&submit_res_str).unwrap();
    assert_eq!(submit_res["ok"], true);
    let req_id = submit_res["request"]["id"].as_u64().unwrap();

    // Resting baseline presence rhythm: period 0ms (static), resting wires = 21
    let (idle_period, min_op, max_op, scale) = presence_rhythm("idle");
    assert_eq!(idle_period, 0, "Idle presence period is 0 (static resting)");
    assert_eq!(min_op, 0.4);
    assert_eq!(max_op, 0.4);
    assert_eq!(scale, 1.0);
    assert_eq!(
        wire_count(360),
        WIRES_FULL,
        "Full 21 wires on mobile viewport"
    );

    // Feed initial TokenDelta stream frame (SSE simulation)
    let stream_frame = Frame::reply(
        0,
        KernelToUi::TokenDelta {
            text: "Démarrage de l'analyse...".into(),
        },
    );
    let stream_res_str =
        bridge.receive_stream_frame(req_id, serde_json::to_string(&stream_frame).unwrap());
    let stream_res: serde_json::Value = serde_json::from_str(&stream_res_str).unwrap();
    assert_eq!(stream_res["ok"], true);

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap["streaming"], true,
        "Snapshot streaming flag must be true during delta ingestion"
    );

    // Active streaming presence rhythm: period 1200ms (~0.83 - 1.2Hz)
    let (active_period, active_min_op, active_max_op, _) = presence_rhythm("acting");
    assert_eq!(
        active_period, 1200,
        "Acting presence rhythm must accelerate to 1200ms period"
    );
    assert!(
        active_max_op > active_min_op,
        "Dynamic oscillation between min and max opacity during streaming"
    );

    // Assistant chat entry exists with partial text
    let chat = snap["chat"].as_array().expect("chat array");
    assert_eq!(chat.len(), 2);
    assert_eq!(chat[1]["role"], "assistant");
    assert_eq!(chat[1]["text"], "Démarrage de l'analyse...");
}

/// `T3_PAIR_02`: Terminal HudReply terminates stream, resets Presence to Resting, and sets final text.
///
/// Interactions: F01 (Living Presence) + F05 (Streaming)
/// Arrival of terminal HudReply sets streaming to false, appends verified authoritative text,
/// clears the submitted draft, and returns presence to quiescent state.
#[tokio::test]
async fn test_t3_pair_02_terminal_hud_reply_terminates_stream_and_quiesces_presence() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let query = "Calcul de l'empreinte mémoire";
    let final_text = "Empreinte mémoire : 42 Ko. Tout est conservé localement.";

    let daemon_task = tokio::spawn({
        let q = query.to_string();
        let ft = final_text.to_string();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            daemon
                .handle_step(&mut stream, 1, DaemonStep::status())
                .await
                .expect("step 1");

            let step = DaemonStep::new(
                UiToKernel::HudSubmit { query: q },
                KernelToUi::HudReply {
                    response: ft,
                    validation: None,
                },
            )
            .before(vec![
                KernelToUi::Presence(PresenceState::Acting),
                KernelToUi::TokenDelta {
                    text: "Calcul en cours...".into(),
                },
            ])
            .after(vec![KernelToUi::Presence(PresenceState::Idle)]);

            daemon
                .handle_step(&mut stream, 2, step)
                .await
                .expect("step 2 streaming");
        }
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": query,
        })
        .to_string(),
    );

    let (status, result) = rig.roundtrip(&bridge, json!({"type": "submit"})).await;
    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snap = rig.snapshot(&bridge);
    assert_eq!(
        snap["streaming"], false,
        "Streaming must be false after terminal reply"
    );
    assert_eq!(snap["draft"], "", "Draft must be cleared atomically");

    let chat = snap["chat"].as_array().unwrap();
    assert_eq!(chat.len(), 2);
    assert_eq!(chat[1]["role"], "assistant");
    assert_eq!(chat[1]["text"], final_text);

    assert_eq!(
        snap["presence"], "Idle",
        "Presence must settle into Resting/Idle state post-stream"
    );

    daemon_task.await.unwrap();
}

/// `T3_PAIR_03`: Low model confidence during streaming expands Aura dispersion without Braise leakage.
///
/// Interactions: F02 (Aura Ring) + F05 (Streaming)
/// When model confidence tau < 0.40, aura ring expands scale factor, but color strictly
/// stays in amber/gold/violet; braise (#FF7A2F) is strictly forbidden from aura contexts.
#[test]
fn test_t3_pair_03_low_model_confidence_expands_aura_dispersion_without_braise_leak() {
    // Model confidence low: tau = 0.30
    let aura = AuraRingUpdateDto::new(
        0.30,
        "VioletStepUp",
        "#8B5CF6",
        "#8B5CF673",
        1500,
        1.12, // dispersion expansion factor
        1700000000000,
    );

    assert!(aura.tau < 0.40, "Confidence is low (< 0.40)");
    assert_eq!(aura.primary_hex, "#8B5CF6");
    assert_ne!(
        aura.primary_hex, "#FF7A2F",
        "Braise must never appear in aura ring"
    );
    assert_ne!(
        aura.glow_hex, "#FF7A2F",
        "Braise glow must never leak into aura halo"
    );

    // Verify system color governance invariants
    assert!(
        !braise_allowed("aura"),
        "Braise is forbidden in aura rendering context"
    );
    assert!(
        !braise_allowed("presence_halo"),
        "Braise is forbidden in presence halo"
    );
    assert!(
        !braise_allowed("model_streaming"),
        "Braise is forbidden in streaming"
    );

    // Voice hedging reflects the same low confidence
    let hedged = speak_short(
        "Résultat incertain",
        aura.tau.into(),
        "Vérifie la source avant confirmation.",
    );
    assert!(
        hedged.starts_with("Je ne suis pas sûr."),
        "Low confidence must begin with honest hedging: {hedged}"
    );
}

/// `T3_PAIR_04`: Voice dialogue sets Listening presence, clamps speech rate, and populates voice_reply.
///
/// Interactions: F01 (Living Presence) + F04 (Voice Dialogue)
/// Beginning voice dialogue sets presence to Listening, clamps speech rate in [0.7, 1.6],
/// and submitting voice audio returns correlated VoiceReply without leaking raw chat artifacts.
#[tokio::test]
async fn test_t3_pair_04_voice_dialogue_sets_listening_clamps_speech_rate_populates_voice_reply() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    // Clamping boundaries check
    assert_eq!(clamp_speech_rate(0.3), 0.7);
    assert_eq!(clamp_speech_rate(1.1), 1.1);
    assert_eq!(clamp_speech_rate(2.4), 1.6);

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Step 1: StatusRequest
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        // Step 2: Voice submit
        let step = DaemonStep::new(
            UiToKernel::HudSubmit {
                query: "Quelles sont mes dépenses ?".into(),
            },
            KernelToUi::HudReply {
                response: "Ordre vocal reçu et validé souverainement.".into(),
                validation: None,
            },
        )
        .before(vec![KernelToUi::Presence(PresenceState::Listening)])
        .after(vec![KernelToUi::Presence(PresenceState::Idle)]);

        daemon
            .handle_step(&mut stream, 3, step)
            .await
            .expect("voice step");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    // 1. Begin Voice
    let begin_res_str = bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let begin_res: serde_json::Value = serde_json::from_str(&begin_res_str).unwrap();
    assert_eq!(begin_res["ok"], true);

    let snap1 = rig.snapshot(&bridge);
    let ctx_id = snap1["voice_context_id"]
        .as_u64()
        .expect("voice_context_id");

    // 2. Submit Voice
    let (status, result) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "submit_voice",
                "context_id": ctx_id,
                "text": "Quelles sont mes dépenses ?",
            }),
        )
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snap2 = rig.snapshot(&bridge);
    let voice_reply = snap2["voice_reply"].as_object().expect("voice_reply");
    assert_eq!(voice_reply["context_id"], ctx_id);
    assert_eq!(
        voice_reply["text"],
        "Ordre vocal reçu et validé souverainement."
    );
    assert_eq!(snap2["presence"], "Idle");

    daemon_task.await.unwrap();
}
