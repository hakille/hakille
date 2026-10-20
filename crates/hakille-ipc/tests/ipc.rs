//! Tests du contrat IPC : sérialisation, plafond, handshake, aller-retour réel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hakille_ipc::transport::{
    IpcListener, client_handshake, connect, fresh_token_hex, read_frame, read_token_file,
    server_handshake, write_frame, write_token_file,
};
use hakille_ipc::{ErrorCode, Frame, FrameBody, IpcError, MAX_FRAME_BYTES};
use hakille_ui_protocol::{KernelToUi, PROTOCOL_VERSION, UiToKernel};
use tokio::io::AsyncWriteExt;

const TOKEN_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TOKEN_B: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

fn unique_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn unique_endpoint(tag: &str) -> String {
    let id = unique_id();
    let pid = std::process::id();
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\hakille-ipc-test-{tag}-{pid}-{id}")
    }
    #[cfg(unix)]
    {
        format!("/tmp/hakille-ipc-test-{tag}-{pid}-{id}.sock")
    }
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "hakille-ipc-{tag}-{}-{}",
        std::process::id(),
        unique_id()
    ))
}

fn cleanup_endpoint(endpoint: &str) {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(endpoint);
    }
    #[cfg(not(unix))]
    {
        let _ = endpoint;
    }
}

#[tokio::test]
async fn frame_roundtrip_serde() {
    let mut cases = vec![
        Frame::hello(TOKEN_A),
        Frame::welcome("hakille-daemon v0.1.0"),
        Frame::request(
            7,
            UiToKernel::HudSubmit {
                query: "résume ma journée".to_string(),
            },
        ),
        Frame::reply(
            7,
            KernelToUi::HudReply {
                response: "Journée calme.".to_string(),
                validation: Some(("Envoi 25.00 EUR".to_string(), 2500)),
            },
        ),
        Frame::reply(
            8,
            KernelToUi::HudReply {
                response: String::new(),
                validation: None,
            },
        ),
    ];
    for code in [
        ErrorCode::BadVersion,
        ErrorCode::BadToken,
        ErrorCode::BadFrame,
        ErrorCode::TooLarge,
        ErrorCode::Busy,
        ErrorCode::Unsupported,
        ErrorCode::Internal,
    ] {
        cases.push(Frame::error(9, code, "détail"));
    }

    for frame in cases {
        assert_eq!(frame.version, PROTOCOL_VERSION);
        assert!(frame.check().is_ok());
        let json = serde_json::to_string(&frame).expect("sérialisation");
        assert!(!json.contains('\n'), "JSON mono-ligne attendu: {json}");
        let back: Frame = serde_json::from_str(&json).expect("parsing");
        assert_eq!(back, frame);
        assert!(back.check().is_ok());
    }
}

#[tokio::test]
async fn consultation_frames_roundtrip_serde() {
    use hakille_ui_protocol::{JournalEntryDto, MemoryRowDto, StatusDto};

    let mut cases = vec![
        Frame::request(21, UiToKernel::JournalRequest { limit: 20 }),
        Frame::request(
            22,
            UiToKernel::MemoryRequest {
                query: None,
                limit: 10,
            },
        ),
        Frame::request(
            23,
            UiToKernel::MemoryRequest {
                query: Some("jazz".to_string()),
                limit: 5,
            },
        ),
        Frame::request(
            24,
            UiToKernel::MemoryRemember {
                text: "Marie aime le jazz".to_string(),
            },
        ),
        Frame::request(25, UiToKernel::StatusRequest),
        Frame::reply(
            26,
            KernelToUi::JournalReply {
                entries: vec![JournalEntryDto {
                    id: "018f0000-0000-7000-8000-000000000001".to_string(),
                    timestamp: "2026-10-19T09:00:00+00:00".to_string(),
                    action: "CognitiveTurn".to_string(),
                    summary: "tour servi".to_string(),
                }],
            },
        ),
        Frame::reply(
            27,
            KernelToUi::MemoryReply {
                rows: vec![MemoryRowDto {
                    id: "018f0000-0000-7000-8000-000000000002".to_string(),
                    text: "Marie aime le jazz".to_string(),
                    tags: vec!["Personal".to_string(), "User".to_string()],
                    source: "User".to_string(),
                }],
            },
        ),
        Frame::reply(
            28,
            KernelToUi::MemoryRememberAck {
                id: "018f0000-0000-7000-8000-000000000003".to_string(),
            },
        ),
        Frame::reply(
            29,
            KernelToUi::StatusReply {
                status: StatusDto {
                    model: "Qwen 0.5B Q4_K_M (timeout 30000 ms)".to_string(),
                    cost: "0.00 EUR".to_string(),
                    state: "Idle".to_string(),
                    memory_count: 3,
                },
            },
        ),
    ];

    for frame in cases.drain(..) {
        assert_eq!(frame.version, PROTOCOL_VERSION);
        assert!(frame.check().is_ok());
        let json = serde_json::to_string(&frame).expect("sérialisation");
        assert!(!json.contains('\n'), "JSON mono-ligne attendu: {json}");
        let back: Frame = serde_json::from_str(&json).expect("parsing");
        assert_eq!(back, frame);
        assert!(back.check().is_ok());
    }
}

