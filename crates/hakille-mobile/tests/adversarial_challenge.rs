use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::KernelToUi;
use serde_json::json;

fn setup_bridge() -> (MobileBridge, u64) {
    let bridge = MobileBridge::new();
    let set_hub = bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "https://hub.example.com"
        })
        .to_string(),
    );
    assert!(set_hub.contains("\"ok\":true"));

    let set_draft = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "test prompt"
        })
        .to_string(),
    );
    assert!(set_draft.contains("\"ok\":true"));

    let submit_res = bridge.dispatch_json(
        json!({
            "type": "submit"
        })
        .to_string(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&submit_res).unwrap();
    assert_eq!(parsed["ok"], true);
    let request_id = parsed["request"]["id"].as_u64().unwrap();
    (bridge, request_id)
}

#[test]
fn test_adversarial_rapid_delta_frames_accumulation() {
    let (bridge, req_id) = setup_bridge();

    // 1. Rapid sequence of 200 token deltas
    let mut expected_text = String::new();
    for i in 0..200 {
        let chunk = format!("tok{i}_");
        expected_text.push_str(&chunk);
        let frame = Frame::reply(0, KernelToUi::TokenDelta { text: chunk });
        let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(parsed["ok"], true, "Delta {i} failed: {res}");
        assert_eq!(parsed["snapshot"]["streaming"], true);
    }

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let chat = snap["chat"].as_array().unwrap();
    assert_eq!(chat.len(), 2); // 0 = user, 1 = assistant
    assert_eq!(chat[1]["role"], "assistant");
    assert_eq!(chat[1]["text"], expected_text);

    // 2. Terminal HudReply authoritatively replaces entire accumulated text
    let authoritative_reply = "Final authoritative summary with no residue.".to_string();
    let reply_frame = Frame::reply(
        req_id,
        KernelToUi::HudReply {
            response: authoritative_reply.clone(),
            validation: None,
        },
    );
    let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&reply_frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["snapshot"]["streaming"], false);
    assert!(parsed["snapshot"]["pending"].is_null());

    let final_chat = parsed["snapshot"]["chat"].as_array().unwrap();
    assert_eq!(final_chat.len(), 2);
    assert_eq!(final_chat[1]["text"], authoritative_reply);
    // Explicitly verify no residue or repetition of previous rapid tokens
    assert!(!final_chat[1]["text"].as_str().unwrap().contains("tok0_"));
}

#[test]
fn test_adversarial_empty_delta_frames() {
    let (bridge, req_id) = setup_bridge();

    // Sending empty delta string
    let frame = Frame::reply(0, KernelToUi::TokenDelta { text: "".into() });
    let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["snapshot"]["streaming"], true);

    let chat = parsed["snapshot"]["chat"].as_array().unwrap();
    assert_eq!(chat[1]["text"], "");

    // Subsequent normal delta works smoothly
    let frame2 = Frame::reply(
        0,
        KernelToUi::TokenDelta {
            text: "normal".into(),
        },
    );
    let res2 = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame2).unwrap());
    let parsed2: serde_json::Value = serde_json::from_str(&res2).unwrap();
    assert_eq!(parsed2["ok"], true);
    assert_eq!(parsed2["snapshot"]["chat"][1]["text"], "normal");
}

