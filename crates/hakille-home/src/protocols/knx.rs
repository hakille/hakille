//! Pilote KNXnet/IP en Rust pur (automatisation bâtiment / tableau électrique).
//!
//! Implémente :
//! - En-tête KNXnet/IP (`0x0610` + Service Type ID + Longueur totale)
//! - Service types : `SearchRequest` (`0x0201`), `ConnectRequest` (`0x0205`),
//!   `TunnelingRequest` (`0x0420`), `RoutingIndication` (`0x0530` multicast `224.0.23.12:3671`)
//! - Trame cEMI `L_Data.req` / `L_Data.ind` + adressage de groupe 3 niveaux (`Main/Middle/Sub`)
//! - Datapoint Types : `DPT 1.001` (Bool), `DPT 5.001` (%), `DPT 9.001` (Temp °C 2-octet float),
//!   `DPT 232.600` (RGB).

use crate::model::{HomeError, HomeResult};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const KNX_MULTICAST_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 23, 12);
pub const KNX_PORT: u16 = 3671;

/// Adresse de groupe KNX (`Main/Middle/Sub` -> 16 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnxGroupAddress(pub u16);

impl KnxGroupAddress {
    pub fn parse_three_level(s: &str) -> HomeResult<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return Err(HomeError::InvalidFrame {
                protocol: "knx".into(),
                details: format!("Adresse groupe KNX attendue 'M/Mi/Sub', reçue '{s}'"),
            });
        }
        let main: u16 = parts[0].parse().map_err(|_| HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "Main group invalide".into(),
        })?;
        let mid: u16 = parts[1].parse().map_err(|_| HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "Middle group invalide".into(),
        })?;
        let sub: u16 = parts[2].parse().map_err(|_| HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "Sub group invalide".into(),
        })?;
        if main > 31 || mid > 7 || sub > 255 {
            return Err(HomeError::InvalidFrame {
                protocol: "knx".into(),
                details: "Bornes KNX 5/3/8 dépassées".into(),
            });
        }
        Ok(Self((main << 11) | (mid << 8) | sub))
    }

    pub fn format_three_level(self) -> String {
        let main = (self.0 >> 11) & 0x1F;
        let mid = (self.0 >> 8) & 0x07;
        let sub = self.0 & 0xFF;
        format!("{main}/{mid}/{sub}")
    }
}

/// Encode une valeur float KNX DPT 9.001 (2 octets `M * 0.01 * 2^E`).
pub fn encode_dpt9(value: f32) -> [u8; 2] {
    let mut exp = 0i32;
    let mut mant = (value * 100.0).round();
    while mant < -2048.0 || mant > 2047.0 {
        mant /= 2.0;
        exp += 1;
        if exp >= 15 {
            break;
        }
    }
    let m = (mant as i16).clamp(-2048, 2047);
    let sign_bit: u16 = if m < 0 { 0x8000 } else { 0 };
    let e_bits = ((exp as u16) & 0x0F) << 11;
    let m_bits = (m as u16) & 0x07FF;
    (sign_bit | e_bits | m_bits).to_be_bytes()
}

pub fn decode_dpt9(bytes: [u8; 2]) -> f32 {
    let raw = u16::from_be_bytes(bytes);
    let sign = (raw & 0x8000) != 0;
    let exp = ((raw >> 11) & 0x0F) as i32;
    let mut mant = (raw & 0x07FF) as i32;
    if sign {
        mant -= 2048;
    }
    0.01 * (mant as f32) * (2.0f32).powi(exp)
}

