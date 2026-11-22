//! Pilote Home Assistant Local WebSocket (`ws://<lan>:8123/api/websocket`) & REST.
//!
//! Strictement restreint au réseau local via `HomeLanGuard`.

use crate::model::{HomeCommand, HomeError, HomeResult};
use crate::protocols::websocket::WsClient;
use crate::security::HomeLanGuard;
use serde_json::{Value, json};
use std::net::SocketAddr;

/// Convertit une `HomeCommand` + `entity_id` Home Assistant en `(domain, service, service_data)`.
pub fn command_to_ha_service(
    entity_id: &str,
    cmd: &HomeCommand,
) -> HomeResult<(&'static str, &'static str, Value)> {
    let domain = entity_id.split('.').next().unwrap_or("homeassistant");
    match cmd {
        HomeCommand::SetPower { on: true } => Ok((
            "homeassistant",
            "turn_on",
            json!({ "entity_id": entity_id }),
        )),
        HomeCommand::SetPower { on: false } => Ok((
            "homeassistant",
            "turn_off",
            json!({ "entity_id": entity_id }),
        )),
        HomeCommand::Toggle => Ok(("homeassistant", "toggle", json!({ "entity_id": entity_id }))),
        HomeCommand::SetLevel { level_pct } => Ok((
            "light",
            "turn_on",
            json!({
                "entity_id": entity_id,
                "brightness_pct": level_pct
            }),
        )),
        HomeCommand::SetColorRgb { r, g, b } => Ok((
            "light",
            "turn_on",
            json!({
                "entity_id": entity_id,
                "rgb_color": [r, g, b]
            }),
        )),
        HomeCommand::SetColorTemp { kelvin } => Ok((
            "light",
            "turn_on",
            json!({
                "entity_id": entity_id,
                "kelvin": kelvin
            }),
        )),
        HomeCommand::SetTemperature { celsius } => Ok((
            "climate",
            "set_temperature",
            json!({
                "entity_id": entity_id,
                "temperature": celsius
            }),
        )),
        HomeCommand::SetCoverPosition { position_pct } => Ok((
            "cover",
            "set_cover_position",
            json!({
                "entity_id": entity_id,
                "position": position_pct
            }),
        )),
        HomeCommand::LockDoor => Ok(("lock", "lock", json!({ "entity_id": entity_id }))),
        HomeCommand::UnlockDoor => Ok(("lock", "unlock", json!({ "entity_id": entity_id }))),
        HomeCommand::OpenGarage => Ok(("cover", "open_cover", json!({ "entity_id": entity_id }))),
        HomeCommand::CloseGarage => Ok(("cover", "close_cover", json!({ "entity_id": entity_id }))),
        HomeCommand::SetAlarm { mode, .. } => {
            let service = match mode.to_ascii_lowercase().as_str() {
                "armed_away" | "away" => "alarm_arm_away",
                "armed_home" | "home" => "alarm_arm_home",
                _ => "alarm_disarm",
            };
            Ok((
                "alarm_control_panel",
                service,
                json!({ "entity_id": entity_id }),
            ))
        }
        HomeCommand::RawProtocol {
            topic_or_path,
            payload,
        } => {
            if let Some((_d, _s)) = topic_or_path.split_once('.') {
                let d_owned: &'static str = match domain {
                    "light" => "light",
                    "switch" => "switch",
                    "lock" => "lock",
                    "cover" => "cover",
                    "climate" => "climate",
                    _ => "homeassistant",
                };
                Ok((d_owned, "turn_on", payload.clone()))
            } else {
                Ok(("homeassistant", "turn_on", payload.clone()))
            }
        }
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non traduite pour Home Assistant",
            other.action_name()
        ))),
    }
}

/// Appelle un service Home Assistant local via WebSocket RFC 6455 (`auth_required` -> `auth` -> `call_service`).
pub async fn ha_ws_call_service(
    addr: SocketAddr,
    bearer_token: &str,
    entity_id: &str,
    cmd: &HomeCommand,
) -> HomeResult<Value> {
    HomeLanGuard::new().validate_ip(addr.ip())?;
    let (domain, service, data) = command_to_ha_service(entity_id, cmd)?;

    let mut ws = WsClient::connect(addr, &addr.to_string(), "/api/websocket", None).await?;
    let greeting: Value =
        serde_json::from_str(&ws.recv_text().await?).map_err(|e| HomeError::InvalidFrame {
            protocol: "home-assistant".into(),
            details: e.to_string(),
        })?;
    if greeting.get("type").and_then(|v| v.as_str()) != Some("auth_required") {
        return Err(HomeError::InvalidFrame {
            protocol: "home-assistant".into(),
            details: format!("Handshake HA inattendu : {greeting}"),
        });
    }

    let auth_msg = json!({ "type": "auth", "access_token": bearer_token }).to_string();
    ws.send_text(&auth_msg).await?;
    let auth_resp: Value =
        serde_json::from_str(&ws.recv_text().await?).map_err(|e| HomeError::InvalidFrame {
            protocol: "home-assistant".into(),
            details: e.to_string(),
        })?;
    if auth_resp.get("type").and_then(|v| v.as_str()) != Some("auth_ok") {
        return Err(HomeError::CapabilityDenied(format!(
            "Authentification Home Assistant refusée : {auth_resp}"
        )));
    }

    let call_msg = json!({
        "id": 1,
        "type": "call_service",
        "domain": domain,
        "service": service,
        "service_data": data
    })
    .to_string();
    ws.send_text(&call_msg).await?;
    let result: Value =
        serde_json::from_str(&ws.recv_text().await?).map_err(|e| HomeError::InvalidFrame {
            protocol: "home-assistant".into(),
            details: e.to_string(),
        })?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::websocket::{
        WsOpcode, compute_websocket_accept, encode_ws_frame, read_ws_frame,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn home_assistant_ws_auth_and_call_service_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 1024];
            let n = sock.read(&mut b).await.unwrap();
            let req = std::str::from_utf8(&b[..n]).unwrap();
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key:"))
                .unwrap()
                .trim();
            let accept = compute_websocket_accept(key);
            let hs = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            sock.write_all(hs.as_bytes()).await.unwrap();
            sock.write_all(&encode_ws_frame(
                WsOpcode::Text,
                br#"{"type":"auth_required","ha_version":"2026.1"}"#,
                false,
            ))
            .await
            .unwrap();
            let (_op, auth_raw) = read_ws_frame(&mut sock).await.unwrap();
            assert!(
                std::str::from_utf8(&auth_raw)
                    .unwrap()
                    .contains("secret-tok")
            );
            sock.write_all(&encode_ws_frame(
                WsOpcode::Text,
                br#"{"type":"auth_ok"}"#,
                false,
            ))
            .await
            .unwrap();
            let (_op, call_raw) = read_ws_frame(&mut sock).await.unwrap();
            let call_json: Value = serde_json::from_slice(&call_raw).unwrap();
            assert_eq!(call_json["service"], "turn_on");
            sock.write_all(&encode_ws_frame(
                WsOpcode::Text,
                br#"{"id":1,"type":"result","success":true}"#,
                false,
            ))
            .await
            .unwrap();
        });

        let res = ha_ws_call_service(
            addr,
            "secret-tok",
            "light.salon",
            &HomeCommand::SetLevel { level_pct: 80 },
        )
        .await
        .unwrap();
        assert_eq!(res["success"], true);
    }
}
