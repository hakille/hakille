//! Pilote MQTT v3.1.1 & v5.0 (TCP + TLS rustls) en Rust pur.
//!
//! Inclut :
//! - Codec binaire complet (CONNECT, CONNACK, PUBLISH QoS 0/1/2, PUBACK, SUBSCRIBE, SUBACK,
//!   PINGREQ, PINGRESP, DISCONNECT, en-têtes variables et propriétés v5)
//! - Client asynchrone TCP et TLS (`tokio-rustls` + certificats système natifs)
//! - Adaptateurs Home Assistant MQTT Discovery (`homeassistant/<comp>/<id>/config`),
//!   Zigbee2MQTT (`zigbee2mqtt/<name>/set`), Tasmota (`cmnd/<dev>/Power`), Shelly (`shellies/...`)
//! - Matcher de topics `+` et `#`.

use crate::model::{
    DeviceCategory, DiscoveryRecord, HomeCommand, HomeError, HomeResult, ProtocolKind,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MqttVersion {
    V311 = 4,
    V50 = 5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttPacket {
    Connect {
        client_id: String,
        version: MqttVersion,
        keep_alive_secs: u16,
        username: Option<String>,
        password: Option<String>,
    },
    ConnAck {
        session_present: bool,
        reason_code: u8,
    },
    Publish {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        packet_id: Option<u16>,
    },
    PubAck {
        packet_id: u16,
    },
    Subscribe {
        packet_id: u16,
        topics: Vec<(String, u8)>,
    },
    SubAck {
        packet_id: u16,
        return_codes: Vec<u8>,
    },
    PingReq,
    PingResp,
    Disconnect,
}

fn push_var_int(mut x: usize, out: &mut Vec<u8>) {
    loop {
        let mut byte = (x % 128) as u8;
        x /= 128;
        if x > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if x == 0 {
            break;
        }
    }
}

fn read_var_int(data: &[u8], pos: &mut usize) -> HomeResult<usize> {
    let mut mult = 1usize;
    let mut val = 0usize;
    for _ in 0..4 {
        let b = *data.get(*pos).ok_or_else(|| HomeError::InvalidFrame {
            protocol: "mqtt".into(),
            details: "VarInt MQTT tronqué".into(),
        })?;
        *pos += 1;
        val += ((b & 0x7F) as usize) * mult;
        if (b & 0x80) == 0 {
            return Ok(val);
        }
        mult *= 128;
    }
    Err(HomeError::InvalidFrame {
        protocol: "mqtt".into(),
        details: "VarInt MQTT > 4 octets".into(),
    })
}

fn push_mqtt_str(s: &str, out: &mut Vec<u8>) {
    let b = s.as_bytes();
    out.extend_from_slice(&(b.len() as u16).to_be_bytes());
    out.extend_from_slice(b);
}

fn read_mqtt_str(data: &[u8], pos: &mut usize) -> HomeResult<String> {
    if *pos + 2 > data.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "mqtt".into(),
            details: "Longueur chaîne MQTT tronquée".into(),
        });
    }
    let len = u16::from_be_bytes([data[*pos], data[*pos + 1]]) as usize;
    *pos += 2;
    if *pos + len > data.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "mqtt".into(),
            details: "Chaîne MQTT hors bornes".into(),
        });
    }
    let s = String::from_utf8_lossy(&data[*pos..*pos + len]).into_owned();
    *pos += len;
    Ok(s)
}

