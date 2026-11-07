use super::*;
use crate::core::MAX_DRAFT_BYTES;
use hakille_ipc::{Frame, FrameBody};
use hakille_ui_protocol::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn call(bridge: &MobileBridge, action: Value) -> Value {
    serde_json::from_str(&bridge.dispatch_json(action.to_string())).unwrap()
}
fn snapshot(bridge: &MobileBridge) -> Value {
    serde_json::from_str(&bridge.snapshot_json()).unwrap()
}
fn hub() -> MobileBridge {
    let bridge = MobileBridge::new();
    assert_eq!(
        call(&bridge, json!({"type":"set_hub","hub_id":"home"}))["ok"],
        true
    );
    bridge
}
fn receive(bridge: &MobileBridge, id: u64, session: &str, messages: Vec<Frame>) -> Value {
    serde_json::from_str(&bridge.receive_json(
        id,
        json!({"session_id":session,"frames":messages}).to_string(),
    ))
    .unwrap()
}
fn status() -> KernelToUi {
    KernelToUi::StatusReply {
        status: StatusDto {
            model: "réel".into(),
            cost: "0 EUR".into(),
            state: "Idle".into(),
            memory_count: 3,
        },
    }
}
fn card(id: u64) -> ActionCard {
    ActionCard {
        id,
        verb: "envoyer".into(),
        object: "rapport".into(),
        reason: "demande utilisateur".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    }
}
fn connected_with_card() -> MobileBridge {
    let bridge = hub();
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        receive(
            &bridge,
            id,
            "session-a",
            vec![
                Frame::reply(id, status()),
                Frame::reply(0, KernelToUi::ShowCard(card(7)))
            ]
        )["ok"],
        true
    );
    bridge
}

#[test]
fn offline_unknown_and_concurrent_requests_are_honest() {
    let empty = MobileBridge::new();
    assert_eq!(call(&empty, json!({"type":"refresh_status"}))["ok"], false);
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"bonjour"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(call(&bridge, json!({"type":"load_agents"}))["ok"], false);
    let failed: Value =
        serde_json::from_str(&bridge.fail_request(id, "Connexion interrompue".into())).unwrap();
    assert_eq!(failed["snapshot"]["outcomes"][0]["state"], "unknown");
    assert_eq!(failed["snapshot"]["draft"], "bonjour");
    assert!(failed["request"].is_null());
    assert_eq!(call(&bridge, json!({"type":"submit"}))["ok"], false);
}

#[test]
fn stale_response_never_closes_a_new_request() {
    let bridge = hub();
    let old = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    call(&bridge, json!({"type":"set_hub","hub_id":"second"}));
    let new = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert!(new > old);
    assert_eq!(
        receive(
            &bridge,
            old,
            "old-session",
            vec![Frame::reply(old, status())]
        )["ok"],
        false
    );
    assert_eq!(snapshot(&bridge)["pending"]["request_id"], new);
    assert_eq!(
        receive(
            &bridge,
            new,
            "new-session",
            vec![Frame::reply(new, status())]
        )["ok"],
        true
    );
}

#[test]
fn full_batch_keeps_followup_cards_and_final_reply_replaces_deltas() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"fais ceci"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(
        &bridge,
        id,
        "s",
        vec![
            Frame::reply(
                0,
                KernelToUi::TokenDelta {
                    text: "provisoire".into(),
                },
            ),
            Frame::reply(
                id,
                KernelToUi::HudReply {
                    response: "réponse finale".into(),
                    validation: None,
                },
            ),
            Frame::reply(0, KernelToUi::ShowCard(card(88))),
        ],
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["snapshot"]["chat"][1]["text"], "réponse finale");
    assert_eq!(result["snapshot"]["cards"][0]["id"], 88);
    assert_eq!(result["snapshot"]["draft"], "");
    assert!(result["snapshot"]["pending"].is_null());
}

#[test]
fn wrong_correlated_reply_and_oversized_input_fail_closed() {
    let bridge = hub();
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        receive(&bridge, id, "s", vec![Frame::reply(id + 1, status())])["ok"],
        false
    );
    assert!(snapshot(&bridge)["status"].is_null());
    assert!(snapshot(&bridge)["pending"].is_null());
    let long = "x".repeat(MAX_JSON_BYTES + 1);
    let result: Value = serde_json::from_str(&bridge.dispatch_json(long)).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(
        call(&bridge, json!({"type":"submit","unexpected":true}))["ok"],
        false
    );
}

#[test]
fn draft_roundtrip_bounded_and_contains_no_transport_state() {
    let bridge = hub();
    let draft = "une note française 日本語 العربية\n";
    call(&bridge, json!({"type":"set_draft","text":draft}));
    let saved: Value = serde_json::from_str(&bridge.draft_json()).unwrap();
    assert_eq!(saved.as_object().unwrap().len(), 5);
    assert_eq!(saved["schema_version"], 2);
    assert!(saved["uncertain_operations"].as_array().unwrap().is_empty());
    let restored = MobileBridge::new();
    let outcome: Value =
        serde_json::from_str(&restored.restore_draft_json(saved.to_string())).unwrap();
    assert_eq!(outcome["snapshot"]["draft"], draft);
    assert!(outcome["snapshot"]["hub_id"].is_null());
    assert_eq!(
        call(
            &bridge,
            json!({"type":"set_draft","text":"x".repeat(MAX_DRAFT_BYTES+1)})
        )["ok"],
        false
    );
    let invalid = json!({"schema_version":1,"draft":"new","token":"secret"});
    let result: Value =
        serde_json::from_str(&restored.restore_draft_json(invalid.to_string())).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(snapshot(&restored)["draft"], draft);
}

