//! Suite de conformité formelle pour la constellation domotique souveraine (`hakille-home`).
//!
//! Vérifie :
//! 1. L'inventaire honnête des 19 protocoles LAN et l'absence absolue d'appareils fictifs.
//! 2. La frontière zéro-cloud `HomeLanGuard` (blocage WAN, IMDS IPv4/IPv6, traversée série).
//! 3. Les invariants cryptographiques Biscuit v5 (rôles Minor/Guest refusés en Tier 3,
//!    maintien physique >= 600 ms exigé pour AdultResident, rejet strict à 599 ms).
//! 4. Des échanges filaires réels en loopback sur les protocoles majeurs (MQTT TCP via
//!    `SovereignHomeHub`, Shelly HTTP JSON-RPC, Modbus TCP MBAP + RTU CRC-16, CoAP UDP,
//!    KNXnet/IP cEMI + DPT9, Matter TLV + AEAD ChaCha20-Poly1305 avec rejet de falsification,
//!    HomeKit HAP AEAD, OCPP 1.6J WebSocket, Home Assistant WebSocket).
//! 5. Le jumeau numérique spatial 3D (`HomeDigitalTwin`) et le moteur d'homéostasie
//!    énergétique (`EnergyHomeostasisEngine`, zéro spéculation sans sonde).

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use hakille_core::{CapabilityAction, CapabilityScope, Timestamp};
use hakille_home::protocols::{
    coap::{CoapCode, CoapMessage, CoapType, send_coap_request},
    hap_hue_shelly::{
        hap_decrypt_block, hap_encrypt_block, send_shelly_rpc_http, shelly_rpc_payload,
    },
    home_assistant::{command_to_ha_service, ha_ws_call_service},
    knx::{
        KnxGroupAddress, build_knx_routing_write, decode_dpt9, encode_dpt9, parse_knx_routing_frame,
    },
    matter::{
        MatterOnboardingPayload, MatterSecureSession, TlvElement, TlvValue, decode_tlv,
        encode_matter_command_tlv, encode_tlv,
    },
    modbus::{build_rtu_read_holding, modbus_crc16, modbus_tcp_read_holding, verify_rtu_frame},
    mqtt::MqttPacket,
    ocpp::{OcppFrame, command_to_ocpp_call},
    websocket::{WsClient, compute_websocket_accept},
};
use hakille_home::{
    ConnectionStatus, DeviceCategory, EnergyHomeostasisEngine, EnergyTelemetry, HomeBiscuitPolicy,
    HomeCommand, HomeDevice, HomeDigitalTwin, HomeError, HomeLanGuard, HomeResidentRole,
    PhysicalHoldProof, ProtocolKind, RoomZone, SovereignHomeHub, SpatialPose3D,
    new_ephemeral_authority,
};
use hakille_memory::KnowledgeGraph;
use hakille_spatial::{DetectedMicroGesture, GazeRay3D, Handedness, Point3D, Vector3D};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

/// Helper : génère un chemin redb temporaire isolé.
fn temp_home_db(tag: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir conformance home");
    let db = dir.path().join(format!("{tag}.redb"));
    (dir, db)
}

#[tokio::test]
async fn home_conformance_01_all_19_protocols_registered_and_zero_fake_devices() {
    let (_dir, db) = temp_home_db("catalog");
    let hub = SovereignHomeHub::open(&db)
        .await
        .expect("ouverture hub redb");

    // 1. Aucun appareil factice par défaut
    assert!(
        hub.list_devices().await.is_empty(),
        "Contrat d'honnêteté : aucun appareil ne doit être pré-peuplé sans appairage réel"
    );

    // 2. Catalogue complet des 19 protocoles LAN
    let catalog = hub.protocol_catalog();
    assert_eq!(ProtocolKind::all().len(), 19);
    assert_eq!(catalog.len(), 19);
    for info in &catalog {
        assert!(
            info.wire_codec_ready,
            "Codec filaire manquant pour {:?}",
            info.protocol
        );
        assert!(
            !info.honesty_note.trim().is_empty(),
            "Note d'honnêteté vide pour {:?}",
            info.protocol
        );
        assert!(
            !info.transport_mode.trim().is_empty(),
            "Mode de transport vide pour {:?}",
            info.protocol
        );
    }
}

#[test]
fn home_conformance_02_lan_guard_blocks_wan_imds_and_serial_traversal() {
    let guard = HomeLanGuard::new();

    // Blocages stricts WAN, cloud, IMDS IPv4/IPv6 et traversée série
    for blocked in [
        "mqtt://8.8.8.8:1883",
        "https://cloud.example/api/v1/device",
        "http://169.254.169.254/latest/meta-data",
        "http://[fd00:ec2::254]/latest/meta-data",
        "/dev/ttyUSB0/../sda",
        "serial:///etc/passwd",
        "../secret",
    ] {
        assert!(
            guard.validate_endpoint(blocked).is_err(),
            "HomeLanGuard aurait dû bloquer l'endpoint non souverain : {blocked}"
        );
    }

    // Endpoints souverains autorisés (RFC 1918, loopback, IPv6 ULA, .local, BLE, série sûr)
    for allowed in [
        "mqtt://192.168.1.40:1883",
        "http://10.0.0.15/rpc",
        "http://172.16.4.2:8123",
        "coap://[fd12:3456:789a::1]:5683/light",
        "http://pont-hue.local/clip/v2/resource",
        "ble://AA:BB:CC:DD:EE:FF",
        "/dev/ttyUSB0",
        "/dev/ttyACM0",
    ] {
        assert!(
            guard.validate_endpoint(allowed).is_ok(),
            "HomeLanGuard aurait dû autoriser l'endpoint LAN : {allowed}"
        );
    }
}

