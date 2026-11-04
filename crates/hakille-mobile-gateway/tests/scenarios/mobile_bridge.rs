//! Contrat complet façade Rust → vrai HTTP → vrai IPC → noyau contrôlé.
//! Le noyau ci-dessous ne prétend exécuter ni modèle ni agent métier.
use super::*;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, JournalEntryDto, MemoryRowDto};
use serde_json::json;

fn action(bridge: &MobileBridge, value: Value) -> Value {
    serde_json::from_str(&bridge.dispatch_json(value.to_string())).unwrap()
}

fn mobile(rig: &Rig) -> MobileBridge {
    let bridge = MobileBridge::new();
    assert_eq!(
        action(&bridge, json!({"type":"set_hub","hub_id":rig.base}))["ok"],
        true
    );
    bridge
}

fn prepare(bridge: &MobileBridge, value: Value) -> (Frame, Option<String>) {
    let result = action(bridge, value);
    assert_eq!(result["ok"], true, "{result}");
    let session = result["snapshot"]["gateway_session"]
        .as_str()
        .map(str::to_owned);
    (
        serde_json::from_value(result["request"].clone()).unwrap(),
        session,
    )
}

fn receive(bridge: &MobileBridge, id: u64, body: String) -> Value {
    serde_json::from_str(&bridge.receive_json(id, body)).unwrap()
}

