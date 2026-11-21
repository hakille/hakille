//! Client & encodeur WebSocket RFC 6455 100% Rust pur (avec SHA-1 RFC 3174 intégré).
//!
//! Sert de transport souverain pour **Home Assistant WebSocket API** et **OCPP 1.6J / 2.0.1**
//! sans aucune nouvelle dépendance dans `Cargo.lock`.

use crate::model::{HomeError, HomeResult};
use base64::Engine;
use rand::RngCore;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Implémentation SHA-1 RFC 3174 autonome uniquement pour le handshake `Sec-WebSocket-Accept`
/// (RFC 6455 §1.3). Aucune utilisation cryptographique de signature/chiffrement.
pub fn sha1_rfc3174(input: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (input.len() as u64) * 8;
    let mut msg = input.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, h) in [h0, h1, h2, h3, h4].iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&h.to_be_bytes());
    }
    out
}

pub fn compute_websocket_accept(client_key_b64: &str) -> String {
    let concat = format!("{}{WS_GUID}", client_key_b64.trim());
    let digest = sha1_rfc3174(concat.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsOpcode {
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

pub fn encode_ws_frame(opcode: WsOpcode, payload: &[u8], masked: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(14 + payload.len());
    out.push(0x80 | (opcode as u8)); // FIN = 1
    let mask_bit = if masked { 0x80u8 } else { 0 };
    if payload.len() <= 125 {
        out.push(mask_bit | (payload.len() as u8));
    } else if payload.len() <= u16::MAX as usize {
        out.push(mask_bit | 126);
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        out.push(mask_bit | 127);
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    if masked {
        let mut key = [0u8; 4];
        rand::thread_rng().fill_bytes(&mut key);
        out.extend_from_slice(&key);
        for (i, &b) in payload.iter().enumerate() {
            out.push(b ^ key[i % 4]);
        }
    } else {
        out.extend_from_slice(payload);
    }
    out
}

pub async fn read_ws_frame(stream: &mut TcpStream) -> HomeResult<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 2];
    stream
        .read_exact(&mut hdr)
        .await
        .map_err(|e| HomeError::Io {
            context: "ws read header".into(),
            message: e.to_string(),
        })?;
    let opcode = hdr[0] & 0x0F;
    let masked = (hdr[1] & 0x80) != 0;
    let len_code = hdr[1] & 0x7F;
    let len = match len_code {
        0..=125 => len_code as usize,
        126 => {
            let mut b = [0u8; 2];
            stream.read_exact(&mut b).await.map_err(|e| HomeError::Io {
                context: "ws read len16".into(),
                message: e.to_string(),
            })?;
            u16::from_be_bytes(b) as usize
        }
        _ => {
            let mut b = [0u8; 8];
            stream.read_exact(&mut b).await.map_err(|e| HomeError::Io {
                context: "ws read len64".into(),
                message: e.to_string(),
            })?;
            u64::from_be_bytes(b) as usize
        }
    };
    if len > 4 * 1024 * 1024 {
        return Err(HomeError::InvalidFrame {
            protocol: "websocket".into(),
            details: "Trame WebSocket > 4 Mo refusée".into(),
        });
    }
    let mask_key = if masked {
        let mut k = [0u8; 4];
        stream.read_exact(&mut k).await.map_err(|e| HomeError::Io {
            context: "ws read mask".into(),
            message: e.to_string(),
        })?;
        Some(k)
    } else {
        None
    };
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| HomeError::Io {
                context: "ws read payload".into(),
                message: e.to_string(),
            })?;
    }
    if let Some(k) = mask_key {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= k[i % 4];
        }
    }
    Ok((opcode, payload))
}

/// Connexion WebSocket client RFC 6455 complète sur TCP local.
pub struct WsClient {
    pub stream: TcpStream,
    read_buf: Vec<u8>,
}

