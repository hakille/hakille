use hakille_ipc::Frame;
use hakille_mobile::{
    MobileBridge, compute_camera_scale, is_allowed_external_destination, parse_pairing_qr,
    types::{
        GUEST_RING_PX, MAX_CAMERA_LONG_EDGE_PX, MAX_CAMERA_PHOTO_BYTES, MAX_CAMERA_TOTAL_PIXELS,
    },
    validate_camera_frame, validate_gps_fix,
};
use hakille_ui_protocol::{
    AgentDto, KernelToUi, MemoryRowDto, PrivacyGuardEvent, PrivacyGuardTrigger, StatusDto,
};
use serde_json::{Value, json};

fn setup_connected_bridge() -> MobileBridge {
    let bridge = MobileBridge::new();
    let set_hub: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "set_hub",
                "hub_id": "https://hub.example.com"
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(set_hub["ok"], true);

    let req: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string()),
    )
    .unwrap();
    let req_id = req["request"]["id"].as_u64().unwrap();
    let status_reply = Frame::reply(
        req_id,
        KernelToUi::StatusReply {
            status: StatusDto {
                model: "Qwen 2.5 0.5B".into(),
                cost: "0.00 EUR".into(),
                state: "Idle".into(),
                memory_count: 2,
            },
        },
    );
    let recv: Value = serde_json::from_str(
        &bridge.receive_json(
            req_id,
            json!({
                "session_id": "session-m4",
                "frames": [status_reply]
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(recv["ok"], true);
    bridge
}

#[test]
fn test_privacy_guard_shoulder_surfing_veil_threshold_and_dismiss() {
    let bridge = setup_connected_bridge();

    // 1. people_count == 1 (user alone): veil must NOT activate
    let req1: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string()),
    )
    .unwrap();
    let req_id1 = req1["request"]["id"].as_u64().unwrap();

    let single_person_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 1 },
            description: "Utilisateur seul face à l'écran".into(),
            recommended_action: "Aucune action".into(),
            timestamp_ms: 1731000001000,
        }),
    );
    let step1: Value = serde_json::from_str(&bridge.receive_stream_frame(
        req_id1,
        serde_json::to_string(&single_person_frame).unwrap(),
    ))
    .unwrap();
    assert_eq!(step1["ok"], true);
    assert_eq!(step1["snapshot"]["privacy_veil_active"], false);
    assert!(step1["snapshot"]["privacy_guard"].is_null());

    // 2. people_count >= 2 (bystander detected): veil MUST activate immediately
    let multi_people_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 2 },
            description: "2 personnes détectées derrière l'écran".into(),
            recommended_action: "Floutage écran activé".into(),
            timestamp_ms: 1731000002000,
        }),
    );
    let step2: Value = serde_json::from_str(
        &bridge.receive_stream_frame(req_id1, serde_json::to_string(&multi_people_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(step2["ok"], true);
    assert_eq!(step2["snapshot"]["privacy_veil_active"], true);
    assert_eq!(
        step2["snapshot"]["privacy_guard"]["trigger"]["ShoulderSurfingDetected"]["people_count"],
        2
    );
    assert_eq!(
        step2["snapshot"]["privacy_guard"]["recommended_action"],
        "Floutage écran activé"
    );

    // Complete request 1
    let status_frame = Frame::reply(
        req_id1,
        KernelToUi::StatusReply {
            status: StatusDto {
                model: "Qwen".into(),
                cost: "0".into(),
                state: "Idle".into(),
                memory_count: 0,
            },
        },
    );
    let _ = bridge.receive_stream_frame(req_id1, serde_json::to_string(&status_frame).unwrap());

    // 3. Explicit user dismissal clears both veil and privacy_guard event
    let dismiss: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "dismiss_privacy_guard" }).to_string()),
    )
    .unwrap();
    assert_eq!(dismiss["ok"], true);
    assert_eq!(dismiss["snapshot"]["privacy_veil_active"], false);
    assert!(dismiss["snapshot"]["privacy_guard"].is_null());
}