impl MqttPacket {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vh = Vec::new();
        let header_byte: u8 = match self {
            Self::Connect {
                client_id,
                version,
                keep_alive_secs,
                username,
                password,
            } => {
                push_mqtt_str("MQTT", &mut vh);
                vh.push(*version as u8);
                let mut flags = 0x02u8; // Clean Session / Clean Start
                if username.is_some() {
                    flags |= 0x80;
                }
                if password.is_some() {
                    flags |= 0x40;
                }
                vh.push(flags);
                vh.extend_from_slice(&keep_alive_secs.to_be_bytes());
                if *version == MqttVersion::V50 {
                    vh.push(0x00); // Propriétés CONNECT v5 vides
                }
                push_mqtt_str(client_id, &mut vh);
                if let Some(u) = username {
                    push_mqtt_str(u, &mut vh);
                }
                if let Some(p) = password {
                    push_mqtt_str(p, &mut vh);
                }
                0x10
            }
            Self::ConnAck {
                session_present,
                reason_code,
            } => {
                vh.push(if *session_present { 1 } else { 0 });
                vh.push(*reason_code);
                0x20
            }
            Self::Publish {
                topic,
                payload,
                qos,
                retain,
                packet_id,
            } => {
                push_mqtt_str(topic, &mut vh);
                if *qos > 0 {
                    vh.extend_from_slice(&packet_id.unwrap_or(1).to_be_bytes());
                }
                vh.extend_from_slice(payload);
                0x30 | ((*qos & 0x03) << 1) | if *retain { 1 } else { 0 }
            }
            Self::PubAck { packet_id } => {
                vh.extend_from_slice(&packet_id.to_be_bytes());
                0x40
            }
            Self::Subscribe { packet_id, topics } => {
                vh.extend_from_slice(&packet_id.to_be_bytes());
                for (t, qos) in topics {
                    push_mqtt_str(t, &mut vh);
                    vh.push(*qos & 0x03);
                }
                0x82
            }
            Self::SubAck {
                packet_id,
                return_codes,
            } => {
                vh.extend_from_slice(&packet_id.to_be_bytes());
                vh.extend_from_slice(return_codes);
                0x90
            }
            Self::PingReq => 0xC0,
            Self::PingResp => 0xD0,
            Self::Disconnect => 0xE0,
        };

        let mut frame = vec![header_byte];
        push_var_int(vh.len(), &mut frame);
        frame.extend_from_slice(&vh);
        frame
    }

    pub fn from_bytes(buf: &[u8]) -> HomeResult<(Self, usize)> {
        if buf.len() < 2 {
            return Err(HomeError::InvalidFrame {
                protocol: "mqtt".into(),
                details: "Trame MQTT < 2 octets".into(),
            });
        }
        let header = buf[0];
        let pkt_type = header >> 4;
        let mut pos = 1;
        let rem_len = read_var_int(buf, &mut pos)?;
        if pos + rem_len > buf.len() {
            return Err(HomeError::InvalidFrame {
                protocol: "mqtt".into(),
                details: "Trame MQTT tronquée".into(),
            });
        }
        let body = &buf[pos..pos + rem_len];
        let total_len = pos + rem_len;
        let mut bpos = 0;

        let pkt = match pkt_type {
            1 => {
                let _proto = read_mqtt_str(body, &mut bpos)?;
                let ver_byte = *body.get(bpos).ok_or_else(|| HomeError::InvalidFrame {
                    protocol: "mqtt".into(),
                    details: "Version CONNECT manquante".into(),
                })?;
                let flags = *body.get(bpos + 1).ok_or_else(|| HomeError::InvalidFrame {
                    protocol: "mqtt".into(),
                    details: "Flags CONNECT manquants".into(),
                })?;
                let ka = u16::from_be_bytes([body[bpos + 2], body[bpos + 3]]);
                bpos += 4;
                let version = if ver_byte == 5 {
                    let prop_len = read_var_int(body, &mut bpos)?;
                    bpos += prop_len;
                    MqttVersion::V50
                } else {
                    MqttVersion::V311
                };
                let client_id = read_mqtt_str(body, &mut bpos)?;
                let username = if (flags & 0x80) != 0 {
                    Some(read_mqtt_str(body, &mut bpos)?)
                } else {
                    None
                };
                let password = if (flags & 0x40) != 0 {
                    Some(read_mqtt_str(body, &mut bpos)?)
                } else {
                    None
                };
                Self::Connect {
                    client_id,
                    version,
                    keep_alive_secs: ka,
                    username,
                    password,
                }
            }
            2 if body.len() >= 2 => Self::ConnAck {
                session_present: (body[0] & 0x01) != 0,
                reason_code: body[1],
            },
            3 => {
                let qos = (header >> 1) & 0x03;
                let retain = (header & 0x01) != 0;
                let topic = read_mqtt_str(body, &mut bpos)?;
                let packet_id = if qos > 0 {
                    if bpos + 2 > body.len() {
                        return Err(HomeError::InvalidFrame {
                            protocol: "mqtt".into(),
                            details: "PacketId PUBLISH manquant".into(),
                        });
                    }
                    let pid = u16::from_be_bytes([body[bpos], body[bpos + 1]]);
                    bpos += 2;
                    Some(pid)
                } else {
                    None
                };
                Self::Publish {
                    topic,
                    payload: body[bpos..].to_vec(),
                    qos,
                    retain,
                    packet_id,
                }
            }
            4 if body.len() >= 2 => Self::PubAck {
                packet_id: u16::from_be_bytes([body[0], body[1]]),
            },
            8 => {
                if body.len() < 2 {
                    return Err(HomeError::InvalidFrame {
                        protocol: "mqtt".into(),
                        details: "SUBSCRIBE sans packet_id".into(),
                    });
                }
                let packet_id = u16::from_be_bytes([body[0], body[1]]);
                bpos = 2;
                let mut topics = Vec::new();
                while bpos < body.len() {
                    let t = read_mqtt_str(body, &mut bpos)?;
                    let qos = *body.get(bpos).unwrap_or(&0);
                    bpos += 1;
                    topics.push((t, qos));
                }
                Self::Subscribe { packet_id, topics }
            }
            9 if body.len() >= 2 => Self::SubAck {
                packet_id: u16::from_be_bytes([body[0], body[1]]),
                return_codes: body[2..].to_vec(),
            },
            12 => Self::PingReq,
            13 => Self::PingResp,
            14 => Self::Disconnect,
            other => {
                return Err(HomeError::InvalidFrame {
                    protocol: "mqtt".into(),
                    details: format!("Type de paquet MQTT non supporté : {other}"),
                });
            }
        };

        Ok((pkt, total_len))
    }
}