impl WsClient {
    pub async fn connect(
        addr: SocketAddr,
        host_header: &str,
        path: &str,
        subprotocol: Option<&str>,
    ) -> HomeResult<Self> {
        let mut stream = TcpStream::connect(addr).await.map_err(|e| HomeError::Io {
            context: format!("ws connect {addr}"),
            message: e.to_string(),
        })?;

        let mut nonce = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
        let expected_accept = compute_websocket_accept(&key_b64);

        let proto_hdr = subprotocol
            .map(|p| format!("Sec-WebSocket-Protocol: {p}\r\n"))
            .unwrap_or_default();

        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host_header}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {key_b64}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             {proto_hdr}\r\n"
        );
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| HomeError::Io {
                context: "ws handshake write".into(),
                message: e.to_string(),
            })?;

        let mut resp_buf = [0u8; 2048];
        let mut filled = 0usize;
        let leftover = loop {
            let n = stream
                .read(&mut resp_buf[filled..])
                .await
                .map_err(|e| HomeError::Io {
                    context: "ws handshake read".into(),
                    message: e.to_string(),
                })?;
            if n == 0 {
                return Err(HomeError::InvalidFrame {
                    protocol: "websocket".into(),
                    details: "Connexion fermée pendant handshake WebSocket".into(),
                });
            }
            filled += n;
            if let Some(pos) = resp_buf[..filled].windows(4).position(|w| w == b"\r\n\r\n") {
                let header_str = String::from_utf8_lossy(&resp_buf[..pos + 4]);
                if !header_str.contains("101") || !header_str.contains(&expected_accept) {
                    return Err(HomeError::InvalidFrame {
                        protocol: "websocket".into(),
                        details: format!("Handshake WebSocket 101 invalide : {header_str}"),
                    });
                }
                break resp_buf[pos + 4..filled].to_vec();
            }
            if filled >= resp_buf.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "websocket".into(),
                    details: "En-tête HTTP WebSocket trop long".into(),
                });
            }
        };

        Ok(Self {
            stream,
            read_buf: leftover,
        })
    }

    async fn read_exact_buffered(&mut self, dst: &mut [u8]) -> HomeResult<()> {
        while self.read_buf.len() < dst.len() {
            let mut tmp = [0u8; 4096];
            let n = self
                .stream
                .read(&mut tmp)
                .await
                .map_err(|e| HomeError::Io {
                    context: "ws read buffered".into(),
                    message: e.to_string(),
                })?;
            if n == 0 {
                return Err(HomeError::Io {
                    context: "ws read buffered".into(),
                    message: "early eof".into(),
                });
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
        dst.copy_from_slice(&self.read_buf[..dst.len()]);
        self.read_buf.drain(..dst.len());
        Ok(())
    }

    pub async fn send_text(&mut self, text: &str) -> HomeResult<()> {
        let frame = encode_ws_frame(WsOpcode::Text, text.as_bytes(), true);
        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| HomeError::Io {
                context: "ws send text".into(),
                message: e.to_string(),
            })
    }

    pub async fn recv_text(&mut self) -> HomeResult<String> {
        let mut hdr = [0u8; 2];
        self.read_exact_buffered(&mut hdr).await?;
        let _opcode = hdr[0] & 0x0F;
        let masked = (hdr[1] & 0x80) != 0;
        let len_code = hdr[1] & 0x7F;
        let len = match len_code {
            0..=125 => len_code as usize,
            126 => {
                let mut b = [0u8; 2];
                self.read_exact_buffered(&mut b).await?;
                u16::from_be_bytes(b) as usize
            }
            _ => {
                let mut b = [0u8; 8];
                self.read_exact_buffered(&mut b).await?;
                u64::from_be_bytes(b) as usize
            }
        };
        if len > 4 * 1024 * 1024 {
            return Err(HomeError::InvalidFrame {
                protocol: "websocket".into(),
                details: "Trame WebSocket > 4 Mo refusée".into(),
            });
        }
        let mask_key = if masked {
            let mut k = [0u8; 4];
            self.read_exact_buffered(&mut k).await?;
            Some(k)
        } else {
            None
        };
        let mut payload = vec![0u8; len];
        if len > 0 {
            self.read_exact_buffered(&mut payload).await?;
        }
        if let Some(k) = mask_key {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= k[i % 4];
            }
        }
        String::from_utf8(payload).map_err(|e| HomeError::InvalidFrame {
            protocol: "websocket".into(),
            details: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn rfc6455_websocket_accept_vector() {
        // Vecteur officiel RFC 6455 §1.3
        assert_eq!(
            compute_websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[tokio::test]
    async fn websocket_client_loopback_handshake_and_message() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = sock.read(&mut buf).await.unwrap();
            let req = std::str::from_utf8(&buf[..n]).unwrap();
            let key = req
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key:"))
                .unwrap()
                .trim();
            let accept = compute_websocket_accept(key);
            let resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            let (_op, payload) = read_ws_frame(&mut sock).await.unwrap();
            let reply = encode_ws_frame(WsOpcode::Text, &payload, false);
            sock.write_all(&reply).await.unwrap();
        });

        let mut client = WsClient::connect(addr, "127.0.0.1", "/api/websocket", None)
            .await
            .unwrap();
        client.send_text("{\"type\":\"ping\"}").await.unwrap();
        let echoed = client.recv_text().await.unwrap();
        assert_eq!(echoed, "{\"type\":\"ping\"}");
    }
}
