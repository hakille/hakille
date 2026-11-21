//! Découverte mDNS / DNS-SD RFC 6762 & RFC 6763 en Rust pur.
//!
//! Encode et décode les paquets DNS binaires (compression de pointeurs `0xC0`, PTR, SRV,
//! TXT, A, AAAA) et interroge le groupe multicast local `224.0.0.251:5353` pour tous
//! les services domotiques standards.

use crate::model::{DeviceCategory, DiscoveryRecord, HomeError, HomeResult, ProtocolKind};
use chrono::Utc;
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const MDNS_MULTICAST_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;

/// Services DNS-SD domotiques interrogés par Hakille.
pub const DEFAULT_HOME_SERVICES: &[&str] = &[
    "_matter._tcp.local",
    "_matterc._udp.local",
    "_hap._tcp.local",
    "_esphomelib._tcp.local",
    "_hue._tcp.local",
    "_shelly._tcp.local",
    "_googlecast._tcp.local",
    "_airplay._tcp.local",
    "_mqtt._tcp.local",
    "_sonos._tcp.local",
    "_home-assistant._tcp.local",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsRecordData {
    A(Ipv4Addr),
    AAAA(std::net::Ipv6Addr),
    Ptr(String),
    Txt(Vec<(String, String)>),
    Srv {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
    Raw(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsResourceRecord {
    pub name: String,
    pub rr_type: u16,
    pub rr_class: u16,
    pub ttl: u32,
    pub data: DnsRecordData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsPacket {
    pub id: u16,
    pub flags: u16,
    pub questions: Vec<(String, u16, u16)>,
    pub answers: Vec<DnsResourceRecord>,
    pub additionals: Vec<DnsResourceRecord>,
}

/// Encode un nom DNS en labels binaires (ex: `_matter._tcp.local` -> `\x07_matter\x04_tcp\x05local\x00`).
pub fn encode_dns_name(name: &str, out: &mut Vec<u8>) -> HomeResult<()> {
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(HomeError::InvalidFrame {
                protocol: "mdns".into(),
                details: format!("Label DNS invalide : '{label}'"),
            });
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    Ok(())
}

/// Décode un nom DNS en gérant les pointeurs de compression RFC 1035 (`0xC0`) avec limite anti-boucle.
pub fn decode_dns_name(packet: &[u8], offset: usize) -> HomeResult<(String, usize)> {
    let mut labels = Vec::new();
    let mut pos = offset;
    let mut next_pos_after_first_ptr: Option<usize> = None;
    let mut jumps = 0u8;

    loop {
        if pos >= packet.len() {
            return Err(HomeError::InvalidFrame {
                protocol: "mdns".into(),
                details: "Troncature lors de la lecture d'un nom DNS".into(),
            });
        }
        let len = packet[pos];
        if len == 0 {
            let final_next = next_pos_after_first_ptr.unwrap_or(pos + 1);
            return Ok((labels.join("."), final_next));
        }
        if (len & 0xC0) == 0xC0 {
            if pos + 1 >= packet.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "mdns".into(),
                    details: "Pointeur de compression DNS tronqué".into(),
                });
            }
            if next_pos_after_first_ptr.is_none() {
                next_pos_after_first_ptr = Some(pos + 2);
            }
            let ptr = (((len & 0x3F) as usize) << 8) | (packet[pos + 1] as usize);
            jumps += 1;
            if jumps > 16 || ptr >= packet.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "mdns".into(),
                    details: "Boucle ou pointeur de compression DNS hors limites".into(),
                });
            }
            pos = ptr;
            continue;
        }
        let label_len = len as usize;
        pos += 1;
        if pos + label_len > packet.len() {
            return Err(HomeError::InvalidFrame {
                protocol: "mdns".into(),
                details: "Label DNS hors bornes".into(),
            });
        }
        labels.push(String::from_utf8_lossy(&packet[pos..pos + label_len]).into_owned());
        pos += label_len;
    }
}

/// Construit une requête mDNS PTR standard pour une liste de types de services DNS-SD.
pub fn build_mdns_query(services: &[&str]) -> HomeResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(64 * services.len().max(1));
    // Header DNS : ID=0, Flags=0 (Standard Query), QDCOUNT, AN=0, NS=0, AR=0
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&(services.len() as u16).to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    for svc in services {
        encode_dns_name(svc, &mut buf)?;
        buf.extend_from_slice(&12u16.to_be_bytes()); // QTYPE = PTR
        buf.extend_from_slice(&0x0001u16.to_be_bytes()); // QCLASS = IN
    }
    Ok(buf)
}

