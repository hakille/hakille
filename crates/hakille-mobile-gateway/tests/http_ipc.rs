//! Vrai HTTP loopback + faux démon IPC : aucune inférence ni succès métier simulé.
use hakille_ipc::{Frame, FrameBody, transport::*};
use hakille_mobile_gateway::{BARRIER_ID, BearerToken, Gateway, GatewayConfig, serve};
use hakille_ui_protocol::{
    ActionCard, DataExit, KernelToUi, Money, PresenceState, Provenance, StatusDto, UiToKernel,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

const IPC_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MOBILE_TOKEN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[path = "scenarios/mobile_bridge.rs"]
mod mobile_bridge;

struct Rig {
    client: reqwest::Client,
    base: String,
    address: std::net::SocketAddr,
    gateway: Arc<Gateway>,
    task: tokio::task::JoinHandle<()>,
    stop: Option<oneshot::Sender<()>>,
    directory: PathBuf,
    endpoint: String,
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
        let _ = std::fs::remove_dir_all(&self.directory);
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.endpoint);
    }
}

impl Rig {
    fn post(&self, frame: &Frame) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}/v1/request", self.base))
            .bearer_auth(MOBILE_TOKEN)
            .json(frame)
    }
}

async fn setup(request_timeout: Duration, idle: Duration) -> (Rig, IpcListener) {
    let id = uuid::Uuid::new_v4();
    let directory = std::env::temp_dir().join(format!("hmg-{id}"));
    std::fs::create_dir_all(&directory).unwrap();
    let ipc_token_file = directory.join("ipc.token");
    write_token_file(&ipc_token_file, IPC_TOKEN).unwrap();
    #[cfg(unix)]
    let endpoint = format!("/tmp/hmg-{id}.sock");
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\hmg-{id}");
    let ipc = IpcListener::bind(&endpoint).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = GatewayConfig {
        listen: address,
        ipc_endpoint: endpoint.clone(),
        ipc_token_file,
        request_timeout,
        body_timeout: Duration::from_millis(100),
        session_idle_timeout: idle,
    };
    let gateway =
        Arc::new(Gateway::new(config, BearerToken::parse(MOBILE_TOKEN).unwrap()).unwrap());
    let (stop, stopped) = oneshot::channel();
    let running = gateway.clone();
    let task = tokio::spawn(async move {
        serve(listener, running, async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    (
        Rig {
            client,
            base: format!("http://{address}"),
            address,
            gateway,
            task,
            stop: Some(stop),
            directory,
            endpoint,
        },
        ipc,
    )
}

fn status() -> KernelToUi {
    KernelToUi::StatusReply {
        status: StatusDto {
            model: "fixture IPC".into(),
            cost: "0".into(),
            state: "Idle".into(),
            memory_count: 0,
        },
    }
}

async fn accept(listener: &IpcListener) -> ServerStream {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    server_handshake(&mut stream, IPC_TOKEN).await.unwrap();
    stream
}

async fn read(stream: &mut ServerStream) -> Frame {
    tokio::time::timeout(Duration::from_secs(5), read_frame(stream))
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

async fn finish_barrier(stream: &mut ServerStream) {
    let barrier = read(stream).await;
    assert_eq!(
        barrier,
        Frame::request(BARRIER_ID, UiToKernel::StatusRequest)
    );
    write_frame(stream, &Frame::reply(BARRIER_ID, status()))
        .await
        .unwrap();
}

fn frames(value: &Value) -> Vec<Frame> {
    serde_json::from_value(value["frames"].clone()).unwrap()
}

#[tokio::test]
async fn http_auth_origin_size_version_and_allowlist_fail_before_ipc() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let health = rig
        .client
        .get(format!("{}/health", rig.base))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);
    assert_eq!(
        health.json::<Value>().await.unwrap(),
        serde_json::json!({"status":"ok"})
    );
    let unauth = rig
        .client
        .post(format!("{}/v1/request", rig.base))
        .body("pas du JSON")
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), 401);
    assert!(!unauth.text().await.unwrap().contains(IPC_TOKEN));
    let request = Frame::request(1, UiToKernel::StatusRequest);
    assert_eq!(
        rig.post(&request)
            .header("Origin", "https://example.test")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    // Le Content-Length excessif doit être refusé avant toute lecture du corps.
    // Envoyer aussi 1 Mio ici crée sous Windows une course entre l'upload et
    // la fermeture HTTP : WSAECONNABORTED peut masquer la réponse 413 valide.
    // Le test chunked ci-dessous vérifie séparément la limite sur les octets lus.
    let mut socket = tokio::net::TcpStream::connect(rig.address).await.unwrap();
    let headers = format!(
        "POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {MOBILE_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nExpect: 100-continue\r\n\r\n",
        rig.address,
        (1 << 20) + 1
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    let mut oversized_response = String::new();
    tokio::time::timeout(
        Duration::from_secs(2),
        socket.read_to_string(&mut oversized_response),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        oversized_response.starts_with("HTTP/1.1 413"),
        "{oversized_response}"
    );
    let (_, body) = oversized_response.split_once("\r\n\r\n").unwrap();
    let rejected: Value = serde_json::from_str(body).unwrap();
    assert_eq!(rejected["error"]["code"], "too_large");
    assert_eq!(rejected["error"]["issue_unknown"], false);
    let mut wrong_version = request.clone();
    wrong_version.version = 999;
    for refused in [
        wrong_version,
        Frame::request(0, UiToKernel::StatusRequest),
        Frame::request(BARRIER_ID, UiToKernel::StatusRequest),
        Frame::hello(IPC_TOKEN),
        Frame::request(2, UiToKernel::OutboxFlush),
        Frame::request(3, UiToKernel::JournalRequest { limit: 201 }),
    ] {
        let response = rig.post(&refused).send().await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["issue_unknown"],
            false
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn late_card_and_token_deltas_survive_final_reply_on_persistent_session() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        assert_eq!(read(&mut stream).await.id, 1);
        write_frame(
            &mut stream,
            &Frame::reply(0, KernelToUi::Presence(PresenceState::Acting)),
        )
        .await
        .unwrap();
        for _ in 0..300 {
            write_frame(
                &mut stream,
                &Frame::reply(0, KernelToUi::TokenDelta { text: "é".into() }),
            )
            .await
            .unwrap();
        }
        write_frame(
            &mut stream,
            &Frame::reply(
                1,
                KernelToUi::HudReply {
                    response: "réponse complète".into(),
                    validation: None,
                },
            ),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        write_frame(
            &mut stream,
            &Frame::reply(
                0,
                KernelToUi::ShowCard(ActionCard {
                    id: 42,
                    verb: "Proposer".into(),
                    object: "une action".into(),
                    reason: "test du transport".into(),
                    cost: Money::from_cents(0, "EUR"),
                    data_exit: DataExit::Local,
                    reversible: true,
                    provenance: Provenance::Hakille,
                }),
            ),
        )
        .await
        .unwrap();
        finish_barrier(&mut stream).await;
        assert_eq!(
            read(&mut stream).await,
            Frame::request(2, UiToKernel::Approve { card_id: 42 })
        );
        write_frame(
            &mut stream,
            &Frame::reply(
                2,
                KernelToUi::CardDecisionAck {
                    card_id: 42,
                    approved: true,
                    audit_id: "fixture-sans-effet-métier".into(),
                    effect: None,
                },
            ),
        )
        .await
        .unwrap();
        finish_barrier(&mut stream).await;
    });
    let result = rig
        .post(&Frame::request(
            1,
            UiToKernel::HudSubmit {
                query: "bonjour".into(),
            },
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), 200);
    let body: Value = result.json().await.unwrap();
    let received = frames(&body);
    assert_eq!(received.len(), 4);
    assert_eq!(
        received[1],
        Frame::reply(
            0,
            KernelToUi::TokenDelta {
                text: "é".repeat(300)
            }
        )
    );
    assert_eq!(received[2].id, 1);
    assert!(matches!(
        received[3].body,
        FrameBody::Reply {
            message: KernelToUi::ShowCard(_)
        }
    ));
    assert!(!received.iter().any(|frame| frame.id == BARRIER_ID));
    let approval = Frame::request(2, UiToKernel::Approve { card_id: 42 });
    assert_eq!(rig.post(&approval).send().await.unwrap().status(), 409);
    assert_eq!(
        rig.post(&approval)
            .header("x-hakille-session", "ancienne")
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let approved = rig
        .post(&approval)
        .header("x-hakille-session", body["session_id"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(approved.status(), 200);
    assert_eq!(
        approved.json::<Value>().await.unwrap()["session_id"],
        body["session_id"]
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn interrupted_sent_request_is_unknown_never_replayed_and_changes_session() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let fake = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        assert_eq!(read(&mut first).await.id, 1);
        write_frame(&mut first, &Frame::reply(1, status()))
            .await
            .unwrap();
        finish_barrier(&mut first).await;
        assert_eq!(read(&mut first).await.id, 2);
        drop(first);
        let mut second = accept(&listener).await;
        // Aucune répétition automatique de la mutation #2.
        assert_eq!(read(&mut second).await.id, 3);
        write_frame(&mut second, &Frame::reply(3, status()))
            .await
            .unwrap();
        finish_barrier(&mut second).await;
    });
    let first: Value = rig
        .post(&Frame::request(1, UiToKernel::StatusRequest))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let lost = rig
        .post(&Frame::request(
            2,
            UiToKernel::MemoryRemember {
                text: "fait".into(),
            },
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(lost.status(), 502);
    let lost: Value = lost.json().await.unwrap();
    assert_eq!(lost["error"]["issue_unknown"], true);
    assert_eq!(lost["session_id"], first["session_id"]);
    assert!(frames(&lost).is_empty());
    let stale = rig
        .post(&Frame::request(4, UiToKernel::Approve { card_id: 42 }))
        .header("x-hakille-session", first["session_id"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    let next: Value = rig
        .post(&Frame::request(3, UiToKernel::StatusRequest))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(next["session_id"], first["session_id"]);
    fake.await.unwrap();
}

#[tokio::test]
async fn busy_is_not_sent_and_timeout_closes_ipc_with_unknown_outcome() {
    let (rig, listener) = setup(Duration::from_millis(150), Duration::from_secs(60)).await;
    let (started, signal) = oneshot::channel();
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        assert_eq!(read(&mut stream).await.id, 1);
        assert_eq!(read(&mut stream).await.id, BARRIER_ID);
        started.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), read_frame(&mut stream))
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    });
    let first = rig.post(&Frame::request(
        1,
        UiToKernel::HudSubmit {
            query: "bonjour".into(),
        },
    ));
    let waiting = tokio::spawn(async move { first.send().await.unwrap() });
    signal.await.unwrap();
    let busy = rig
        .post(&Frame::request(2, UiToKernel::StatusRequest))
        .send()
        .await
        .unwrap();
    assert_eq!(busy.status(), 429);
    assert_eq!(
        busy.json::<Value>().await.unwrap()["error"]["issue_unknown"],
        false
    );
    let timed_out = waiting.await.unwrap();
    assert_eq!(timed_out.status(), 504);
    assert_eq!(
        timed_out.json::<Value>().await.unwrap()["error"]["issue_unknown"],
        true
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn wrong_response_id_and_excess_frames_are_protocol_failures() {
    for excessive in [false, true] {
        let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
        let fake = tokio::spawn(async move {
            let mut stream = accept(&listener).await;
            assert_eq!(read(&mut stream).await.id, 1);
            if excessive {
                for _ in 0..257 {
                    if write_frame(
                        &mut stream,
                        &Frame::reply(0, KernelToUi::Presence(PresenceState::Acting)),
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
            } else {
                write_frame(&mut stream, &Frame::reply(77, status()))
                    .await
                    .unwrap();
            }
        });
        let response = rig
            .post(&Frame::request(1, UiToKernel::StatusRequest))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["error"]["issue_unknown"], true);
        assert!(frames(&body).is_empty());
        fake.await.unwrap();
    }
}

#[tokio::test]
async fn idle_session_expiry_prevents_stale_card_decision() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_millis(40)).await;
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        read(&mut stream).await;
        write_frame(&mut stream, &Frame::reply(1, status()))
            .await
            .unwrap();
        finish_barrier(&mut stream).await;
        assert!(
            tokio::time::timeout(Duration::from_secs(2), read_frame(&mut stream))
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    });
    let first: Value = rig
        .post(&Frame::request(1, UiToKernel::StatusRequest))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    let response = rig
        .post(&Frame::request(2, UiToKernel::Approve { card_id: 42 }))
        .header("x-hakille-session", first["session_id"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["issue_unknown"],
        false
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn incomplete_http_body_times_out_without_contacting_ipc() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let mut socket = tokio::net::TcpStream::connect(rig.address).await.unwrap();
    let headers = format!(
        "POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {MOBILE_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{",
        rig.address
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(2), socket.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn chunked_body_limit_cannot_be_bypassed_without_content_length() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let mut socket = tokio::net::TcpStream::connect(rig.address).await.unwrap();
    let mut request = format!("POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {MOBILE_TOKEN}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n", rig.address).into_bytes();
    request.extend(std::iter::repeat_n(b' ', (1 << 20) + 1));
    request.extend_from_slice(b"\r\n0\r\n\r\n");
    socket.write_all(&request).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::io::BufReader::new(socket).read_line(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancelled_request_future_drops_ipc_instead_of_reusing_pending_reply() {
    let (rig, listener) = setup(Duration::from_secs(3), Duration::from_secs(60)).await;
    let (started, signal) = oneshot::channel();
    let fake = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        assert_eq!(read(&mut stream).await.id, 1);
        assert_eq!(read(&mut stream).await.id, BARRIER_ID);
        started.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), read_frame(&mut stream))
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
        let mut next = accept(&listener).await;
        assert_eq!(read(&mut next).await.id, 2);
        write_frame(&mut next, &Frame::reply(2, status()))
            .await
            .unwrap();
        finish_barrier(&mut next).await;
    });
    let gateway = rig.gateway.clone();
    let running = tokio::spawn(async move {
        gateway
            .request(
                Frame::request(
                    1,
                    UiToKernel::HudSubmit {
                        query: "bonjour".into(),
                    },
                ),
                None,
            )
            .await
    });
    signal.await.unwrap();
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    let response = rig
        .post(&Frame::request(2, UiToKernel::StatusRequest))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    fake.await.unwrap();
}

#[test]
fn private_token_creation_bounds_and_redaction() {
    let directory = std::env::temp_dir().join(format!("hmg-token-{}", uuid::Uuid::new_v4()));
    let path = directory.join("mobile.token");
    let secret = BearerToken::create_file(&path).unwrap();
    let text = hakille_mobile_gateway::read_private_token(&path).unwrap();
    assert_eq!(text.len(), 64);
    assert!(!format!("{secret:?}").contains(&text));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(hakille_mobile_gateway::read_private_token(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::write(&path, "a".repeat(257)).unwrap();
    assert!(hakille_mobile_gateway::read_private_token(&path).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let link = directory.join("linked.token");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(hakille_mobile_gateway::read_private_token(&link).is_err());
        let fifo = directory.join("pipe.token");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // Le chemin CString reste valide pendant cet appel de création local.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(hakille_mobile_gateway::read_private_token(&fifo).is_err());
    }
    std::fs::remove_dir_all(directory).unwrap();
    let config = GatewayConfig {
        listen: "0.0.0.0:17840".parse().unwrap(),
        ..GatewayConfig::default()
    };
    assert!(config.validate().is_err());
}