#[test]
fn approval_requires_rust_hold_and_background_cancels_it() {
    let bridge = connected_with_card();
    assert_eq!(
        call(&bridge, json!({"type":"approve","card_id":7}))["ok"],
        false
    );
    assert_eq!(
        call(&bridge, json!({"type":"begin_approval","card_id":7}))["ok"],
        true
    );
    assert_eq!(
        call(&bridge, json!({"type":"approve","card_id":7}))["ok"],
        false
    );
    call(&bridge, json!({"type":"begin_approval","card_id":7}));
    call(&bridge, json!({"type":"set_foreground","active":false}));
    call(&bridge, json!({"type":"set_foreground","active":true}));
    assert!(snapshot(&bridge)["approval"].is_null());
    assert_eq!(snapshot(&bridge)["microphone_enabled"], false);
    assert_eq!(
        call(&bridge, json!({"type":"approve","card_id":7}))["ok"],
        false
    );
    // Horloge injectée au niveau de la machine interne, sans attente réelle.
    let start = Instant::now();
    let mut state = bridge.state.lock().unwrap();
    state
        .dispatch(MobileAction::BeginApproval { card_id: 7 }, start)
        .unwrap();
    let frame = state
        .dispatch(
            MobileAction::Approve { card_id: 7 },
            start + Duration::from_millis(600),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        frame.body,
        FrameBody::Request {
            message: UiToKernel::Approve { card_id: 7 }
        }
    ));
}

#[test]
fn rejection_is_immediate_and_ack_preserves_audit_effect() {
    let bridge = connected_with_card();
    let id = call(&bridge, json!({"type":"reject","card_id":7}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(snapshot(&bridge)["cards"].as_array().unwrap().len(), 1);
    let ack = KernelToUi::CardDecisionAck {
        card_id: 7,
        approved: false,
        audit_id: "audit-real".into(),
        effect: None,
    };
    let result = receive(&bridge, id, "session-a", vec![Frame::reply(id, ack)]);
    assert_eq!(result["ok"], true);
    assert!(result["snapshot"]["cards"].as_array().unwrap().is_empty());
    assert_eq!(
        result["snapshot"]["outcomes"][1]["result"]["CardDecisionAck"]["audit_id"],
        "audit-real"
    );
}

#[test]
fn session_change_invalidates_cards_before_new_payload() {
    let bridge = connected_with_card();
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(&bridge, id, "session-b", vec![Frame::reply(id, status())]);
    assert_eq!(result["ok"], true);
    assert!(result["snapshot"]["cards"].as_array().unwrap().is_empty());
    assert_eq!(result["snapshot"]["gateway_session"], "session-b");
}

#[test]
fn memory_journal_and_agents_keep_complete_payloads() {
    let bridge = hub();
    let id = call(
        &bridge,
        json!({"type":"load_memory","query":null,"limit":50}),
    )["request"]["id"]
        .as_u64()
        .unwrap();
    let memory = KernelToUi::MemoryReply {
        rows: vec![MemoryRowDto {
            id: "m1".into(),
            text: "mémoire réelle".into(),
            tags: vec!["user".into()],
            source: "User".into(),
        }],
    };
    assert_eq!(
        receive(&bridge, id, "s", vec![Frame::reply(id, memory)])["snapshot"]["memory"][0]["text"],
        "mémoire réelle"
    );
    let id = call(&bridge, json!({"type":"load_journal","limit":50}))["request"]["id"]
        .as_u64()
        .unwrap();
    let journal = KernelToUi::JournalReply {
        entries: vec![JournalEntryDto {
            id: "a1".into(),
            timestamp: "2026-11-04T00:00:00Z".into(),
            action: "Recorded".into(),
            summary: "résumé réel".into(),
        }],
    };
    assert_eq!(
        receive(&bridge, id, "s", vec![Frame::reply(id, journal)])["snapshot"]["journal"][0]["summary"],
        "résumé réel"
    );
    let id = call(&bridge, json!({"type":"load_agents"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let agents = KernelToUi::AgentListReply {
        agents: vec![AgentDto {
            id: "agent".into(),
            name: "declarative".into(),
            required_caps: vec![],
            artifact_present: false,
            sha256: String::new(),
        }],
    };
    assert_eq!(
        receive(&bridge, id, "s", vec![Frame::reply(id, agents)])["ok"],
        true
    );
    assert_eq!(
        call(
            &bridge,
            json!({"type":"run_agent","name":"declarative","input_json":"{}"})
        )["ok"],
        false
    );
}

#[test]
fn only_status_reply_recovers_unknown_state_and_gateway_errors_cannot_unlock() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"demande"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    bridge.fail_request(id, "transport interrompu".into());
    let id = call(&bridge, json!({"type":"load_agents"}))["request"]["id"]
        .as_u64()
        .unwrap();
    receive(
        &bridge,
        id,
        "new",
        vec![Frame::reply(
            id,
            KernelToUi::AgentListReply { agents: vec![] },
        )],
    );
    assert_eq!(snapshot(&bridge)["connection"], "unknown");
    assert_eq!(call(&bridge, json!({"type":"submit"}))["ok"], false);
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let error = json!({"session_id":"new","frames":[],"error":{"code":"busy","message":"occupé","issue_unknown":false}});
    bridge.receive_json(id, error.to_string());
    assert_eq!(snapshot(&bridge)["connection"], "unknown");
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    receive(&bridge, id, "new", vec![Frame::reply(id, status())]);
    assert_eq!(snapshot(&bridge)["connection"], "connected");
    assert_eq!(
        snapshot(&bridge)["outcomes"][0]["state"],
        "unknown",
        "l'effet ancien reste inconnu malgré reconnexion"
    );
}

#[test]
fn token_delta_after_final_is_rejected_atomically() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"bonjour"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(
        &bridge,
        id,
        "s",
        vec![
            Frame::reply(
                id,
                KernelToUi::HudReply {
                    response: "final".into(),
                    validation: None,
                },
            ),
            Frame::reply(
                0,
                KernelToUi::TokenDelta {
                    text: "trop tard".into(),
                },
            ),
        ],
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["snapshot"]["chat"].as_array().unwrap().len(), 1);
    assert_eq!(result["snapshot"]["streaming"], false);
    assert_eq!(result["snapshot"]["outcomes"][0]["state"], "unknown");
}

#[test]
fn changed_session_during_mutation_never_accepts_ack() {
    let bridge = connected_with_card();
    let id = call(&bridge, json!({"type":"reject","card_id":7}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(
        &bridge,
        id,
        "different",
        vec![Frame::reply(
            id,
            KernelToUi::CardDecisionAck {
                card_id: 7,
                approved: false,
                audit_id: "not-accepted".into(),
                effect: None,
            },
        )],
    );
    assert_eq!(result["ok"], false);
    assert!(result["snapshot"]["cards"].as_array().unwrap().is_empty());
    assert_eq!(result["snapshot"]["outcomes"][1]["state"], "unknown");
}

#[test]
fn unsupported_microphone_and_corrupted_restore_never_change_draft() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"à conserver"}));
    assert_eq!(
        call(&bridge, json!({"type":"set_microphone","enabled":true}))["ok"],
        false
    );
    assert_eq!(snapshot(&bridge)["microphone_enabled"], false);
    for invalid in ["{broken", r#"{"schema_version":2,"draft":"other"}"#] {
        let result: Value =
            serde_json::from_str(&bridge.restore_draft_json(invalid.into())).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["snapshot"]["draft"], "à conserver");
    }
    assert_eq!(
        call(
            &bridge,
            json!({"type":"set_hub","hub_id":"https://user:secret@hub"})
        )["ok"],
        false
    );
}