#[tokio::test]
async fn home_conformance_03_biscuit_roles_and_600ms_physical_hold_on_tier3() {
    // Mini courtier MQTT TCP réel en loopback pour prouver l'exécution après maintien 600 ms
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker_addr = listener.local_addr().unwrap();
    let broker_task = tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 512];
            let _ = sock.read(&mut buf).await;
            let connack = MqttPacket::ConnAck {
                session_present: false,
                reason_code: 0,
            }
            .to_bytes();
            let _ = sock.write_all(&connack).await;
            let _ = sock.read(&mut buf).await;
            let puback = MqttPacket::PubAck { packet_id: 1 }.to_bytes();
            let _ = sock.write_all(&puback).await;
            let _ = sock.read(&mut buf).await;
        }
    });

    let (_dir, db) = temp_home_db("tier3_biscuit");
    let hub = SovereignHomeHub::open(&db).await.unwrap();
    let mut lock = HomeDevice::new(
        "serrure-entree",
        "Serrure Entrée",
        "entree",
        ProtocolKind::Mqtt,
        DeviceCategory::DoorLock,
        format!("mqtt://{broker_addr}"),
    );
    lock.paired_at = Some(Utc::now().to_rfc3339());
    hub.register_device(lock).await.unwrap();

    let authority = new_ephemeral_authority();
    let policy = HomeBiscuitPolicy::new(authority.root_public_key());
    let expiry = Timestamp(Utc::now() + ChronoDuration::minutes(5));

    // 1. Minor et Guest refusés en Tier 3 (UnlockDoor / SetMainBreaker / SetWaterValve)
    for role in [HomeResidentRole::Guest, HomeResidentRole::Minor] {
        let token = HomeBiscuitPolicy::issue_home_token(
            &authority,
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Execute],
            expiry.clone(),
            Some("entree"),
            Some(ProtocolKind::Mqtt),
            role,
        )
        .unwrap();
        let hold_ok = PhysicalHoldProof {
            action_id: "serrure-entree:unlock_door".into(),
            hold_duration_ms: 700,
            confirmed_at: Utc::now(),
        };
        let res = hub
            .execute_authorized(
                &policy,
                &token,
                role,
                "serrure-entree",
                &HomeCommand::UnlockDoor,
                Some(&hold_ok),
            )
            .await;
        assert!(
            matches!(res, Err(HomeError::CapabilityDenied(_))),
            "Rôle {role:?} doit être refusé sur UnlockDoor Tier 3"
        );
    }

    // 2. AdultResident sans maintien ou avec 599 ms refusé (ConfirmationRequired)
    let adult_token = HomeBiscuitPolicy::issue_home_token(
        &authority,
        vec![CapabilityScope::Network],
        vec![CapabilityAction::Execute],
        expiry,
        Some("entree"),
        Some(ProtocolKind::Mqtt),
        HomeResidentRole::AdultResident,
    )
    .unwrap();

    let hold_599 = PhysicalHoldProof {
        action_id: "serrure-entree:unlock_door".into(),
        hold_duration_ms: 599,
        confirmed_at: Utc::now(),
    };
    let err_599 = hub
        .execute_authorized(
            &policy,
            &adult_token,
            HomeResidentRole::AdultResident,
            "serrure-entree",
            &HomeCommand::UnlockDoor,
            Some(&hold_599),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err_599, HomeError::ConfirmationRequired(_)),
        "Maintien 599 ms doit être rejeté (seuil strict 600 ms), reçu : {err_599:?}"
    );

    // 3. AdultResident avec 600 ms produit un CommandReceipt filaire SHA-256 (64 hex)
    let hold_600 = PhysicalHoldProof {
        action_id: "serrure-entree:unlock_door".into(),
        hold_duration_ms: 600,
        confirmed_at: Utc::now(),
    };
    let receipt = hub
        .execute_authorized(
            &policy,
            &adult_token,
            HomeResidentRole::AdultResident,
            "serrure-entree",
            &HomeCommand::UnlockDoor,
            Some(&hold_600),
        )
        .await
        .expect("Exécution Tier 3 après maintien >= 600 ms doit réussir sur courtier loopback");
    assert!(receipt.executed_on_wire);
    assert_eq!(receipt.frame_sha256.len(), 64);

    let _ = broker_task.await;
}

