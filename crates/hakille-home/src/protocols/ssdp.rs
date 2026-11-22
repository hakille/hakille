//! Découverte SSDP / UPnP LAN, ONVIF WS-Discovery (caméras locales) et RTSP OPTIONS.
//!
//! Fonctionne 100% en local sur UDP (`239.255.255.250:1900` et `:3702`) sans dépendre d'aucun
//! serveur cloud tiers.

use crate::model::{DeviceCategory, DiscoveryRecord, HomeError, HomeResult, ProtocolKind};
use chrono::Utc;
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

pub const SSDP_MULTICAST_V4: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const SSDP_PORT: u16 = 1900;
pub const ONVIF_WSD_PORT: u16 = 3702;

/// Construit une requête HTTP-over-UDP SSDP M-SEARCH.
pub fn build_ssdp_msearch(st: &str, mx_secs: u8) -> Vec<u8> {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: {mx_secs}\r\n\
         ST: {st}\r\n\
         USER-AGENT: Hakille-Home/1.0 UPnP/1.1\r\n\r\n"
    )
    .into_bytes()
}

/// Parse une réponse HTTP SSDP (`HTTP/1.1 200 OK` ou `NOTIFY *`).
pub fn parse_ssdp_response(raw: &[u8], peer: SocketAddr) -> Option<DiscoveryRecord> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.lines();
    let first = lines.next()?.trim();
    if !first.starts_with("HTTP/1.1 200") && !first.starts_with("NOTIFY *") {
        return None;
    }

    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let st = headers
        .get("st")
        .or_else(|| headers.get("nt"))
        .cloned()
        .unwrap_or_else(|| "upnp:rootdevice".into());
    let location = headers
        .get("location")
        .cloned()
        .unwrap_or_else(|| peer.to_string());
    let server = headers
        .get("server")
        .cloned()
        .unwrap_or_else(|| "UPnP Device".into());

    let st_lower = st.to_ascii_lowercase();
    let category = if st_lower.contains("mediarenderer") || st_lower.contains("sonos") {
        DeviceCategory::MediaPlayer
    } else {
        DeviceCategory::BridgeCoordinator
    };

    Some(DiscoveryRecord {
        protocol: ProtocolKind::SsdpUpnp,
        service_name: format!("{server} ({st})"),
        endpoint: location,
        inferred_category: category,
        txt_metadata: headers,
        discovered_at: Utc::now().to_rfc3339(),
    })
}

/// Construit une sonde SOAP WS-Discovery ONVIF (Profile S/T/G caméras IP locales).
pub fn build_onvif_probe(message_uuid: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope"
            xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing"
            xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery"
            xmlns:dn="http://www.onvif.org/ver10/network/wsdl">
  <e:Header>
    <w:MessageID>uuid:{message_uuid}</w:MessageID>
    <w:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To>
    <w:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action>
  </e:Header>
  <e:Body>
    <d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe>
  </e:Body>
</e:Envelope>"#
    )
    .into_bytes()
}

/// Extrait l'URL `XAddrs` d'un paquet XML `ProbeMatches` ONVIF.
pub fn parse_onvif_probe_match(raw: &[u8], peer: SocketAddr) -> Option<DiscoveryRecord> {
    let xml = std::str::from_utf8(raw).ok()?;
    if !xml.contains("ProbeMatch") {
        return None;
    }
    let xaddr = extract_xml_tag(xml, "XAddrs").unwrap_or_else(|| peer.to_string());
    let first_url = xaddr
        .split_whitespace()
        .next()
        .unwrap_or(&xaddr)
        .to_string();
    let scopes = extract_xml_tag(xml, "Scopes").unwrap_or_default();

    let mut meta = BTreeMap::new();
    meta.insert("onvif_scopes".into(), scopes);
    meta.insert("xaddrs".into(), xaddr);

    Some(DiscoveryRecord {
        protocol: ProtocolKind::OnvifRtsp,
        service_name: format!("Caméra ONVIF ({})", peer.ip()),
        endpoint: first_url,
        inferred_category: DeviceCategory::SecurityCamera,
        txt_metadata: meta,
        discovered_at: Utc::now().to_rfc3339(),
    })
}

fn extract_xml_tag(xml: &str, tag_suffix: &str) -> Option<String> {
    let open_pattern = format!("{tag_suffix}>");
    let start_marker = xml.find(&open_pattern)?;
    let content_start = start_marker + open_pattern.len();
    let rest = &xml[content_start..];
    let close_idx = rest.find("</")?;
    Some(rest[..close_idx].trim().to_string())
}

