//! Pilote Thread / CoAP RFC 7252 & CoRE Link Format RFC 6690 en Rust pur.
//!
//! Utilisé pour les appareils Thread (IPv6 ULA/Link-Local), passerelles IKEA Tradfri,
//! capteurs LwM2M et nœuds contraints IPv6.

use crate::model::{HomeError, HomeResult};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

pub const COAP_PORT: u16 = 5683;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoapType {
    Confirmable = 0,
    NonConfirmable = 1,
    Acknowledgement = 2,
    Reset = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoapCode {
    pub class: u8,
    pub detail: u8,
}

impl CoapCode {
    pub const GET: Self = Self {
        class: 0,
        detail: 1,
    };
    pub const POST: Self = Self {
        class: 0,
        detail: 2,
    };
    pub const PUT: Self = Self {
        class: 0,
        detail: 3,
    };
    pub const DELETE: Self = Self {
        class: 0,
        detail: 4,
    };
    pub const CONTENT_205: Self = Self {
        class: 2,
        detail: 5,
    };

    pub fn to_byte(self) -> u8 {
        ((self.class & 0x07) << 5) | (self.detail & 0x1F)
    }

    pub fn from_byte(b: u8) -> Self {
        Self {
            class: b >> 5,
            detail: b & 0x1F,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoapOption {
    pub number: u16,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoapMessage {
    pub msg_type: CoapType,
    pub code: CoapCode,
    pub message_id: u16,
    pub token: Vec<u8>,
    pub options: Vec<CoapOption>,
    pub payload: Vec<u8>,
}

impl CoapMessage {
    pub fn get(message_id: u16, path: &str) -> Self {
        let options = path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|seg| CoapOption {
                number: 11, // Uri-Path
                value: seg.as_bytes().to_vec(),
            })
            .collect();
        Self {
            msg_type: CoapType::Confirmable,
            code: CoapCode::GET,
            message_id,
            token: vec![0x48, 0x4B],
            options,
            payload: Vec::new(),
        }
    }

    pub fn put_json(message_id: u16, path: &str, json: &[u8]) -> Self {
        let mut msg = Self::get(message_id, path);
        msg.code = CoapCode::PUT;
        msg.options.push(CoapOption {
            number: 12,      // Content-Format
            value: vec![50], // application/json
        });
        msg.payload = json.to_vec();
        msg
    }

    pub fn to_bytes(&self) -> HomeResult<Vec<u8>> {
        if self.token.len() > 8 {
            return Err(HomeError::InvalidFrame {
                protocol: "coap".into(),
                details: "Token CoAP > 8 octets".into(),
            });
        }
        let mut out = Vec::with_capacity(16 + self.payload.len());
        let first = (1u8 << 6) | ((self.msg_type as u8) << 4) | (self.token.len() as u8);
        out.push(first);
        out.push(self.code.to_byte());
        out.extend_from_slice(&self.message_id.to_be_bytes());
        out.extend_from_slice(&self.token);

        let mut opts = self.options.clone();
        opts.sort_by_key(|o| o.number);
        let mut prev_num = 0u16;

        for opt in opts {
            let delta = opt.number - prev_num;
            prev_num = opt.number;
            let len = opt.value.len();
            let (d_nib, d_ext) = encode_nibble(delta);
            let (l_nib, l_ext) = encode_nibble(len as u16);
            out.push((d_nib << 4) | l_nib);
            out.extend_from_slice(&d_ext);
            out.extend_from_slice(&l_ext);
            out.extend_from_slice(&opt.value);
        }

        if !self.payload.is_empty() {
            out.push(0xFF);
            out.extend_from_slice(&self.payload);
        }
        Ok(out)
    }

    pub fn from_bytes(raw: &[u8]) -> HomeResult<Self> {
        if raw.len() < 4 {
            return Err(HomeError::InvalidFrame {
                protocol: "coap".into(),
                details: "Trame CoAP < 4 octets".into(),
            });
        }
        let ver = raw[0] >> 6;
        if ver != 1 {
            return Err(HomeError::InvalidFrame {
                protocol: "coap".into(),
                details: format!("Version CoAP invalide : {ver}"),
            });
        }
        let msg_type = match (raw[0] >> 4) & 0x03 {
            0 => CoapType::Confirmable,
            1 => CoapType::NonConfirmable,
            2 => CoapType::Acknowledgement,
            _ => CoapType::Reset,
        };
        let tkl = (raw[0] & 0x0F) as usize;
        if tkl > 8 || 4 + tkl > raw.len() {
            return Err(HomeError::InvalidFrame {
                protocol: "coap".into(),
                details: "Token length CoAP invalide".into(),
            });
        }
        let code = CoapCode::from_byte(raw[1]);
        let message_id = u16::from_be_bytes([raw[2], raw[3]]);
        let token = raw[4..4 + tkl].to_vec();

        let mut pos = 4 + tkl;
        let mut options = Vec::new();
        let mut current_num = 0u16;

        while pos < raw.len() {
            if raw[pos] == 0xFF {
                pos += 1;
                break;
            }
            let d_nib = raw[pos] >> 4;
            let l_nib = raw[pos] & 0x0F;
            pos += 1;
            let delta = decode_nibble(d_nib, raw, &mut pos)?;
            let len = decode_nibble(l_nib, raw, &mut pos)? as usize;
            current_num =
                current_num
                    .checked_add(delta)
                    .ok_or_else(|| HomeError::InvalidFrame {
                        protocol: "coap".into(),
                        details: "Overflow numéro d'option CoAP".into(),
                    })?;
            if pos + len > raw.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "coap".into(),
                    details: "Valeur d'option CoAP tronquée".into(),
                });
            }
            options.push(CoapOption {
                number: current_num,
                value: raw[pos..pos + len].to_vec(),
            });
            pos += len;
        }

        let payload = if pos <= raw.len() {
            raw[pos..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            msg_type,
            code,
            message_id,
            token,
            options,
            payload,
        })
    }
}

fn encode_nibble(val: u16) -> (u8, Vec<u8>) {
    if val < 13 {
        (val as u8, vec![])
    } else if val < 269 {
        (13, vec![(val - 13) as u8])
    } else {
        (14, (val - 269).to_be_bytes().to_vec())
    }
}

fn decode_nibble(nib: u8, raw: &[u8], pos: &mut usize) -> HomeResult<u16> {
    match nib {
        0..=12 => Ok(nib as u16),
        13 => {
            let b = *raw.get(*pos).ok_or_else(|| HomeError::InvalidFrame {
                protocol: "coap".into(),
                details: "Option étendue CoAP tronquée".into(),
            })?;
            *pos += 1;
            Ok(13 + b as u16)
        }
        14 => {
            if *pos + 2 > raw.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "coap".into(),
                    details: "Option 16-bit CoAP tronquée".into(),
                });
            }
            let v = u16::from_be_bytes([raw[*pos], raw[*pos + 1]]);
            *pos += 2;
            Ok(269 + v)
        }
        _ => Err(HomeError::InvalidFrame {
            protocol: "coap".into(),
            details: "Marqueur 15 réservé dans option CoAP".into(),
        }),
    }
}

