//! Pilotes locaux spécialisés :
//! 1. **Apple HomeKit HAP** : cadrage HTTP chiffré ChaCha20Poly1305 (blocks `AAD(2 LE) | Cipher | Tag(16)`)
//!    + caractéristiques JSON (`/characteristics`).
//! 2. **Philips Hue CLIP v2** : requêtes `/clip/v2/resource/light/<id>` & parseur SSE `text/event-stream`.
//! 3. **Shelly Gen2/Gen3 JSON-RPC** : `Switch.Set`, `Light.Set`, `Cover.GoToPosition`, `Shelly.GetStatus`
//!    sur `/rpc` local.

use crate::model::{HomeCommand, HomeError, HomeResult};
use crate::security::HomeLanGuard;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use serde_json::{Value, json};
use std::time::Duration;

// ---------------------------------------------------------------------------
// 1. Apple HomeKit Accessory Protocol (HAP) — Cadrage chiffré ChaCha20Poly1305
// ---------------------------------------------------------------------------

/// Chiffre un bloc HTTP HAP selon la spec Apple HAP §6.5.2 (`AAD(2 LE) || Ciphertext || Tag(16)`).
pub fn hap_encrypt_block(key: &[u8; 32], counter: u64, plaintext: &[u8]) -> HomeResult<Vec<u8>> {
    if plaintext.len() > 1024 {
        return Err(HomeError::InvalidFrame {
            protocol: "homekit-hap".into(),
            details: "Bloc HAP > 1024 octets".into(),
        });
    }
    let aad = (plaintext.len() as u16).to_le_bytes();
    let mut nonce = [0u8; 12];
    nonce[4..12].copy_from_slice(&counter.to_le_bytes());

    let cipher = ChaCha20Poly1305::new_from_slice(key).unwrap();
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|e| HomeError::InvalidFrame {
            protocol: "homekit-hap".into(),
            details: e.to_string(),
        })?;
    let mut out = Vec::with_capacity(2 + ct.len());
    out.extend_from_slice(&aad);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn hap_decrypt_block(key: &[u8; 32], counter: u64, block: &[u8]) -> HomeResult<Vec<u8>> {
    if block.len() < 18 {
        return Err(HomeError::InvalidFrame {
            protocol: "homekit-hap".into(),
            details: "Bloc HAP chiffré < 18 octets".into(),
        });
    }
    let aad = &block[..2];
    let expected_len = u16::from_le_bytes([aad[0], aad[1]]) as usize;
    if block.len() < 2 + expected_len + 16 {
        return Err(HomeError::InvalidFrame {
            protocol: "homekit-hap".into(),
            details: "Bloc HAP tronqué".into(),
        });
    }
    let mut nonce = [0u8; 12];
    nonce[4..12].copy_from_slice(&counter.to_le_bytes());
    let cipher = ChaCha20Poly1305::new_from_slice(key).unwrap();
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &block[2..2 + expected_len + 16],
                aad,
            },
        )
        .map_err(|_| HomeError::InvalidFrame {
            protocol: "homekit-hap".into(),
            details: "Tag Poly1305 HAP invalide".into(),
        })
}

pub fn hap_characteristics_payload(aid: u64, iid: u64, cmd: &HomeCommand) -> HomeResult<Value> {
    let value = match cmd {
        HomeCommand::SetPower { on } => json!(on),
        HomeCommand::SetLevel { level_pct } => json!(level_pct),
        HomeCommand::LockDoor => json!(1),
        HomeCommand::UnlockDoor => json!(0),
        HomeCommand::SetTemperature { celsius } => json!(celsius),
        HomeCommand::SetCoverPosition { position_pct } => json!(position_pct),
        other => {
            return Err(HomeError::InvalidCommand(format!(
                "Commande '{}' non traduite en caractéristique HAP",
                other.action_name()
            )));
        }
    };
    Ok(json!({
        "characteristics": [{
            "aid": aid,
            "iid": iid,
            "value": value
        }]
    }))
}

// ---------------------------------------------------------------------------
// 2. Philips Hue CLIP v2
// ---------------------------------------------------------------------------

pub fn hue_clip_v2_light_payload(cmd: &HomeCommand) -> HomeResult<Value> {
    match cmd {
        HomeCommand::SetPower { on } => Ok(json!({ "on": { "on": on } })),
        HomeCommand::SetLevel { level_pct } => Ok(json!({
            "on": { "on": *level_pct > 0 },
            "dimming": { "brightness": (*level_pct as f32).clamp(0.0, 100.0) }
        })),
        HomeCommand::SetColorTemp { kelvin } => {
            let mirek = (1_000_000u32 / (*kelvin).max(1500) as u32).clamp(153, 500);
            Ok(json!({
                "on": { "on": true },
                "color_temperature": { "mirek": mirek }
            }))
        }
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non traduite pour Hue CLIP v2",
            other.action_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// 3. Shelly Gen2 / Gen3 JSON-RPC local
// ---------------------------------------------------------------------------

pub fn shelly_rpc_payload(id: u32, cmd: &HomeCommand) -> HomeResult<(&'static str, Value)> {
    match cmd {
        HomeCommand::SetPower { on } => Ok(("Switch.Set", json!({ "id": id, "on": on }))),
        HomeCommand::Toggle => Ok(("Switch.Toggle", json!({ "id": id }))),
        HomeCommand::SetLevel { level_pct } => Ok((
            "Light.Set",
            json!({ "id": id, "on": *level_pct > 0, "brightness": level_pct }),
        )),
        HomeCommand::SetCoverPosition { position_pct } => Ok((
            "Cover.GoToPosition",
            json!({ "id": id, "pos": position_pct }),
        )),
        HomeCommand::ReadState => Ok(("Shelly.GetStatus", json!({}))),
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non traduite en JSON-RPC Shelly",
            other.action_name()
        ))),
    }
}

