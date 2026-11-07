//! Inférence visuelle UI-TARS réelle via un `llama-server` local configuré
//! avec le GGUF Qwen2-VL et son projecteur. Ce module ne lance aucun effet GUI.
//! Le serveur est un composant de confiance explicitement provisionné : le nom
//! de modèle renvoyé ne prouve pas à lui seul l'identité de ses poids.

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Cursor, net::IpAddr, time::Duration};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MODEL_ALIAS: &str = "ui-tars-2b-sft";

#[derive(Debug, thiserror::Error)]
pub enum UiTarsLocalError {
    #[error("Endpoint UI-TARS invalide : HTTP avec IP littérale loopback et port explicite requis")]
    Endpoint,
    #[error("Image UI-TARS invalide : PNG/JPEG valide, au plus 8 Mio et 2048 × 2048 requis")]
    Image,
    #[error("Instruction UI-TARS vide ou supérieure à 8192 octets")]
    Task,
    #[error("Serveur UI-TARS local inaccessible ou délai de 600 secondes dépassé")]
    Transport,
    #[error("Réponse UI-TARS locale refusée (statut HTTP {0})")]
    Http(u16),
    #[error("Réponse UI-TARS trop volumineuse")]
    ResponseTooLarge,
    #[error("Réponse UI-TARS invalide, tronquée ou action hors limites")]
    Response,
}

/// Coordonnées normalisées entières dans [0, 1000], indépendantes de la
/// résolution. Une proposition valide n'est jamais une autorisation d'effet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiTarsAction {
    Click {
        x: u16,
        y: u16,
    },
    DoubleClick {
        x: u16,
        y: u16,
    },
    RightClick {
        x: u16,
        y: u16,
    },
    Type {
        text: String,
    },
    Hotkey {
        keys: String,
    },
    Scroll {
        direction: UiTarsScrollDirection,
    },
    Drag {
        x: u16,
        y: u16,
        to_x: u16,
        to_y: u16,
    },
    Wait {},
    Finished {
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTarsScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiTarsProposal {
    pub action: UiTarsAction,
    pub image_width: u32,
    pub image_height: u32,
    pub generated_text: String,
}

pub struct UiTarsLocalClient {
    client: reqwest::Client,
    endpoint: reqwest::Url,
}

impl UiTarsLocalClient {
    /// Accepte une origine comme `http://127.0.0.1:8080` ou `[::1]`.
    /// Aucun proxy, DNS, redirect, secret URL ou endpoint distant n'est admis.
    pub fn new(endpoint: &str) -> Result<Self, UiTarsLocalError> {
        let endpoint = checked_endpoint(endpoint)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|_| UiTarsLocalError::Transport)?;
        Ok(Self { client, endpoint })
    }

    /// Décode une image bornée, demande au VLM une seule proposition structurée,
    /// puis valide sa syntaxe et ses bornes. Aucun repli vers la vision classique.
    pub async fn ground(
        &self,
        image_bytes: &[u8],
        task: &str,
    ) -> Result<UiTarsProposal, UiTarsLocalError> {
        if task.trim().is_empty() || task.len() > 8192 {
            return Err(UiTarsLocalError::Task);
        }
        let (image_width, image_height, mime) = validate_image(image_bytes)?;
        let payload = json!({
            "model": MODEL_ALIAS,
            "messages": [
                {"role":"system", "content":"You are UI-TARS, a GUI agent. Inspect the screenshot and propose exactly one next action for the user's task. Return only the JSON object required by the schema. Coordinates x,y,to_x,to_y are integers normalized to 0..1000 across the screenshot, not pixels. Use finished only if the task is already complete, wait if no action can be grounded. You only propose actions; never claim that an action has been executed."},
                {"role":"user", "content":[
                    {"type":"image_url","image_url":{"url":format!("data:{mime};base64,{}", STANDARD.encode(image_bytes))}},
                    {"type":"text","text":task}
                ]}
            ],
            "temperature":0.0, "max_tokens":256, "stream":false,
            "response_format":{"type":"json_object", "schema": action_schema()}
        });
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .json(&payload)
            .send()
            .await
            .map_err(|_| UiTarsLocalError::Transport)?;
        if !response.status().is_success() {
            return Err(UiTarsLocalError::Http(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(UiTarsLocalError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| UiTarsLocalError::Transport)?
        {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                return Err(UiTarsLocalError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let (action, generated_text) = parse_response(&bytes)?;
        Ok(UiTarsProposal {
            action,
            image_width,
            image_height,
            generated_text,
        })
    }
}

fn checked_endpoint(value: &str) -> Result<reqwest::Url, UiTarsLocalError> {
    let mut url = reqwest::Url::parse(value).map_err(|_| UiTarsLocalError::Endpoint)?;
    let host = url.host_str().ok_or(UiTarsLocalError::Endpoint)?;
    let ip: IpAddr = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .map_err(|_| UiTarsLocalError::Endpoint)?;
    if url.scheme() != "http"
        || !ip.is_loopback()
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.path(),
            "" | "/" | "/v1" | "/v1/" | "/v1/chat/completions"
        )
    {
        return Err(UiTarsLocalError::Endpoint);
    }
    url.set_path("/v1/chat/completions");
    Ok(url)
}

fn validate_image(bytes: &[u8]) -> Result<(u32, u32, &'static str), UiTarsLocalError> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err(UiTarsLocalError::Image);
    }
    let format = image::guess_format(bytes).map_err(|_| UiTarsLocalError::Image)?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(UiTarsLocalError::Image),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(64 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| UiTarsLocalError::Image)?;
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return Err(UiTarsLocalError::Image);
    }
    Ok((width, height, mime))
}

fn action_schema() -> Value {
    let point = json!({"type":"integer","minimum":0,"maximum":1000});
    let mut variants = Vec::new();
    for name in ["click", "double_click", "right_click", "drag"] {
        let mut properties = json!({"action":{"const":name},"x":point,"y":point});
        let mut required = vec!["action", "x", "y"];
        if name == "drag" {
            properties["to_x"] = point.clone();
            properties["to_y"] = point.clone();
            required.extend(["to_x", "to_y"]);
        }
        variants.push(json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}));
    }
    for (name, field, max) in [
        ("type", "text", 4096),
        ("hotkey", "keys", 128),
        ("finished", "summary", 4096),
    ] {
        variants.push(json!({"type":"object","properties":{"action":{"const":name},field:{"type":"string","maxLength":max}},"required":["action",field],"additionalProperties":false}));
    }
    variants.push(json!({"type":"object","properties":{"action":{"const":"scroll"},"direction":{"enum":["up","down","left","right"]}},"required":["action","direction"],"additionalProperties":false}));
    variants.push(json!({"type":"object","properties":{"action":{"const":"wait"}},"required":["action"],"additionalProperties":false}));
    json!({"oneOf":variants})
}