/// Parse un payload CoRE Link Format (`/.well-known/core`), ex: `</light>;rt="core.light",</temp>;obs`.
pub fn parse_core_link_format(body: &str) -> Vec<(String, Vec<(String, String)>)> {
    body.split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            let start = trimmed.find('<')?;
            let end = trimmed.find('>')?;
            let uri = trimmed[start + 1..end].to_string();
            let attrs = trimmed[end + 1..]
                .split(';')
                .filter_map(|kv| {
                    let kv = kv.trim();
                    if kv.is_empty() {
                        return None;
                    }
                    if let Some((k, v)) = kv.split_once('=') {
                        Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
                    } else {
                        Some((kv.to_string(), "true".to_string()))
                    }
                })
                .collect();
            Some((uri, attrs))
        })
        .collect()
}

/// Échange CoAP UDP asynchrone réel.
pub async fn send_coap_request(
    addr: SocketAddr,
    req: &CoapMessage,
    timeout: Duration,
) -> HomeResult<CoapMessage> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "coap bind".into(),
            message: e.to_string(),
        })?;
    let bytes = req.to_bytes()?;
    sock.send_to(&bytes, addr)
        .await
        .map_err(|e| HomeError::Io {
            context: "coap send".into(),
            message: e.to_string(),
        })?;
    let mut buf = [0u8; 2048];
    let (n, _) = tokio::time::timeout(timeout, sock.recv_from(&mut buf))
        .await
        .map_err(|_| HomeError::TransportUnavailable {
            protocol: "thread-coap".into(),
            reason: format!("Pas de réponse CoAP de {addr}"),
        })?
        .map_err(|e| HomeError::Io {
            context: "coap recv".into(),
            message: e.to_string(),
        })?;
    CoapMessage::from_bytes(&buf[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coap_codec_core_links_and_loopback() {
        let srv = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = srv.local_addr().unwrap();
        tokio::spawn(async move {
            let mut b = [0u8; 512];
            let (n, peer) = srv.recv_from(&mut b).await.unwrap();
            let req = CoapMessage::from_bytes(&b[..n]).unwrap();
            let resp = CoapMessage {
                msg_type: CoapType::Acknowledgement,
                code: CoapCode::CONTENT_205,
                message_id: req.message_id,
                token: req.token,
                options: vec![],
                payload: br#"</15001>;rt="light";obs,</3303/0/5700>;rt="temperature""#.to_vec(),
            };
            srv.send_to(&resp.to_bytes().unwrap(), peer).await.unwrap();
        });

        let msg = CoapMessage::get(0x1234, "/.well-known/core");
        let reply = send_coap_request(addr, &msg, Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(reply.code, CoapCode::CONTENT_205);
        let links = parse_core_link_format(std::str::from_utf8(&reply.payload).unwrap());
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "/15001");
    }
}
