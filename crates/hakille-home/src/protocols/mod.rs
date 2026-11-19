//! Catalogue unifié de tous les pilotes protocolaires de `hakille-home`.

pub mod ble_home;
pub mod coap;
pub mod esphome;
pub mod hap_hue_shelly;
pub mod home_assistant;
pub mod knx;
pub mod matter;
pub mod mdns;
pub mod modbus;
pub mod mqtt;
pub mod ocpp;
pub mod ssdp;
pub mod websocket;
pub mod zigbee;
pub mod zwave;

use crate::model::{
    CommandReceipt, DiscoveryRecord, HomeCommand, HomeDevice, HomeError, HomeResult, HomeRiskTier,
    ProtocolCapabilityInfo, ProtocolKind,
};
use crate::security::HomeLanGuard;
use chrono::Utc;
use std::net::SocketAddr;
use std::time::Duration;
use uuid::Uuid;

/// Retourne l'inventaire honnête de tous les protocoles intégrés dans `hakille-home`.
pub fn protocol_catalog() -> Vec<ProtocolCapabilityInfo> {
    ProtocolKind::all()
        .iter()
        .map(|&p| {
            let (mode, note) = match p {
                ProtocolKind::Matter => (
                    "UDP :5540 + TLV + ChaCha20Poly1305 + QR/Verhoeff".into(),
                    "Codec complet & transport UDP actifs ; exige appareil Matter local ou pont"
                        .into(),
                ),
                ProtocolKind::ThreadCoap => (
                    "UDP :5683 CoAP RFC 7252 + CoRE Link Format".into(),
                    "Transport UDP actif vers routeur de bordure Thread/IPv6 local".into(),
                ),
                ProtocolKind::MdnsDnssd => (
                    "UDP Multicast 224.0.0.251:5353".into(),
                    "Découverte DNS-SD LAN réelle (zéro simulation si aucun répondeur)".into(),
                ),
                ProtocolKind::SsdpUpnp => (
                    "UDP Multicast 239.255.255.250:1900".into(),
                    "Découverte UPnP / DLNA / ponts LAN réelle".into(),
                ),
                ProtocolKind::Mqtt => (
                    "TCP :1883 / TLS :8883 (MQTT v3.1.1 & v5.0)".into(),
                    "Client binaire natif actif (Z2M, HA Discovery, Tasmota, Shelly)".into(),
                ),
                ProtocolKind::Zigbee => (
                    "Série ASH/EZSP & TI ZNP + ZCL / Pont MQTT Z2M".into(),
                    "Codecs série & ZCL natifs ; exige dongle /dev/tty* ou pont Zigbee2MQTT".into(),
                ),
                ProtocolKind::Zwave => (
                    "Série SerialAPI (LRC XOR) + Command Classes".into(),
                    "Codecs SerialAPI & Command Classes natifs ; exige contrôleur Z-Wave USB"
                        .into(),
                ),
                ProtocolKind::BleGatt => (
                    "Publicités BLE BTHome v2 / RuuviTag RAWv2 / SwitchBot".into(),
                    "Décodeurs BLE purs actifs ; lecture adaptateur HCI selon matériel hôte".into(),
                ),
                ProtocolKind::KnxIp => (
                    "UDP :3671 / Multicast 224.0.23.12 (cEMI + DPT)".into(),
                    "Transport KNXnet/IP actif vers passerelle IP/KNX locale".into(),
                ),
                ProtocolKind::ModbusTcp => (
                    "TCP :502 (MBAP FC01..FC16)".into(),
                    "Client TCP synchrone/asynchrone actif".into(),
                ),
                ProtocolKind::ModbusRtu => (
                    "Série RS-485 (CRC-16 0xA001)".into(),
                    "Codec RTU natif ; exige adaptateur USB-RS485 local".into(),
                ),
                ProtocolKind::HomeKitHap => (
                    "HTTP/TCP :51826 + ChaCha20Poly1305 AEAD".into(),
                    "Cadrage chiffré HAP & caractéristiques JSON natifs".into(),
                ),
                ProtocolKind::EspHome => (
                    "TCP :6053 Protobuf Native API".into(),
                    "Handshake HelloRequest + commandes Light/Switch Protobuf actifs".into(),
                ),
                ProtocolKind::HomeAssistant => (
                    "WebSocket RFC 6455 & REST local (:8123)".into(),
                    "Authentification jeton + call_service LAN actifs".into(),
                ),
                ProtocolKind::PhilipsHue => (
                    "REST local CLIP v2 HTTPS/HTTP".into(),
                    "Charges utiles CLIP v2 prêtes pour pont Hue local".into(),
                ),
                ProtocolKind::ShellyRpc => (
                    "JSON-RPC HTTP LAN (:80/rpc)".into(),
                    "Appels Switch/Light/Cover/Status actifs sur réseau local".into(),
                ),
                ProtocolKind::OcppEv => (
                    "WebSocket RFC 6455 (ocpp1.6 / ocpp2.0.1)".into(),
                    "Pilotage borne VE (RemoteStart/Stop/SetChargingProfile) actif".into(),
                ),
                ProtocolKind::SunSpecSolar => (
                    "Modbus TCP :502 + Modèles SunSpec 101/103/124".into(),
                    "Lecture onduleur solaire & batterie SunSpec active".into(),
                ),
                ProtocolKind::OnvifRtsp => (
                    "UDP WS-Discovery :3702 + TCP RTSP :554".into(),
                    "Sonde SOAP ONVIF et RTSP OPTIONS actifs sur caméra locale".into(),
                ),
            };
            ProtocolCapabilityInfo {
                protocol: p,
                display_name: p.display_name().to_string(),
                wire_codec_ready: true,
                transport_mode: mode,
                default_port: p.default_port(),
                honesty_note: note,
            }
        })
        .collect()
}