#[test]
fn test_privacy_guard_exfiltration_kill_switch_and_ttl_events() {
    let bridge = setup_connected_bridge();
    let req: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "refresh_status" }).to_string()),
    )
    .unwrap();
    let req_id = req["request"]["id"].as_u64().unwrap();

    // 1. RawDataExfiltrationBlocked
    let exfil_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::RawDataExfiltrationBlocked {
                blocked_scope: "biometrics.face_mesh".into(),
            },
            description: "Extraction maillage facial bloquée".into(),
            recommended_action: "Bac à sable hermétique".into(),
            timestamp_ms: 1731000010000,
        }),
    );
    let r1: Value = serde_json::from_str(
        &bridge.receive_stream_frame(req_id, serde_json::to_string(&exfil_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(r1["ok"], true);
    assert_eq!(
        r1["snapshot"]["privacy_guard"]["trigger"]["RawDataExfiltrationBlocked"]["blocked_scope"],
        "biometrics.face_mesh"
    );

    // 2. HardwareKillSwitchActivated
    let kill_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::HardwareKillSwitchActivated {
                device: "camera_shutter".into(),
            },
            description: "Obturateur physique fermé".into(),
            recommended_action: "Mode audio seul".into(),
            timestamp_ms: 1731000020000,
        }),
    );
    let r2: Value = serde_json::from_str(
        &bridge.receive_stream_frame(req_id, serde_json::to_string(&kill_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(r2["ok"], true);
    assert_eq!(
        r2["snapshot"]["privacy_guard"]["trigger"]["HardwareKillSwitchActivated"]["device"],
        "camera_shutter"
    );

    // 3. EphemeralTtlPurged
    let ttl_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::EphemeralTtlPurged,
            description: "Tampons biométriques RAM purgés".into(),
            recommended_action: "Aucune action requise".into(),
            timestamp_ms: 1731000030000,
        }),
    );
    let r3: Value = serde_json::from_str(
        &bridge.receive_stream_frame(req_id, serde_json::to_string(&ttl_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(r3["ok"], true);
    assert_eq!(
        r3["snapshot"]["privacy_guard"]["trigger"],
        "EphemeralTtlPurged"
    );

    // 4. Adversarial oversized blocked_scope (> 256 bytes) must fail closed
    let bad_frame = Frame::reply(
        0,
        KernelToUi::PrivacyGuardAlert(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::RawDataExfiltrationBlocked {
                blocked_scope: "s".repeat(257),
            },
            description: "Scope excessif".into(),
            recommended_action: "Rejet".into(),
            timestamp_ms: 1731000040000,
        }),
    );
    let r_bad: Value = serde_json::from_str(
        &bridge.receive_stream_frame(req_id, serde_json::to_string(&bad_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(r_bad["ok"], false);
}

#[test]
fn test_guest_pass_lifecycle_ring_fraction_and_1px_invariant() {
    let bridge = setup_connected_bridge();
    assert_eq!(GUEST_RING_PX, 1);

    // 1. Start guest pass with default duration (3600s)
    let start: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "start_guest_pass",
                "name": "Amina",
                "power": "salon.lumieres"
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(start["ok"], true);
    let gp = &start["snapshot"]["guest_pass"];
    assert_eq!(gp["name"], "Amina");
    assert_eq!(gp["power"], "salon.lumieres");
    assert_eq!(gp["duration_secs"], 3600);
    assert_eq!(gp["elapsed_secs"], 0);
    assert_eq!(gp["ring_px"], 1);
    assert!((gp["ring_fraction"].as_f64().unwrap() - 1.0).abs() < 1e-5);
    assert_eq!(gp["expired"], false);

    // 2. Tick halfway (1800s) -> ring_fraction == 0.5
    let tick_half: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "tick_guest_pass",
                "elapsed_secs": 1800
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(tick_half["ok"], true);
    assert!(
        (tick_half["snapshot"]["guest_pass"]["ring_fraction"]
            .as_f64()
            .unwrap()
            - 0.5)
            .abs()
            < 1e-5
    );
    assert_eq!(tick_half["snapshot"]["guest_pass"]["expired"], false);

    // 3. Tick to expiration (3600s) -> expired == true, ring_fraction == 0.0
    let tick_end: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "tick_guest_pass",
                "elapsed_secs": 3600
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(tick_end["ok"], true);
    assert_eq!(tick_end["snapshot"]["guest_pass"]["expired"], true);
    assert_eq!(
        tick_end["snapshot"]["guest_pass"]["ring_fraction"]
            .as_f64()
            .unwrap(),
        0.0
    );
}

#[test]
fn test_guest_pass_clamps_autonomy_to_crank_zero_and_restores_on_expiry() {
    let bridge = setup_connected_bridge();

    // Promote owner's Mail domain to Crank 3 and Agenda to Crank 2
    bridge.dispatch_json(
        json!({
            "type": "set_autonomy_crank",
            "domain": "Mail",
            "crank": 3,
            "max_cost_cents": 2000
        })
        .to_string(),
    );
    bridge.dispatch_json(
        json!({
            "type": "set_autonomy_crank",
            "domain": "Agenda",
            "crank": 2
        })
        .to_string(),
    );

    // Start guest pass (600s) -> all domains must clamp to Crank 0 (Observer)
    let start: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "start_guest_pass",
                "name": "Invité",
                "power": "musique",
                "duration_secs": 600
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(start["ok"], true);
    for domain in start["snapshot"]["domain_autonomies"].as_array().unwrap() {
        assert_eq!(
            domain["crank"], 0,
            "Domain {} must be clamped to Crank 0 during guest pass",
            domain["domain"]
        );
        assert_eq!(domain["crank_label"], "Observer");
    }

    // Expire guest pass -> owner cranks (Mail=3, Agenda=2, others=1) must be restored
    let expired: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "tick_guest_pass",
                "elapsed_secs": 600
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(expired["ok"], true);
    let domains = expired["snapshot"]["domain_autonomies"].as_array().unwrap();
    let mail = domains.iter().find(|d| d["domain"] == "Mail").unwrap();
    let agenda = domains.iter().find(|d| d["domain"] == "Agenda").unwrap();
    assert_eq!(mail["crank"], 3);
    assert_eq!(agenda["crank"], 2);

    // Start another guest pass and revoke manually -> owner cranks restored and guest_pass cleared
    bridge.dispatch_json(
        json!({
            "type": "start_guest_pass",
            "name": "Coloc",
            "power": "wifi",
            "duration_secs": 1800
        })
        .to_string(),
    );
    let revoked: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "revoke_guest_pass" }).to_string()),
    )
    .unwrap();
    assert_eq!(revoked["ok"], true);
    assert!(revoked["snapshot"]["guest_pass"].is_null());
    let domains_after = revoked["snapshot"]["domain_autonomies"].as_array().unwrap();
    let mail_after = domains_after
        .iter()
        .find(|d| d["domain"] == "Mail")
        .unwrap();
    assert_eq!(mail_after["crank"], 3);
}