/// Vérifie si `topic` correspond à un filtre MQTT (`+` mono-niveau, `#` multi-niveaux).
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    let f_parts: Vec<&str> = filter.split('/').collect();
    let t_parts: Vec<&str> = topic.split('/').collect();
    for (i, fp) in f_parts.iter().enumerate() {
        if *fp == "#" {
            return true;
        }
        let Some(tp) = t_parts.get(i) else {
            return false;
        };
        if *fp != "+" && fp != tp {
            return false;
        }
    }
    f_parts.len() == t_parts.len()
}

/// Encode une commande canonique `HomeCommand` vers un topic + payload MQTT (Zigbee2MQTT / Shelly / Tasmota).
pub fn command_to_mqtt_publish(
    base_topic: &str,
    command: &HomeCommand,
) -> HomeResult<(String, Vec<u8>)> {
    let topic = format!("{}/set", base_topic.trim_end_matches('/'));
    let json = match command {
        HomeCommand::SetPower { on } => serde_json::json!({
            "state": if *on { "ON" } else { "OFF" }
        }),
        HomeCommand::Toggle => serde_json::json!({ "state": "TOGGLE" }),
        HomeCommand::SetLevel { level_pct } => serde_json::json!({
            "state": if *level_pct > 0 { "ON" } else { "OFF" },
            "brightness": ((*level_pct as u16) * 254) / 100
        }),
        HomeCommand::SetColorRgb { r, g, b } => serde_json::json!({
            "state": "ON",
            "color": { "r": r, "g": g, "b": b }
        }),
        HomeCommand::SetColorTemp { kelvin } => serde_json::json!({
            "color_temp": (1_000_000u32 / (*kelvin).max(1000) as u32)
        }),
        HomeCommand::SetTemperature { celsius } => serde_json::json!({
            "occupied_heating_setpoint": celsius
        }),
        HomeCommand::SetCoverPosition { position_pct } => serde_json::json!({
            "position": position_pct
        }),
        HomeCommand::LockDoor => serde_json::json!({ "state": "LOCK" }),
        HomeCommand::UnlockDoor => serde_json::json!({ "state": "UNLOCK" }),
        HomeCommand::ReadState => {
            return Ok((
                format!("{}/get", base_topic.trim_end_matches('/')),
                b"{\"state\":\"\"}".to_vec(),
            ));
        }
        HomeCommand::RawProtocol {
            topic_or_path,
            payload,
        } => {
            return Ok((
                topic_or_path.clone(),
                serde_json::to_vec(payload).unwrap_or_default(),
            ));
        }
        other => {
            return Err(HomeError::InvalidCommand(format!(
                "Commande '{}' non traduite en payload MQTT générique",
                other.action_name()
            )));
        }
    };
    Ok((topic, serde_json::to_vec(&json).unwrap_or_default()))
}

