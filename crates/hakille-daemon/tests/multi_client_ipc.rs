//! Test d'intégration multi-clients du serveur IPC de hakille-daemon.
//!
//! Valide la coexistence concurrente du Desktop HUD et du Mobile Gateway :
//! 1. Connexion et handshake concurrents sans blocage séquentiel
//! 2. Requêtes simultanées avec routage unicast strict (`id = request.id`)
//! 3. Diffusion broadcast des événements système (`id = 0`)
//! 4. Résolution mutuelle de carte d'action (décision par Mobile répercutée sur Desktop)
//! 5. Résilience à la déconnexion abrupte d'un client

use hakille_ipc::{Frame, FrameBody, transport};
use hakille_ui_protocol::{KernelToUi, PresenceState, UiToKernel};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::BufReader;

struct DaemonHarness {
    child: tokio::process::Child,
    endpoint: String,
    token_file: PathBuf,
    token: String,
    data_dir: PathBuf,
}

impl Drop for DaemonHarness {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.endpoint);
        let _ = std::fs::remove_file(&self.token_file);
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

async fn connect_with_retry(endpoint: &str) -> std::io::Result<transport::ClientStream> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match transport::connect(endpoint).await {
            Ok(stream) => return Ok(stream),
            Err(err) if std::time::Instant::now() < deadline => {
                let _ = err;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn start_daemon_server() -> DaemonHarness {
    let id = uuid::Uuid::now_v7();
    let data_dir = std::env::temp_dir().join(format!("hakille-daemon-test-data-{id}"));
    std::fs::create_dir_all(&data_dir).unwrap();

    #[cfg(unix)]
    let endpoint = format!("/tmp/hakille-ipc-test-{id}.sock");
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\hakille-ipc-test-{id}");
    let token_file = data_dir.join("ipc.token");

    let exe = env!("CARGO_BIN_EXE_hakille-daemon");
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("--serve")
        .env("HAKILLE_IPC_ENDPOINT", &endpoint)
        .env("HAKILLE_IPC_TOKEN_FILE", &token_file)
        .env("HAKILLE_DATA_DIR", &data_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn hakille-daemon binary");

    let mut token = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        #[cfg(unix)]
        let endpoint_ready = std::path::Path::new(&endpoint).exists();
        #[cfg(windows)]
        let endpoint_ready = true;
        if endpoint_ready && token_file.exists() {
            if let Ok(t) = std::fs::read_to_string(&token_file) {
                let trimmed = t.trim().to_string();
                if !trimmed.is_empty() {
                    token = trimmed;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if token.is_empty() {
        // Le diagnostic d'un boot bloqué ne doit pas attendre indéfiniment.
        let _ = child.start_kill();
        let output = child.wait_with_output().await.expect("wait failed");
        panic!(
            "le serveur daemon n'a pas écrit le jeton IPC à temps.\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    DaemonHarness {
        child,
        endpoint,
        token_file,
        token,
        data_dir,
    }
}

#[tokio::test]
async fn test_concurrent_clients_and_mutual_card_resolution() {
    let harness = start_daemon_server().await;

    // 1. Client 1 (Desktop HUD) se connecte et effectue le handshake
    let mut desktop_stream = connect_with_retry(&harness.endpoint)
        .await
        .expect("connexion Desktop HUD");
    transport::client_handshake(&mut desktop_stream, &harness.token)
        .await
        .expect("handshake Desktop HUD");
    let (desktop_reader, mut desktop_writer) = tokio::io::split(desktop_stream);
    let mut desktop_reader = BufReader::new(desktop_reader);

    // 2. Client 2 (Mobile Gateway) se connecte simultanément et effectue le handshake sans blocage
    let mut mobile_stream = connect_with_retry(&harness.endpoint)
        .await
        .expect("connexion Mobile Gateway");
    transport::client_handshake(&mut mobile_stream, &harness.token)
        .await
        .expect("handshake Mobile Gateway");
    let (mobile_reader, mut mobile_writer) = tokio::io::split(mobile_stream);
    let mut mobile_reader = BufReader::new(mobile_reader);

    // 3. REQUÊTES CONCURRENTES SIMULTANÉES : Desktop et Mobile envoient chacun une requête de statut
    let req_desktop = Frame::request(101, UiToKernel::StatusRequest);
    transport::write_frame(&mut desktop_writer, &req_desktop)
        .await
        .unwrap();

    let req_mobile = Frame::request(201, UiToKernel::StatusRequest);
    transport::write_frame(&mut mobile_writer, &req_mobile)
        .await
        .unwrap();

    // Desktop reçoit sa réponse avec son id = 101
    let resp_desktop = transport::read_frame(&mut desktop_reader)
        .await
        .unwrap()
        .expect("desktop status frame");
    assert_eq!(
        resp_desktop.id, 101,
        "Desktop doit recevoir la réponse à sa propre requête (id=101)"
    );
    assert!(matches!(
        resp_desktop.body,
        FrameBody::Reply {
            message: KernelToUi::StatusReply { .. }
        }
    ));

    // Mobile reçoit sa réponse avec son id = 201 (aucune fuite du paquet Desktop)
    let resp_mobile = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile status frame");
    assert_eq!(
        resp_mobile.id, 201,
        "Mobile doit recevoir sa propre réponse (id=201)"
    );
    assert!(matches!(
        resp_mobile.body,
        FrameBody::Reply {
            message: KernelToUi::StatusReply { .. }
        }
    ));

    // 4. PROPOSITION DE CARTE D'ACTION : Desktop soumet un tour irréversible ("payer 25 EUR à Awa")
    let prompt_card = Frame::request(
        102,
        UiToKernel::HudSubmit {
            query: "payer 25 EUR à Awa".to_string(),
        },
    );
    transport::write_frame(&mut desktop_writer, &prompt_card)
        .await
        .unwrap();

    // Desktop reçoit sa réponse finale Unicast (id=102) et les 3 événements broadcast (id=0)
    let mut desktop_turn_frames = Vec::new();
    for _ in 0..4 {
        let f = transport::read_frame(&mut desktop_reader)
            .await
            .unwrap()
            .expect("desktop turn frame");
        desktop_turn_frames.push(f);
    }

    let unicast_d = desktop_turn_frames
        .iter()
        .find(|f| f.id == 102)
        .expect("Desktop doit recevoir id=102");
    assert!(matches!(
        unicast_d.body,
        FrameBody::Reply {
            message: KernelToUi::HudReply { .. }
        }
    ));

    let bcast_d: Vec<&Frame> = desktop_turn_frames.iter().filter(|f| f.id == 0).collect();
    assert_eq!(
        bcast_d.len(),
        3,
        "Desktop doit recevoir les 3 frames broadcast"
    );
    let card_id = bcast_d
        .iter()
        .find_map(|f| match &f.body {
            FrameBody::Reply {
                message: KernelToUi::ShowCard(card),
            } => Some(card.id),
            _ => None,
        })
        .expect("Desktop doit observer ShowCard");

    // Mobile (qui n'a rien demandé) reçoit les 3 trames diffusées sur le bus broadcast avec id = 0
    let bcast_m1 = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile bcast 1");
    assert_eq!(bcast_m1.id, 0);
    assert!(matches!(
        bcast_m1.body,
        FrameBody::Reply {
            message: KernelToUi::Presence(PresenceState::Acting)
        }
    ));

    let bcast_m2 = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile bcast 2");
    assert_eq!(bcast_m2.id, 0);
    assert!(matches!(
        bcast_m2.body,
        FrameBody::Reply {
            message: KernelToUi::Presence(PresenceState::DecisionRequired)
        }
    ));

    let bcast_m3 = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile bcast 3");
    assert_eq!(bcast_m3.id, 0);
    match bcast_m3.body {
        FrameBody::Reply {
            message: KernelToUi::ShowCard(ref card),
        } => {
            assert_eq!(card.id, card_id);
            assert_eq!(card.verb, "Confirmer");
        }
        other => panic!("ShowCard attendu sur Mobile, reçu : {other:?}"),
    }

    // 5. RÉSOLUTION MUTUELLE : Mobile décide et approuve la carte (id=202)
    let approve_req = Frame::request(202, UiToKernel::Approve { card_id });
    transport::write_frame(&mut mobile_writer, &approve_req)
        .await
        .unwrap();

    // Mobile reçoit son accusé Unicast direct avec son id 202
    let ack_mobile = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile ack frame");
    assert_eq!(ack_mobile.id, 202);
    assert!(matches!(
        ack_mobile.body,
        FrameBody::Reply {
            message: KernelToUi::CardDecisionAck { approved: true, .. }
        }
    ));

    // Desktop reçoit la notification de décision via broadcast avec id = 0
    let ack_desktop = transport::read_frame(&mut desktop_reader)
        .await
        .unwrap()
        .expect("desktop ack broadcast");
    assert_eq!(ack_desktop.id, 0);
    assert!(matches!(
        ack_desktop.body,
        FrameBody::Reply {
            message: KernelToUi::CardDecisionAck { approved: true, .. }
        }
    ));

    // Tous les deux reçoivent Presence(Idle) avec id = 0
    let idle_d = transport::read_frame(&mut desktop_reader)
        .await
        .unwrap()
        .expect("desktop idle");
    assert_eq!(idle_d.id, 0);
    assert!(matches!(
        idle_d.body,
        FrameBody::Reply {
            message: KernelToUi::Presence(PresenceState::Idle)
        }
    ));

    let idle_m = transport::read_frame(&mut mobile_reader)
        .await
        .unwrap()
        .expect("mobile idle");
    assert_eq!(idle_m.id, 0);
    assert!(matches!(
        idle_m.body,
        FrameBody::Reply {
            message: KernelToUi::Presence(PresenceState::Idle)
        }
    ));

    // 6. RÉSILIENCE À LA DÉCONNEXION : Mobile se déconnecte abruptement
    drop(mobile_reader);
    drop(mobile_writer);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Desktop continue à communiquer sans aucune erreur
    let ping = Frame::request(103, UiToKernel::StatusRequest);
    transport::write_frame(&mut desktop_writer, &ping)
        .await
        .unwrap();
    let pong = transport::read_frame(&mut desktop_reader)
        .await
        .unwrap()
        .expect("desktop pong");
    assert_eq!(pong.id, 103);
    assert!(matches!(
        pong.body,
        FrameBody::Reply {
            message: KernelToUi::StatusReply { .. }
        }
    ));

    // 7. Le refus réseau termine le tour sans publier de synthèse non validée.
    // Le canal de deltas est fermé par le harness : il ne doit pas affamer la boucle IPC.
    transport::write_frame(
        &mut desktop_writer,
        &Frame::request(
            104,
            UiToKernel::HudSubmit {
                query: ":read-web https://127.0.0.1/".into(),
            },
        ),
    )
    .await
    .unwrap();
    let refusal = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let frame = transport::read_frame(&mut desktop_reader)
                .await
                .unwrap()
                .unwrap();
            assert!(
                !matches!(
                    frame.body,
                    FrameBody::Reply {
                        message: KernelToUi::TokenDelta { .. }
                    }
                ),
                "aucune synthèse provisoire pendant une recherche"
            );
            if frame.id == 104 {
                break frame;
            }
        }
    })
    .await
    .expect("le refus réseau doit terminer sans bloquer l'IPC");
    match refusal.body {
        FrameBody::Reply {
            message: KernelToUi::Toast { text },
        } => {
            assert!(text.contains("non publique"), "refus réel attendu : {text}");
        }
        other => panic!("refus explicite attendu, reçu : {other:?}"),
    }
    transport::write_frame(
        &mut desktop_writer,
        &Frame::request(105, UiToKernel::StatusRequest),
    )
    .await
    .unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = transport::read_frame(&mut desktop_reader)
                .await
                .unwrap()
                .unwrap();
            if frame.id == 105 {
                break frame;
            }
        }
    })
    .await
    .expect("le serveur reste réactif après refus");
    assert!(matches!(
        reply.body,
        FrameBody::Reply {
            message: KernelToUi::StatusReply { .. }
        }
    ));
}