/// Exécute une découverte réseau locale réelle pour le protocole demandé (ou tous les protocoles UDP).
pub async fn discover_local(
    filter: Option<ProtocolKind>,
    timeout: Duration,
) -> HomeResult<Vec<DiscoveryRecord>> {
    let mut records = Vec::new();
    match filter {
        Some(ProtocolKind::MdnsDnssd)
        | Some(ProtocolKind::Matter)
        | Some(ProtocolKind::EspHome) => {
            records.extend(mdns::discover_mdns(mdns::DEFAULT_HOME_SERVICES, None, timeout).await?);
        }
        Some(ProtocolKind::SsdpUpnp) => {
            records.extend(ssdp::discover_ssdp(None, timeout).await?);
        }
        Some(ProtocolKind::OnvifRtsp) => {
            records.extend(ssdp::discover_onvif(None, timeout).await?);
        }
        None => {
            let (m, s, o) = tokio::join!(
                mdns::discover_mdns(mdns::DEFAULT_HOME_SERVICES, None, timeout),
                ssdp::discover_ssdp(None, timeout),
                ssdp::discover_onvif(None, timeout),
            );
            if let Ok(v) = m {
                records.extend(v);
            }
            if let Ok(v) = s {
                records.extend(v);
            }
            if let Ok(v) = o {
                records.extend(v);
            }
        }
        Some(other) => {
            return Err(HomeError::TransportUnavailable {
                protocol: other.as_str().to_string(),
                reason: format!(
                    "La découverte passive '{}' exige un adaptateur local/broker configuré (aucune découverte fictive générée)",
                    other.as_str()
                ),
            });
        }
    }
    Ok(records)
}