/// Construit une réponse mDNS (PTR + SRV + TXT + A) — utile pour annoncer un pont ou tester en loopback.
pub fn build_mdns_service_response(
    service_type: &str,
    instance_name: &str,
    host_name: &str,
    ip: Ipv4Addr,
    port: u16,
    txt: &[(&str, &str)],
) -> HomeResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);
    // QR=1, AA=1 -> 0x8400, QD=0, AN=4
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0x8400u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&4u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    // 1. PTR
    encode_dns_name(service_type, &mut buf)?;
    buf.extend_from_slice(&12u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&120u32.to_be_bytes());
    let mut ptr_rdata = Vec::new();
    encode_dns_name(instance_name, &mut ptr_rdata)?;
    buf.extend_from_slice(&(ptr_rdata.len() as u16).to_be_bytes());
    buf.extend_from_slice(&ptr_rdata);

    // 2. SRV
    encode_dns_name(instance_name, &mut buf)?;
    buf.extend_from_slice(&33u16.to_be_bytes());
    buf.extend_from_slice(&0x8001u16.to_be_bytes());
    buf.extend_from_slice(&120u32.to_be_bytes());
    let mut srv_rdata = Vec::new();
    srv_rdata.extend_from_slice(&0u16.to_be_bytes());
    srv_rdata.extend_from_slice(&0u16.to_be_bytes());
    srv_rdata.extend_from_slice(&port.to_be_bytes());
    encode_dns_name(host_name, &mut srv_rdata)?;
    buf.extend_from_slice(&(srv_rdata.len() as u16).to_be_bytes());
    buf.extend_from_slice(&srv_rdata);

    // 3. TXT
    encode_dns_name(instance_name, &mut buf)?;
    buf.extend_from_slice(&16u16.to_be_bytes());
    buf.extend_from_slice(&0x8001u16.to_be_bytes());
    buf.extend_from_slice(&120u32.to_be_bytes());
    let mut txt_rdata = Vec::new();
    for (k, v) in txt {
        let entry = format!("{k}={v}");
        if entry.len() <= 255 {
            txt_rdata.push(entry.len() as u8);
            txt_rdata.extend_from_slice(entry.as_bytes());
        }
    }
    if txt_rdata.is_empty() {
        txt_rdata.push(0);
    }
    buf.extend_from_slice(&(txt_rdata.len() as u16).to_be_bytes());
    buf.extend_from_slice(&txt_rdata);

    // 4. A
    encode_dns_name(host_name, &mut buf)?;
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0x8001u16.to_be_bytes());
    buf.extend_from_slice(&120u32.to_be_bytes());
    buf.extend_from_slice(&4u16.to_be_bytes());
    buf.extend_from_slice(&ip.octets());

    Ok(buf)
}

/// Décode un paquet mDNS complet.
pub fn parse_mdns_packet(packet: &[u8]) -> HomeResult<MdnsPacket> {
    if packet.len() < 12 {
        return Err(HomeError::InvalidFrame {
            protocol: "mdns".into(),
            details: "Paquet DNS < 12 octets".into(),
        });
    }
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let ancount = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let nscount = u16::from_be_bytes([packet[8], packet[9]]) as usize;
    let arcount = u16::from_be_bytes([packet[10], packet[11]]) as usize;

    let mut pos = 12;
    let mut questions = Vec::with_capacity(qdcount);
    for _ in 0..qdcount {
        let (name, next) = decode_dns_name(packet, pos)?;
        if next + 4 > packet.len() {
            return Err(HomeError::InvalidFrame {
                protocol: "mdns".into(),
                details: "Question DNS tronquée".into(),
            });
        }
        let qtype = u16::from_be_bytes([packet[next], packet[next + 1]]);
        let qclass = u16::from_be_bytes([packet[next + 2], packet[next + 3]]);
        questions.push((name, qtype, qclass));
        pos = next + 4;
    }

    let mut answers = Vec::with_capacity(ancount);
    for _ in 0..ancount {
        let (rr, next) = parse_rr(packet, pos)?;
        answers.push(rr);
        pos = next;
    }
    for _ in 0..nscount {
        let (_rr, next) = parse_rr(packet, pos)?;
        pos = next;
    }
    let mut additionals = Vec::with_capacity(arcount);
    for _ in 0..arcount {
        let (rr, next) = parse_rr(packet, pos)?;
        additionals.push(rr);
        pos = next;
    }

    Ok(MdnsPacket {
        id,
        flags,
        questions,
        answers,
        additionals,
    })
}