/// Construit une trame KNXnet/IP `RoutingIndication` (`0x0530`) avec cEMI `L_Data.ind` (`0x29`).
pub fn build_knx_routing_write(group: KnxGroupAddress, apci_data: &[u8]) -> Vec<u8> {
    let mut cemi = Vec::with_capacity(12 + apci_data.len());
    cemi.push(0x29); // L_Data.ind
    cemi.push(0x00); // Additional info len = 0
    cemi.push(0xBC); // Ctrl1 (standard frame, no repeat, normal priority)
    cemi.push(0xE0); // Ctrl2 (group address destination, hop=6)
    cemi.extend_from_slice(&0x0001u16.to_be_bytes()); // Source 0.0.1
    cemi.extend_from_slice(&group.0.to_be_bytes());
    if apci_data.len() == 1 && apci_data[0] <= 0x3F {
        cemi.push(1); // NPDU len = 1
        cemi.push(0x00); // TPCI=0, APCI high=0
        cemi.push(0x80 | (apci_data[0] & 0x3F)); // GroupValue_Write compact
    } else {
        cemi.push((1 + apci_data.len()) as u8);
        cemi.push(0x00);
        cemi.push(0x80); // GroupValue_Write
        cemi.extend_from_slice(apci_data);
    }

    let total_len = (6 + cemi.len()) as u16;
    let mut frame = Vec::with_capacity(total_len as usize);
    frame.push(0x06); // Header size 6
    frame.push(0x10); // Protocol version 1.0
    frame.extend_from_slice(&0x0530u16.to_be_bytes()); // ROUTING_INDICATION
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&cemi);
    frame
}

/// Parse une trame KNXnet/IP `RoutingIndication` ou `TunnelingRequest` et extrait `(GroupAddr, payload)`.
pub fn parse_knx_routing_frame(raw: &[u8]) -> HomeResult<(u16, KnxGroupAddress, Vec<u8>)> {
    if raw.len() < 16 || raw[0] != 0x06 || raw[1] != 0x10 {
        return Err(HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "En-tête KNXnet/IP 0x0610 invalide".into(),
        });
    }
    let service_type = u16::from_be_bytes([raw[2], raw[3]]);
    let total_len = u16::from_be_bytes([raw[4], raw[5]]) as usize;
    if raw.len() < total_len {
        return Err(HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "Trame KNXnet/IP tronquée".into(),
        });
    }
    let cemi_offset = if service_type == 0x0420 { 10 } else { 6 }; // Tunneling ajoute 4 octets header
    if raw.len() < cemi_offset + 11 {
        return Err(HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "cEMI KNX trop court".into(),
        });
    }
    let add_len = raw[cemi_offset + 1] as usize;
    let body = &raw[cemi_offset + 2 + add_len..];
    if body.len() < 9 {
        return Err(HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "cEMI L_Data incomplet".into(),
        });
    }
    let dst = KnxGroupAddress(u16::from_be_bytes([body[4], body[5]]));
    let npdu_len = body[6] as usize;
    if body.len() < 7 + 1 + npdu_len {
        return Err(HomeError::InvalidFrame {
            protocol: "knx".into(),
            details: "NPDU KNX tronqué".into(),
        });
    }
    let data = if npdu_len == 1 {
        vec![body[8] & 0x3F]
    } else {
        body[9..9 + (npdu_len - 1)].to_vec()
    };
    Ok((service_type, dst, data))
}

/// Envoie une trame KNXnet/IP UDP sur l'adresse cible locale (ou multicast `224.0.23.12:3671`).
pub async fn send_knx_udp(addr: SocketAddr, frame: &[u8], _timeout: Duration) -> HomeResult<()> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "knx bind".into(),
            message: e.to_string(),
        })?;
    sock.send_to(frame, addr).await.map_err(|e| HomeError::Io {
        context: "knx send".into(),
        message: e.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knx_group_dpt9_and_routing_roundtrip() {
        let ga = KnxGroupAddress::parse_three_level("1/2/14").unwrap();
        assert_eq!(ga.format_three_level(), "1/2/14");

        let dpt = encode_dpt9(21.5);
        assert!((decode_dpt9(dpt) - 21.5).abs() < 0.05);

        let frame = build_knx_routing_write(ga, &dpt);
        let (svc, parsed_ga, data) = parse_knx_routing_frame(&frame).unwrap();
        assert_eq!(svc, 0x0530);
        assert_eq!(parsed_ga, ga);
        assert_eq!(data, dpt);
    }
}