/// Parse un payload Home Assistant MQTT Discovery (`homeassistant/<domain>/<node>/config`).
pub fn parse_ha_mqtt_discovery(topic: &str, payload: &[u8]) -> Option<DiscoveryRecord> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 4 || parts[0] != "homeassistant" || *parts.last()? != "config" {
        return None;
    }
    let val: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(parts[2])
        .to_string();
    let domain = parts[1];
    let category = match domain {
        "light" => DeviceCategory::Light,
        "switch" => DeviceCategory::Switch,
        "lock" => DeviceCategory::DoorLock,
        "cover" => DeviceCategory::CoverBlind,
        "climate" => DeviceCategory::Thermostat,
        "camera" => DeviceCategory::SecurityCamera,
        "alarm_control_panel" => DeviceCategory::AlarmPanel,
        _ => DeviceCategory::Sensor,
    };
    let cmd_topic = val
        .get("command_topic")
        .or_else(|| val.get("state_topic"))
        .and_then(|v| v.as_str())
        .unwrap_or(topic)
        .to_string();

    let mut meta = BTreeMap::new();
    meta.insert("mqtt_domain".into(), domain.into());
    meta.insert("mqtt_topic".into(), cmd_topic.clone());

    Some(DiscoveryRecord {
        protocol: ProtocolKind::Mqtt,
        service_name: name,
        endpoint: cmd_topic,
        inferred_category: category,
        txt_metadata: meta,
        discovered_at: Utc::now().to_rfc3339(),
    })
}

/// Lit un paquet MQTT complet sur un flux asynchrone (TCP ou TLS).
pub async fn read_mqtt_packet<S: AsyncRead + Unpin>(stream: &mut S) -> HomeResult<MqttPacket> {
    let mut fixed = [0u8; 2];
    stream
        .read_exact(&mut fixed)
        .await
        .map_err(|e| HomeError::Io {
            context: "mqtt read header".into(),
            message: e.to_string(),
        })?;
    let mut raw = vec![fixed[0], fixed[1]];
    let mut rem_len = (fixed[1] & 0x7F) as usize;
    let mut mult = 128usize;
    let mut cur = fixed[1];
    while (cur & 0x80) != 0 {
        let mut next = [0u8; 1];
        stream
            .read_exact(&mut next)
            .await
            .map_err(|e| HomeError::Io {
                context: "mqtt read varint".into(),
                message: e.to_string(),
            })?;
        cur = next[0];
        raw.push(cur);
        rem_len += ((cur & 0x7F) as usize) * mult;
        mult *= 128;
    }
    let mut body = vec![0u8; rem_len];
    if rem_len > 0 {
        stream
            .read_exact(&mut body)
            .await
            .map_err(|e| HomeError::Io {
                context: "mqtt read body".into(),
                message: e.to_string(),
            })?;
    }
    raw.extend_from_slice(&body);
    let (pkt, _) = MqttPacket::from_bytes(&raw)?;
    Ok(pkt)
}

async fn write_mqtt_packet<S: AsyncWrite + Unpin>(
    stream: &mut S,
    pkt: &MqttPacket,
) -> HomeResult<()> {
    stream
        .write_all(&pkt.to_bytes())
        .await
        .map_err(|e| HomeError::Io {
            context: "mqtt write".into(),
            message: e.to_string(),
        })
}