#[test]
fn test_guest_pass_blocks_high_stakes_mutations_and_restricts_agents_to_zero_caps() {
    let bridge = setup_connected_bridge();

    // Seed agent catalog with 1 zero-cap executable agent and 1 host-cap executable agent
    let load_req: Value =
        serde_json::from_str(&bridge.dispatch_json(json!({ "type": "load_agents" }).to_string()))
            .unwrap();
    let load_id = load_req["request"]["id"].as_u64().unwrap();
    let agents_frame = Frame::reply(
        load_id,
        KernelToUi::AgentListReply {
            agents: vec![
                AgentDto {
                    id: "zero-cap-calc".into(),
                    name: "zero-cap-calc".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "a".repeat(64),
                },
                AgentDto {
                    id: "wallet-agent".into(),
                    name: "wallet-agent".into(),
                    required_caps: vec!["wallet.read".into()],
                    artifact_present: true,
                    sha256: "b".repeat(64),
                },
            ],
        },
    );
    let _ = bridge.receive_stream_frame(load_id, serde_json::to_string(&agents_frame).unwrap());

    // Activate guest pass
    bridge.dispatch_json(
        json!({
            "type": "start_guest_pass",
            "name": "Invité",
            "power": "calc.basic",
            "duration_secs": 1800
        })
        .to_string(),
    );

    // 1. High-stakes mutations must fail closed
    for action in [
        json!({ "type": "forget_memory", "row_id": "fact-1" }),
        json!({ "type": "set_autonomy_crank", "domain": "Mail", "crank": 4 }),
        json!({ "type": "undo", "journal_id": 1 }),
    ] {
        let res: Value = serde_json::from_str(&bridge.dispatch_json(action.to_string())).unwrap();
        assert_eq!(res["ok"], false);
        assert!(
            res["error"]
                .as_str()
                .unwrap()
                .contains("Action interdite en mode invité éphémère")
        );
    }

    // 2. Executable agent requiring host capabilities must be rejected in guest mode
    let cap_agent_res: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "run_agent",
                "name": "wallet-agent",
                "input_json": "{}"
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(cap_agent_res["ok"], false);
    assert!(
        cap_agent_res["error"]
            .as_str()
            .unwrap()
            .contains("sans capacité hôte")
    );

    // 3. Zero-capability sandboxed WASM agent is allowed
    let zero_cap_res: Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "run_agent",
                "name": "zero-cap-calc",
                "input_json": "{\"x\":2}"
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(zero_cap_res["ok"], true);
}