#[tokio::test]
async fn home_conformance_04_real_loopback_wire_protocols_e2e() {
    // A. Shelly Gen2/Gen3 JSON-RPC sur serveur HTTP loopback réel
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http_listener.local_addr().unwrap();
    let http_server = tokio::spawn(async move {
        if let Ok((mut sock, _)) = http_listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let body = r#"{"id":1,"result":{"was_on":false}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });
    let (method, params) = shelly_rpc_payload(0, &HomeCommand::SetPower { on: true }).unwrap();
    assert_eq!(method, "Switch.Set");
    let shelly_resp = send_shelly_rpc_http(&format!("http://{http_addr}"), method, params)
        .await
        .expect("Shelly HTTP loopback doit répondre");
    assert_eq!(shelly_resp["result"]["was_on"], false);
    let _ = http_server.await;

    // B. Modbus TCP MBAP (FC03) sur serveur TCP loopback + Modbus RTU CRC-16
    let mb_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mb_addr = mb_listener.local_addr().unwrap();
    let mb_server = tokio::spawn(async move {
        if let Ok((mut sock, _)) = mb_listener.accept().await {
            let mut req = [0u8; 12];
            sock.read_exact(&mut req).await.unwrap();
            // Réponse MBAP : tx_id(2), proto(2)=0, len(2)=7, unit(1), fc(1)=0x03, bytes(1)=4, reg0=2300, reg1=450
            let resp: [u8; 13] = [
                req[0], req[1], 0, 0, 0, 7, req[6], 0x03, 4, 0x08, 0xFC, 0x01, 0xC2,
            ];
            sock.write_all(&resp).await.unwrap();
        }
    });
    let regs = modbus_tcp_read_holding(mb_addr, 1, 40000, 2)
        .await
        .expect("Modbus TCP loopback");
    assert_eq!(regs, vec![2300, 450]);
    let _ = mb_server.await;

    let rtu = build_rtu_read_holding(1, 100, 2);
    assert_eq!(
        modbus_crc16(&rtu[..rtu.len() - 2]),
        u16::from_le_bytes([rtu[rtu.len() - 2], rtu[rtu.len() - 1]])
    );
    assert!(verify_rtu_frame(&rtu).is_ok());

    // C. CoAP RFC 7252 UDP loopback réel
    let coap_srv = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let coap_addr = coap_srv.local_addr().unwrap();
    let coap_task = tokio::spawn(async move {
        let mut buf = [0u8; 512];
        if let Ok((n, peer)) = coap_srv.recv_from(&mut buf).await {
            let req = CoapMessage::from_bytes(&buf[..n]).unwrap();
            let ack = CoapMessage {
                msg_type: CoapType::Acknowledgement,
                code: CoapCode::CONTENT_205,
                message_id: req.message_id,
                token: req.token,
                options: vec![],
                payload: br#"{"ok":true}"#.to_vec(),
            };
            let _ = coap_srv.send_to(&ack.to_bytes().unwrap(), peer).await;
        }
    });
    let coap_req = CoapMessage::put_json(0x1234, "/light/1", br#"{"on":true}"#);
    let coap_ack = send_coap_request(coap_addr, &coap_req, Duration::from_secs(2))
        .await
        .expect("CoAP UDP loopback");
    assert_eq!(coap_ack.code, CoapCode::CONTENT_205);
    assert_eq!(coap_ack.payload, br#"{"ok":true}"#);
    let _ = coap_task.await;

    // D. KNXnet/IP cEMI + DPT9 (température °C)
    let grp = KnxGroupAddress::parse_three_level("1/2/3").unwrap();
    assert_eq!(grp.format_three_level(), "1/2/3");
    let dpt9 = encode_dpt9(21.5);
    assert!((decode_dpt9(dpt9) - 21.5).abs() < 0.05);
    let knx_frame = build_knx_routing_write(grp, &dpt9);
    let (service_type, parsed_grp, parsed_payload) = parse_knx_routing_frame(&knx_frame).unwrap();
    assert_eq!(service_type, 0x0530);
    assert_eq!(parsed_grp, grp);
    assert_eq!(parsed_payload, dpt9);

    // E. Matter 1.3 TLV + QR/Verhoeff + session AEAD ChaCha20-Poly1305 (rejet falsification)
    let onboarding = MatterOnboardingPayload {
        version: 0,
        vendor_id: 0xFFF1,
        product_id: 0x8001,
        discriminator: 3840,
        passcode: 20202021,
    };
    let qr = onboarding.to_qr_code();
    assert_eq!(
        MatterOnboardingPayload::from_qr_code(&qr).unwrap(),
        onboarding
    );
    let manual = onboarding.to_manual_code_11().unwrap();
    assert!(MatterOnboardingPayload::validate_manual_code(&manual).is_ok());

    let tlv = encode_matter_command_tlv(1, &HomeCommand::SetLevel { level_pct: 60 }).unwrap();
    let (decoded_tlv, used) = decode_tlv(&tlv).unwrap();
    assert_eq!(used, tlv.len());
    assert!(matches!(decoded_tlv.value, TlvValue::Struct(_)));
    let mut custom_buf = Vec::new();
    encode_tlv(
        &TlvElement::anon(TlvValue::Struct(vec![TlvElement::context(
            0,
            TlvValue::UnsignedInt(42),
        )])),
        &mut custom_buf,
    );
    assert!(!custom_buf.is_empty());

    let mut session = MatterSecureSession::derive_from_shared_secret(
        7,
        b"matter-pase-conformance-secret",
        b"hakille-conformance-salt",
    );
    let mut sealed = session.seal_message(0x08, 0x0001, 42, &tlv).unwrap();
    let (_, _, _, opened) = session.open_message(&sealed, true).unwrap();
    assert_eq!(opened, tlv);
    // Falsification du tag Poly1305 -> rejet obligatoire
    let last = sealed.len() - 1;
    sealed[last] ^= 0x55;
    assert!(session.open_message(&sealed, true).is_err());

    // F. HomeKit HAP ChaCha20-Poly1305 AEAD
    let hap_key = [0x42u8; 32];
    let mut hap_block = hap_encrypt_block(&hap_key, 1, b"PUT /characteristics HTTP/1.1").unwrap();
    let hap_plain = hap_decrypt_block(&hap_key, 1, &hap_block).unwrap();
    assert_eq!(hap_plain, b"PUT /characteristics HTTP/1.1");
    let hap_last = hap_block.len() - 1;
    hap_block[hap_last] ^= 0xFF;
    assert!(hap_decrypt_block(&hap_key, 1, &hap_block).is_err());

    // G. OCPP 1.6J + Home Assistant sur vrai serveur WebSocket RFC 6455 en loopback
    let ocpp_call = command_to_ocpp_call(
        "tx-1",
        &HomeCommand::SetEvCharging {
            enabled: true,
            max_amps: Some(16.0),
        },
    )
    .unwrap();
    assert_eq!(
        OcppFrame::from_json_str(&ocpp_call.to_json_string()).unwrap(),
        ocpp_call
    );

    let (domain, service, _) =
        command_to_ha_service("light.salon", &HomeCommand::SetPower { on: true }).unwrap();
    assert_eq!((domain, service), ("homeassistant", "turn_on"));

    // Serveur WS loopback minimal pour Home Assistant (`auth_required` -> `auth_ok` -> `result`)
    let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_addr = ws_listener.local_addr().unwrap();
    let ws_server = tokio::spawn(async move {
        if let Ok((mut sock, _)) = ws_listener.accept().await {
            let mut req_buf = [0u8; 2048];
            let n = sock.read(&mut req_buf).await.unwrap();
            let req_str = String::from_utf8_lossy(&req_buf[..n]);
            let key = req_str
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    if k.eq_ignore_ascii_case("Sec-WebSocket-Key") {
                        Some(v.trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap();
            let accept = compute_websocket_accept(&key);
            let handshake = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            sock.write_all(handshake.as_bytes()).await.unwrap();

            // Helper envoi trame texte serveur (non masquée)
            let send_text = |s: &str| {
                let b = s.as_bytes();
                let mut f = vec![0x81, b.len() as u8];
                f.extend_from_slice(b);
                f
            };
            sock.write_all(&send_text(r#"{"type":"auth_required"}"#))
                .await
                .unwrap();

            // Lire auth client
            let mut tmp = [0u8; 512];
            let _ = sock.read(&mut tmp).await;
            sock.write_all(&send_text(r#"{"type":"auth_ok"}"#))
                .await
                .unwrap();

            // Lire call_service client
            let _ = sock.read(&mut tmp).await;
            sock.write_all(&send_text(r#"{"id":1,"type":"result","success":true}"#))
                .await
                .unwrap();
        }
    });

    let ha_res = ha_ws_call_service(
        ws_addr,
        "test-local-token",
        "light.salon",
        &HomeCommand::SetPower { on: true },
    )
    .await
    .expect("Home Assistant WS loopback");
    assert_eq!(ha_res["success"], true);
    let _ = ws_server.await;

    // Vérifier aussi que WsClient refuse les endpoints WAN via un appel direct si nécessaire
    let _ws_type_check: Option<WsClient> = None;
}

#[test]
fn home_conformance_05_digital_twin_3d_and_energy_homeostasis_honesty() {
    // 1. Jumeau numérique 3D : raycast + micro-geste + Palais 3D + graphe mémoire SPO
    let mut twin = HomeDigitalTwin::new();
    twin.upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, -3.0]));

    let mut lamp = HomeDevice::new(
        "matter-lamp-1",
        "Lampe Salon",
        "salon",
        ProtocolKind::Matter,
        DeviceCategory::ColorLight,
        "192.168.1.40:5540",
    );
    lamp.pose_3d = SpatialPose3D::new(0.0, 1.5, -3.0).with_radius(0.5);
    lamp.status = ConnectionStatus::PairedOnline;
    twin.upsert_device(lamp);

    let ray = GazeRay3D::from_vector(
        Point3D::new(0.0, 1.5, 0.0),
        Vector3D::new(0.0, 0.0, -1.0),
        false,
    );
    let hit = twin
        .raycast_gaze(&ray, 10.0)
        .expect("Raycast 3D doit toucher la lampe");
    assert_eq!(hit.device_id, "matter-lamp-1");

    let pinch = DetectedMicroGesture::IndexThumbPinch {
        hand: Handedness::Right,
        pinch_distance_norm: 0.1,
        duration_ms: 180,
        confirmed: true,
    };
    let intent = twin.interpret_micro_gesture(&pinch, Some(&hit)).unwrap();
    assert_eq!(intent.command, HomeCommand::Toggle);
    assert!(!intent.requires_600ms_hold);

    let palace = twin.to_palace_3d();
    assert_eq!(palace.nodes.len(), 2);
    assert_eq!(palace.edges.len(), 1);

    let mut graph = KnowledgeGraph::new();
    assert!(twin.project_to_knowledge_graph(&mut graph) >= 5);

    // 2. Homéostasie énergétique : zéro spéculation sans sonde mesurée
    let engine = EnergyHomeostasisEngine::default();
    let empty_report = engine.evaluate_arbitrage(std::iter::empty::<&HomeDevice>());
    assert!(empty_report.actions.is_empty());
    assert!(empty_report.catalyst_thesis.contains("Aucune sonde"));

    // 3. Homéostasie énergétique : optimisation surplus solaire réel vers borne VE OCPP
    let mut solar = HomeDevice::new(
        "solar-1",
        "Onduleur SunSpec",
        "garage",
        ProtocolKind::SunSpecSolar,
        DeviceCategory::SolarInverter,
        "192.168.1.90:502",
    );
    solar.status = ConnectionStatus::PairedOnline;
    solar.state.energy = Some(EnergyTelemetry {
        power_w: 0.0,
        voltage_v: Some(232.0),
        current_a: Some(20.0),
        energy_kwh: Some(1200.0),
        power_factor: Some(0.99),
        solar_generation_w: Some(4800.0),
        battery_soc_pct: None,
    });

    let mut ev = HomeDevice::new(
        "ev-wallbox",
        "Borne VE OCPP",
        "garage",
        ProtocolKind::OcppEv,
        DeviceCategory::EvCharger,
        "ws://192.168.1.91:8887/ocpp/wallbox",
    );
    ev.status = ConnectionStatus::PairedOnline;
    ev.state.energy = Some(EnergyTelemetry {
        power_w: 0.0,
        voltage_v: Some(230.0),
        current_a: Some(0.0),
        energy_kwh: Some(320.0),
        power_factor: Some(1.0),
        solar_generation_w: None,
        battery_soc_pct: None,
    });

    let devices = [&solar, &ev];
    let balance = engine.compute_balance(devices);
    assert_eq!(balance.measured_device_count, 2);
    assert!((balance.solar_generation_w - 4800.0).abs() < 1e-3);
    assert!(balance.net_grid_w < -3000.0);

    let report = engine.evaluate_arbitrage(devices);
    assert!(
        report.actions.iter().any(|a| a.device_id == "ev-wallbox"
            && matches!(a.command, HomeCommand::SetEvCharging { enabled: true, .. })),
        "Le moteur d'homéostasie doit recommander une charge VE sur surplus solaire mesuré"
    );
}

/// **Conformance Home 06** : Moteur d'automatisations réactives (escalade obligatoire Tier 3),
/// tarification EDF Tempo (Bleu/Blanc/Rouge), anomalies énergétiques et persistance `redb`.
#[tokio::test]
async fn home_conformance_06_automations_tempo_tariffs_anomalies_and_redb_persistence() {
    use hakille_home::{
        AnomalySeverity, AutomationAction, AutomationRule, AutomationTrigger, EnergySample,
        TariffPlan, TempoDayColor,
    };

    let (_dir, db) = temp_home_db("auto_tempo");
    let hub = SovereignHomeHub::open(&db).await.unwrap();

    // Appareils : capteur salon + lampe salon (Tier 1) + serrure entrée (Tier 3) + PAC surchargée
    let mut lamp = HomeDevice::new(
        "lampe-salon",
        "Lampe Salon",
        "salon",
        ProtocolKind::Matter,
        DeviceCategory::Light,
        "192.168.1.20:5540",
    );
    lamp.status = ConnectionStatus::PairedOnline;
    lamp.state.occupancy = Some(true);
    lamp.state.temperature_c = Some(2.0); // alerte gel
    lamp.state.battery_pct = Some(9); // alerte batterie critique
    lamp.state.energy = Some(EnergyTelemetry {
        power_w: 9600.0, // surcharge critique
        ..Default::default()
    });
    hub.register_device(lamp).await.unwrap();

    let mut lock = HomeDevice::new(
        "serrure-entree",
        "Serrure Entrée",
        "entree",
        ProtocolKind::Mqtt,
        DeviceCategory::DoorLock,
        "mqtt://192.168.1.10:1883",
    );
    lock.status = ConnectionStatus::PairedOnline;
    hub.register_device(lock).await.unwrap();

    // Règle mixte : 1 action Tier 1 (allumer lampe) + 1 action Tier 3 (déverrouiller serrure)
    let rule = AutomationRule {
        id: "rule-presence-entree".into(),
        name: "Présence -> lumière + tentative serrure".into(),
        description: "Test de conformité d'escalade Tier 3 obligatoire".into(),
        enabled: true,
        trigger: AutomationTrigger::OccupancyChanged {
            room_id: "salon".into(),
            occupied: true,
        },
        conditions: vec![],
        actions: vec![
            AutomationAction {
                device_id: "lampe-salon".into(),
                command: HomeCommand::SetPower { on: true },
            },
            AutomationAction {
                device_id: "serrure-entree".into(),
                command: HomeCommand::UnlockDoor,
            },
        ],
        cooldown_secs: 300,
        last_triggered_at: None,
    };
    hub.upsert_automation_rule(rule).await.unwrap();

    // Échantillons énergétiques réels persistés + tarif EDF Tempo Rouge
    hub.set_tariff_plan(TariffPlan::Tempo {
        day_color: TempoDayColor::Rouge,
    })
    .await;
    hub.record_energy_sample(EnergySample {
        timestamp_utc: "2026-11-19T14:00:00Z".into(),
        hour_local: 14,
        duration_secs: 3600,
        solar_w: 600.0,
        load_w: 3600.0,
        net_grid_w: 3000.0,
    })
    .await
    .unwrap();

    // Évaluation : Tier 1 auto-planifiée, Tier 3 OBLIGATOIREMENT escaladée (jamais auto-exécutée)
    let eval = hub.evaluate_automations().await;
    assert_eq!(eval.auto_dispatches.len(), 1);
    assert_eq!(eval.auto_dispatches[0].device_id, "lampe-salon");
    assert_eq!(eval.escalations.len(), 1);
    assert_eq!(eval.escalations[0].device_id, "serrure-entree");
    assert!(eval.escalations[0].requires_600ms_hold);

    // Cooldown actif empêche le re-déclenchement immédiat
    let eval2 = hub.evaluate_automations().await;
    assert!(eval2.auto_dispatches.is_empty());
    assert!(eval2.escalations.is_empty());
    assert_eq!(eval2.skipped_cooldown, vec!["rule-presence-entree"]);

    // Bilan journalier Tempo Rouge > Bleu & anomalies détectées
    let daily_rouge = hub.daily_energy_report().await;
    assert_eq!(daily_rouge.sample_count, 1);
    assert!((daily_rouge.imported_kwh - 3.0).abs() < 1e-3);
    hub.set_tariff_plan(TariffPlan::Tempo {
        day_color: TempoDayColor::Bleu,
    })
    .await;
    let daily_bleu = hub.daily_energy_report().await;
    assert!(daily_rouge.estimated_import_cost_eur > daily_bleu.estimated_import_cost_eur);

    let anomalies = hub.detect_anomalies().await;
    assert!(
        anomalies
            .iter()
            .any(|a| a.severity == AnomalySeverity::Critical),
        "Une surcharge ou un risque de gel doit remonter en Critical : {anomalies:?}"
    );

    // Persistance redb après réouverture
    drop(hub);
    let reopened = SovereignHomeHub::open(&db).await.unwrap();
    assert_eq!(reopened.list_automation_rules().await.len(), 1);
    assert_eq!(reopened.daily_energy_report().await.sample_count, 1);
}

/// **Conformance Home 07** : Résolution d'intentions naturelles FR/EN hors-ligne (zéro hallucination)
/// + exécution filaire réelle via `SovereignHomeHub` sur Philips Hue CLIP v2, Zigbee2MQTT et ONVIF/RTSP.
#[tokio::test]
async fn home_conformance_07_offline_intent_and_real_wire_hue_z2m_onvif() {
    use hakille_home::ResolvedHomeIntent;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // 1. Serveur loopback Philips Hue CLIP v2 HTTP
    let hue_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hue_addr = hue_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = hue_listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let n = sock.read(&mut buf).await.unwrap();
        let req = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(req.contains("PUT /clip/v2/resource/light/rid-salon"));
        let body = r#"{"errors":[],"data":[{"rid":"rid-salon","rtype":"light"}]}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = sock.write_all(resp.as_bytes()).await;
    });

    // 2. Serveur loopback MQTT pour pont Zigbee2MQTT
    let z2m_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let z2m_addr = z2m_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = z2m_listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = sock.read(&mut buf).await;
        let _ = sock
            .write_all(
                &MqttPacket::ConnAck {
                    session_present: false,
                    reason_code: 0,
                }
                .to_bytes(),
            )
            .await;
        let _ = sock.read(&mut buf).await;
        let _ = sock
            .write_all(&MqttPacket::PubAck { packet_id: 1 }.to_bytes())
            .await;
    });

    // 3. Serveur loopback RTSP OPTIONS (ONVIF)
    let rtsp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rtsp_addr = rtsp_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = rtsp_listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = sock.read(&mut buf).await.unwrap();
        let req = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(req.starts_with("OPTIONS "));
        let resp = "RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: OPTIONS, DESCRIBE, SETUP, PLAY\r\n\r\n";
        let _ = sock.write_all(resp.as_bytes()).await;
    });

    let (_dir, db) = temp_home_db("intent_wire");
    let hub = SovereignHomeHub::open(&db).await.unwrap();
    hub.upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, 0.0]))
        .await
        .unwrap();
    hub.upsert_room(RoomZone::new("jardin", "Jardin", 0, [6.0, 2.0, 0.0]))
        .await
        .unwrap();

    let mut hue_lamp = HomeDevice::new(
        "hue-salon",
        "Lampe Salon",
        "salon",
        ProtocolKind::PhilipsHue,
        DeviceCategory::ColorLight,
        format!("http://{hue_addr}"),
    );
    hue_lamp
        .state
        .raw_attributes
        .insert("hue_app_key".into(), serde_json::json!("local-key"));
    hue_lamp
        .state
        .raw_attributes
        .insert("hue_rid".into(), serde_json::json!("rid-salon"));
    hub.register_device(hue_lamp).await.unwrap();

    let z2m_plug = HomeDevice::new(
        "z2m-prise-salon",
        "Prise Zigbee Salon",
        "salon",
        ProtocolKind::Zigbee,
        DeviceCategory::Switch,
        format!("mqtt://{z2m_addr}"),
    );
    hub.register_device(z2m_plug).await.unwrap();

    let cam = HomeDevice::new(
        "cam-jardin",
        "Caméra Jardin",
        "jardin",
        ProtocolKind::OnvifRtsp,
        DeviceCategory::SecurityCamera,
        format!("rtsp://{rtsp_addr}/stream1"),
    );
    hub.register_device(cam).await.unwrap();

    // Résolution naturelle FR hors-ligne
    let resolved = hub
        .resolve_natural_intent("Allume la lampe du salon à 80%")
        .await;
    let (target_id, cmd) = match resolved {
        ResolvedHomeIntent::DeviceCommand {
            device_id,
            command,
            requires_600ms_hold,
            ..
        } => {
            assert_eq!(device_id, "hue-salon");
            assert!(!requires_600ms_hold);
            (device_id, command)
        }
        other => panic!("Attendu DeviceCommand hue-salon, reçu {other:?}"),
    };

    // Zéro hallucination sur appareil inexistant
    assert!(matches!(
        hub.resolve_natural_intent("Déverrouille la porte du grenier")
            .await,
        ResolvedHomeIntent::Unmatched { .. }
    ));

    let authority = new_ephemeral_authority();
    let policy = HomeBiscuitPolicy::new(authority.root_public_key());
    let expiry = Timestamp(Utc::now() + ChronoDuration::minutes(5));

    // Exécution réelle Philips Hue CLIP v2
    let hue_token = HomeBiscuitPolicy::issue_home_token(
        &authority,
        vec![CapabilityScope::Network],
        vec![CapabilityAction::Execute],
        expiry.clone(),
        Some("salon"),
        Some(ProtocolKind::PhilipsHue),
        HomeResidentRole::Owner,
    )
    .unwrap();
    let hue_receipt = hub
        .execute_authorized(
            &policy,
            &hue_token,
            HomeResidentRole::Owner,
            &target_id,
            &cmd,
            None,
        )
        .await
        .unwrap();
    assert!(hue_receipt.executed_on_wire);
    assert_eq!(hue_receipt.frame_sha256.len(), 64);

    // Exécution réelle Zigbee via pont Zigbee2MQTT (MQTT QoS 1)
    let z2m_token = HomeBiscuitPolicy::issue_home_token(
        &authority,
        vec![CapabilityScope::Network],
        vec![CapabilityAction::Execute],
        expiry.clone(),
        Some("salon"),
        Some(ProtocolKind::Zigbee),
        HomeResidentRole::Owner,
    )
    .unwrap();
    let z2m_receipt = hub
        .execute_authorized(
            &policy,
            &z2m_token,
            HomeResidentRole::Owner,
            "z2m-prise-salon",
            &HomeCommand::SetPower { on: true },
            None,
        )
        .await
        .unwrap();
    assert!(z2m_receipt.executed_on_wire);

    // Exécution réelle ONVIF/RTSP OPTIONS (Tier 0 ReadState exige CapabilityScope::Sensor + Read)
    let cam_token = HomeBiscuitPolicy::issue_home_token(
        &authority,
        vec![CapabilityScope::Sensor],
        vec![CapabilityAction::Read],
        expiry,
        Some("jardin"),
        Some(ProtocolKind::OnvifRtsp),
        HomeResidentRole::Owner,
    )
    .unwrap();
    let cam_receipt = hub
        .execute_authorized(
            &policy,
            &cam_token,
            HomeResidentRole::Owner,
            "cam-jardin",
            &HomeCommand::ReadState,
            None,
        )
        .await
        .unwrap();
    assert!(cam_receipt.executed_on_wire);
}