/// Exécute un appel Shelly Gen2/Gen3 JSON-RPC réel sur HTTP LAN (bloqué hors LAN par `HomeLanGuard`).
pub async fn send_shelly_rpc_http(
    base_url: &str,
    method: &str,
    params: Value,
) -> HomeResult<Value> {
    HomeLanGuard::new().validate_endpoint(base_url)?;
    let url = format!("{}/rpc", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|e| HomeError::Io {
            context: "shelly client".into(),
            message: e.to_string(),
        })?;
    let body = json!({
        "id": 1,
        "method": method,
        "params": params
    });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| HomeError::Io {
            context: format!("shelly rpc {url}"),
            message: e.to_string(),
        })?;
    if !resp.status().is_success() {
        return Err(HomeError::Io {
            context: "shelly http status".into(),
            message: format!("HTTP {}", resp.status()),
        });
    }
    resp.json::<Value>()
        .await
        .map_err(|e| HomeError::InvalidFrame {
            protocol: "shelly-rpc".into(),
            details: e.to_string(),
        })
}

/// Exécute une requête PUT locale Philips Hue CLIP v2 (`/clip/v2/resource/light/{rid}`).
/// Protégé par `HomeLanGuard` (aucun pont cloud Hue autorisé).
pub async fn send_hue_clip_v2_http(
    base_url: &str,
    resource_id: &str,
    application_key: &str,
    payload: Value,
) -> HomeResult<Value> {
    HomeLanGuard::new().validate_endpoint(base_url)?;
    if application_key.trim().is_empty() {
        return Err(HomeError::TransportUnavailable {
            protocol: "philips-hue".into(),
            reason: "Clé locale 'hue_app_key' absente dans les métadonnées de l'appareil".into(),
        });
    }
    let url = format!(
        "{}/clip/v2/resource/light/{}",
        base_url.trim_end_matches('/'),
        resource_id.trim_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|e| HomeError::Io {
            context: "hue clip v2 client".into(),
            message: e.to_string(),
        })?;
    let resp = client
        .put(&url)
        .header("hue-application-key", application_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| HomeError::Io {
            context: format!("hue clip v2 {url}"),
            message: e.to_string(),
        })?;
    if !resp.status().is_success() {
        return Err(HomeError::Io {
            context: "hue http status".into(),
            message: format!("HTTP {}", resp.status()),
        });
    }
    resp.json::<Value>()
        .await
        .map_err(|e| HomeError::InvalidFrame {
            protocol: "philips-hue".into(),
            details: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn hap_chacha_and_hue_clip_v2_roundtrip() {
        let key = [0x42u8; 32];
        let payload = br#"{"characteristics":[{"aid":1,"iid":10,"value":true}]}"#;
        let sealed = hap_encrypt_block(&key, 7, payload).unwrap();
        let opened = hap_decrypt_block(&key, 7, &sealed).unwrap();
        assert_eq!(opened, payload);

        let hue = hue_clip_v2_light_payload(&HomeCommand::SetLevel { level_pct: 65 }).unwrap();
        assert_eq!(hue["dimming"]["brightness"], 65.0);
    }

    #[tokio::test]
    async fn shelly_rpc_loopback_http() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).await.unwrap();
            let req = std::str::from_utf8(&buf[..n]).unwrap();
            assert!(req.contains("Switch.Set"));
            let json_body = r#"{"id":1,"src":"shellyplus1","result":{"was_on":false}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
        });

        let (method, params) = shelly_rpc_payload(0, &HomeCommand::SetPower { on: true }).unwrap();
        let res = send_shelly_rpc_http(&format!("http://{addr}"), method, params)
            .await
            .unwrap();
        assert_eq!(res["src"], "shellyplus1");
    }

    #[tokio::test]
    async fn hue_clip_v2_loopback_http() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).await.unwrap();
            let req = std::str::from_utf8(&buf[..n]).unwrap();
            assert!(req.contains("PUT /clip/v2/resource/light/rid-salon"));
            assert!(
                req.to_ascii_lowercase()
                    .contains("hue-application-key: test-hue-key")
            );
            let json_body = r#"{"errors":[],"data":[{"rid":"rid-salon","rtype":"light"}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
        });

        let payload = hue_clip_v2_light_payload(&HomeCommand::SetLevel { level_pct: 75 }).unwrap();
        let res = send_hue_clip_v2_http(
            &format!("http://{addr}"),
            "rid-salon",
            "test-hue-key",
            payload,
        )
        .await
        .unwrap();
        assert_eq!(res["data"][0]["rid"], "rid-salon");
    }
}