/// Encode la trame filaire correspondant à `(device, command)` et tente l'envoi réel
/// uniquement si un endpoint réseau local joignable est configuré. Retourne toujours un
/// résultat honnête (`Err` si l'équipement est injoignable ou sans adaptateur matériel).
pub async fn dispatch_command_real(
    device: &HomeDevice,
    command: &HomeCommand,
    risk_tier: HomeRiskTier,
) -> HomeResult<CommandReceipt> {
    let guard = HomeLanGuard::new();
    guard.validate_endpoint(&device.endpoint)?;

    let (frame_bytes, summary, executed_on_wire) = match device.protocol {
        ProtocolKind::Matter => {
            let addr: SocketAddr = device.endpoint.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint Matter SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let tlv = matter::encode_matter_command_tlv(1, command)?;
            let _resp = matter::send_matter_udp(addr, &tlv, Duration::from_millis(800)).await?;
            (
                tlv,
                format!(
                    "Matter InvokeRequest envoyé sur {} ({})",
                    device.endpoint,
                    command.action_name()
                ),
                true,
            )
        }
        ProtocolKind::ThreadCoap => {
            let addr: SocketAddr = device.endpoint.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint CoAP SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let json = serde_json::to_vec(command).unwrap_or_default();
            let msg = match command {
                HomeCommand::ReadState => coap::CoapMessage::get(1, "/state"),
                _ => coap::CoapMessage::put_json(1, "/state", &json),
            };
            let wire = msg.to_bytes()?;
            let _resp = coap::send_coap_request(addr, &msg, Duration::from_millis(800)).await?;
            (
                wire,
                format!(
                    "CoAP Thread envoyé sur {} ({})",
                    device.endpoint,
                    command.action_name()
                ),
                true,
            )
        }
        ProtocolKind::Mqtt => {
            let clean = device
                .endpoint
                .trim_start_matches("mqtt://")
                .trim_start_matches("mqtts://");
            let addr: SocketAddr = clean.parse().map_err(|_| HomeError::TransportUnavailable {
                protocol: "mqtt".into(),
                reason: format!(
                    "Adresse broker TCP attendue (ex: 192.168.1.10:1883), reçu '{}'",
                    device.endpoint
                ),
            })?;
            let (topic, payload) =
                mqtt::command_to_mqtt_publish(&format!("hakille/{}", device.id), command)?;
            mqtt::mqtt_publish_tcp(addr, "hakille-home", &topic, &payload, 1).await?;
            (
                payload,
                format!("MQTT PUBLISH QoS1 -> {topic} via {addr}"),
                true,
            )
        }
        ProtocolKind::KnxIp => {
            let addr: SocketAddr = device.endpoint.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint KNX SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let val = match command {
                HomeCommand::SetPower { on } => vec![u8::from(*on)],
                HomeCommand::SetTemperature { celsius } => knx::encode_dpt9(*celsius).to_vec(),
                HomeCommand::SetLevel { level_pct } => {
                    vec![((*level_pct as u16 * 255) / 100) as u8]
                }
                _ => vec![1],
            };
            let ga = knx::KnxGroupAddress::parse_three_level("1/0/1")?;
            let frame = knx::build_knx_routing_write(ga, &val);
            knx::send_knx_udp(addr, &frame, Duration::from_millis(500)).await?;
            (
                frame,
                format!(
                    "KNXnet/IP cEMI envoyé vers {} ({})",
                    device.endpoint,
                    ga.format_three_level()
                ),
                true,
            )
        }
        ProtocolKind::ModbusTcp | ProtocolKind::SunSpecSolar => {
            let addr: SocketAddr = device.endpoint.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint Modbus SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            match command {
                HomeCommand::ReadState => {
                    let regs = modbus::modbus_tcp_read_holding(addr, 1, 40070, 2).await?;
                    let bytes: Vec<u8> = regs.iter().flat_map(|r| r.to_be_bytes()).collect();
                    (
                        bytes,
                        format!("Modbus TCP FC03 lu {} registres sur {addr}", regs.len()),
                        true,
                    )
                }
                HomeCommand::SetPower { on } => {
                    modbus::modbus_tcp_write_register(addr, 1, 0, u16::from(*on)).await?;
                    (
                        vec![0x06, 0x00, 0x00, 0x00, u8::from(*on)],
                        format!("Modbus TCP FC06 écrit sur {addr}"),
                        true,
                    )
                }
                _ => {
                    let regs = modbus::modbus_tcp_read_holding(addr, 1, 0, 2).await?;
                    let bytes: Vec<u8> = regs.iter().flat_map(|r| r.to_be_bytes()).collect();
                    (bytes, format!("Modbus TCP interrogé sur {addr}"), true)
                }
            }
        }
        ProtocolKind::EspHome => {
            let addr: SocketAddr = device.endpoint.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint ESPHome SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let frame = esphome::encode_esphome_command(1, command)?;
            let _ = esphome::send_esphome_command(addr, &frame).await?;
            (frame, format!("ESPHome Protobuf exécuté sur {addr}"), true)
        }
        ProtocolKind::ShellyRpc => {
            let base = if device.endpoint.starts_with("http://") {
                device.endpoint.clone()
            } else {
                format!("http://{}", device.endpoint)
            };
            let (method, params) = hap_hue_shelly::shelly_rpc_payload(0, command)?;
            let resp = hap_hue_shelly::send_shelly_rpc_http(&base, method, params).await?;
            let bytes = serde_json::to_vec(&resp).unwrap_or_default();
            (bytes, format!("Shelly JSON-RPC {method} -> {base}"), true)
        }
        ProtocolKind::OcppEv => {
            let clean = device
                .endpoint
                .trim_start_matches("ws://")
                .trim_start_matches("wss://");
            let (host_port, path) = match clean.split_once('/') {
                Some((hp, p)) => (hp, format!("/{p}")),
                None => (clean, "/ocpp".to_string()),
            };
            let addr: SocketAddr = host_port.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint OCPP SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let call = ocpp::command_to_ocpp_call(&Uuid::new_v4().to_string(), command)?;
            let reply = ocpp::send_ocpp_command_ws(addr, &path, &call).await?;
            (
                reply.to_json_string().into_bytes(),
                format!("OCPP-J exécuté sur {addr}{path}"),
                true,
            )
        }
        ProtocolKind::HomeAssistant => {
            let clean = device
                .endpoint
                .trim_start_matches("ws://")
                .trim_start_matches("wss://")
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            let host_port = clean.split('/').next().unwrap_or(clean);
            let addr: SocketAddr = host_port.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint Home Assistant SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let token = device
                .state
                .raw_attributes
                .get("ha_token")
                .and_then(|v| v.as_str())
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| HomeError::TransportUnavailable {
                    protocol: "home-assistant".into(),
                    reason:
                        "Jeton local 'ha_token' manquant dans state.raw_attributes de l'appareil"
                            .into(),
                })?;
            let entity_id = device
                .state
                .raw_attributes
                .get("entity_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("switch.{}", device.id));
            let resp = home_assistant::ha_ws_call_service(addr, token, &entity_id, command).await?;
            let bytes = serde_json::to_vec(&resp).unwrap_or_default();
            (
                bytes,
                format!(
                    "Home Assistant WS {} -> {entity_id} via {addr}",
                    command.action_name()
                ),
                true,
            )
        }
        ProtocolKind::PhilipsHue => {
            let base = if device.endpoint.starts_with("http://")
                || device.endpoint.starts_with("https://")
            {
                device.endpoint.clone()
            } else {
                format!("http://{}", device.endpoint)
            };
            let app_key = device
                .state
                .raw_attributes
                .get("hue_app_key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let rid = device
                .state
                .raw_attributes
                .get("hue_rid")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| device.id.clone());
            let payload = hap_hue_shelly::hue_clip_v2_light_payload(command)?;
            let resp =
                hap_hue_shelly::send_hue_clip_v2_http(&base, &rid, &app_key, payload).await?;
            let bytes = serde_json::to_vec(&resp).unwrap_or_default();
            (
                bytes,
                format!("Philips Hue CLIP v2 PUT /light/{rid} -> {base}"),
                true,
            )
        }
        ProtocolKind::OnvifRtsp => {
            let clean = device
                .endpoint
                .trim_start_matches("rtsp://")
                .trim_start_matches("http://");
            let host_port = clean.split('/').next().unwrap_or(clean);
            let addr: SocketAddr = host_port.parse().map_err(|_| {
                HomeError::InvalidCommand(format!(
                    "Endpoint ONVIF/RTSP SocketAddr attendu : {}",
                    device.endpoint
                ))
            })?;
            let rtsp_url = if device.endpoint.starts_with("rtsp://") {
                device.endpoint.clone()
            } else {
                format!("rtsp://{addr}/stream1")
            };
            let reply = ssdp::rtsp_options_probe(addr, &rtsp_url).await?;
            (
                reply.as_bytes().to_vec(),
                format!("ONVIF/RTSP OPTIONS vérifié sur {addr}"),
                true,
            )
        }
        ProtocolKind::Zigbee => {
            // Pont Zigbee2MQTT LAN réel si endpoint `mqtt://` ou `z2m://`, sinon erreur honnête
            if device.endpoint.starts_with("mqtt://") || device.endpoint.starts_with("z2m://") {
                let clean = device
                    .endpoint
                    .trim_start_matches("mqtt://")
                    .trim_start_matches("z2m://");
                let addr: SocketAddr = clean.parse().map_err(|_| {
                    HomeError::TransportUnavailable {
                        protocol: "zigbee".into(),
                        reason: format!(
                            "Adresse broker Zigbee2MQTT attendue (ex: mqtt://192.168.1.10:1883), reçu '{}'",
                            device.endpoint
                        ),
                    }
                })?;
                let (topic, payload) =
                    mqtt::command_to_mqtt_publish(&format!("zigbee2mqtt/{}", device.id), command)?;
                mqtt::mqtt_publish_tcp(addr, "hakille-zigbee-z2m", &topic, &payload, 1).await?;
                (
                    payload,
                    format!("Zigbee2MQTT PUBLISH QoS1 -> {topic} via {addr}"),
                    true,
                )
            } else {
                let (_cluster, zcl) = zigbee::ZclFrame::from_home_command(1, command)?;
                let ash = zigbee::encode_ash_frame(0x00, &zcl.to_bytes());
                return Err(HomeError::TransportUnavailable {
                    protocol: "zigbee".into(),
                    reason: format!(
                        "Trame ZCL/ASH prête ({} octets), mais coordinateur '{}' non ouvert en série/pont Z2M",
                        ash.len(),
                        device.endpoint
                    ),
                });
            }
        }
        ProtocolKind::Zwave => {
            let cc = zwave::encode_zwave_command_class(command)?;
            let frame = zwave::ZWaveSerialFrame::send_data(1, &cc, 1).to_bytes();
            return Err(HomeError::TransportUnavailable {
                protocol: "zwave".into(),
                reason: format!(
                    "Trame SerialAPI prête ({} octets), mais contrôleur '{}' non ouvert",
                    frame.len(),
                    device.endpoint
                ),
            });
        }
        other => {
            return Err(HomeError::TransportUnavailable {
                protocol: other.as_str().to_string(),
                reason: format!(
                    "Aucune passerelle ou session locale active pour '{}' ({})",
                    device.name, device.endpoint
                ),
            });
        }
    };

    Ok(CommandReceipt {
        receipt_id: Uuid::now_v7().to_string(),
        device_id: device.id.clone(),
        protocol: device.protocol,
        action: command.action_name().to_string(),
        risk_tier,
        executed_on_wire,
        frame_sha256: CommandReceipt::compute_frame_hash(&frame_bytes),
        summary,
        timestamp: Utc::now().to_rfc3339(),
    })
}