/// Découverte SSDP UDP asynchrone.
pub async fn discover_ssdp(
    target: Option<SocketAddr>,
    timeout: Duration,
) -> HomeResult<Vec<DiscoveryRecord>> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "ssdp bind".into(),
            message: e.to_string(),
        })?;
    let dest =
        target.unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(SSDP_MULTICAST_V4, SSDP_PORT)));
    let payload = build_ssdp_msearch("ssdp:all", 1);
    sock.send_to(&payload, dest)
        .await
        .map_err(|e| HomeError::Io {
            context: "ssdp send".into(),
            message: e.to_string(),
        })?;

    let mut results = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, sock.recv_from(&mut buf)).await {
            Ok(Ok((n, peer))) => {
                if let Some(rec) = parse_ssdp_response(&buf[..n], peer) {
                    results.push(rec);
                }
            }
            _ => break,
        }
    }
    Ok(results)
}

/// Découverte ONVIF WS-Discovery UDP asynchrone.
pub async fn discover_onvif(
    target: Option<SocketAddr>,
    timeout: Duration,
) -> HomeResult<Vec<DiscoveryRecord>> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "onvif bind".into(),
            message: e.to_string(),
        })?;
    let dest = target
        .unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(SSDP_MULTICAST_V4, ONVIF_WSD_PORT)));
    let probe = build_onvif_probe(&uuid::Uuid::new_v4().to_string());
    sock.send_to(&probe, dest)
        .await
        .map_err(|e| HomeError::Io {
            context: "onvif send".into(),
            message: e.to_string(),
        })?;

    let mut results = Vec::new();
    let mut buf = [0u8; 8192];
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, sock.recv_from(&mut buf)).await {
            Ok(Ok((n, peer))) => {
                if let Some(rec) = parse_onvif_probe_match(&buf[..n], peer) {
                    results.push(rec);
                }
            }
            _ => break,
        }
    }
    Ok(results)
}

/// Envoie un probe RTSP `OPTIONS` sur une caméra IP locale et vérifie le code 200 OK.
pub async fn rtsp_options_probe(addr: SocketAddr, rtsp_url: &str) -> HomeResult<String> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| HomeError::Io {
        context: "rtsp connect".into(),
        message: e.to_string(),
    })?;
    let req =
        format!("OPTIONS {rtsp_url} RTSP/1.0\r\nCSeq: 1\r\nUser-Agent: Hakille-Home/1.0\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| HomeError::Io {
            context: "rtsp write".into(),
            message: e.to_string(),
        })?;
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.map_err(|e| HomeError::Io {
        context: "rtsp read".into(),
        message: e.to_string(),
    })?;
    let resp = String::from_utf8_lossy(&buf[..n]).into_owned();
    if !resp.starts_with("RTSP/1.0 200") {
        return Err(HomeError::InvalidFrame {
            protocol: "rtsp".into(),
            details: format!("Réponse RTSP inattendue : {resp}"),
        });
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ssdp_and_onvif_loopback_discovery() {
        let ssdp_srv = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ssdp_addr = ssdp_srv.local_addr().unwrap();
        tokio::spawn(async move {
            let mut b = [0u8; 512];
            let (_n, peer) = ssdp_srv.recv_from(&mut b).await.unwrap();
            let reply = b"HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.20/description.xml\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\nSERVER: Sonos/1.0\r\n\r\n";
            ssdp_srv.send_to(reply, peer).await.unwrap();
        });

        let found_ssdp = discover_ssdp(Some(ssdp_addr), Duration::from_millis(400))
            .await
            .unwrap();
        assert_eq!(found_ssdp.len(), 1);
        assert_eq!(found_ssdp[0].inferred_category, DeviceCategory::MediaPlayer);

        let onvif_srv = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onvif_addr = onvif_srv.local_addr().unwrap();
        tokio::spawn(async move {
            let mut b = [0u8; 2048];
            let (_n, peer) = onvif_srv.recv_from(&mut b).await.unwrap();
            let xml = b"<Envelope><Body><ProbeMatches><ProbeMatch><d:XAddrs>http://192.168.1.90/onvif/device_service</d:XAddrs><d:Scopes>onvif://www.onvif.org/Profile/Streaming</d:Scopes></ProbeMatch></ProbeMatches></Body></Envelope>";
            onvif_srv.send_to(xml, peer).await.unwrap();
        });

        let found_onvif = discover_onvif(Some(onvif_addr), Duration::from_millis(400))
            .await
            .unwrap();
        assert_eq!(found_onvif.len(), 1);
        assert_eq!(found_onvif[0].protocol, ProtocolKind::OnvifRtsp);
        assert_eq!(
            found_onvif[0].endpoint,
            "http://192.168.1.90/onvif/device_service"
        );
    }
}
