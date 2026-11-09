use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{
    ActionCard, DataExit, FatigueAlertEvent, FatigueSeverity, GhostActionDto, KernelToUi, Money,
    Provenance,
};
use serde_json::json;

fn setup_bridge() -> MobileBridge {
    let bridge = MobileBridge::new();
    let set_hub = bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "https://hub.example.com"
        })
        .to_string(),
    );
    assert!(set_hub.contains("\"ok\":true"));
    bridge
}

fn sample_ghost(id: &str) -> GhostActionDto {
    GhostActionDto {
        id: id.into(),
        label: "Archiver note".into(),
        confidence: 0.95,
        focal_u: 0.5,
        focal_v: 0.5,
        hold_duration_ms: 600,
        action: ActionCard {
            id: 10,
            verb: "Archiver".into(),
            object: "Note".into(),
            reason: "Fin de tâche".into(),
            cost: Money::from_cents(0, "EUR"),
            data_exit: DataExit::Local,
            reversible: true,
            provenance: Provenance::Hakille,
        },
    }
}

fn sample_status() -> KernelToUi {
    KernelToUi::StatusReply {
        status: hakille_ui_protocol::StatusDto {
            model: "Qwen".into(),
            cost: "0".into(),
            state: "Idle".into(),
            memory_count: 0,
        },
    }
}

#[test]
fn test_ghost_action_suggest_dismiss_and_accept_lifecycle() {
    let bridge = setup_bridge();

    // 1. Issue a request (refresh_status)
    let refresh = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&refresh).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    // 2. Stream frame arrives with GhostActionSuggest (id = 0)
    let frame_suggest = Frame::reply(0, KernelToUi::GhostActionSuggest(sample_ghost("ghost-1")));
    let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_suggest).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["snapshot"]["ghost_action"]["id"], "ghost-1");
    assert_eq!(parsed["snapshot"]["ghost_action"]["label"], "Archiver note");

    // 3. Request completes with terminal reply (StatusReply)
    let status_frame = Frame::reply(req_id, sample_status());
    let res_status =
        bridge.receive_stream_frame(req_id, serde_json::to_string(&status_frame).unwrap());
    let parsed_status: serde_json::Value = serde_json::from_str(&res_status).unwrap();
    assert_eq!(parsed_status["ok"], true);
    assert!(parsed_status["snapshot"]["pending"].is_null());
    // Ghost action persists in snapshot after request completion
    assert_eq!(parsed_status["snapshot"]["ghost_action"]["id"], "ghost-1");

    // 4. Client swipe dismisses ghost action
    let dismiss_res = bridge.dispatch_json(
        json!({
            "type": "ghost_action_dismiss",
            "id": "ghost-1"
        })
        .to_string(),
    );
    let parsed_dismiss: serde_json::Value = serde_json::from_str(&dismiss_res).unwrap();
    assert_eq!(parsed_dismiss["ok"], true);
    // Snapshot must clear ghost_action immediately
    assert!(parsed_dismiss["snapshot"]["ghost_action"].is_null());
    let dismiss_req_id = parsed_dismiss["request"]["id"].as_u64().unwrap();

    // Complete the dismiss request with correlated terminal reply
    let _ = bridge.receive_stream_frame(
        dismiss_req_id,
        serde_json::to_string(&Frame::reply(
            dismiss_req_id,
            KernelToUi::GhostActionDismissed {
                id: "ghost-1".into(),
            },
        ))
        .unwrap(),
    );

    // 5. Re-suggest new ghost action during a new request turn
    let refresh2 = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let req_id2 = serde_json::from_str::<serde_json::Value>(&refresh2).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let frame_suggest2 = Frame::reply(0, KernelToUi::GhostActionSuggest(sample_ghost("ghost-2")));
    let res2 =
        bridge.receive_stream_frame(req_id2, serde_json::to_string(&frame_suggest2).unwrap());
    let parsed2: serde_json::Value = serde_json::from_str(&res2).unwrap();
    assert_eq!(parsed2["ok"], true);
    assert_eq!(parsed2["snapshot"]["ghost_action"]["id"], "ghost-2");

    // Complete request 2
    let _ = bridge.receive_stream_frame(
        req_id2,
        serde_json::to_string(&Frame::reply(req_id2, sample_status())).unwrap(),
    );

    // 6. Client accepts ghost action
    let accept_res = bridge.dispatch_json(
        json!({
            "type": "ghost_action_accept",
            "id": "ghost-2"
        })
        .to_string(),
    );
    let parsed_accept: serde_json::Value = serde_json::from_str(&accept_res).unwrap();
    assert_eq!(parsed_accept["ok"], true);
    // Snapshot must clear ghost_action immediately
    assert!(parsed_accept["snapshot"]["ghost_action"].is_null());
}