fn parse_response(bytes: &[u8]) -> Result<(UiTarsAction, String), UiTarsLocalError> {
    let response: Value = serde_json::from_slice(bytes).map_err(|_| UiTarsLocalError::Response)?;
    if response["model"] != MODEL_ALIAS
        || response["choices"].as_array().map(Vec::len) != Some(1)
        || response["choices"][0]["finish_reason"] != "stop"
    {
        return Err(UiTarsLocalError::Response);
    }
    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(UiTarsLocalError::Response)?;
    if content.len() > 16 * 1024 {
        return Err(UiTarsLocalError::Response);
    }
    let action: UiTarsAction =
        serde_json::from_str(content).map_err(|_| UiTarsLocalError::Response)?;
    let valid = match &action {
        UiTarsAction::Click { x, y }
        | UiTarsAction::DoubleClick { x, y }
        | UiTarsAction::RightClick { x, y } => *x <= 1000 && *y <= 1000,
        UiTarsAction::Drag { x, y, to_x, to_y } => {
            [x, y, to_x, to_y].into_iter().all(|v| *v <= 1000)
        }
        UiTarsAction::Type { text } => {
            !text.is_empty() && text.len() <= 4096 && !text.contains('\0')
        }
        UiTarsAction::Hotkey { keys } => {
            !keys.is_empty()
                && keys.len() <= 128
                && keys
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_+ -".contains(&c))
        }
        UiTarsAction::Finished { summary } => {
            !summary.is_empty() && summary.len() <= 4096 && !summary.contains('\0')
        }
        UiTarsAction::Scroll { .. } | UiTarsAction::Wait { .. } => true,
    };
    if !valid {
        return Err(UiTarsLocalError::Response);
    }
    Ok((action, content.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn screenshot() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(32, 24)
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    fn response(content: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"model":MODEL_ALIAS,"choices":[{"finish_reason":"stop","message":{"content":content}}]})).unwrap()
    }

    #[test]
    fn ui_tars_endpoint_only_literal_loopback() {
        for endpoint in [
            "http://127.0.0.1:8080",
            "http://[::1]:8080/v1",
            "http://127.0.0.2:1234/v1/chat/completions",
        ] {
            assert_eq!(
                checked_endpoint(endpoint).unwrap().path(),
                "/v1/chat/completions"
            );
        }
        for endpoint in [
            "https://127.0.0.1:8080",
            "http://localhost:8080",
            "http://example.com:8080",
            "http://0.0.0.0:8080",
            "http://192.168.1.1:8080",
            "http://127.0.0.1",
            "http://user:secret@127.0.0.1:8080",
            "http://127.0.0.1:8080/?secret=1",
            "http://127.0.0.1:8080/#fragment",
            "http://127.0.0.1:8080/arbitrary",
        ] {
            assert!(checked_endpoint(endpoint).is_err(), "{endpoint}");
        }
    }

    #[test]
    fn ui_tars_image_decode_and_resource_limits() {
        assert_eq!(
            validate_image(&screenshot()).unwrap(),
            (32, 24, "image/png")
        );
        assert!(validate_image(b"\x89PNG\r\n\x1a\ntruncated").is_err());
        assert!(validate_image(&vec![0; MAX_IMAGE_BYTES + 1]).is_err());
        let mut oversized = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2049, 1)
            .write_to(&mut oversized, ImageFormat::Png)
            .unwrap();
        assert!(validate_image(&oversized.into_inner()).is_err());
    }

    #[test]
    fn ui_tars_actions_are_strict_and_bounded() {
        let (action, _) =
            parse_response(&response(r#"{"action":"click","x":1000,"y":0}"#)).unwrap();
        assert_eq!(action, UiTarsAction::Click { x: 1000, y: 0 });
        for content in [
            r#"{"action":"click","x":1001,"y":0}"#,
            r#"{"action":"click","x":-1,"y":0}"#,
            r#"{"action":"click","x":0.5,"y":0}"#,
            r#"{"action":"wait","execute":true}"#,
            r#"{"action":"run_shell","command":"echo test"}"#,
            r#"{"action":"hotkey","keys":"$(echo test)"}"#,
            r#"{"action":"type","text":""}"#,
            r#"{"action":"drag","x":0,"y":0,"to_x":65535,"to_y":0}"#,
            r#"{"action":"wait"}{"action":"wait"}"#,
        ] {
            assert!(parse_response(&response(content)).is_err(), "{content}");
        }
    }

    #[test]
    fn ui_tars_rejects_wrong_model_and_truncated_completion() {
        let mut value: Value = serde_json::from_slice(&response(r#"{"action":"wait"}"#)).unwrap();
        value["choices"][0]["finish_reason"] = json!("length");
        assert!(parse_response(&serde_json::to_vec(&value).unwrap()).is_err());
        value["choices"][0]["finish_reason"] = json!("stop");
        value["model"] = json!("another-model");
        assert!(parse_response(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    async fn serve_once(
        status: &str,
        extra_headers: &str,
        body: Vec<u8>,
    ) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let headers = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{extra_headers}Connection: close\r\n\r\n",
            body.len()
        );
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let len: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let _ = stream.write_all(headers.as_bytes()).await;
            let _ = stream.write_all(&body).await;
            request
        });
        (endpoint, handle)
    }

    #[tokio::test]
    async fn ui_tars_local_transport_sends_real_image_and_schema() {
        let (endpoint, server) = serve_once(
            "200 OK",
            "",
            response(r#"{"action":"click","x":500,"y":400}"#),
        )
        .await;
        let proposal = UiTarsLocalClient::new(&endpoint)
            .unwrap()
            .ground(&screenshot(), "Click the button")
            .await
            .unwrap();
        assert_eq!(proposal.image_width, 32);
        assert_eq!(proposal.action, UiTarsAction::Click { x: 500, y: 400 });
        let request = server.await.unwrap();
        let end = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let payload: Value = serde_json::from_slice(&request[end + 4..]).unwrap();
        assert_eq!(payload["model"], MODEL_ALIAS);
        assert!(
            payload["messages"][1]["content"][0]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(
            payload["response_format"]["schema"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
    }

    #[tokio::test]
    async fn ui_tars_redirects_and_large_response_fail_closed() {
        let (endpoint, server) =
            serve_once("302 Found", "Location: http://192.0.2.1/\r\n", Vec::new()).await;
        assert!(matches!(
            UiTarsLocalClient::new(&endpoint)
                .unwrap()
                .ground(&screenshot(), "test")
                .await,
            Err(UiTarsLocalError::Http(302))
        ));
        server.await.unwrap();
        let (endpoint, server) = serve_once("200 OK", "", vec![b'x'; MAX_RESPONSE_BYTES + 1]).await;
        assert!(matches!(
            UiTarsLocalClient::new(&endpoint)
                .unwrap()
                .ground(&screenshot(), "test")
                .await,
            Err(UiTarsLocalError::ResponseTooLarge)
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn ui_tars_streamed_response_limit_without_content_length() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut part = [0; 4096];
                let count = stream.read(&mut part).await.unwrap();
                assert!(count > 0 && request.len() < 16384);
                request.extend_from_slice(&part[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let size: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            let body = vec![b'x'; MAX_RESPONSE_BYTES + 1];
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            stream
                .write_all(format!("{:x}\r\n", body.len()).as_bytes())
                .await
                .unwrap();
            let _ = stream.write_all(&body).await;
            let _ = stream.write_all(b"\r\n0\r\n\r\n").await;
        });
        assert!(matches!(
            UiTarsLocalClient::new(&endpoint)
                .unwrap()
                .ground(&screenshot(), "test")
                .await,
            Err(UiTarsLocalError::ResponseTooLarge)
        ));
        server.await.unwrap();
    }
}
