use crate::{Gateway, GatewayResponse, MAX_RESPONSE_BYTES};
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::body::Incoming;
use hyper::header::{ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, ORIGIN};
use hyper::{Method, Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::convert::Infallible;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

type HttpResponse = Response<BoxBody<Bytes, Infallible>>;

struct SseStream {
    rx: tokio::sync::mpsc::Receiver<Bytes>,
}

impl futures_util::Stream for SseStream {
    type Item = Result<hyper::body::Frame<Bytes>, Infallible>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(bytes)) => {
                std::task::Poll::Ready(Some(Ok(hyper::body::Frame::data(bytes))))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

fn json(status: u16, value: &impl serde::Serialize) -> HttpResponse {
    let bytes =
        serde_json::to_vec(value).unwrap_or_else(|_| b"{\"error\":\"serialization\"}".to_vec());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .header(CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(Full::new(Bytes::from(bytes)).boxed())
        .expect("en-têtes constants valides")
}

fn reject(status: u16, code: &'static str, message: &'static str) -> HttpResponse {
    json(
        status,
        &GatewayResponse::failure(code, message, false, None),
    )
}

async fn handle(
    request: Request<Incoming>,
    gateway: Arc<Gateway>,
) -> Result<HttpResponse, Infallible> {
    if request.headers().contains_key(ORIGIN) {
        return Ok(reject(
            403,
            "origin_refused",
            "clients natifs uniquement : Origin refusé",
        ));
    }
    if request.uri().query().is_some()
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
    {
        return Ok(reject(
            400,
            "bad_target",
            "cible HTTP invalide ou paramètres interdits",
        ));
    }
    if request.method() == Method::GET && request.uri().path() == "/health" {
        return Ok(json(200, &serde_json::json!({"status":"ok"})));
    }
    if request.uri().path() != "/v1/request" {
        return Ok(reject(404, "not_found", "route inconnue"));
    }
    if request.method() != Method::POST {
        return Ok(reject(405, "method_not_allowed", "POST requis"));
    }
    // Authentifier avant de lire ou de parser un corps potentiellement hostile.
    let mut authorization = request.headers().get_all(AUTHORIZATION).iter();
    let valid = authorization
        .next()
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| gateway.bearer.matches(token.as_bytes()))
        && authorization.next().is_none();
    if !valid {
        return Ok(reject(401, "unauthorized", "authentification requise"));
    }
    let content_type = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if !content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Ok(reject(
            415,
            "unsupported_media_type",
            "application/json requis",
        ));
    }
    let mut session_headers = request.headers().get_all("x-hakille-session").iter();
    let expected_session = match session_headers.next() {
        Some(header) => match header.to_str() {
            Ok(value) if value.len() <= 64 && session_headers.next().is_none() => {
                Some(value.to_owned())
            }
            _ => return Ok(reject(400, "bad_session", "en-tête de session invalide")),
        },
        None => None,
    };
    let is_sse = request
        .headers()
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("text/event-stream"));
    if request
        .headers()
        .get("content-length")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Ok(reject(413, "too_large", "corps supérieur à 1 Mio"));
    }
    let body = tokio::time::timeout(
        gateway.config.body_timeout,
        Limited::new(request.into_body(), hakille_ipc::MAX_FRAME_BYTES).collect(),
    )
    .await;
    let bytes = match body {
        Err(_) => return Ok(reject(408, "body_timeout", "lecture du corps expirée")),
        Ok(Err(_)) => {
            return Ok(reject(
                413,
                "invalid_body",
                "corps illisible ou supérieur à 1 Mio",
            ));
        }
        Ok(Ok(body)) => body.to_bytes(),
    };
    let frame = match serde_json::from_slice::<hakille_ipc::Frame>(&bytes) {
        Ok(frame) => frame,
        Err(_) => return Ok(reject(400, "bad_frame", "trame JSON invalide")),
    };

    if is_sse {
        match gateway
            .request_stream(frame, expected_session.as_deref())
            .await
        {
            Ok((session_id, rx)) => {
                let response = Response::builder()
                    .status(200)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .header(CACHE_CONTROL, "no-cache")
                    .header("x-content-type-options", "nosniff")
                    .header("x-hakille-session", session_id)
                    .body(StreamBody::new(SseStream { rx }).boxed())
                    .expect("en-têtes constants valides");
                Ok(response)
            }
            Err((status, err_resp)) => Ok(json(status, &err_resp)),
        }
    } else {
        let (status, response) = gateway.request(frame, expected_session.as_deref()).await;
        Ok(json(status, &response))
    }
}

/// Serveur HTTP/1 local : au plus 32 connexions, sans keep-alive, délais bornés.
/// Le shutdown ferme aussi la session IPC et invalide les cartes du client.
pub async fn serve(
    listener: TcpListener,
    gateway: Arc<Gateway>,
    shutdown: impl std::future::Future<Output = ()>,
) -> std::io::Result<()> {
    if listener.local_addr()?.ip() != Ipv4Addr::LOCALHOST {
        return Err(std::io::Error::other("listener non local refusé"));
    }
    let slots = Arc::new(Semaphore::new(32));
    let mut tasks = JoinSet::new();
    let mut maintenance = tokio::time::interval(Duration::from_secs(1));
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = maintenance.tick() => gateway.expire_idle_session(),
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let Ok(slot) = slots.clone().try_acquire_owned() else { continue; };
                let gateway = gateway.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let maximum = gateway.config.request_timeout + gateway.config.body_timeout + Duration::from_secs(25);
                    let service = hyper::service::service_fn(move |request| handle(request, gateway.clone()));
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder.keep_alive(false).max_headers(32).max_buf_size(16 * 1024)
                        .timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(10));
                    // Le timeout global borne aussi un client qui ne lit jamais
                    // sa réponse. Les erreurs HTTP ne contiennent aucun jeton.
                    let _ = tokio::time::timeout(maximum, builder.serve_connection(TokioIo::new(stream), service)).await;
                });
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    gateway.close_session().await;
    Ok(())
}
