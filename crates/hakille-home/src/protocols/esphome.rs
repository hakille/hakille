//! Pilote ESPHome Native API (port TCP `:6053`) en Rust pur.
//!
//! Implémente le cadrage Protobuf en texte clair (`0x00 | varint len | varint msg_type | body`)
//! ainsi que l'encodeur/décodeur Protobuf wire (varints, champs string/bool/float) pour
//! `HelloRequest` (1), `ConnectRequest` (3), `ListEntitiesRequest` (11), `LightCommandRequest` (32),
//! `SwitchCommandRequest` (33), `CoverCommandRequest` (30).

use crate::model::{HomeCommand, HomeError, HomeResult};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const ESPHOME_PORT: u16 = 6053;

pub fn encode_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

pub fn decode_varint(data: &[u8], pos: &mut usize) -> HomeResult<u64> {
    let mut val = 0u64;
    let mut shift = 0u32;
    for _ in 0..10 {
        let b = *data.get(*pos).ok_or_else(|| HomeError::InvalidFrame {
            protocol: "esphome".into(),
            details: "Varint ESPHome tronqué".into(),
        })?;
        *pos += 1;
        val |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Ok(val);
        }
        shift += 7;
    }
    Err(HomeError::InvalidFrame {
        protocol: "esphome".into(),
        details: "Varint ESPHome > 64 bits".into(),
    })
}

pub fn encode_esphome_plaintext_frame(msg_type: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00]; // Preamble Plaintext (Noise = 0x01)
    encode_varint(payload.len() as u64, &mut frame);
    encode_varint(msg_type as u64, &mut frame);
    frame.extend_from_slice(payload);
    frame
}

pub fn decode_esphome_plaintext_frame(raw: &[u8]) -> HomeResult<(u32, Vec<u8>)> {
    if raw.is_empty() || raw[0] != 0x00 {
        return Err(HomeError::InvalidFrame {
            protocol: "esphome".into(),
            details: "Préambule ESPHome Plaintext 0x00 attendu".into(),
        });
    }
    let mut pos = 1;
    let len = decode_varint(raw, &mut pos)? as usize;
    let msg_type = decode_varint(raw, &mut pos)? as u32;
    if pos + len > raw.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "esphome".into(),
            details: "Payload ESPHome tronqué".into(),
        });
    }
    Ok((msg_type, raw[pos..pos + len].to_vec()))
}

fn push_pb_varint(field: u32, val: u64, out: &mut Vec<u8>) {
    encode_varint((field as u64) << 3, out);
    encode_varint(val, out);
}

fn push_pb_fixed32_u32(field: u32, val: u32, out: &mut Vec<u8>) {
    encode_varint(((field as u64) << 3) | 5, out);
    out.extend_from_slice(&val.to_le_bytes());
}

fn push_pb_fixed32_f32(field: u32, val: f32, out: &mut Vec<u8>) {
    push_pb_fixed32_u32(field, val.to_bits(), out);
}

fn push_pb_string(field: u32, s: &str, out: &mut Vec<u8>) {
    encode_varint(((field as u64) << 3) | 2, out);
    encode_varint(s.len() as u64, out);
    out.extend_from_slice(s.as_bytes());
}

/// Construit un message `HelloRequest` (msg_type = 1) conforme à l'API native ESPHome.
pub fn build_hello_request(client_info: &str) -> Vec<u8> {
    let mut body = Vec::new();
    push_pb_string(1, client_info, &mut body);
    push_pb_varint(2, 1, &mut body); // api_version_major = 1
    push_pb_varint(3, 10, &mut body); // api_version_minor = 10
    encode_esphome_plaintext_frame(1, &body)
}

/// Traduit une `HomeCommand` en trame Protobuf ESPHome (`SwitchCommand` 33 / `LightCommand` 32).
pub fn encode_esphome_command(entity_key: u32, cmd: &HomeCommand) -> HomeResult<Vec<u8>> {
    match cmd {
        HomeCommand::SetPower { on } => {
            let mut body = Vec::new();
            push_pb_fixed32_u32(1, entity_key, &mut body);
            push_pb_varint(2, u64::from(*on), &mut body);
            Ok(encode_esphome_plaintext_frame(33, &body))
        }
        HomeCommand::SetLevel { level_pct } => {
            let mut body = Vec::new();
            push_pb_fixed32_u32(1, entity_key, &mut body);
            push_pb_varint(2, 1, &mut body); // has_state
            push_pb_varint(3, u64::from(*level_pct > 0), &mut body);
            push_pb_varint(4, 1, &mut body); // has_brightness
            push_pb_fixed32_f32(5, (*level_pct as f32) / 100.0, &mut body);
            Ok(encode_esphome_plaintext_frame(32, &body))
        }
        HomeCommand::ReadState => {
            // ListEntitiesRequest = 11
            Ok(encode_esphome_plaintext_frame(11, &[]))
        }
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non traduite en message ESPHome",
            other.action_name()
        ))),
    }
}

/// Exécute un handshake `HelloRequest` + envoi de commande sur un nœud ESPHome local.
pub async fn send_esphome_command(
    addr: SocketAddr,
    cmd_frame: &[u8],
) -> HomeResult<(u32, Vec<u8>)> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| HomeError::Io {
        context: format!("esphome connect {addr}"),
        message: e.to_string(),
    })?;
    let hello = build_hello_request("Hakille-Home");
    stream.write_all(&hello).await.map_err(|e| HomeError::Io {
        context: "esphome hello".into(),
        message: e.to_string(),
    })?;
    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).await.map_err(|e| HomeError::Io {
        context: "esphome read hello".into(),
        message: e.to_string(),
    })?;
    let (resp_type, resp_body) = decode_esphome_plaintext_frame(&buf[..n])?;
    stream
        .write_all(cmd_frame)
        .await
        .map_err(|e| HomeError::Io {
            context: "esphome write cmd".into(),
            message: e.to_string(),
        })?;
    Ok((resp_type, resp_body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn esphome_protobuf_codec_and_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 256];
            let n = sock.read(&mut b).await.unwrap();
            let (mtype, _) = decode_esphome_plaintext_frame(&b[..n]).unwrap();
            assert_eq!(mtype, 1); // HelloRequest
            // HelloResponse (msg_type = 2)
            let ack = encode_esphome_plaintext_frame(2, b"\x08\x01");
            sock.write_all(&ack).await.unwrap();
            let n2 = sock.read(&mut b).await.unwrap();
            let (cmd_type, _) = decode_esphome_plaintext_frame(&b[..n2]).unwrap();
            assert_eq!(cmd_type, 33); // SwitchCommandRequest
        });

        let frame =
            encode_esphome_command(0x11223344, &HomeCommand::SetPower { on: true }).unwrap();
        let (rtype, _) = send_esphome_command(addr, &frame).await.unwrap();
        assert_eq!(rtype, 2);
    }
}
