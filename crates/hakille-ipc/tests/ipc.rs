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

fn wire(frame: &Frame) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(frame).expect("JSON");
    bytes.push(b'\n');
    bytes
}

#[tokio::test]
async fn coalesced_frames_are_preserved_in_order() {
    let frames = [
        Frame::hello(TOKEN_A),
        Frame::welcome("prêt"),
        Frame::request(9, UiToKernel::StatusRequest),
    ];
    let bytes: Vec<u8> = frames.iter().flat_map(wire).collect();
    let mut reader = bytes.as_slice();
    for expected in frames {
        assert_eq!(read_frame(&mut reader).await.unwrap(), Some(expected));
    }
    assert!(read_frame(&mut reader).await.unwrap().is_none());
}

#[tokio::test]
async fn fragmented_utf8_and_multiple_frames_survive_small_buffers() {
    let frames = vec![
        Frame::welcome("État : prêt 🦀"),
        Frame::request(27, UiToKernel::StatusRequest),
    ];
    let bytes: Vec<u8> = frames.iter().flat_map(wire).collect();
    let (mut writer, stream) = tokio::io::duplex(17);
    let sender = tokio::spawn(async move {
        for chunk in bytes.chunks(3) {
            writer.write_all(chunk).await.unwrap();
            tokio::task::yield_now().await;
        }
    });
    let mut reader = tokio::io::BufReader::with_capacity(7, stream);
    for expected in frames {
        assert_eq!(read_frame(&mut reader).await.unwrap(), Some(expected));
    }
    assert!(read_frame(&mut reader).await.unwrap().is_none());
    sender.await.unwrap();
}

#[tokio::test]
async fn handshake_preserves_pipelined_request() {
    let (mut client, server) = tokio::io::duplex(4096);
    let request = Frame::request(42, UiToKernel::StatusRequest);
    let mut bytes = wire(&Frame::hello(TOKEN_A));
    bytes.extend(wire(&request));
    client.write_all(&bytes).await.unwrap();
    let mut server = tokio::io::BufReader::new(server);
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server_handshake(&mut server, TOKEN_A),
    )
    .await
    .unwrap()
    .unwrap();
    let received = tokio::time::timeout(std::time::Duration::from_secs(2), read_frame(&mut server))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, Some(request));
}

#[tokio::test]
async fn client_handshake_preserves_coalesced_reply() {
    let (client, mut server) = tokio::io::duplex(4096);
    let reply = Frame::error(3, ErrorCode::Busy, "occupé");
    let mut bytes = wire(&Frame::welcome("prêt"));
    bytes.extend(wire(&reply));
    server.write_all(&bytes).await.unwrap();
    let mut client = tokio::io::BufReader::new(client);
    client_handshake(&mut client, TOKEN_A).await.unwrap();
    let received = tokio::time::timeout(std::time::Duration::from_secs(2), read_frame(&mut client))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, Some(reply));
}

#[tokio::test]
async fn eof_requires_a_complete_delimited_frame() {
    let frame = Frame::welcome("prêt");
    let bytes = serde_json::to_vec(&frame).unwrap();
    let mut reader = bytes.as_slice();
    assert!(
        matches!(read_frame(&mut reader).await, Err(IpcError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    let mut empty = &b""[..];
    assert!(read_frame(&mut empty).await.unwrap().is_none());
}

#[tokio::test]
async fn frame_size_limit_is_exact_and_leaves_next_frame() {
    let base = Frame::error(1, ErrorCode::Internal, "");
    let overhead = serde_json::to_vec(&base).unwrap().len();
    let frame = Frame::error(
        1,
        ErrorCode::Internal,
        "x".repeat(MAX_FRAME_BYTES - overhead),
    );
    let next = Frame::welcome("suite");
    let mut bytes = wire(&frame);
    bytes.extend(wire(&next));
    let mut reader = tokio::io::BufReader::with_capacity(13, bytes.as_slice());
    assert_eq!(read_frame(&mut reader).await.unwrap(), Some(frame));
    assert_eq!(read_frame(&mut reader).await.unwrap(), Some(next));
    let oversized = Frame::error(
        1,
        ErrorCode::Internal,
        "x".repeat(MAX_FRAME_BYTES - overhead + 1),
    );
    let bytes = wire(&oversized);
    let mut reader = bytes.as_slice();
    assert!(matches!(
        read_frame(&mut reader).await,
        Err(IpcError::TooLarge)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn bind_never_removes_existing_files_or_sockets() {
    let endpoint = unique_endpoint("occupied");
    std::fs::write(&endpoint, b"fichier a conserver").unwrap();
    assert!(IpcListener::bind(&endpoint).await.is_err());
    assert_eq!(std::fs::read(&endpoint).unwrap(), b"fichier a conserver");
    std::fs::remove_file(&endpoint).unwrap();
    let listener = IpcListener::bind(&endpoint).await.unwrap();
    assert!(IpcListener::bind(&endpoint).await.is_err());
    let client = connect(&endpoint).await.unwrap();
    let accepted = tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    drop((client, accepted, listener));
    // Même un socket périmé nécessite une décision explicite de nettoyage.
    assert!(IpcListener::bind(&endpoint).await.is_err());
    assert!(std::fs::symlink_metadata(&endpoint).is_ok());
    cleanup_endpoint(&endpoint);
}

#[cfg(unix)]
#[test]
fn token_publication_does_not_follow_destination_symlink() {
    let dir = unique_temp_dir("token-symlink");
    std::fs::create_dir_all(&dir).unwrap();
    let victim = dir.join("unrelated");
    std::fs::write(&victim, b"a conserver").unwrap();
    let token = dir.join("ipc.token");
    std::os::unix::fs::symlink(&victim, &token).unwrap();
    write_token_file(&token, TOKEN_A).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"a conserver");
    assert!(
        !std::fs::symlink_metadata(&token)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read_token_file(&token).unwrap(), TOKEN_A);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_token_publication_cleans_temporary_file() {
    let dir = unique_temp_dir("token-failure");
    let destination = dir.join("directory");
    std::fs::create_dir_all(&destination).unwrap();
    assert!(write_token_file(&destination, TOKEN_A).is_err());
    assert!(destination.is_dir());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
