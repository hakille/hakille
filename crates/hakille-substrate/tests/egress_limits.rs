//! Régressions du transport commun aux six substrats ; serveurs locaux réels.
use hakille_substrate::{EGRESS_MAX_BODY_BYTES, EgressPolicy, HttpRequest, fetch_via_egress};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn request(url: String) -> HttpRequest {
    HttpRequest {
        method: "GET".into(),
        url,
        headers: HashMap::new(),
        body: Vec::new(),
    }
}

async fn read_headers(stream: &mut tokio::net::TcpStream) {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let byte = stream.read_u8().await.expect("requête HTTP locale");
        bytes.push(byte);
        assert!(bytes.len() < 8192);
    }
}

#[tokio::test]
async fn redirect_to_unlisted_host_is_not_contacted() {
    let source = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_addr = source.local_addr().unwrap();
    let target_port = target.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = source.accept().await.unwrap();
        read_headers(&mut stream).await;
        stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://localhost:{target_port}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    });
    let result = fetch_via_egress(
        request(format!("http://{source_addr}/")),
        &EgressPolicy::with_allowlist(["127.0.0.1"]),
    )
    .await;
    server.await.unwrap();
    assert!(result.is_err(), "redirection hors liste refusée");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), target.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn allowed_redirect_returns_real_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for response in [
            "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nbonjour",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_headers(&mut stream).await;
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let response = fetch_via_egress(
        request(format!("http://{addr}/")),
        &EgressPolicy::loopback_only(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body, b"bonjour");
}

#[tokio::test]
async fn oversized_chunked_body_is_rejected_before_end_of_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_headers(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..=EGRESS_MAX_BODY_BYTES / chunk.len() {
            if stream.write_all(b"10000\r\n").await.is_err() {
                return;
            }
            if stream.write_all(&chunk).await.is_err() {
                return;
            }
            if stream.write_all(b"\r\n").await.is_err() {
                return;
            }
        }
        // Pas de chunk terminal : attendre la fin du flux serait un défaut.
        std::future::pending::<()>().await;
    });
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        fetch_via_egress(
            request(format!("http://{addr}/")),
            &EgressPolicy::loopback_only(),
        ),
    )
    .await;
    server.abort();
    let error = result
        .expect("refus avant fin du flux / timeout global")
        .expect_err("corps trop grand");
    assert!(error.to_string().contains("5MiB"));
}

#[test]
fn ipv6_allowlist_uses_canonical_addresses() {
    let policy = EgressPolicy::with_allowlist(["[::1]:8080"]);
    assert!(policy.check_url("http://[0:0:0:0:0:0:0:1]:8080/").is_ok());
    assert!(!policy.is_host_allowed("[::2]"));
}

#[test]
fn credentials_and_invalid_urls_do_not_leak_secrets_in_errors() {
    let policy = EgressPolicy::loopback_only();
    for url in [
        "http://user:secret@127.0.0.1/",
        "http://[secret",
        "http://127.0.0.1:secret/",
    ] {
        let error = policy.check_url(url).unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
    }
}

#[tokio::test]
async fn fetch_errors_do_not_include_query_secrets() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let error = fetch_via_egress(
        request(format!("http://{addr}/?token=secret")),
        &EgressPolicy::loopback_only(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(!error.contains("secret"), "{error}");
}

#[test]
fn denied_destination_does_not_leak_allowlist_credentials() {
    let policy = EgressPolicy::with_allowlist(["https://user:secret@example.com"]);
    let error = policy
        .check_url("https://other.example/")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("secret"), "{error}");
}

#[tokio::test]
async fn response_at_exact_limit_is_preserved() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_headers(&mut stream).await;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    EGRESS_MAX_BODY_BYTES
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        stream
            .write_all(&vec![b'x'; EGRESS_MAX_BODY_BYTES])
            .await
            .unwrap();
    });
    let response = fetch_via_egress(
        request(format!("http://{addr}/")),
        &EgressPolicy::loopback_only(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(response.body.len(), EGRESS_MAX_BODY_BYTES);
    assert!(response.body.iter().all(|&byte| byte == b'x'));
}
