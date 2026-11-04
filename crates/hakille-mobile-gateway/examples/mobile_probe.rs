//! Smoke de consultation contre un hub explicitement choisi et déjà démarré.
//! Aucun démarrage de démon, écriture mémoire, chat ou exécution d'agent.
use hakille_ipc::{Frame, FrameBody};
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::{MAX_RESPONSE_BYTES, read_private_token};
use hakille_ui_protocol::UiToKernel;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn validate_hub(raw: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(raw).map_err(|_| "URL de hub invalide")?;
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https"
            || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
    {
        return Err(
            "hub HTTPS requis, sauf HTTP explicite sur 127.0.0.1 ; aucun credential/query/fragment dans l'URL",
        );
    }
    Ok(url)
}

fn read_only(frame: &Frame) -> bool {
    matches!(
        &frame.body,
        FrameBody::Request {
            message: UiToKernel::StatusRequest
                | UiToKernel::MemoryRequest { .. }
                | UiToKernel::JournalRequest { .. }
                | UiToKernel::AgentListRequest
        }
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut hub = None;
    let mut token_file = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if matches!(argument.as_str(), "--help" | "-h") {
            println!(
                "mobile_probe --hub URL --token-file FICHIER_PRIVE\nConsultations seules : statut, mémoire, journal, agents. Aucun démon démarré.\nLe rapport ne contient ni jeton ni contenu mémoire/journal."
            );
            return Ok(());
        }
        let value = args.next().ok_or("valeur d'option manquante")?;
        match argument.as_str() {
            "--hub" => hub = Some(validate_hub(&value)?),
            "--token-file" => token_file = Some(PathBuf::from(value)),
            _ => return Err("option inconnue ; utiliser --help".into()),
        }
    }
    let hub = hub.ok_or("--hub explicite requis")?;
    let token = read_private_token(&token_file.ok_or("--token-file explicite requis")?)?;
    let mut endpoint = hub.clone();
    endpoint.set_path(&format!("{}/v1/request", hub.path().trim_end_matches('/')));
    let mut client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(700));
    if hub.scheme() == "http" {
        // Un accès localhost ne doit pas transmettre son bearer à un proxy.
        client = client.no_proxy();
    }
    let client = client.build().map_err(|_| "client HTTP indisponible")?;
    let bridge = MobileBridge::new();
    let setup: Value = serde_json::from_str(
        &bridge.dispatch_json(json!({"type":"set_hub","hub_id":hub.as_str()}).to_string()),
    )?;
    if setup["ok"] != true {
        return Err("hub refusé par la façade mobile".into());
    }
    let mut checks = Vec::new();
    for action in [
        json!({"type":"refresh_status"}),
        json!({"type":"load_memory","query":null,"limit":10}),
        json!({"type":"load_journal","limit":10}),
        json!({"type":"load_agents"}),
    ] {
        let dispatched: Value = serde_json::from_str(&bridge.dispatch_json(action.to_string()))?;
        if dispatched["ok"] != true {
            return Err("consultation refusée par la façade mobile".into());
        }
        let frame: Frame = serde_json::from_value(dispatched["request"].clone())?;
        if !read_only(&frame) {
            return Err("garde-fou : une mutation ne peut pas être envoyée par ce probe".into());
        }
        let mut request = client
            .post(endpoint.clone())
            .bearer_auth(&token)
            .json(&frame);
        if let Some(session) = dispatched["snapshot"]["gateway_session"].as_str() {
            request = request.header("x-hakille-session", session);
        }
        let started = Instant::now();
        let mut response = request
            .send()
            .await
            .map_err(|_| "transport HTTP interrompu ; aucune relance")?;
        let http_status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err("réponse HTTP trop volumineuse".into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "lecture HTTP interrompue ; aucune relance")?
        {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err("réponse HTTP trop volumineuse".into());
            }
            body.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(body).map_err(|_| "réponse non UTF-8")?;
        let received: Value = serde_json::from_str(&bridge.receive_json(frame.id, text))?;
        let snapshot = &received["snapshot"];
        checks.push(json!({
            "action": action["type"], "http_status": http_status, "accepted": received["ok"],
            "elapsed_ms": started.elapsed().as_millis(), "request_id": frame.id,
            "memory_rows": snapshot["memory"].as_array().map(Vec::len),
            "journal_rows": snapshot["journal"].as_array().map(Vec::len),
            "agents": snapshot["agents"].as_array().map(Vec::len),
        }));
        if received["ok"] != true {
            println!("{}", json!({"read_only":true,"checks":checks}));
            return Err("consultation non confirmée ; aucune relance".into());
        }
    }
    println!("{}", json!({"read_only":true,"checks":checks}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_never_accepts_mutation_or_insecure_remote_hub() {
        for message in [
            UiToKernel::HudSubmit {
                query: "bonjour".into(),
            },
            UiToKernel::MemoryRemember {
                text: "fait".into(),
            },
            UiToKernel::Approve { card_id: 1 },
            UiToKernel::AgentRunRequest {
                name: "agent".into(),
                input_json: "{}".into(),
            },
        ] {
            assert!(!read_only(&Frame::request(1, message)));
        }
        for url in [
            "http://example.test",
            "http://localhost",
            "https://user:password@example.test",
            "https://example.test/?token=x",
            "https://example.test/#x",
        ] {
            assert!(validate_hub(url).is_err());
        }
        assert!(validate_hub("http://127.0.0.1:17840").is_ok());
        assert!(validate_hub("https://example.test/hakille").is_ok());
    }
}