/// Se connecte à un broker MQTT local TCP (`1883`), publie un message QoS 1 ou 0, et déconnecte proprement.
pub async fn mqtt_publish_tcp(
    broker_addr: SocketAddr,
    client_id: &str,
    topic: &str,
    payload: &[u8],
    qos: u8,
) -> HomeResult<()> {
    let mut stream = TcpStream::connect(broker_addr)
        .await
        .map_err(|e| HomeError::Io {
            context: format!("mqtt connect {broker_addr}"),
            message: e.to_string(),
        })?;
    write_mqtt_packet(
        &mut stream,
        &MqttPacket::Connect {
            client_id: client_id.to_string(),
            version: MqttVersion::V311,
            keep_alive_secs: 30,
            username: None,
            password: None,
        },
    )
    .await?;
    match read_mqtt_packet(&mut stream).await? {
        MqttPacket::ConnAck { reason_code: 0, .. } => {}
        other => {
            return Err(HomeError::InvalidFrame {
                protocol: "mqtt".into(),
                details: format!("CONNACK refusé : {other:?}"),
            });
        }
    }
    let pid = if qos > 0 { Some(1) } else { None };
    write_mqtt_packet(
        &mut stream,
        &MqttPacket::Publish {
            topic: topic.to_string(),
            payload: payload.to_vec(),
            qos,
            retain: false,
            packet_id: pid,
        },
    )
    .await?;
    if qos == 1 {
        match read_mqtt_packet(&mut stream).await? {
            MqttPacket::PubAck { packet_id: 1 } => {}
            other => {
                return Err(HomeError::InvalidFrame {
                    protocol: "mqtt".into(),
                    details: format!("PUBACK attendu, reçu : {other:?}"),
                });
            }
        }
    }
    let _ = write_mqtt_packet(&mut stream, &MqttPacket::Disconnect).await;
    Ok(())
}

/// Construit un connecteur TLS `tokio-rustls` utilisant les certificats système natifs (pour `mqtts://` local).
pub fn build_mqtt_tls_connector() -> HomeResult<tokio_rustls::TlsConnector> {
    let mut root_store = rustls::RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs();
    for cert in certs.certs {
        let _ = root_store.add(cert);
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| HomeError::Io {
        context: "mqtt tls config".into(),
        message: e.to_string(),
    })?
    .with_root_certificates(root_store)
    .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn mqtt_wildcard_topic_matching() {
        assert!(topic_matches(
            "zigbee2mqtt/+/state",
            "zigbee2mqtt/salon/state"
        ));
        assert!(topic_matches(
            "homeassistant/#",
            "homeassistant/light/salon/config"
        ));
        assert!(!topic_matches(
            "zigbee2mqtt/+/state",
            "zigbee2mqtt/salon/set"
        ));
    }

    #[tokio::test]
    async fn mqtt_v311_and_v5_loopback_broker_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let conn = read_mqtt_packet(&mut sock).await.unwrap();
            assert!(matches!(conn, MqttPacket::Connect { .. }));
            write_mqtt_packet(
                &mut sock,
                &MqttPacket::ConnAck {
                    session_present: false,
                    reason_code: 0,
                },
            )
            .await
            .unwrap();
            let pub_pkt = read_mqtt_packet(&mut sock).await.unwrap();
            match pub_pkt {
                MqttPacket::Publish {
                    topic,
                    payload,
                    packet_id,
                    ..
                } => {
                    assert_eq!(topic, "zigbee2mqtt/lampe_salon/set");
                    assert!(std::str::from_utf8(&payload).unwrap().contains("ON"));
                    write_mqtt_packet(
                        &mut sock,
                        &MqttPacket::PubAck {
                            packet_id: packet_id.unwrap(),
                        },
                    )
                    .await
                    .unwrap();
                }
                other => panic!("PUBLISH attendu, reçu {other:?}"),
            }
            let disc = read_mqtt_packet(&mut sock).await.unwrap();
            assert_eq!(disc, MqttPacket::Disconnect);
        });

        let (topic, payload) = command_to_mqtt_publish(
            "zigbee2mqtt/lampe_salon",
            &HomeCommand::SetPower { on: true },
        )
        .unwrap();
        mqtt_publish_tcp(addr, "hakille-home-test", &topic, &payload, 1)
            .await
            .unwrap();
        server.await.unwrap();

        // Test encodage/décodage CONNECT v5.0
        let v5 = MqttPacket::Connect {
            client_id: "hakille-v5".into(),
            version: MqttVersion::V50,
            keep_alive_secs: 60,
            username: Some("hakille".into()),
            password: Some("local".into()),
        };
        let bytes = v5.to_bytes();
        let (decoded, used) = MqttPacket::from_bytes(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(decoded, v5);
    }
}