#[tokio::test]
async fn frame_check_rejects_bad_version() {
    let mut frame = Frame::hello(TOKEN_A);
    frame.version = PROTOCOL_VERSION + 1;
    assert!(matches!(
        frame.check(),
        Err(IpcError::BadVersion(v)) if v == PROTOCOL_VERSION + 1
    ));
}

#[tokio::test]
async fn wire_roundtrip_over_listener() {
    let endpoint = unique_endpoint("roundtrip");
    let listener = IpcListener::bind(&endpoint).await.expect("bind");
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.expect("accept");
        server_handshake(&mut stream, TOKEN_A)
            .await
            .expect("handshake serveur");
        let request = read_frame(&mut stream)
            .await
            .expect("lecture requête")
            .expect("trame requête");
        assert_eq!(request.version, PROTOCOL_VERSION);
        match &request.body {
            FrameBody::Request { message } => {
                assert_eq!(
                    message,
                    &UiToKernel::HudSubmit {
                        query: "état du système".to_string(),
                    }
                );
            }
            other => panic!("trame inattendue: {other:?}"),
        }
        let reply = Frame::reply(
            request.id,
            KernelToUi::HudReply {
                response: "tout va bien".to_string(),
                validation: None,
            },
        );
        write_frame(&mut stream, &reply)
            .await
            .expect("écriture réponse");
    });

    let mut client = connect(&endpoint).await.expect("connect");
    client_handshake(&mut client, TOKEN_A)
        .await
        .expect("handshake client");
    let request = Frame::request(
        7,
        UiToKernel::HudSubmit {
            query: "état du système".to_string(),
        },
    );
    write_frame(&mut client, &request)
        .await
        .expect("écriture requête");
    let reply = read_frame(&mut client)
        .await
        .expect("lecture réponse")
        .expect("trame réponse");
    assert_eq!(reply.id, 7);
    assert_eq!(reply.version, PROTOCOL_VERSION);
    match &reply.body {
        FrameBody::Reply { message } => {
            assert_eq!(
                message,
                &KernelToUi::HudReply {
                    response: "tout va bien".to_string(),
                    validation: None,
                }
            );
        }
        other => panic!("trame inattendue: {other:?}"),
    }

    server.await.expect("tâche serveur");
    cleanup_endpoint(&endpoint);
}

#[tokio::test]
async fn bad_token_is_rejected() {
    let endpoint = unique_endpoint("badtoken");
    let listener = IpcListener::bind(&endpoint).await.expect("bind");
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.expect("accept");
        let refused = server_handshake(&mut stream, TOKEN_A).await;
        assert!(
            matches!(refused, Err(IpcError::Handshake(_))),
            "le serveur doit refuser: {refused:?}"
        );
    });

    let mut client = connect(&endpoint).await.expect("connect");
    let denied = client_handshake(&mut client, TOKEN_B).await;
    assert!(
        matches!(denied, Err(IpcError::Handshake(_))),
        "le client doit être refusé: {denied:?}"
    );

    server.await.expect("tâche serveur");
    cleanup_endpoint(&endpoint);
}

#[tokio::test]
async fn read_frame_rejects_oversized_line() {
    let endpoint = unique_endpoint("oversize");
    let listener = IpcListener::bind(&endpoint).await.expect("bind");
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.expect("accept");
        let oversized = read_frame(&mut stream).await;
        assert!(
            matches!(oversized, Err(IpcError::TooLarge)),
            "trame hors plafond attendue: {oversized:?}"
        );
    });

    let mut client = connect(&endpoint).await.expect("connect");
    let blob = vec![b'x'; MAX_FRAME_BYTES + 1];
    client.write_all(&blob).await.expect("écriture brute");
    client.flush().await.expect("flush");

    server.await.expect("tâche serveur");
    cleanup_endpoint(&endpoint);
}

#[tokio::test]
async fn token_file_roundtrip() {
    let dir = unique_temp_dir("token");
    let path = dir.join("nested").join("ipc.token");
    write_token_file(&path, TOKEN_A).expect("écriture jeton");
    assert_eq!(read_token_file(&path).expect("lecture jeton"), TOKEN_A);

    write_token_file(&path, "  jeton-avec-blancs\n").expect("réécriture jeton");
    assert_eq!(
        read_token_file(&path).expect("lecture trim"),
        "jeton-avec-blancs"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "permissions du jeton");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn fresh_token_is_64_lowercase_hex() {
    let first = fresh_token_hex();
    let second = fresh_token_hex();
    for token in [&first, &second] {
        assert_eq!(token.len(), 64, "64 caractères attendus: {token}");
        assert!(
            token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "hex minuscules attendu: {token}"
        );
    }
    assert_ne!(first, second, "deux jetons ne doivent pas coïncider");
}