#[test]
fn test_checkpoint_v2_excludes_ephemeral_privacy_and_guest_state() {
    let bridge = setup_connected_bridge();
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Brouillon persistant sécurisé"
        })
        .to_string(),
    );
    bridge.dispatch_json(
        json!({
            "type": "start_guest_pass",
            "name": "Invité",
            "power": "wifi",
            "duration_secs": 3600
        })
        .to_string(),
    );

    let draft_raw = bridge.draft_json();
    let saved: Value = serde_json::from_str(&draft_raw).unwrap();
    let obj = saved.as_object().unwrap();

    // Strictly 5 keys in Checkpoint v2
    assert_eq!(obj.len(), 5);
    assert_eq!(saved["schema_version"], 2);
    assert_eq!(saved["draft"], "Brouillon persistant sécurisé");
    assert!(obj.contains_key("next_request_id"));
    assert!(obj.contains_key("uncertain_operations"));
    assert!(obj.contains_key("acknowledged_operations"));
    assert!(!draft_raw.contains("guest_pass"));
    assert!(!draft_raw.contains("privacy_guard"));
}

#[test]
fn test_device_capability_policies_and_qr_pairing_validation() {
    // 1. Camera scaling & bounds (1536px long edge, 8 MiB, 4 MP)
    assert_eq!(MAX_CAMERA_LONG_EDGE_PX, 1536);
    assert_eq!(MAX_CAMERA_PHOTO_BYTES, 8 * 1024 * 1024);
    assert_eq!(MAX_CAMERA_TOTAL_PIXELS, 4_000_000);
    assert!((compute_camera_scale(2000, 1500) - 0.768).abs() < 1e-4);
    assert_eq!(
        validate_camera_frame(2000, 1500, 2_000_000),
        Ok((1536, 1152))
    );
    assert!(validate_camera_frame(0, 100, 500).is_err());
    assert!(validate_camera_frame(3000, 2000, 500_000).is_err());
    assert!(validate_camera_frame(1024, 768, MAX_CAMERA_PHOTO_BYTES + 1).is_err());

    // 2. Single-shot GPS validation (bounds, age window [-2, 15]s, mock rejection)
    assert!(validate_gps_fix(48.8566, 2.3522, 12.0, 2.5, false).is_ok());
    assert!(validate_gps_fix(91.0, 2.3522, 12.0, 1.0, false).is_err());
    assert!(validate_gps_fix(48.8566, 2.3522, 50_000_000.0, 1.0, false).is_err());
    assert!(validate_gps_fix(48.8566, 2.3522, 12.0, 20.0, false).is_err());
    assert_eq!(
        validate_gps_fix(48.8566, 2.3522, 5.0, 1.0, true),
        Err("Position simulée (mock provider) refusée")
    );

    // 3. Allowlisted external launcher policy
    assert!(is_allowed_external_destination("maps://"));
    assert!(is_allowed_external_destination("app-settings:"));
    assert!(!is_allowed_external_destination("https://evil.example.com"));
    assert!(!is_allowed_external_destination("javascript:alert(1)"));

    // 4. QR Pairing payload validation and secret redaction
    let valid_token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let qr = json!({
        "endpoint": "https://Hub.Local:8443/",
        "token": valid_token
    })
    .to_string();
    let parsed = parse_pairing_qr(&qr).expect("valid pairing QR");
    assert_eq!(parsed.endpoint, "https://hub.local:8443");
    assert_eq!(parsed.token, valid_token);
    let debug_str = format!("{parsed:?}");
    assert!(
        !debug_str.contains(valid_token),
        "Debug output must redact pairing token"
    );
    assert!(debug_str.contains("[redacted]"));

    // Reject HTTP on non-loopback or non-64-hex token
    assert!(
        parse_pairing_qr(
            &json!({"endpoint": "http://192.168.1.10:8080", "token": valid_token}).to_string()
        )
        .is_err()
    );
    assert!(
        parse_pairing_qr(
            &json!({"endpoint": "https://hub.local", "token": "short-token"}).to_string()
        )
        .is_err()
    );
}