#[test]
fn duplicate_terminal_bad_version_and_wrong_card_ack_are_refused() {
    for messages in [
        vec![Frame::reply(
            2,
            KernelToUi::CardDecisionAck {
                card_id: 99,
                approved: false,
                audit_id: "audit".into(),
                effect: None,
            },
        )],
        vec![Frame::reply(
            2,
            KernelToUi::CardDecisionAck {
                card_id: 7,
                approved: true,
                audit_id: "audit".into(),
                effect: None,
            },
        )],
        vec![
            Frame::reply(
                2,
                KernelToUi::CardDecisionAck {
                    card_id: 7,
                    approved: false,
                    audit_id: "audit".into(),
                    effect: None,
                },
            ),
            Frame::reply(
                2,
                KernelToUi::CardDecisionAck {
                    card_id: 7,
                    approved: false,
                    audit_id: "audit".into(),
                    effect: None,
                },
            ),
        ],
        vec![Frame {
            version: 2,
            id: 2,
            body: FrameBody::Reply {
                message: KernelToUi::CardDecisionAck {
                    card_id: 7,
                    approved: false,
                    audit_id: "audit".into(),
                    effect: None,
                },
            },
        }],
    ] {
        let bridge = connected_with_card();
        assert_eq!(
            call(&bridge, json!({"type":"reject","card_id":7}))["request"]["id"],
            2
        );
        let result = receive(&bridge, 2, "session-a", messages);
        assert_eq!(result["ok"], false);
        assert_eq!(result["snapshot"]["outcomes"][1]["state"], "unknown");
    }
}

#[test]
fn editing_draft_while_waiting_keeps_new_text_after_reply() {
    let bridge = hub();
    call(
        &bridge,
        json!({"type":"set_draft","text":"première demande"}),
    );
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    call(
        &bridge,
        json!({"type":"set_draft","text":"brouillon suivant"}),
    );
    receive(
        &bridge,
        id,
        "s",
        vec![Frame::reply(
            id,
            KernelToUi::HudReply {
                response: "réponse".into(),
                validation: None,
            },
        )],
    );
    assert_eq!(snapshot(&bridge)["draft"], "brouillon suivant");
}

#[test]
fn notifications_and_journal_identifiers_are_preserved() {
    let bridge = hub();
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(
        &bridge,
        id,
        "s",
        vec![
            Frame::reply(
                0,
                KernelToUi::Toast {
                    text: "Information réelle".into(),
                },
            ),
            Frame::reply(id, status()),
            Frame::reply(0, KernelToUi::JournalAppended { id: u64::MAX }),
        ],
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["snapshot"]["notifications"][0], "Information réelle");
    assert_eq!(
        result["snapshot"]["last_journal_entry_id"].as_u64(),
        Some(u64::MAX)
    );
}

#[test]
fn invalid_session_header_and_large_received_body_do_not_apply_partial_data() {
    for session in ["bad\r\nheader", "space token", ""] {
        let bridge = hub();
        let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
            .as_u64()
            .unwrap();
        assert_eq!(
            receive(&bridge, id, session, vec![Frame::reply(id, status())])["ok"],
            false
        );
        assert!(snapshot(&bridge)["status"].is_null());
    }
    let bridge = hub();
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result: Value =
        serde_json::from_str(&bridge.receive_json(id, "x".repeat(MAX_JSON_BYTES + 1))).unwrap();
    assert_eq!(result["ok"], false);
    assert!(result["snapshot"]["pending"].is_null());
}

#[test]
fn history_byte_budget_evicts_old_entries_and_does_not_deadlock_future_requests() {
    let bridge = hub();
    for turn in 0..12 {
        call(
            &bridge,
            json!({"type":"set_draft","text":format!("question {turn}")}),
        );
        let request = call(&bridge, json!({"type":"submit"}));
        assert_eq!(request["ok"], true);
        let id = request["request"]["id"].as_u64().unwrap();
        let result = receive(
            &bridge,
            id,
            "s",
            vec![Frame::reply(
                id,
                KernelToUi::HudReply {
                    response: "x".repeat(60 * 1024),
                    validation: None,
                },
            )],
        );
        assert_eq!(result["ok"], true, "{result}");
        assert!(bridge.snapshot_json().len() <= 512 * 1024);
    }
    assert_eq!(snapshot(&bridge)["history_trimmed"], true);
    assert_eq!(call(&bridge, json!({"type":"refresh_status"}))["ok"], true);
}