#[test]
fn test_adversarial_oversized_text_block_rejected_fail_closed() {
    // Case A: Single frame delta exceeding MAX_TEXT_BYTES (64 KiB)
    let (bridge_a, req_id_a) = setup_bridge();
    let oversized = "x".repeat(65537);
    let frame = Frame::reply(0, KernelToUi::TokenDelta { text: oversized });
    let res = bridge_a.receive_stream_frame(req_id_a, serde_json::to_string(&frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], false);
    assert!(
        parsed["error"]
            .as_str()
            .unwrap()
            .contains("Texte invalide (limite 65536 octets")
    );
    assert!(parsed["snapshot"]["pending"].is_null());

    // Case B: Cumulative deltas exceeding MAX_TEXT_BYTES across multiple frames
    let (bridge_b, req_id_b) = setup_bridge();
    let chunk1 = "a".repeat(40000);
    let frame1 = Frame::reply(0, KernelToUi::TokenDelta { text: chunk1 });
    let res1 = bridge_b.receive_stream_frame(req_id_b, serde_json::to_string(&frame1).unwrap());
    let parsed1: serde_json::Value = serde_json::from_str(&res1).unwrap();
    assert_eq!(parsed1["ok"], true);

    let chunk2 = "b".repeat(30000); // 40000 + 30000 = 70000 > 65536
    let frame2 = Frame::reply(0, KernelToUi::TokenDelta { text: chunk2 });
    let res2 = bridge_b.receive_stream_frame(req_id_b, serde_json::to_string(&frame2).unwrap());
    let parsed2: serde_json::Value = serde_json::from_str(&res2).unwrap();
    assert_eq!(parsed2["ok"], false);
    assert!(
        parsed2["error"]
            .as_str()
            .unwrap()
            .contains("Réponse de dialogue trop longue")
    );
    assert!(parsed2["snapshot"]["pending"].is_null());
}

#[test]
fn test_adversarial_delta_after_final_reply_rejected() {
    let (bridge, req_id) = setup_bridge();

    // Final reply arrives first
    let reply_frame = Frame::reply(
        req_id,
        KernelToUi::HudReply {
            response: "Done".into(),
            validation: None,
        },
    );
    let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&reply_frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], true);
    assert!(parsed["snapshot"]["pending"].is_null());

    // Late delta arriving after terminal reply must be rejected fail-closed
    let late_delta = Frame::reply(
        0,
        KernelToUi::TokenDelta {
            text: "late token".into(),
        },
    );
    let late_res = bridge.receive_stream_frame(req_id, serde_json::to_string(&late_delta).unwrap());
    let late_parsed: serde_json::Value = serde_json::from_str(&late_res).unwrap();
    assert_eq!(late_parsed["ok"], false);
    assert!(
        late_parsed["error"]
            .as_str()
            .unwrap()
            .contains("Trame de streaming sans requête active")
    );
}

#[test]
fn test_adversarial_mismatched_request_id_rejected() {
    let (bridge, req_id) = setup_bridge();

    // Frame with wrong request ID
    let wrong_id = req_id + 999;
    let reply_frame = Frame::reply(
        wrong_id,
        KernelToUi::HudReply {
            response: "Intruder".into(),
            validation: None,
        },
    );
    let res = bridge.receive_stream_frame(wrong_id, serde_json::to_string(&reply_frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(parsed["ok"], false);
    assert!(
        parsed["error"]
            .as_str()
            .unwrap()
            .contains("Trame de streaming sans requête active")
    );
}

#[test]
fn test_adversarial_double_submit_rejected_fail_closed() {
    let (bridge, _req_id) = setup_bridge();

    // While first request is pending, attempt a second submit
    let second_submit = bridge.dispatch_json(json!({ "type": "submit" }).to_string());
    let parsed2: serde_json::Value = serde_json::from_str(&second_submit).unwrap();
    assert_eq!(parsed2["ok"], false);
    assert!(
        parsed2["error"]
            .as_str()
            .unwrap()
            .contains("Une requête est déjà en cours")
    );

    // Attempt other mutations while pending -> all rejected
    let refresh = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let parsed_ref: serde_json::Value = serde_json::from_str(&refresh).unwrap();
    assert_eq!(parsed_ref["ok"], false);
    assert!(
        parsed_ref["error"]
            .as_str()
            .unwrap()
            .contains("Une requête est déjà en cours")
    );

    let remember = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": "secret"
        })
        .to_string(),
    );
    let parsed_rem: serde_json::Value = serde_json::from_str(&remember).unwrap();
    assert_eq!(parsed_rem["ok"], false);
    assert!(
        parsed_rem["error"]
            .as_str()
            .unwrap()
            .contains("Une requête est déjà en cours")
    );
}

