//! Pilote **OCPP 1.6J / 2.0.1** (Open Charge Point Protocol) pour bornes de recharge VE.
//!
//! Implémente les trames JSON-RPC OCPP-J (`CALL=2`, `CALLRESULT=3`, `CALLERROR=4`)
//! transportées sur WebSocket RFC 6455 (`ocpp1.6`, `ocpp2.0.1`).

use crate::model::{HomeCommand, HomeError, HomeResult};
use crate::protocols::websocket::WsClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OcppFrame {
    Call {
        unique_id: String,
        action: String,
        payload: Value,
    },
    CallResult {
        unique_id: String,
        payload: Value,
    },
    CallError {
        unique_id: String,
        error_code: String,
        error_description: String,
        details: Value,
    },
}

impl OcppFrame {
    pub fn to_json_string(&self) -> String {
        match self {
            Self::Call {
                unique_id,
                action,
                payload,
            } => json!([2, unique_id, action, payload]).to_string(),
            Self::CallResult { unique_id, payload } => json!([3, unique_id, payload]).to_string(),
            Self::CallError {
                unique_id,
                error_code,
                error_description,
                details,
            } => json!([4, unique_id, error_code, error_description, details]).to_string(),
        }
    }

    pub fn from_json_str(raw: &str) -> HomeResult<Self> {
        let arr: Vec<Value> = serde_json::from_str(raw).map_err(|e| HomeError::InvalidFrame {
            protocol: "ocpp".into(),
            details: e.to_string(),
        })?;
        let msg_type =
            arr.first()
                .and_then(|v| v.as_u64())
                .ok_or_else(|| HomeError::InvalidFrame {
                    protocol: "ocpp".into(),
                    details: "MessageTypeId OCPP manquant".into(),
                })?;
        let unique_id = arr
            .get(1)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match msg_type {
            2 => Ok(Self::Call {
                unique_id,
                action: arr
                    .get(2)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                payload: arr.get(3).cloned().unwrap_or(Value::Null),
            }),
            3 => Ok(Self::CallResult {
                unique_id,
                payload: arr.get(2).cloned().unwrap_or(Value::Null),
            }),
            4 => Ok(Self::CallError {
                unique_id,
                error_code: arr
                    .get(2)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                error_description: arr
                    .get(3)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                details: arr.get(4).cloned().unwrap_or(Value::Null),
            }),
            other => Err(HomeError::InvalidFrame {
                protocol: "ocpp".into(),
                details: format!("MessageTypeId OCPP inconnu : {other}"),
            }),
        }
    }
}

/// Convertit une `HomeCommand` destinée à une borne VE en trame OCPP 1.6J `CALL`.
pub fn command_to_ocpp_call(unique_id: &str, cmd: &HomeCommand) -> HomeResult<OcppFrame> {
    match cmd {
        HomeCommand::SetEvCharging {
            enabled: true,
            max_amps: Some(amps),
        } => Ok(OcppFrame::Call {
            unique_id: unique_id.to_string(),
            action: "SetChargingProfile".to_string(),
            payload: json!({
                "connectorId": 1,
                "csChargingProfiles": {
                    "chargingProfileId": 1,
                    "stackLevel": 0,
                    "chargingProfilePurpose": "TxDefaultProfile",
                    "chargingProfileKind": "Absolute",
                    "chargingSchedule": {
                        "chargingRateUnit": "A",
                        "chargingSchedulePeriod": [{ "startPeriod": 0, "limit": amps }]
                    }
                }
            }),
        }),
        HomeCommand::SetEvCharging { enabled: true, .. } | HomeCommand::SetPower { on: true } => {
            Ok(OcppFrame::Call {
                unique_id: unique_id.to_string(),
                action: "RemoteStartTransaction".to_string(),
                payload: json!({
                    "connectorId": 1,
                    "idTag": "HAKILLE-LOCAL-OWNER"
                }),
            })
        }
        HomeCommand::SetEvCharging { enabled: false, .. } | HomeCommand::SetPower { on: false } => {
            Ok(OcppFrame::Call {
                unique_id: unique_id.to_string(),
                action: "RemoteStopTransaction".to_string(),
                payload: json!({ "transactionId": 1 }),
            })
        }
        HomeCommand::ReadState => Ok(OcppFrame::Call {
            unique_id: unique_id.to_string(),
            action: "TriggerMessage".to_string(),
            payload: json!({ "requestedMessage": "MeterValues" }),
        }),
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non supportée sur borne OCPP",
            other.action_name()
        ))),
    }
}

/// Envoie une trame OCPP-J sur une borne locale via WebSocket RFC 6455 (`ocpp1.6`).
pub async fn send_ocpp_command_ws(
    addr: SocketAddr,
    charge_point_path: &str,
    frame: &OcppFrame,
) -> HomeResult<OcppFrame> {
    let mut client =
        WsClient::connect(addr, &addr.to_string(), charge_point_path, Some("ocpp1.6")).await?;
    client.send_text(&frame.to_json_string()).await?;
    let reply_raw = client.recv_text().await?;
    OcppFrame::from_json_str(&reply_raw)
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
    async fn ocpp_16j_set_charging_profile_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 1024];
            let n = sock.read(&mut b).await.unwrap();
            let req = std::str::from_utf8(&b[..n]).unwrap();
            assert!(req.contains("Sec-WebSocket-Protocol: ocpp1.6"));
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key:"))
                .unwrap()
                .trim();
            let accept = compute_websocket_accept(key);
            let hs = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Protocol: ocpp1.6\r\n\r\n"
            );
            sock.write_all(hs.as_bytes()).await.unwrap();
            let (_op, payload) = read_ws_frame(&mut sock).await.unwrap();
            let call = OcppFrame::from_json_str(std::str::from_utf8(&payload).unwrap()).unwrap();
            match call {
                OcppFrame::Call {
                    unique_id, action, ..
                } => {
                    assert_eq!(action, "SetChargingProfile");
                    let res = OcppFrame::CallResult {
                        unique_id,
                        payload: json!({"status": "Accepted"}),
                    };
                    sock.write_all(&encode_ws_frame(
                        WsOpcode::Text,
                        res.to_json_string().as_bytes(),
                        false,
                    ))
                    .await
                    .unwrap();
                }
                _ => panic!("CALL OCPP attendu"),
            }
        });

        let call = command_to_ocpp_call(
            "ocpp-1",
            &HomeCommand::SetEvCharging {
                enabled: true,
                max_amps: Some(16.0),
            },
        )
        .unwrap();
        let reply = send_ocpp_command_ws(addr, "/ocpp/CP01", &call)
            .await
            .unwrap();
        assert!(matches!(reply, OcppFrame::CallResult { .. }));
    }
}