#[test]
fn test_ghost_action_adversarial_inputs_fail_closed() {
    let bridge = setup_bridge();

    // 1. Oversized ghost action ID (>256 chars) rejected
    let long_id = "g".repeat(257);
    let oversized_dismiss = bridge.dispatch_json(
        json!({
            "type": "ghost_action_dismiss",
            "id": long_id
        })
        .to_string(),
    );
    let parsed_over: serde_json::Value = serde_json::from_str(&oversized_dismiss).unwrap();
    assert_eq!(parsed_over["ok"], false);

    let oversized_accept = bridge.dispatch_json(
        json!({
            "type": "ghost_action_accept",
            "id": long_id
        })
        .to_string(),
    );
    let parsed_over2: serde_json::Value = serde_json::from_str(&oversized_accept).unwrap();
    assert_eq!(parsed_over2["ok"], false);

    // 2. Empty ID rejected
    let empty_dismiss = bridge.dispatch_json(
        json!({
            "type": "ghost_action_dismiss",
            "id": ""
        })
        .to_string(),
    );
    let parsed_empty: serde_json::Value = serde_json::from_str(&empty_dismiss).unwrap();
    assert_eq!(parsed_empty["ok"], false);

    // 3. Null ID rejected
    let null_accept = bridge.dispatch_json(
        json!({
            "type": "ghost_action_accept"
        })
        .to_string(),
    );
    let parsed_null: serde_json::Value = serde_json::from_str(&null_accept).unwrap();
    assert_eq!(parsed_null["ok"], false);
}

#[test]
fn test_ghost_action_cleared_on_kernel_dismissal() {
    let bridge = setup_bridge();

    let refresh = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&refresh).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    // Suggest
    let frame_suggest = Frame::reply(
        0,
        KernelToUi::GhostActionSuggest(sample_ghost("ghost-kernel")),
    );
    let _ = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_suggest).unwrap());

    // Kernel dismisses with matching ID -> ghost_action cleared
    let frame_dismiss = Frame::reply(
        0,
        KernelToUi::GhostActionDismissed {
            id: "ghost-kernel".into(),
        },
    );
    let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_dismiss).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], true);
    assert!(parsed["snapshot"]["ghost_action"].is_null());

    // Kernel dismisses with non-matching ID -> ghost_action remains
    let _ = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_suggest).unwrap());
    let frame_wrong_dismiss = Frame::reply(
        0,
        KernelToUi::GhostActionDismissed {
            id: "different-id".into(),
        },
    );
    let res_wrong =
        bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_wrong_dismiss).unwrap());
    let parsed_wrong: serde_json::Value = serde_json::from_str(&res_wrong).unwrap();
    assert_eq!(parsed_wrong["ok"], true);
    assert_eq!(
        parsed_wrong["snapshot"]["ghost_action"]["id"],
        "ghost-kernel"
    );
}

#[test]
fn test_fatigue_alert_accumulation_and_bounding() {
    let bridge = setup_bridge();

    let refresh = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&refresh).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    // Send 20 fatigue alerts -> bounded to MAX 16 notifications, history_trimmed set to true
    for i in 0..20 {
        let alert = FatigueAlertEvent {
            severity: FatigueSeverity::Moderate,
            perclos_p80: 0.35,
            ear: 0.22,
            mar: 0.45,
            yawn_count: i as u32,
            recommendation: format!("Pause ergonomique #{i}"),
            timestamp_ms: 1000 + i as u64,
        };
        let frame = Frame::reply(0, KernelToUi::FatigueAlert(alert));
        let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(parsed["ok"], true);
    }

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let notifs = snap["notifications"].as_array().unwrap();
    assert_eq!(notifs.len(), 16);
    assert_eq!(snap["history_trimmed"], true);
    // Oldest surviving is #4, newest is #19
    assert!(notifs[0].as_str().unwrap().contains("Pause ergonomique #4"));
    assert!(
        notifs[15]
            .as_str()
            .unwrap()
            .contains("Pause ergonomique #19")
    );
}

#[test]
fn test_voice_error_string_and_design_canonical_order() {
    // 1. Verify 4-step error template order from hakille-design states.rs
    let states_source = include_str!("../../../crates/hakille-design/src/states.rs");
    assert!(
        states_source.contains("\"Fait : {}\\nPas fait : {}\\n[{}] [{}]\\nDétails : {}\""),
        "Canonical format in hakille-design states.rs must be Fait -> Pas fait -> [Actions] -> Details"
    );

    // 2. Exact French missing speech engine error string verification
    let exact_missing_speech_error = "Reconnaissance vocale locale indisponible sur cet appareil";
    // Check against the raw file content in Android values-fr/strings.xml
    let android_strings =
        include_str!("../../../mobile/android/app/src/main/res/values-fr/strings.xml");
    assert!(
        android_strings.contains(&format!(
            "<string name=\"dictation_engine_missing\">{exact_missing_speech_error}</string>"
        )),
        "Android values-fr/strings.xml missing exact error: {exact_missing_speech_error}"
    );

    // Check against iOS fr.lproj/Localizable.strings
    let ios_strings = include_str!("../../../mobile/ios/Hakille/fr.lproj/Localizable.strings");
    assert!(
        ios_strings.contains(&format!(
            "\"{exact_missing_speech_error}\" = \"{exact_missing_speech_error}\";"
        )),
        "iOS Localizable.strings missing exact error mapping for: {exact_missing_speech_error}"
    );
}