async fn roundtrip(rig: &Rig, bridge: &MobileBridge, value: Value) -> (u16, Value) {
    let (frame, session) = prepare(bridge, value);
    let mut request = rig.post(&frame);
    if let Some(session) = session {
        request = request.header("x-hakille-session", session);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    // Même les enveloppes HTTP non2xx sont décodées par la vraie façade.
    (
        status,
        receive(bridge, frame.id, response.text().await.unwrap()),
    )
}

fn card(id: u64) -> KernelToUi {
    KernelToUi::ShowCard(ActionCard {
        id,
        verb: "Proposer".into(),
        object: "une action de test".into(),
        reason: "fixture contrôlée".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    })
}

struct Step {
    request: UiToKernel,
    reply: KernelToUi,
    before: Vec<KernelToUi>,
    after: Vec<KernelToUi>,
}

impl Step {
    fn new(request: UiToKernel, reply: KernelToUi) -> Self {
        Self {
            request,
            reply,
            before: Vec::new(),
            after: Vec::new(),
        }
    }
    fn chat(query: &str, id: u64) -> Self {
        Self {
            request: UiToKernel::HudSubmit {
                query: query.into(),
            },
            reply: KernelToUi::HudReply {
                response: "Proposition de la fixture".into(),
                validation: None,
            },
            before: vec![
                KernelToUi::Presence(PresenceState::Acting),
                KernelToUi::TokenDelta {
                    text: "texte provisoire".into(),
                },
            ],
            after: vec![
                KernelToUi::Presence(PresenceState::DecisionRequired),
                card(id),
            ],
        }
    }
}

async fn step(stream: &mut ServerStream, expected_id: u64, step: Step) {
    assert_eq!(
        read(stream).await,
        Frame::request(expected_id, step.request)
    );
    for message in step.before {
        write_frame(stream, &Frame::reply(0, message))
            .await
            .unwrap();
    }
    write_frame(stream, &Frame::reply(expected_id, step.reply))
        .await
        .unwrap();
    for message in step.after {
        write_frame(stream, &Frame::reply(0, message))
            .await
            .unwrap();
    }
    finish_barrier(stream).await;
}

#[tokio::test]
async fn facade_status_memory_journal_agents_chat_and_card_decisions_cross_real_transports() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let bridge = mobile(&rig);
    let script = vec![
        Step::new(UiToKernel::StatusRequest, status()),
        Step::new(
            UiToKernel::MemoryRemember {
                text: "fait de fixture".into(),
            },
            KernelToUi::MemoryRememberAck {
                id: "fixture-memory".into(),
            },
        ),
        Step::new(
            UiToKernel::MemoryRequest {
                query: None,
                limit: 10,
            },
            KernelToUi::MemoryReply {
                rows: vec![MemoryRowDto {
                    id: "fixture-memory".into(),
                    text: "fait de fixture".into(),
                    tags: vec!["test".into()],
                    source: "User".into(),
                }],
            },
        ),
        Step::new(
            UiToKernel::JournalRequest { limit: 10 },
            KernelToUi::JournalReply {
                entries: vec![JournalEntryDto {
                    id: "fixture-journal".into(),
                    timestamp: "2026-11-04T00:00:00Z".into(),
                    action: "Fixture".into(),
                    summary: "transport contrôlé".into(),
                }],
            },
        ),
        Step::new(
            UiToKernel::AgentListRequest,
            KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "fixture-agent".into(),
                    name: "fixture-agent".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "a".repeat(64),
                }],
            },
        ),
        Step::new(
            UiToKernel::AgentRunRequest {
                name: "fixture-agent".into(),
                input_json: "{\"test\":true}".into(),
            },
            KernelToUi::AgentRunReply {
                name: "fixture-agent".into(),
                output_json: "{\"fixture_only\":true}".into(),
                duration_ms: 0,
                input_sha256: "a".repeat(64),
                output_sha256: "b".repeat(64),
            },
        ),
        Step::chat("proposition un", 42),
        Step::new(
            UiToKernel::Reject { card_id: 42 },
            KernelToUi::CardDecisionAck {
                card_id: 42,
                approved: false,
                audit_id: "fixture-rejet".into(),
                effect: None,
            },
        ),
        Step::chat("proposition deux", 43),
        Step::new(
            UiToKernel::Approve { card_id: 43 },
            KernelToUi::CardDecisionAck {
                card_id: 43,
                approved: true,
                audit_id: "fixture-accord".into(),
                effect: Some("outbox_enqueued".into()),
            },
        ),
    ];
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        for (index, expected) in script.into_iter().enumerate() {
            step(&mut stream, index as u64 + 1, expected).await;
        }
    });
    for request in [
        json!({"type":"refresh_status"}),
        json!({"type":"remember","text":"fait de fixture"}),
        json!({"type":"load_memory","query":null,"limit":10}),
        json!({"type":"load_journal","limit":10}),
        json!({"type":"load_agents"}),
        json!({"type":"run_agent","name":"fixture-agent","input_json":"{\"test\":true}"}),
    ] {
        let (http, result) = roundtrip(&rig, &bridge, request).await;
        assert_eq!(http, 200);
        assert_eq!(result["ok"], true, "{result}");
        assert!(result["snapshot"]["pending"].is_null());
        assert_eq!(
            result["snapshot"]["outcomes"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["state"],
            "confirmed"
        );
    }
    let snapshot: Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snapshot["status"]["model"], "fixture IPC");
    assert_eq!(snapshot["memory"][0]["id"], "fixture-memory");
    assert_eq!(snapshot["journal"][0]["id"], "fixture-journal");
    assert_eq!(snapshot["agents"][0]["name"], "fixture-agent");
    for (query, card_id) in [("proposition un", 42), ("proposition deux", 43)] {
        assert_eq!(
            action(&bridge, json!({"type":"set_draft","text":query}))["ok"],
            true
        );
        let (_, result) = roundtrip(&rig, &bridge, json!({"type":"submit"})).await;
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(
            result["snapshot"]["chat"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["text"],
            "Proposition de la fixture"
        );
        assert_eq!(result["snapshot"]["cards"][0]["id"], card_id);
        assert_eq!(result["snapshot"]["presence"], "DecisionRequired");
        let decision = if card_id == 42 {
            "reject"
        } else {
            assert_eq!(
                action(&bridge, json!({"type":"begin_approval","card_id":card_id}))["ok"],
                true
            );
            let premature = action(&bridge, json!({"type":"approve","card_id":card_id}));
            assert_eq!(premature["ok"], false);
            assert!(premature["request"].is_null());
            assert_eq!(
                action(&bridge, json!({"type":"begin_approval","card_id":card_id}))["ok"],
                true
            );
            tokio::time::sleep(Duration::from_millis(620)).await;
            "approve"
        };
        let (http, result) =
            roundtrip(&rig, &bridge, json!({"type":decision,"card_id":card_id})).await;
        assert_eq!(http, 200);
        assert_eq!(result["ok"], true, "{result}");
        assert!(result["snapshot"]["cards"].as_array().unwrap().is_empty());
        assert_eq!(
            result["snapshot"]["outcomes"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["state"],
            "confirmed"
        );
    }
    fake.await.unwrap();
}

#[tokio::test]
async fn facade_stale_card_after_restart_is_rejected_then_new_token_and_session_are_used() {
    const ROTATED: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let bridge = mobile(&rig);
    let fake = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        step(
            &mut first,
            1,
            Step::new(UiToKernel::StatusRequest, status()),
        )
        .await;
        step(&mut first, 2, Step::chat("proposition", 42)).await;
        assert!(read_frame(&mut first).await.unwrap().is_none());
        drop(first);
        let mut second = listener.accept().await.unwrap();
        server_handshake(&mut second, ROTATED).await.unwrap();
        // La décision #3 refusée par la passerelle n'arrive jamais au noyau.
        step(
            &mut second,
            4,
            Step::new(UiToKernel::StatusRequest, status()),
        )
        .await;
    });
    let (_, first) = roundtrip(&rig, &bridge, json!({"type":"refresh_status"})).await;
    let initial_session = first["snapshot"]["gateway_session"].clone();
    action(&bridge, json!({"type":"set_draft","text":"proposition"}));
    let (_, shown) = roundtrip(&rig, &bridge, json!({"type":"submit"})).await;
    assert_eq!(shown["snapshot"]["cards"][0]["id"], 42);
    rig.gateway.close_session().await;
    write_token_file(&rig.directory.join("ipc.token"), ROTATED).unwrap();
    let (http, refused) = roundtrip(&rig, &bridge, json!({"type":"reject","card_id":42})).await;
    assert_eq!(http, 409);
    assert_eq!(refused["ok"], false);
    assert!(refused["snapshot"]["cards"].as_array().unwrap().is_empty());
    assert!(refused["snapshot"]["gateway_session"].is_null());
    assert_eq!(
        refused["snapshot"]["outcomes"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["state"],
        "failed"
    );
    let (http, recovered) = roundtrip(&rig, &bridge, json!({"type":"refresh_status"})).await;
    assert_eq!(http, 200);
    assert_eq!(recovered["ok"], true, "{recovered}");
    assert_ne!(recovered["snapshot"]["gateway_session"], initial_session);
    assert!(
        recovered["snapshot"]["cards"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn facade_busy_and_timeout_distinguish_unsent_from_unknown_and_never_replay_mutation() {
    let (rig, listener) = setup(Duration::from_millis(500), Duration::from_secs(60)).await;
    let first_bridge = mobile(&rig);
    let other_bridge = mobile(&rig);
    let (started, signal) = oneshot::channel();
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        assert_eq!(
            read(&mut stream).await,
            Frame::request(
                1,
                UiToKernel::MemoryRemember {
                    text: "effet potentiellement engagé".into()
                }
            )
        );
        assert_eq!(read(&mut stream).await.id, BARRIER_ID);
        started.send(()).unwrap();
        assert!(read_frame(&mut stream).await.unwrap().is_none());
        let mut recovered = accept(&listener).await;
        // Ni la mutation #1 ni le statut busy de l'autre client ne sont rejoués.
        step(
            &mut recovered,
            2,
            Step::new(UiToKernel::StatusRequest, status()),
        )
        .await;
    });
    let (request, _) = prepare(
        &first_bridge,
        json!({"type":"remember","text":"effet potentiellement engagé"}),
    );
    let http = rig.post(&request);
    let waiting = tokio::spawn(async move { http.send().await.unwrap() });
    signal.await.unwrap();
    let (status, busy) = roundtrip(&rig, &other_bridge, json!({"type":"refresh_status"})).await;
    assert_eq!(status, 429);
    assert_eq!(busy["ok"], false);
    assert_eq!(
        busy["snapshot"]["outcomes"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["state"],
        "failed"
    );
    let response = waiting.await.unwrap();
    assert_eq!(response.status(), 504);
    let unknown = receive(&first_bridge, request.id, response.text().await.unwrap());
    assert_eq!(unknown["ok"], false);
    assert_eq!(
        unknown["snapshot"]["outcomes"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["state"],
        "unknown"
    );
    assert!(unknown["snapshot"]["pending"].is_null());
    assert!(unknown["snapshot"]["gateway_session"].is_null());
    let no_retry = action(
        &first_bridge,
        json!({"type":"remember","text":"effet potentiellement engagé"}),
    );
    assert_eq!(no_retry["ok"], false);
    assert!(no_retry["request"].is_null());
    let (http, recovered) = roundtrip(&rig, &first_bridge, json!({"type":"refresh_status"})).await;
    assert_eq!(http, 200);
    assert_eq!(recovered["ok"], true, "{recovered}");
    assert_eq!(recovered["snapshot"]["outcomes"][0]["state"], "unknown");
    fake.await.unwrap();
}

#[tokio::test]
async fn actual_http_disconnect_expires_ipc_and_facade_keeps_unknown_without_retry() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let bridge = mobile(&rig);
    let (started, signal) = oneshot::channel();
    let fake = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        assert_eq!(read(&mut first).await.id, 1);
        assert_eq!(read(&mut first).await.id, BARRIER_ID);
        started.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), read_frame(&mut first))
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
        let mut second = accept(&listener).await;
        step(
            &mut second,
            2,
            Step::new(UiToKernel::StatusRequest, status()),
        )
        .await;
    });
    let (request, _) = prepare(
        &bridge,
        json!({"type":"remember","text":"requête abandonnée"}),
    );
    let bytes = serde_json::to_vec(&request).unwrap();
    let headers = format!(
        "POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {MOBILE_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        rig.address,
        bytes.len()
    );
    let mut socket = tokio::net::TcpStream::connect(rig.address).await.unwrap();
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(&bytes).await.unwrap();
    signal.await.unwrap();
    drop(socket);
    let failed: Value =
        serde_json::from_str(&bridge.fail_request(request.id, "connexion HTTP abandonnée".into()))
            .unwrap();
    assert_eq!(failed["snapshot"]["outcomes"][0]["state"], "unknown");
    // Laisser le serveur observer EOF ; la borne de deux secondes ci-dessus
    // distingue l'abandon réseau du timeout IPC de trois secondes.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (http, recovered) = roundtrip(&rig, &bridge, json!({"type":"refresh_status"})).await;
    assert_eq!(http, 200);
    assert_eq!(recovered["ok"], true, "{recovered}");
    fake.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn stale_unix_socket_is_reported_unavailable_without_deleting_it() {
    use std::os::unix::fs::FileTypeExt;
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    drop(listener);
    let bridge = mobile(&rig);
    assert!(
        std::fs::symlink_metadata(&rig.endpoint)
            .unwrap()
            .file_type()
            .is_socket()
    );
    let (http, unavailable) = roundtrip(&rig, &bridge, json!({"type":"refresh_status"})).await;
    assert_eq!(http, 503);
    assert_eq!(unavailable["ok"], false);
    assert_eq!(unavailable["snapshot"]["outcomes"][0]["state"], "failed");
    assert!(
        std::fs::symlink_metadata(&rig.endpoint)
            .unwrap()
            .file_type()
            .is_socket()
    );
}