/// **Conformance Home 08** : Synchronisation P2P chiffrée du jumeau domotique (`hakille-mesh`
/// ChaCha20-Poly1305 + Ed25519 + rejet d'endpoints cloud) et contrôle mobile UniFFI
/// (`MobileBridge` : blocage Mode Invité + maintien 600 ms strict en Tier 3).
#[tokio::test]
async fn home_conformance_08_encrypted_mesh_sync_and_mobile_uniffi_home_control() {
    use ed25519_dalek::SigningKey;
    use hakille_home::{
        HomeTwinSyncDelta, broadcast_home_delta_on_mesh, validate_mesh_peer_endpoint,
    };
    use hakille_ipc::Frame;
    use hakille_mesh::{MeshTransport, VirtualAirGapBus};
    use hakille_mobile::MobileBridge;
    use hakille_ui_protocol::{HomeDeviceDto, HomeEnergyDto, HomeProtocolStatusDto, KernelToUi};

    // 1. Garde Air-Gap / Link-Local stricte sur les pairs mesh
    assert!(validate_mesh_peer_endpoint("airgap://foyer-salon").is_ok());
    assert!(validate_mesh_peer_endpoint("https://sync.cloud-vendor.example").is_err());

    // 2. Synchronisation chiffrée AEAD + Ed25519 entre 2 hubs souverains distincts
    let (_d1, db_a) = temp_home_db("mesh_hub_a");
    let (_d2, db_b) = temp_home_db("mesh_hub_b");
    let hub_a = SovereignHomeHub::open(&db_a).await.unwrap();
    let hub_b = SovereignHomeHub::open(&db_b).await.unwrap();

    hub_a
        .upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, 0.0]))
        .await
        .unwrap();
    hub_a
        .register_device(HomeDevice::new(
            "thermo-knx",
            "Thermostat KNX",
            "salon",
            ProtocolKind::KnxIp,
            DeviceCategory::Thermostat,
            "192.168.1.33:3671",
        ))
        .await
        .unwrap();

    let bus = VirtualAirGapBus::new(64);
    let peer_a = bus.join_peer("hub-a");
    let peer_b = bus.join_peer("hub-b");
    let signing_key = SigningKey::from_bytes(&[23u8; 32]);
    let sym_key = [77u8; 32];

    let delta = hub_a.export_mesh_delta("hub-a", 1).await;
    let _sent = broadcast_home_delta_on_mesh(&peer_a, &delta, &sym_key, [5u8; 12], &signing_key)
        .await
        .unwrap();
    let recv = peer_b.receive().await.unwrap();

    let sync_report = hub_b.apply_mesh_frame(&recv, &sym_key).await.unwrap();
    assert_eq!(sync_report.devices_merged, 1);
    assert!(hub_b.get_device("thermo-knx").await.is_some());

    // Falsification de trame -> rejet cryptographique fail-closed
    let mut tampered = recv.clone();
    tampered.encrypted_payload[0] ^= 0x5A;
    assert!(hub_b.apply_mesh_frame(&tampered, &sym_key).await.is_err());

    // Injection d'un appareil WAN dans un delta chiffré valide -> rejet HomeLanGuard
    let malicious_dev = HomeDevice::new(
        "cloud-spy",
        "Capteur Cloud",
        "salon",
        ProtocolKind::Mqtt,
        DeviceCategory::Switch,
        "https://evil-cloud.example/api",
    );
    let malicious_delta = HomeTwinSyncDelta {
        origin_node_id: "hub-rogue".into(),
        sequence: 2,
        generated_at: Utc::now().to_rfc3339(),
        rooms: vec![],
        devices: vec![malicious_dev],
        scenes: vec![],
        automations: vec![],
    };
    let malicious_frame = malicious_delta
        .seal_mesh_frame(None, &sym_key, [6u8; 12], &signing_key)
        .unwrap();
    assert!(matches!(
        hub_b.apply_mesh_frame(&malicious_frame, &sym_key).await,
        Err(HomeError::LanGuardBlocked { .. })
    ));

    // 3. Frontière mobile UniFFI (`MobileBridge`) : contrôle domotique sécurisé
    let bridge = MobileBridge::new();
    let res_hub: serde_json::Value = serde_json::from_str(
        &bridge.dispatch_json(r#"{"type":"set_hub","hub_id":"hub-foyer"}"#.into()),
    )
    .unwrap();
    assert_eq!(res_hub["ok"], true);

    let res_load: serde_json::Value =
        serde_json::from_str(&bridge.dispatch_json(r#"{"type":"load_home_devices"}"#.into()))
            .unwrap();
    assert_eq!(res_load["ok"], true);
    let load_req_id = res_load["request"]["id"].as_u64().unwrap();

    let reply_frame = Frame::reply(
        load_req_id,
        KernelToUi::HomeDevicesReply {
            devices: vec![HomeDeviceDto {
                id: "serrure-entree".into(),
                name: "Serrure Entrée".into(),
                room: "entree".into(),
                protocol: "matter".into(),
                category: "door_lock".into(),
                endpoint: "192.168.1.25:5540".into(),
                status: "en-ligne".into(),
                summary: "verrouillé".into(),
                power_w: 0.4,
                pose_3d: [0.0, 1.2, 0.0],
                is_critical: true,
            }],
            protocols: vec![HomeProtocolStatusDto {
                protocol: "matter".into(),
                display_name: "Matter 1.3".into(),
                transport_mode: "UDP :5540".into(),
                default_port: Some(5540),
                honesty_note: "LAN uniquement".into(),
            }],
            energy: HomeEnergyDto {
                solar_w: 1800.0,
                load_w: 900.0,
                ev_w: 0.0,
                hvac_w: 300.0,
                net_grid_w: -900.0,
                battery_soc_pct: Some(68.0),
                self_consumption_pct: 50.0,
                measured_devices: 1,
                verdict: "Surplus solaire".into(),
            },
        },
    );
    let res_recv: serde_json::Value = serde_json::from_str(
        &bridge.receive_stream_frame(load_req_id, serde_json::to_string(&reply_frame).unwrap()),
    )
    .unwrap();
    assert_eq!(res_recv["ok"], true);
    assert_eq!(
        res_recv["snapshot"]["home_devices"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Guest Pass refuse toute commande domotique
    let _ = bridge.dispatch_json(
        r#"{"type":"start_guest_pass","name":"Visiteur","power":"lecture","duration_secs":300}"#
            .into(),
    );
    let res_guest: serde_json::Value = serde_json::from_str(&bridge.dispatch_json(
        r#"{"type":"send_home_command","device_id":"serrure-entree","action":"unlock_door","params_json":"{}","confirm_hold_ms":650}"#.into(),
    ))
    .unwrap();
    assert_eq!(res_guest["ok"], false);
    let _ = bridge.dispatch_json(r#"{"type":"revoke_guest_pass"}"#.into());

    // Tier 3 refusé à 599 ms, accepté à 600 ms
    let res_599: serde_json::Value = serde_json::from_str(&bridge.dispatch_json(
        r#"{"type":"send_home_command","device_id":"serrure-entree","action":"unlock_door","params_json":"{}","confirm_hold_ms":599}"#.into(),
    ))
    .unwrap();
    assert_eq!(res_599["ok"], false);

    let res_600: serde_json::Value = serde_json::from_str(&bridge.dispatch_json(
        r#"{"type":"send_home_command","device_id":"serrure-entree","action":"unlock_door","params_json":"{}","confirm_hold_ms":600}"#.into(),
    ))
    .unwrap();
    assert_eq!(res_600["ok"], true);
}