#[test]
fn test_rollback_on_rejected_mutation_and_tombstone_filtering_on_memory_reply() {
    let bridge = setup_connected_bridge();

    // 1. Seed memory with 2 rows
    let load_req: Value =
        serde_json::from_str(&bridge.dispatch_json(
            json!({ "type": "load_memory", "query": null, "limit": 10 }).to_string(),
        ))
        .unwrap();
    let load_id = load_req["request"]["id"].as_u64().unwrap();
    let mem_frame = Frame::reply(
        load_id,
        KernelToUi::MemoryReply {
            rows: vec![
                MemoryRowDto {
                    id: "row-1".into(),
                    text: "Fait numéro 1".into(),
                    tags: vec!["user".into()],
                    source: "User".into(),
                },
                MemoryRowDto {
                    id: "row-2".into(),
                    text: "Fait numéro 2".into(),
                    tags: vec!["user".into()],
                    source: "User".into(),
                },
            ],
        },
    );
    let _ = bridge.receive_stream_frame(load_id, serde_json::to_string(&mem_frame).unwrap());

    // 2. Forget row-1 while connected -> succeeds and records tombstone
    let forget_ok: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "forget_memory", "row_id": "row-1" }).to_string()),
    )
    .unwrap();
    assert_eq!(forget_ok["ok"], true);
    let forget_req_id = forget_ok["request"]["id"].as_u64().unwrap();
    let ack_frame = Frame::reply(
        forget_req_id,
        KernelToUi::JournalAppended { id: forget_req_id },
    );
    let _ = bridge.receive_stream_frame(forget_req_id, serde_json::to_string(&ack_frame).unwrap());

    // 3. Subsequent MemoryReply from hub still containing row-1 must filter it out via tombstones
    let reload_req: Value =
        serde_json::from_str(&bridge.dispatch_json(
            json!({ "type": "load_memory", "query": null, "limit": 10 }).to_string(),
        ))
        .unwrap();
    let reload_id = reload_req["request"]["id"].as_u64().unwrap();
    let stale_mem_frame = Frame::reply(
        reload_id,
        KernelToUi::MemoryReply {
            rows: vec![
                MemoryRowDto {
                    id: "row-1".into(),
                    text: "Fait numéro 1".into(),
                    tags: vec!["user".into()],
                    source: "User".into(),
                },
                MemoryRowDto {
                    id: "row-2".into(),
                    text: "Fait numéro 2".into(),
                    tags: vec!["user".into()],
                    source: "User".into(),
                },
            ],
        },
    );
    let reload_res: Value = serde_json::from_str(
        &bridge.receive_stream_frame(reload_id, serde_json::to_string(&stale_mem_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(reload_res["ok"], true);
    let rows_after = reload_res["snapshot"]["memory"].as_array().unwrap();
    assert_eq!(rows_after.len(), 1);
    assert_eq!(rows_after[0]["id"], "row-2");

    // 4. Put bridge into ConnectionState::Unknown / recovery_required by failing a mutation
    bridge
        .dispatch_json(json!({ "type": "set_draft", "text": "mutation interrompue" }).to_string());
    let sub_req: Value =
        serde_json::from_str(&bridge.dispatch_json(json!({ "type": "submit" }).to_string()))
            .unwrap();
    let sub_id = sub_req["request"]["id"].as_u64().unwrap();
    bridge.fail_request(sub_id, "Coupure réseau".into());

    // Attempting to forget row-2 while recovery_required is true MUST fail AND roll back any local tombstone/memory change
    let rejected_forget: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({ "type": "forget_memory", "row_id": "row-2" }).to_string()),
    )
    .unwrap();
    assert_eq!(rejected_forget["ok"], false);
    assert_eq!(
        rejected_forget["snapshot"]["memory"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "Rejected forget_memory must not delete row-2 locally"
    );
    assert!(
        !rejected_forget["snapshot"]["tombstones"]
            .as_array()
            .unwrap()
            .contains(&json!("row-2")),
        "Rejected forget_memory must not add row-2 to tombstones"
    );
}