fn parse_rr(packet: &[u8], offset: usize) -> HomeResult<(DnsResourceRecord, usize)> {
    let (name, pos) = decode_dns_name(packet, offset)?;
    if pos + 10 > packet.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "mdns".into(),
            details: "Enregistrement RR DNS tronqué".into(),
        });
    }
    let rr_type = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
    let rr_class = u16::from_be_bytes([packet[pos + 2], packet[pos + 3]]);
    let ttl = u32::from_be_bytes([
        packet[pos + 4],
        packet[pos + 5],
        packet[pos + 6],
        packet[pos + 7],
    ]);
    let rdlen = u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]) as usize;
    let rdata_start = pos + 10;
    let rdata_end = rdata_start + rdlen;
    if rdata_end > packet.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "mdns".into(),
            details: "RDATA DNS hors bornes".into(),
        });
    }
    let rdata = &packet[rdata_start..rdata_end];

    let data = match rr_type {
        1 if rdlen == 4 => DnsRecordData::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
        28 if rdlen == 16 => {
            let arr: [u8; 16] = rdata.try_into().unwrap();
            DnsRecordData::AAAA(std::net::Ipv6Addr::from(arr))
        }
        12 => {
            let (target, _) = decode_dns_name(packet, rdata_start)?;
            DnsRecordData::Ptr(target)
        }
        16 => {
            let mut pairs = Vec::new();
            let mut idx = 0;
            while idx < rdata.len() {
                let len = rdata[idx] as usize;
                idx += 1;
                if idx + len > rdata.len() {
                    break;
                }
                let s = String::from_utf8_lossy(&rdata[idx..idx + len]);
                if let Some((k, v)) = s.split_once('=') {
                    pairs.push((k.to_string(), v.to_string()));
                } else if !s.is_empty() {
                    pairs.push((s.into_owned(), String::new()));
                }
                idx += len;
            }
            DnsRecordData::Txt(pairs)
        }
        33 if rdlen >= 6 => {
            let priority = u16::from_be_bytes([rdata[0], rdata[1]]);
            let weight = u16::from_be_bytes([rdata[2], rdata[3]]);
            let port = u16::from_be_bytes([rdata[4], rdata[5]]);
            let (target, _) = decode_dns_name(packet, rdata_start + 6)?;
            DnsRecordData::Srv {
                priority,
                weight,
                port,
                target,
            }
        }
        _ => DnsRecordData::Raw(rdata.to_vec()),
    };

    Ok((
        DnsResourceRecord {
            name,
            rr_type,
            rr_class,
            ttl,
            data,
        },
        rdata_end,
    ))
}

/// Convertit un paquet mDNS en `DiscoveryRecord` structurés.
pub fn packet_to_discoveries(packet: &MdnsPacket, source_ip: SocketAddr) -> Vec<DiscoveryRecord> {
    let all_rrs: Vec<&DnsResourceRecord> = packet
        .answers
        .iter()
        .chain(packet.additionals.iter())
        .collect();

    let mut srv_by_instance: BTreeMap<String, (String, u16)> = BTreeMap::new();
    let mut txt_by_instance: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut addr_by_host: BTreeMap<String, String> = BTreeMap::new();
    let mut ptrs: Vec<(String, String)> = Vec::new();

    for rr in &all_rrs {
        match &rr.data {
            DnsRecordData::Ptr(target) => ptrs.push((rr.name.clone(), target.clone())),
            DnsRecordData::Srv { port, target, .. } => {
                srv_by_instance.insert(rr.name.clone(), (target.clone(), *port));
            }
            DnsRecordData::Txt(pairs) => {
                let entry = txt_by_instance.entry(rr.name.clone()).or_default();
                for (k, v) in pairs {
                    entry.insert(k.clone(), v.clone());
                }
            }
            DnsRecordData::A(ip) => {
                addr_by_host.insert(rr.name.clone(), ip.to_string());
            }
            DnsRecordData::AAAA(ip) => {
                addr_by_host
                    .entry(rr.name.clone())
                    .or_insert_with(|| format!("[{ip}]"));
            }
            DnsRecordData::Raw(_) => {}
        }
    }

    let now = Utc::now().to_rfc3339();
    let mut out = Vec::new();

    for (svc_type, instance) in ptrs {
        let (proto, cat) = classify_mdns_service(&svc_type);
        let (host, port) = srv_by_instance
            .get(&instance)
            .cloned()
            .unwrap_or_else(|| (String::new(), proto.default_port().unwrap_or(0)));
        let ip = addr_by_host
            .get(&host)
            .cloned()
            .unwrap_or_else(|| source_ip.ip().to_string());
        let endpoint = if port > 0 { format!("{ip}:{port}") } else { ip };
        let mut txt = txt_by_instance.remove(&instance).unwrap_or_default();
        txt.insert("mdns_service".into(), svc_type);
        if !host.is_empty() {
            txt.insert("mdns_host".into(), host);
        }
        out.push(DiscoveryRecord {
            protocol: proto,
            service_name: instance,
            endpoint,
            inferred_category: cat,
            txt_metadata: txt,
            discovered_at: now.clone(),
        });
    }

    out
}