#[test]
fn stale_session_rejection_before_send_is_known_and_allows_clean_status_bootstrap() {
    let bridge = connected_with_card();
    let id = call(&bridge, json!({"type":"reject","card_id":7}))["request"]["id"]
        .as_u64()
        .unwrap();
    let envelope = json!({"session_id":"session-b","frames":[],"error":{"code":"stale_session","message":"session périmée, non envoyé","issue_unknown":false}});
    let result: Value =
        serde_json::from_str(&bridge.receive_json(id, envelope.to_string())).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["snapshot"]["outcomes"][1]["state"], "failed");
    assert!(result["snapshot"]["gateway_session"].is_null());
    assert_eq!(result["snapshot"]["connection"], "unknown");
    let id = call(&bridge, json!({"type":"refresh_status"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert!(snapshot(&bridge)["gateway_session"].is_null());
    assert_eq!(
        receive(&bridge, id, "session-b", vec![Frame::reply(id, status())])["ok"],
        true
    );
}

fn checkpoint(bridge: &MobileBridge) -> Value {
    serde_json::from_str(&bridge.draft_json()).unwrap()
}
fn restore(bridge: &MobileBridge, saved: &Value) -> Value {
    serde_json::from_str(&bridge.restore_draft_json(saved.to_string())).unwrap()
}
fn checkpoint_with(ids: &[u64], next: u64) -> Value {
    json!({"schema_version":2,"draft":"brouillon","next_request_id":next,
        "uncertain_operations":ids.iter().map(|id|json!({"request_id":id,"kind":"remember","phase":"in_flight"})).collect::<Vec<_>>(),
        "acknowledged_operations":[]})
}
fn refresh(bridge: &MobileBridge) {
    let request = call(bridge, json!({"type":"refresh_status"}));
    let id = request["request"]["id"].as_u64().unwrap();
    assert_eq!(
        receive(bridge, id, "session-a", vec![Frame::reply(id, status())])["ok"],
        true
    );
}

#[test]
fn write_ahead_checkpoint_contains_only_bounded_metadata_and_never_replays() {
    let original = hub();
    call(
        &original,
        json!({"type":"set_draft","text":"brouillon distinct"}),
    );
    let dispatched = call(
        &original,
        json!({"type":"remember","text":"contenu personnel absent du checkpoint"}),
    );
    let id = dispatched["request"]["id"].as_u64().unwrap();
    let saved = checkpoint(&original);
    assert_eq!(
        saved["uncertain_operations"],
        json!([{"request_id":id,"kind":"remember","phase":"in_flight"}])
    );
    assert_eq!(saved["next_request_id"], id + 1);
    assert_eq!(saved.as_object().unwrap().len(), 5);
    assert!(!saved.to_string().contains("contenu personnel"));
    assert!(!saved.to_string().contains("home"));
    let restored = MobileBridge::new();
    let result = restore(&restored, &saved);
    assert_eq!(result["ok"], true);
    assert!(result["request"].is_null());
    let state = &result["snapshot"];
    for field in ["pending", "hub_id", "gateway_session", "approval"] {
        assert!(state[field].is_null(), "{field}");
    }
    assert_eq!(state["microphone_enabled"], false);
    assert_eq!(state["recovery_required"], true);
    assert_eq!(state["uncertain_operations"][0]["phase"], "unknown");
    assert_eq!(state["outcomes"][0]["state"], "unknown");
    assert_eq!(state["outcomes"][0]["acknowledged"], false);
    assert_eq!(state["draft"], "brouillon distinct");
    assert_eq!(restore(&restored, &saved)["ok"], true);
    assert_eq!(
        snapshot(&restored)["uncertain_operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(snapshot(&restored)["outcomes"].as_array().unwrap().len(), 1);
}

#[test]
fn restored_unknown_requires_awareness_even_after_status_and_journal() {
    let bridge = MobileBridge::new();
    assert_eq!(restore(&bridge, &checkpoint_with(&[8], 9))["ok"], true);
    call(&bridge, json!({"type":"set_hub","hub_id":"autre-hub"}));
    refresh(&bridge);
    assert_eq!(snapshot(&bridge)["connection"], "connected");
    assert_eq!(snapshot(&bridge)["recovery_required"], true);
    assert_eq!(
        call(&bridge, json!({"type":"remember","text":"nouveau"}))["ok"],
        false
    );
    let journal = call(&bridge, json!({"type":"load_journal","limit":10}));
    let id = journal["request"]["id"].as_u64().unwrap();
    assert_eq!(
        receive(
            &bridge,
            id,
            "session-a",
            vec![Frame::reply(
                id,
                KernelToUi::JournalReply { entries: vec![] }
            )]
        )["ok"],
        true
    );
    let ack = call(
        &bridge,
        json!({"type":"acknowledge_unknown","request_id":8}),
    );
    assert_eq!(ack["ok"], true);
    assert!(ack["request"].is_null());
    assert_eq!(ack["snapshot"]["recovery_required"], false);
    let old = ack["snapshot"]["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["request_id"] == 8)
        .unwrap();
    assert_eq!(old["state"], "unknown");
    assert_eq!(old["acknowledged"], true);
    assert!(old["result"].is_null());
    assert_eq!(
        call(
            &bridge,
            json!({"type":"acknowledge_unknown","request_id":8})
        )["ok"],
        true
    );
    assert_eq!(
        call(&bridge, json!({"type":"remember","text":"nouveau"}))["ok"],
        true
    );
}

#[test]
fn known_refusal_and_before_send_failure_clear_marker_but_transport_failure_preserves_it() {
    let bridge = hub();
    let first = call(&bridge, json!({"type":"remember","text":"x"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        call(
            &bridge,
            json!({"type":"acknowledge_unknown","request_id":first})
        )["ok"],
        false
    );
    let result = call(
        &bridge,
        json!({"type":"fail_before_send","request_id":first,"message":"sauvegarde impossible"}),
    );
    assert_eq!(result["ok"], true);
    assert!(result["request"].is_null());
    assert_eq!(result["snapshot"]["outcomes"][0]["state"], "failed");
    assert!(
        result["snapshot"]["outcomes"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("Non envoyée")
    );
    assert_eq!(checkpoint(&bridge)["uncertain_operations"], json!([]));
    let second = call(&bridge, json!({"type":"remember","text":"y"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        call(
            &bridge,
            json!({"type":"fail_before_send","request_id":first,"message":"ancien"})
        )["ok"],
        false
    );
    assert_eq!(snapshot(&bridge)["pending"]["request_id"], second);
    let rejected = json!({"session_id":null,"frames":[],"error":{"code":"busy","message":"non envoyée","issue_unknown":false}});
    bridge.receive_json(second, rejected.to_string());
    assert_eq!(checkpoint(&bridge)["uncertain_operations"], json!([]));
    let third = call(&bridge, json!({"type":"remember","text":"z"}))["request"]["id"]
        .as_u64()
        .unwrap();
    bridge.fail_request(third, "perte du transport après émission".into());
    assert_eq!(
        checkpoint(&bridge)["uncertain_operations"],
        json!([{"request_id":third,"kind":"remember","phase":"unknown"}])
    );
    assert_eq!(snapshot(&bridge)["recovery_required"], true);
}

#[test]
fn final_reply_removes_write_ahead_marker_and_readonly_has_none() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"bonjour"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        checkpoint(&bridge)["uncertain_operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    receive(
        &bridge,
        id,
        "session-a",
        vec![Frame::reply(
            id,
            KernelToUi::HudReply {
                response: "bonjour".into(),
                validation: None,
            },
        )],
    );
    assert_eq!(checkpoint(&bridge)["uncertain_operations"], json!([]));
    let read = call(&bridge, json!({"type":"load_agents"}));
    assert!(read["request"].is_object());
    assert_eq!(checkpoint(&bridge)["uncertain_operations"], json!([]));
    let restored = MobileBridge::new();
    assert_eq!(restore(&restored, &checkpoint(&bridge))["ok"], true);
    assert_eq!(snapshot(&restored)["recovery_required"], false);
    assert!(snapshot(&restored)["pending"].is_null());
}

#[test]
fn compatible_v1_and_repeated_restore_cannot_erase_or_rearm_uncertainty() {
    let bridge = MobileBridge::new();
    let saved = checkpoint_with(&[42], 43);
    assert_eq!(restore(&bridge, &saved)["ok"], true);
    assert_eq!(
        restore(&bridge, &json!({"schema_version":1,"draft":"ancien"}))["ok"],
        true
    );
    assert_eq!(checkpoint(&bridge)["next_request_id"], 43);
    assert_eq!(snapshot(&bridge)["recovery_required"], true);
    call(
        &bridge,
        json!({"type":"acknowledge_unknown","request_id":42}),
    );
    assert_eq!(restore(&bridge, &saved)["ok"], true);
    assert_eq!(snapshot(&bridge)["recovery_required"], false);
    assert_eq!(snapshot(&bridge)["uncertain_operations"], json!([]));
    let saved_ack = checkpoint(&bridge);
    assert_eq!(
        saved_ack["acknowledged_operations"],
        json!([{"request_id":42,"kind":"remember"}])
    );
    let restarted = MobileBridge::new();
    assert_eq!(restore(&restarted, &saved_ack)["ok"], true);
    assert_eq!(restore(&restarted, &saved_ack)["ok"], true);
    assert_eq!(snapshot(&restarted)["outcomes"][0]["state"], "unknown");
    assert_eq!(snapshot(&restarted)["outcomes"][0]["acknowledged"], true);
    assert_eq!(
        snapshot(&restarted)["outcomes"].as_array().unwrap().len(),
        1
    );
    assert_eq!(checkpoint(&restarted)["next_request_id"], 43);
}

#[test]
fn malformed_or_forged_checkpoints_leave_state_unchanged() {
    let bridge = MobileBridge::new();
    call(&bridge, json!({"type":"set_draft","text":"à préserver"}));
    let before = snapshot(&bridge);
    let valid = checkpoint_with(&[1], 2);
    let mut cases = vec![];
    for version in [0, 1, 3] {
        let mut value = valid.clone();
        value["schema_version"] = json!(version);
        cases.push(value);
    }
    for next in [0, 1] {
        let mut value = valid.clone();
        value["next_request_id"] = json!(next);
        cases.push(value);
    }
    for id in [0, u64::MAX] {
        let mut value = valid.clone();
        value["uncertain_operations"][0]["request_id"] = json!(id);
        cases.push(value);
    }
    for (key, value) in [
        ("kind", json!("refresh_status")),
        ("phase", json!("confirmed")),
        ("token", json!("secret")),
    ] {
        let mut malformed = valid.clone();
        malformed["uncertain_operations"][0][key] = value;
        cases.push(malformed);
    }
    let mut secret = valid.clone();
    secret["token"] = json!("secret");
    cases.push(secret);
    let mut duplicate = valid.clone();
    duplicate["acknowledged_operations"] = json!([{"request_id":1,"kind":"remember"}]);
    cases.push(duplicate);
    cases.push(checkpoint_with(&[1, 1], 2));
    cases.push(checkpoint_with(&(1..=33).collect::<Vec<_>>(), 34));
    let mut oversized = valid.clone();
    oversized["draft"] = json!("x".repeat(MAX_DRAFT_BYTES + 1));
    cases.push(oversized);
    let mut nul = valid.clone();
    nul["draft"] = json!("\0");
    cases.push(nul);
    for malformed in cases {
        assert_eq!(restore(&bridge, &malformed)["ok"], false, "{malformed}");
        assert_eq!(snapshot(&bridge), before);
    }
    for malformed in [
        " ".repeat(65537),
        valid.to_string().replace(
            "\"next_request_id\":2",
            "\"next_request_id\":18446744073709551616",
        ),
    ] {
        let result: Value = serde_json::from_str(&bridge.restore_draft_json(malformed)).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(snapshot(&bridge), before);
    }
}

#[test]
fn exhausted_counter_survives_restart_and_local_acknowledgment_remains_available() {
    let bridge = hub();
    assert_eq!(
        restore(&bridge, &checkpoint_with(&[], u64::MAX - 1))["ok"],
        true
    );
    let id = call(&bridge, json!({"type":"remember","text":"dernier"}))["request"]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(id, u64::MAX - 1);
    let restarted = MobileBridge::new();
    assert_eq!(restore(&restarted, &checkpoint(&bridge))["ok"], true);
    assert_eq!(checkpoint(&restarted)["next_request_id"], u64::MAX);
    call(&restarted, json!({"type":"set_hub","hub_id":"hub"}));
    assert_eq!(
        call(
            &restarted,
            json!({"type":"acknowledge_unknown","request_id":id})
        )["ok"],
        true
    );
    assert_eq!(
        call(&restarted, json!({"type":"refresh_status"}))["ok"],
        false
    );
    assert_eq!(checkpoint(&restarted)["next_request_id"], u64::MAX);
}

#[test]
fn full_recovery_capacity_can_be_acknowledged_without_permanent_lock() {
    let bridge = hub();
    assert_eq!(
        restore(&bridge, &checkpoint_with(&(1..=32).collect::<Vec<_>>(), 33))["ok"],
        true
    );
    refresh(&bridge);
    for id in 1..=32 {
        let result = call(
            &bridge,
            json!({"type":"acknowledge_unknown","request_id":id}),
        );
        assert_eq!(result["ok"], true);
        assert!(result["request"].is_null());
    }
    assert_eq!(snapshot(&bridge)["recovery_required"], false);
    let id = call(
        &bridge,
        json!({"type":"remember","text":"nouvelle demande"}),
    )["request"]["id"]
        .as_u64()
        .unwrap();
    bridge.fail_request(id, "transport interrompu".into());
    call(
        &bridge,
        json!({"type":"acknowledge_unknown","request_id":id}),
    );
    let saved = checkpoint(&bridge);
    assert_eq!(saved["uncertain_operations"], json!([]));
    assert_eq!(
        saved["acknowledged_operations"].as_array().unwrap().len(),
        32
    );
    assert_eq!(saved["acknowledged_operations"][31]["request_id"], id);
    let restarted = MobileBridge::new();
    assert_eq!(restore(&restarted, &saved)["ok"], true);
    assert_eq!(snapshot(&restarted)["recovery_required"], false);
}

#[test]
fn checkpoint_budget_allows_maximum_escaped_draft_and_metadata() {
    let mut saved = checkpoint_with(&(33..=64).collect::<Vec<_>>(), 65);
    saved["draft"] = json!("\u{1}".repeat(MAX_DRAFT_BYTES));
    saved["acknowledged_operations"] = json!(
        (1..=32)
            .map(|id| json!({"request_id":id,"kind":"remember"}))
            .collect::<Vec<_>>()
    );
    let bridge = MobileBridge::new();
    assert_eq!(restore(&bridge, &saved)["ok"], true);
    assert!(bridge.draft_json().len() <= 64 * 1024);
    let restarted = MobileBridge::new();
    assert_eq!(restore(&restarted, &checkpoint(&bridge))["ok"], true);
    assert_eq!(snapshot(&restarted)["draft"], saved["draft"]);
    assert_eq!(
        snapshot(&restarted)["uncertain_operations"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
}

#[test]
fn checkpoint_import_cannot_override_known_outcome_or_ack_live_uncertainty() {
    let bridge = hub();
    let id = call(&bridge, json!({"type":"remember","text":"jamais envoyé"}))["request"]["id"]
        .as_u64()
        .unwrap();
    call(
        &bridge,
        json!({"type":"fail_before_send","request_id":id,"message":"aucun réseau"}),
    );
    let before = snapshot(&bridge);
    assert_eq!(
        restore(&bridge, &checkpoint_with(&[id], id + 1))["ok"],
        false
    );
    assert_eq!(snapshot(&bridge), before);
    let uncertain = MobileBridge::new();
    assert_eq!(restore(&uncertain, &checkpoint_with(&[5], 6))["ok"], true);
    let before = snapshot(&uncertain);
    let forged = json!({"schema_version":2,"draft":"altéré","next_request_id":6,"uncertain_operations":[],"acknowledged_operations":[{"request_id":5,"kind":"remember"}]});
    assert_eq!(restore(&uncertain, &forged)["ok"], false);
    assert_eq!(snapshot(&uncertain), before);
}

fn voice_ready() -> MobileBridge {
    let bridge = hub();
    refresh(&bridge);
    bridge
}
fn arm_voice(bridge: &MobileBridge) -> u64 {
    let result = call(bridge, json!({"type":"begin_voice"}));
    assert_eq!(result["ok"], true, "{result}");
    result["snapshot"]["voice_context_id"].as_u64().unwrap()
}
fn submit_voice(bridge: &MobileBridge, context: u64) -> u64 {
    let result = call(
        bridge,
        json!({"type":"submit_voice","context_id":context,"text":"Bonjour"}),
    );
    assert_eq!(result["ok"], true, "{result}");
    result["request"]["id"].as_u64().unwrap()
}
fn voice_final(id: u64, value: &str) -> Frame {
    Frame::reply(
        id,
        KernelToUi::HudReply {
            response: value.into(),
            validation: None,
        },
    )
}

#[test]
fn voice_requires_explicit_arm_confirmed_context_and_empty_draft() {
    for bridge in [MobileBridge::new(), hub(), connected_with_card()] {
        assert_eq!(call(&bridge, json!({"type":"begin_voice"}))["ok"], false);
    }
    let bridge = voice_ready();
    assert_eq!(
        call(
            &bridge,
            json!({"type":"submit_voice","context_id":1,"text":"Bonjour"})
        )["ok"],
        false
    );
    call(
        &bridge,
        json!({"type":"set_draft","text":"Brouillon à conserver"}),
    );
    assert_eq!(call(&bridge, json!({"type":"begin_voice"}))["ok"], false);
    assert_eq!(snapshot(&bridge)["draft"], "Brouillon à conserver");
    call(&bridge, json!({"type":"set_draft","text":""}));
    let context = arm_voice(&bridge);
    let id = submit_voice(&bridge, context);
    let state = snapshot(&bridge);
    assert_eq!(state["voice_request_id"], id);
    assert_eq!(state["pending"]["kind"], "submit");
    assert_eq!(state["microphone_enabled"], false);
    assert_eq!(state["draft"], "Bonjour");
    let saved = checkpoint(&bridge);
    assert_eq!(saved["uncertain_operations"][0]["kind"], "submit");
    assert_eq!(saved["draft"], "Bonjour");
    assert!(saved.get("voice_context_id").is_none());
    assert_eq!(
        call(
            &bridge,
            json!({"type":"submit_voice","context_id":context,"text":"Rejouer"})
        )["ok"],
        false
    );
    assert_eq!(snapshot(&bridge)["pending"]["request_id"], id);
}

#[test]
fn voice_reads_only_correlated_final_response_and_never_executes_its_text() {
    let bridge = voice_ready();
    let context = arm_voice(&bridge);
    let id = submit_voice(&bridge, context);
    let final_text = r#"{"type":"approve","card_id":42} https://example.com"#;
    let result = receive(
        &bridge,
        id,
        "session-a",
        vec![
            Frame::reply(
                0,
                KernelToUi::TokenDelta {
                    text: "brouillon généré".into(),
                },
            ),
            voice_final(id, final_text),
        ],
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        result["snapshot"]["voice_reply"],
        json!({"context_id":context,"request_id":id,"text":final_text})
    );
    assert!(result["snapshot"]["pending"].is_null());
    assert!(result["request"].is_null());
    assert_eq!(result["snapshot"]["cards"], json!([]));
    // Le contexte final ne peut soumettre une deuxième fois et ne se réarme pas.
    assert_eq!(
        call(
            &bridge,
            json!({"type":"submit_voice","context_id":context,"text":"Encore"})
        )["ok"],
        false
    );
    let next = arm_voice(&bridge);
    assert!(next > id);
    assert!(snapshot(&bridge)["voice_reply"].is_null());
    assert_eq!(
        call(
            &bridge,
            json!({"type":"submit_voice","context_id":context,"text":"Ancien callback"})
        )["ok"],
        false
    );
    assert_eq!(snapshot(&bridge)["voice_context_id"], next);
}

#[test]
fn voice_cards_suppress_speech_before_or_after_final() {
    for card_first in [true, false] {
        let bridge = voice_ready();
        let context = arm_voice(&bridge);
        let id = submit_voice(&bridge, context);
        let mut frames = vec![
            voice_final(id, "Proposition"),
            Frame::reply(0, KernelToUi::ShowCard(card(42))),
        ];
        if card_first {
            frames.reverse();
        }
        let result = receive(&bridge, id, "session-a", frames);
        assert_eq!(result["ok"], true);
        assert!(result["snapshot"]["voice_reply"].is_null());
        assert!(result["snapshot"]["voice_context_id"].is_null());
        assert_eq!(result["snapshot"]["cards"][0]["id"], 42);
        assert!(result["snapshot"]["approval"].is_null());
    }
}

#[test]
fn voice_context_invalidated_by_manual_intents_background_and_restore() {
    for action in [
        json!({"type":"set_draft","text":"édition"}),
        json!({"type":"set_hub","hub_id":"autre"}),
        json!({"type":"set_foreground","active":false}),
        json!({"type":"refresh_status"}),
        json!({"type":"cancel_voice"}),
    ] {
        let bridge = voice_ready();
        let context = arm_voice(&bridge);
        call(&bridge, action);
        assert!(snapshot(&bridge)["voice_context_id"].is_null());
        assert_eq!(
            call(
                &bridge,
                json!({"type":"submit_voice","context_id":context,"text":"Ancien"})
            )["ok"],
            false
        );
        assert!(snapshot(&bridge)["chat"].as_array().unwrap().is_empty());
    }
    let bridge = voice_ready();
    let context = arm_voice(&bridge);
    call(&bridge, json!({"type":"set_foreground","active":true}));
    assert_eq!(snapshot(&bridge)["voice_context_id"], context);
    let saved = checkpoint(&bridge);
    assert_eq!(restore(&bridge, &saved)["ok"], true);
    assert!(snapshot(&bridge)["voice_context_id"].is_null());
    assert!(arm_voice(&bridge) > context);
}

#[test]
fn voice_cancel_preserves_unknown_mutation_and_rejects_late_response() {
    let bridge = voice_ready();
    let context = arm_voice(&bridge);
    let id = submit_voice(&bridge, context);
    let cancelled = call(&bridge, json!({"type":"cancel_voice"}));
    assert_eq!(cancelled["snapshot"]["recovery_required"], true);
    assert_eq!(
        cancelled["snapshot"]["outcomes"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["state"],
        "unknown"
    );
    assert_eq!(cancelled["snapshot"]["draft"], "Bonjour");
    assert!(cancelled["snapshot"]["voice_context_id"].is_null());
    assert_eq!(
        receive(&bridge, id, "session-a", vec![voice_final(id, "Trop tard")])["ok"],
        false
    );
    assert!(snapshot(&bridge)["voice_reply"].is_null());
}

#[test]
fn voice_changed_session_and_failed_reply_never_produce_speech() {
    for change_session in [true, false] {
        let bridge = voice_ready();
        let context = arm_voice(&bridge);
        let id = submit_voice(&bridge, context);
        if change_session {
            assert_eq!(
                receive(
                    &bridge,
                    id,
                    "session-new",
                    vec![voice_final(id, "Autre session")]
                )["ok"],
                false
            );
        } else {
            call(
                &bridge,
                json!({"type":"fail_before_send","request_id":id,"message":"Pas de réseau"}),
            );
        }
        assert!(snapshot(&bridge)["voice_reply"].is_null());
        assert!(snapshot(&bridge)["voice_context_id"].is_null());
    }
}

#[test]
fn voice_capture_expires_without_sleep_and_identifiers_do_not_wrap() {
    let bridge = voice_ready();
    let mut state = bridge.state.lock().unwrap();
    let now = Instant::now();
    state.dispatch(MobileAction::BeginVoice {}, now).unwrap();
    let context = state.snapshot.voice_context_id.unwrap();
    assert!(
        state
            .dispatch(
                MobileAction::SubmitVoice {
                    context_id: context,
                    text: "Trop tard".into()
                },
                now + Duration::from_secs(120)
            )
            .is_err()
    );
    assert!(state.snapshot.voice_context_id.is_none());
    assert!(state.snapshot.pending.is_none());
    state.next_id = u64::MAX - 1;
    assert!(state.dispatch(MobileAction::BeginVoice {}, now).is_err());
    assert_eq!(state.next_id, u64::MAX - 1);
}

#[test]
fn voice_bounds_preserve_draft_and_long_final_remains_text_only() {
    let bridge = voice_ready();
    let context = arm_voice(&bridge);
    for value in [
        " ".to_owned(),
        "x\0y".to_owned(),
        "x".repeat(MAX_DRAFT_BYTES + 1),
    ] {
        let result = call(
            &bridge,
            json!({"type":"submit_voice","context_id":context,"text":value}),
        );
        assert_eq!(result["ok"], false);
        assert_eq!(result["snapshot"]["draft"], "");
        assert!(result["snapshot"]["pending"].is_null());
    }
    let id = submit_voice(&bridge, context);
    let result = receive(
        &bridge,
        id,
        "session-a",
        vec![voice_final(id, &"x".repeat(MAX_DRAFT_BYTES + 1))],
    );
    assert_eq!(result["ok"], true);
    assert!(result["snapshot"]["voice_reply"].is_null());
    assert_eq!(
        result["snapshot"]["chat"][1]["text"]
            .as_str()
            .unwrap()
            .len(),
        MAX_DRAFT_BYTES + 1
    );
}

#[test]
fn voice_stale_transport_callbacks_cannot_cancel_or_speak_for_next_turn() {
    let bridge = voice_ready();
    let first_context = arm_voice(&bridge);
    let first_id = submit_voice(&bridge, first_context);
    receive(
        &bridge,
        first_id,
        "session-a",
        vec![voice_final(first_id, "Premier")],
    );
    let next_context = arm_voice(&bridge);
    let next_id = submit_voice(&bridge, next_context);
    assert_eq!(
        receive(
            &bridge,
            first_id,
            "session-a",
            vec![voice_final(first_id, "Ancien callback")]
        )["ok"],
        false
    );
    assert_eq!(
        call(
            &bridge,
            json!({"type":"fail_before_send","request_id":first_id,"message":"Ancien échec"})
        )["ok"],
        false
    );
    let current = snapshot(&bridge);
    assert_eq!(current["voice_context_id"], next_context);
    assert_eq!(current["voice_request_id"], next_id);
    assert_eq!(current["pending"]["request_id"], next_id);
    assert!(current["voice_reply"].is_null());
    let result = receive(
        &bridge,
        next_id,
        "session-a",
        vec![voice_final(next_id, "Deuxième")],
    );
    assert_eq!(result["snapshot"]["voice_reply"]["text"], "Deuxième");
}

#[test]
fn voice_never_reads_text_submission_or_incomplete_stream() {
    let bridge = voice_ready();
    arm_voice(&bridge);
    call(
        &bridge,
        json!({"type":"set_draft","text":"Saisie manuelle"}),
    );
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();
    let result = receive(
        &bridge,
        id,
        "session-a",
        vec![voice_final(id, "Réponse au clavier")],
    );
    assert_eq!(result["ok"], true);
    assert!(result["snapshot"]["voice_reply"].is_null());
    let context = arm_voice(&bridge);
    let id = submit_voice(&bridge, context);
    let result = receive(
        &bridge,
        id,
        "session-a",
        vec![Frame::reply(
            0,
            KernelToUi::TokenDelta {
                text: "Provisoire seulement".into(),
            },
        )],
    );
    assert_eq!(result["ok"], false);
    assert!(result["snapshot"]["voice_reply"].is_null());
    assert_eq!(result["snapshot"]["recovery_required"], true);
}

#[test]
fn detached_in_flight_on_background_does_not_lock_recovery() {
    let bridge = hub();
    call(&bridge, json!({"type":"set_draft","text":"message urgent"}));
    let id = call(&bridge, json!({"type":"submit"}))["request"]["id"]
        .as_u64()
        .unwrap();

    let snap_before = snapshot(&bridge);
    assert_eq!(snap_before["pending"]["request_id"], id);
    assert_eq!(snap_before["uncertain_operations"][0]["phase"], "in_flight");
    assert_eq!(snap_before["recovery_required"], false);

    // App passe en arrière-plan
    call(&bridge, json!({"type":"set_foreground","active":false}));
    let snap_bg = snapshot(&bridge);
    assert_eq!(snap_bg["foreground"], false);
    assert_eq!(snap_bg["uncertain_operations"][0]["phase"], "in_flight_detached");
    // CRITIQUE : l'application n'est PAS verrouillée avec recovery_required = true
    assert_eq!(snap_bg["recovery_required"], false);

    // App revient au premier plan
    call(&bridge, json!({"type":"set_foreground","active":true}));
    let snap_fg = snapshot(&bridge);
    assert_eq!(snap_fg["foreground"], true);
    assert_eq!(snap_fg["recovery_required"], false);

    // Réception de la réponse du daemon pour la requête détachée
    let res = receive(
        &bridge,
        id,
        "session-1",
        vec![Frame::reply(
            id,
            KernelToUi::HudReply {
                response: "Traîté avec succès".into(),
                validation: None,
            },
        )],
    );
    assert_eq!(res["ok"], true);
    let snap_after = snapshot(&bridge);
    assert!(snap_after["pending"].is_null());
    assert_eq!(snap_after["uncertain_operations"].as_array().unwrap().len(), 0);
    assert_eq!(snap_after["recovery_required"], false);
    assert_eq!(snap_after["chat"].as_array().unwrap().len(), 2);
}

#[test]
fn mobile_battery_and_shadow_sync_bindings() {
    let bridge = MobileBridge::new();
    assert_eq!(bridge.battery_tier(90.0, false), "normal");
    assert_eq!(bridge.battery_tier(20.0, false), "conserve");
    assert_eq!(bridge.battery_tier(5.0, false), "critical");
    assert_eq!(bridge.battery_tier(10.0, true), "charging");

    assert_eq!(bridge.perception_fps_limit(90.0, false), 30);
    assert_eq!(bridge.perception_fps_limit(20.0, false), 1);
    assert_eq!(bridge.perception_fps_limit(5.0, false), 0);

    assert_eq!(bridge.should_offload_inference(20.0, false, true), true);
    assert_eq!(bridge.should_offload_inference(20.0, false, false), false);

    let battery = MobileBatteryManager::new(85.0, false);
    assert_eq!(battery.tier(), "normal");
    assert_eq!(battery.perception_fps(), 30);
    assert_eq!(battery.record_energy("AgentMail".into(), 1200), Ok(()));
    assert_eq!(battery.get_energy("AgentMail".into()), 1200);

    let shadow = MobileShadowSyncBridge::new("https://shadow.example.com".into(), "device-123".into());
    assert_eq!(shadow.state(), "disconnected");
    assert_eq!(shadow.pending_count(), 0);
    assert_eq!(shadow.queue_delta("notes/test".into(), b"data".to_vec()), Ok(()));
    assert_eq!(shadow.pending_count(), 1);
    shadow.enter_doze();
    assert_eq!(shadow.wake_and_resync(), 0);
}