#[test]
fn test_adversarial_cancellation_cleans_up_and_enforces_honesty_contract() {
    let (bridge, req_id) = setup_bridge();

    // Cancel the pending request via fail_request
    let cancel_res = bridge.fail_request(req_id, "Annulation utilisateur".into());
    let parsed_cancel: serde_json::Value = serde_json::from_str(&cancel_res).unwrap();
    assert_eq!(parsed_cancel["ok"], true);
    assert!(parsed_cancel["snapshot"]["pending"].is_null());
    assert_eq!(parsed_cancel["snapshot"]["connection"], "unknown");

    // Outbox/outcomes has recorded failure
    let outcomes = parsed_cancel["snapshot"]["outcomes"].as_array().unwrap();
    let last = outcomes.last().unwrap();
    assert_eq!(last["request_id"], req_id);
    assert!(
        last["message"]
            .as_str()
            .unwrap()
            .contains("Annulation utilisateur")
    );

    // Old stream frame arriving now is rejected
    let late_frame = Frame::reply(
        req_id,
        KernelToUi::HudReply {
            response: "Late ghost".into(),
            validation: None,
        },
    );
    let late_res = bridge.receive_stream_frame(req_id, serde_json::to_string(&late_frame).unwrap());
    let parsed_late: serde_json::Value = serde_json::from_str(&late_res).unwrap();
    assert_eq!(parsed_late["ok"], false);

    // Honesty contract: Blindly submitting another mutating action is refused
    // because previous outcome was unknown!
    let blind_submit = bridge.dispatch_json(json!({ "type": "submit" }).to_string());
    let parsed_blind: serde_json::Value = serde_json::from_str(&blind_submit).unwrap();
    assert_eq!(parsed_blind["ok"], false);
    assert!(
        parsed_blind["error"]
            .as_str()
            .unwrap()
            .contains("Résultat précédent inconnu")
    );

    // Honest recovery protocol:
    // 1. Refresh status (non-mutating)
    let refresh = bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string());
    let parsed_ref: serde_json::Value = serde_json::from_str(&refresh).unwrap();
    assert_eq!(parsed_ref["ok"], true);
    let ref_id = parsed_ref["request"]["id"].as_u64().unwrap();

    let status_reply = Frame::reply(
        ref_id,
        KernelToUi::StatusReply {
            status: hakille_ui_protocol::StatusDto {
                model: "qwen".into(),
                cost: "0.0".into(),
                state: "idle".into(),
                memory_count: 0,
            },
        },
    );
    let res_status = bridge.receive_json(
        ref_id,
        json!({
            "session_id": "session-recovered",
            "frames": [status_reply]
        })
        .to_string(),
    );
    let parsed_status: serde_json::Value = serde_json::from_str(&res_status).unwrap();
    assert_eq!(parsed_status["ok"], true);
    assert_eq!(parsed_status["snapshot"]["connection"], "connected");

    // 2. Acknowledge unknown operation
    let ack = bridge.dispatch_json(
        json!({
            "type": "acknowledge_unknown",
            "request_id": req_id
        })
        .to_string(),
    );
    let parsed_ack: serde_json::Value = serde_json::from_str(&ack).unwrap();
    assert_eq!(parsed_ack["ok"], true);

    // 3. Now new submit succeeds cleanly!
    let set_draft = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "new request after honest recovery"
        })
        .to_string(),
    );
    assert!(set_draft.contains("\"ok\":true"));

    let submit2 = bridge.dispatch_json(json!({ "type": "submit" }).to_string());
    let parsed_submit2: serde_json::Value = serde_json::from_str(&submit2).unwrap();
    assert_eq!(parsed_submit2["ok"], true);
    let new_req_id = parsed_submit2["request"]["id"].as_u64().unwrap();
    assert_ne!(new_req_id, req_id);
}

#[test]
fn test_adversarial_multibyte_utf8_accumulation() {
    let (bridge, req_id) = setup_bridge();

    let chunks = vec![
        "Salut ",
        "l'équipe ",
        "Hakille 🦀 ",
        "états ",
        "honnêtes ",
        "€ ",
        "日本語 ",
        "🚀",
    ];
    let full_expected: String = chunks.concat();

    for chunk in chunks {
        let frame = Frame::reply(0, KernelToUi::TokenDelta { text: chunk.into() });
        let res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());
        assert!(res.contains("\"ok\":true"));
    }

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let chat = snap["chat"].as_array().unwrap();
    assert_eq!(chat[1]["text"], full_expected);
}