pub fn classify_mdns_service(svc: &str) -> (ProtocolKind, DeviceCategory) {
    let lower = svc.to_ascii_lowercase();
    if lower.contains("_matter") {
        (ProtocolKind::Matter, DeviceCategory::Light)
    } else if lower.contains("_hap") {
        (ProtocolKind::HomeKitHap, DeviceCategory::Switch)
    } else if lower.contains("_esphomelib") {
        (ProtocolKind::EspHome, DeviceCategory::Sensor)
    } else if lower.contains("_hue") {
        (ProtocolKind::PhilipsHue, DeviceCategory::BridgeCoordinator)
    } else if lower.contains("_shelly") {
        (ProtocolKind::ShellyRpc, DeviceCategory::EnergyMeter)
    } else if lower.contains("_mqtt") {
        (ProtocolKind::Mqtt, DeviceCategory::BridgeCoordinator)
    } else if lower.contains("_home-assistant") {
        (
            ProtocolKind::HomeAssistant,
            DeviceCategory::BridgeCoordinator,
        )
    } else if lower.contains("_googlecast")
        || lower.contains("_sonos")
        || lower.contains("_airplay")
    {
        (ProtocolKind::MdnsDnssd, DeviceCategory::Speaker)
    } else {
        (ProtocolKind::MdnsDnssd, DeviceCategory::Sensor)
    }
}

/// Exécute une découverte mDNS UDP réelle vers `target` (par défaut `224.0.0.251:5353`)
/// avec fenêtre d'écoute `timeout`. Retourne liste vide si aucun répondeur local (honnête).
pub async fn discover_mdns(
    services: &[&str],
    target: Option<SocketAddr>,
    timeout: Duration,
) -> HomeResult<Vec<DiscoveryRecord>> {
    let query = build_mdns_query(services)?;
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "mdns bind".into(),
            message: e.to_string(),
        })?;
    let dest =
        target.unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(MDNS_MULTICAST_V4, MDNS_PORT)));
    sock.send_to(&query, dest)
        .await
        .map_err(|e| HomeError::Io {
            context: "mdns send_to".into(),
            message: e.to_string(),
        })?;

    let mut discovered = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let rem = deadline - now;
        match tokio::time::timeout(rem, sock.recv_from(&mut buf)).await {
            Ok(Ok((n, addr))) => {
                if let Ok(pkt) = parse_mdns_packet(&buf[..n]) {
                    discovered.extend(packet_to_discoveries(&pkt, addr));
                }
            }
            Ok(Err(_)) | Err(_) => break,
        }
    }

    Ok(discovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mdns_binary_codec_and_loopback_discovery() {
        let responder = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resp_addr = responder.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let (n, peer) = responder.recv_from(&mut buf).await.unwrap();
            let q = parse_mdns_packet(&buf[..n]).unwrap();
            assert_eq!(q.questions[0].0, "_matter._tcp.local");
            let reply = build_mdns_service_response(
                "_matter._tcp.local",
                "Lampe-Salon._matter._tcp.local",
                "lampe-salon.local",
                Ipv4Addr::new(192, 168, 1, 77),
                5540,
                &[("D", "3840"), ("VP", "65521+32768")],
            )
            .unwrap();
            responder.send_to(&reply, peer).await.unwrap();
        });

        let found = discover_mdns(
            &["_matter._tcp.local"],
            Some(resp_addr),
            Duration::from_millis(500),
        )
        .await
        .unwrap();
        server_task.await.unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].protocol, ProtocolKind::Matter);
        assert_eq!(found[0].endpoint, "192.168.1.77:5540");
        assert_eq!(found[0].txt_metadata.get("D").unwrap(), "3840");
    }
}
