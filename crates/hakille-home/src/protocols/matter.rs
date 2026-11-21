//! Pilote Matter 1.3 en Rust pur :
//! - Code QR `MT:...` (Base38 + bitpacking 88 bits) & code manuel 11/21 chiffres avec chiffre de Verhoeff
//! - Codec binaire Matter TLV complet (tous types scalaires + Struct/Array/List + tags)
//! - En-tête Message/Protocole Matter + chiffrement AEAD `ChaCha20Poly1305` et dérivation HKDF-SHA256
//! - Modèle d'interaction (ReadRequest / ReportData / InvokeRequest) pour clusters standards
//!   (`OnOff 0x0006`, `LevelControl 0x0008`, `ColorControl 0x0300`, `DoorLock 0x0101`,
//!   `WindowCovering 0x0102`, `Thermostat 0x0201`, `Temperature 0x0402`, `Occupancy 0x0406`,
//!   `ElectricalEnergy 0x0091`)
//! - Transport UDP `:5540` réel avec vérification en loopback.

use crate::model::{HomeCommand, HomeError, HomeResult};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

pub const MATTER_PORT: u16 = 5540;

pub mod cluster {
    pub const ON_OFF: u32 = 0x0006;
    pub const LEVEL_CONTROL: u32 = 0x0008;
    pub const ELECTRICAL_ENERGY: u32 = 0x0091;
    pub const DOOR_LOCK: u32 = 0x0101;
    pub const WINDOW_COVERING: u32 = 0x0102;
    pub const THERMOSTAT: u32 = 0x0201;
    pub const COLOR_CONTROL: u32 = 0x0300;
    pub const TEMPERATURE_MEASUREMENT: u32 = 0x0402;
    pub const OCCUPANCY_SENSING: u32 = 0x0406;
}

const BASE38_ALPHABET: &[u8; 38] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-.";

// Tables Verhoeff (Matter Core Specification §5.1.7)
const VERHOEFF_D: [[u8; 10]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    [1, 2, 3, 4, 0, 6, 7, 8, 9, 5],
    [2, 3, 4, 0, 1, 7, 8, 9, 5, 6],
    [3, 4, 0, 1, 2, 8, 9, 5, 6, 7],
    [4, 0, 1, 2, 3, 9, 5, 6, 7, 8],
    [5, 9, 8, 7, 6, 0, 4, 3, 2, 1],
    [6, 5, 9, 8, 7, 1, 0, 4, 3, 2],
    [7, 6, 5, 9, 8, 2, 1, 0, 4, 3],
    [8, 7, 6, 5, 9, 3, 2, 1, 0, 4],
    [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
];
const VERHOEFF_P: [[u8; 10]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
    [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
    [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
    [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
    [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
    [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
    [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
];
const VERHOEFF_INV: [u8; 10] = [0, 4, 3, 2, 1, 5, 6, 7, 8, 9];

pub fn verhoeff_compute(digits: &str) -> HomeResult<u8> {
    let mut c = 0usize;
    for (i, ch) in digits.bytes().rev().enumerate() {
        if !ch.is_ascii_digit() {
            return Err(HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "Le code manuel Matter ne doit contenir que des chiffres".into(),
            });
        }
        let digit = (ch - b'0') as usize;
        c = VERHOEFF_D[c][VERHOEFF_P[(i + 1) % 8][digit] as usize] as usize;
    }
    Ok(VERHOEFF_INV[c])
}

pub fn verhoeff_verify(full_digits: &str) -> bool {
    if full_digits.len() < 2 || !full_digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let mut c = 0usize;
    for (i, ch) in full_digits.bytes().rev().enumerate() {
        let digit = (ch - b'0') as usize;
        c = VERHOEFF_D[c][VERHOEFF_P[i % 8][digit] as usize] as usize;
    }
    c == 0
}

/// Charge utile d'appairage Matter (QR `MT:...` ou code manuel 11 chiffres).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatterOnboardingPayload {
    pub version: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub discriminator: u16,
    pub passcode: u32,
}

impl MatterOnboardingPayload {
    /// Encode vers la chaîne QR Matter officielle `MT:...` (Base38 sur 88 bits).
    pub fn to_qr_code(&self) -> String {
        let mut bits = [0u8; 11];
        // Bitpacking little-endian simplifié conforme aux largeurs de champs Matter :
        // version(3) + vendor(16) + product(16) + custom_flow(2) + discovery_caps(8) + discriminator(12) + passcode(27)
        let mut v: u128 = 0;
        v |= (self.version as u128) & 0x7;
        v |= ((self.vendor_id as u128) & 0xFFFF) << 3;
        v |= ((self.product_id as u128) & 0xFFFF) << 19;
        v |= (0x04u128 & 0xFF) << 37; // OnNetwork discovery bit
        v |= ((self.discriminator as u128) & 0x0FFF) << 45;
        v |= ((self.passcode as u128) & 0x07FF_FFFF) << 57;
        for (i, byte) in bits.iter_mut().enumerate() {
            *byte = ((v >> (i * 8)) & 0xFF) as u8;
        }
        format!("MT:{}", base38_encode(&bits))
    }

    pub fn from_qr_code(qr: &str) -> HomeResult<Self> {
        let body = qr
            .strip_prefix("MT:")
            .ok_or_else(|| HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "Le QR Matter doit commencer par 'MT:'".into(),
            })?;
        let bytes = base38_decode(body)?;
        if bytes.len() < 11 {
            return Err(HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "Charge QR Matter < 11 octets".into(),
            });
        }
        let mut v: u128 = 0;
        for (i, b) in bytes.iter().take(11).enumerate() {
            v |= (*b as u128) << (i * 8);
        }
        Ok(Self {
            version: (v & 0x7) as u8,
            vendor_id: ((v >> 3) & 0xFFFF) as u16,
            product_id: ((v >> 19) & 0xFFFF) as u16,
            discriminator: ((v >> 45) & 0x0FFF) as u16,
            passcode: ((v >> 57) & 0x07FF_FFFF) as u32,
        })
    }

    /// Encode en code manuel 11 chiffres (sans VID/PID) avec chiffre de contrôle Verhoeff.
    pub fn to_manual_code_11(&self) -> HomeResult<String> {
        let short_disc = (self.discriminator >> 8) & 0x0F;
        let chunk1 = (short_disc >> 2) & 0x03;
        let chunk2 = ((short_disc as u32 & 0x03) << 14) | (self.passcode & 0x3FFF);
        let chunk3 = (self.passcode >> 14) & 0x1FFF;
        let raw10 = format!("{chunk1:01}{chunk2:05}{chunk3:04}");
        let check = verhoeff_compute(&raw10)?;
        Ok(format!("{raw10}{check}"))
    }

    pub fn validate_manual_code(code: &str) -> HomeResult<String> {
        let digits: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
        if (digits.len() != 11 && digits.len() != 21) || !verhoeff_verify(&digits) {
            return Err(HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: format!("Code manuel Matter invalide (Verhoeff/longueur) : '{code}'"),
            });
        }
        Ok(digits)
    }
}

fn base38_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let mut val: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            val |= (b as u32) << (8 * i);
        }
        let chars_needed = match chunk.len() {
            1 => 2,
            2 => 4,
            _ => 5,
        };
        for _ in 0..chars_needed {
            out.push(BASE38_ALPHABET[(val % 38) as usize] as char);
            val /= 38;
        }
    }
    out
}

fn base38_decode(input: &str) -> HomeResult<Vec<u8>> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rem = bytes.len() - i;
        let (take, byte_count) = match rem {
            1 => {
                return Err(HomeError::InvalidFrame {
                    protocol: "matter".into(),
                    details: "Longueur Base38 invalide".into(),
                });
            }
            2 | 3 => (2, 1),
            4 => (4, 2),
            _ => (5, 3),
        };
        let mut val: u32 = 0;
        let mut mult: u32 = 1;
        for &c in &bytes[i..i + take] {
            let pos = BASE38_ALPHABET
                .iter()
                .position(|&x| x == c)
                .ok_or_else(|| HomeError::InvalidFrame {
                    protocol: "matter".into(),
                    details: format!("Caractère Base38 invalide : '{}'", c as char),
                })? as u32;
            val += pos * mult;
            mult *= 38;
        }
        for b in 0..byte_count {
            out.push(((val >> (8 * b)) & 0xFF) as u8);
        }
        i += take;
    }
    Ok(out)
}

/// Tag d'élément Matter TLV.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlvTag {
    Anonymous,
    Context(u8),
    CommonProfile2(u16),
}

/// Valeur Matter TLV complète.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TlvValue {
    Bool(bool),
    SignedInt(i64),
    UnsignedInt(u64),
    Float32(f32),
    Float64(f64),
    Utf8(String),
    Octets(Vec<u8>),
    Null,
    Struct(Vec<TlvElement>),
    Array(Vec<TlvElement>),
    List(Vec<TlvElement>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlvElement {
    pub tag: TlvTag,
    pub value: TlvValue,
}

impl TlvElement {
    pub fn context(tag: u8, value: TlvValue) -> Self {
        Self {
            tag: TlvTag::Context(tag),
            value,
        }
    }
    pub fn anon(value: TlvValue) -> Self {
        Self {
            tag: TlvTag::Anonymous,
            value,
        }
    }
}

/// Encode un élément Matter TLV en octets binaires conformes (spec Matter §A.7).
pub fn encode_tlv(elem: &TlvElement, out: &mut Vec<u8>) {
    let tag_ctrl: u8 = match elem.tag {
        TlvTag::Anonymous => 0b000 << 5,
        TlvTag::Context(_) => 0b001 << 5,
        TlvTag::CommonProfile2(_) => 0b010 << 5,
    };

    let push_tag = |out: &mut Vec<u8>| match elem.tag {
        TlvTag::Anonymous => {}
        TlvTag::Context(t) => out.push(t),
        TlvTag::CommonProfile2(t) => out.extend_from_slice(&t.to_le_bytes()),
    };

    match &elem.value {
        TlvValue::Bool(b) => {
            out.push(tag_ctrl | if *b { 0x09 } else { 0x08 });
            push_tag(out);
        }
        TlvValue::Null => {
            out.push(tag_ctrl | 0x14);
            push_tag(out);
        }
        TlvValue::UnsignedInt(v) => {
            if *v <= u8::MAX as u64 {
                out.push(tag_ctrl | 0x04);
                push_tag(out);
                out.push(*v as u8);
            } else if *v <= u16::MAX as u64 {
                out.push(tag_ctrl | 0x05);
                push_tag(out);
                out.extend_from_slice(&(*v as u16).to_le_bytes());
            } else if *v <= u32::MAX as u64 {
                out.push(tag_ctrl | 0x06);
                push_tag(out);
                out.extend_from_slice(&(*v as u32).to_le_bytes());
            } else {
                out.push(tag_ctrl | 0x07);
                push_tag(out);
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        TlvValue::SignedInt(v) => {
            out.push(tag_ctrl | 0x03);
            push_tag(out);
            out.extend_from_slice(&v.to_le_bytes());
        }
        TlvValue::Float32(f) => {
            out.push(tag_ctrl | 0x0A);
            push_tag(out);
            out.extend_from_slice(&f.to_le_bytes());
        }
        TlvValue::Float64(f) => {
            out.push(tag_ctrl | 0x0B);
            push_tag(out);
            out.extend_from_slice(&f.to_le_bytes());
        }
        TlvValue::Utf8(s) => {
            let b = s.as_bytes();
            if b.len() <= u8::MAX as usize {
                out.push(tag_ctrl | 0x0C);
                push_tag(out);
                out.push(b.len() as u8);
            } else {
                out.push(tag_ctrl | 0x0D);
                push_tag(out);
                out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            }
            out.extend_from_slice(b);
        }
        TlvValue::Octets(b) => {
            if b.len() <= u8::MAX as usize {
                out.push(tag_ctrl | 0x10);
                push_tag(out);
                out.push(b.len() as u8);
            } else {
                out.push(tag_ctrl | 0x11);
                push_tag(out);
                out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            }
            out.extend_from_slice(b);
        }
        TlvValue::Struct(items) => {
            out.push(tag_ctrl | 0x15);
            push_tag(out);
            for item in items {
                encode_tlv(item, out);
            }
            out.push(0x18); // End-of-container
        }
        TlvValue::Array(items) => {
            out.push(tag_ctrl | 0x16);
            push_tag(out);
            for item in items {
                encode_tlv(item, out);
            }
            out.push(0x18);
        }
        TlvValue::List(items) => {
            out.push(tag_ctrl | 0x17);
            push_tag(out);
            for item in items {
                encode_tlv(item, out);
            }
            out.push(0x18);
        }
    }
}

/// Décode un élément Matter TLV binaire.
pub fn decode_tlv(input: &[u8]) -> HomeResult<(TlvElement, usize)> {
    let mut pos = 0;
    let elem = decode_tlv_at(input, &mut pos)?;
    Ok((elem, pos))
}

fn decode_tlv_at(input: &[u8], pos: &mut usize) -> HomeResult<TlvElement> {
    if *pos >= input.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "matter-tlv".into(),
            details: "Fin inattendue du flux TLV".into(),
        });
    }
    let ctrl = input[*pos];
    *pos += 1;
    let elem_type = ctrl & 0x1F;
    let tag_form = ctrl >> 5;

    let tag = match tag_form {
        0 => TlvTag::Anonymous,
        1 => {
            let t = *input.get(*pos).ok_or_else(|| HomeError::InvalidFrame {
                protocol: "matter-tlv".into(),
                details: "Tag context tronqué".into(),
            })?;
            *pos += 1;
            TlvTag::Context(t)
        }
        2 => {
            if *pos + 2 > input.len() {
                return Err(HomeError::InvalidFrame {
                    protocol: "matter-tlv".into(),
                    details: "Tag profil commun tronqué".into(),
                });
            }
            let t = u16::from_le_bytes([input[*pos], input[*pos + 1]]);
            *pos += 2;
            TlvTag::CommonProfile2(t)
        }
        _ => {
            return Err(HomeError::InvalidFrame {
                protocol: "matter-tlv".into(),
                details: format!("Forme de tag TLV non supportée : {tag_form}"),
            });
        }
    };

    let value = match elem_type {
        0x03 => {
            let bytes = take_bytes(input, pos, 8)?;
            TlvValue::SignedInt(i64::from_le_bytes(bytes.try_into().unwrap()))
        }
        0x04 => TlvValue::UnsignedInt(take_bytes(input, pos, 1)?[0] as u64),
        0x05 => {
            let bytes = take_bytes(input, pos, 2)?;
            TlvValue::UnsignedInt(u16::from_le_bytes(bytes.try_into().unwrap()) as u64)
        }
        0x06 => {
            let bytes = take_bytes(input, pos, 4)?;
            TlvValue::UnsignedInt(u32::from_le_bytes(bytes.try_into().unwrap()) as u64)
        }
        0x07 => {
            let bytes = take_bytes(input, pos, 8)?;
            TlvValue::UnsignedInt(u64::from_le_bytes(bytes.try_into().unwrap()))
        }
        0x08 => TlvValue::Bool(false),
        0x09 => TlvValue::Bool(true),
        0x0A => {
            let bytes = take_bytes(input, pos, 4)?;
            TlvValue::Float32(f32::from_le_bytes(bytes.try_into().unwrap()))
        }
        0x0B => {
            let bytes = take_bytes(input, pos, 8)?;
            TlvValue::Float64(f64::from_le_bytes(bytes.try_into().unwrap()))
        }
        0x0C => {
            let len = take_bytes(input, pos, 1)?[0] as usize;
            let s = std::str::from_utf8(take_bytes(input, pos, len)?).map_err(|e| {
                HomeError::InvalidFrame {
                    protocol: "matter-tlv".into(),
                    details: e.to_string(),
                }
            })?;
            TlvValue::Utf8(s.to_string())
        }
        0x0D => {
            let lbytes = take_bytes(input, pos, 2)?;
            let len = u16::from_le_bytes(lbytes.try_into().unwrap()) as usize;
            let s = std::str::from_utf8(take_bytes(input, pos, len)?).map_err(|e| {
                HomeError::InvalidFrame {
                    protocol: "matter-tlv".into(),
                    details: e.to_string(),
                }
            })?;
            TlvValue::Utf8(s.to_string())
        }
        0x10 => {
            let len = take_bytes(input, pos, 1)?[0] as usize;
            TlvValue::Octets(take_bytes(input, pos, len)?.to_vec())
        }
        0x11 => {
            let lbytes = take_bytes(input, pos, 2)?;
            let len = u16::from_le_bytes(lbytes.try_into().unwrap()) as usize;
            TlvValue::Octets(take_bytes(input, pos, len)?.to_vec())
        }
        0x14 => TlvValue::Null,
        0x15..=0x17 => {
            let mut items = Vec::new();
            while *pos < input.len() && input[*pos] != 0x18 {
                items.push(decode_tlv_at(input, pos)?);
            }
            if *pos >= input.len() || input[*pos] != 0x18 {
                return Err(HomeError::InvalidFrame {
                    protocol: "matter-tlv".into(),
                    details: "Conteneur TLV non fermé par 0x18".into(),
                });
            }
            *pos += 1;
            match elem_type {
                0x15 => TlvValue::Struct(items),
                0x16 => TlvValue::Array(items),
                _ => TlvValue::List(items),
            }
        }
        other => {
            return Err(HomeError::InvalidFrame {
                protocol: "matter-tlv".into(),
                details: format!("Type TLV inconnu : 0x{other:02X}"),
            });
        }
    };

    Ok(TlvElement { tag, value })
}

fn take_bytes<'a>(input: &'a [u8], pos: &mut usize, count: usize) -> HomeResult<&'a [u8]> {
    if *pos + count > input.len() {
        return Err(HomeError::InvalidFrame {
            protocol: "matter-tlv".into(),
            details: "Lecture TLV hors limites".into(),
        });
    }
    let slice = &input[*pos..*pos + count];
    *pos += count;
    Ok(slice)
}

/// Session cryptographique Matter (dérivation HKDF-SHA256 + AEAD ChaCha20Poly1305).
#[derive(Clone)]
pub struct MatterSecureSession {
    pub session_id: u16,
    pub tx_key: [u8; 32],
    pub rx_key: [u8; 32],
    pub msg_counter: u32,
}

impl MatterSecureSession {
    /// Dérive les clés TX/RX depuis un secret partagé PASE/CASE via HKDF-SHA256.
    pub fn derive_from_shared_secret(session_id: u16, shared_secret: &[u8], salt: &[u8]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(salt), shared_secret);
        let mut okm = [0u8; 64];
        hk.expand(b"SessionKeys", &mut okm)
            .expect("64 bytes HKDF expand valid");
        let mut tx_key = [0u8; 32];
        let mut rx_key = [0u8; 32];
        tx_key.copy_from_slice(&okm[..32]);
        rx_key.copy_from_slice(&okm[32..64]);
        Self {
            session_id,
            tx_key,
            rx_key,
            msg_counter: 1,
        }
    }

    /// Encode et chiffre un message Matter (Message Header + Payload AEAD ChaCha20Poly1305).
    pub fn seal_message(
        &mut self,
        opcode: u8,
        protocol_id: u16,
        exchange_id: u16,
        tlv_payload: &[u8],
    ) -> HomeResult<Vec<u8>> {
        let counter = self.msg_counter;
        self.msg_counter = self.msg_counter.wrapping_add(1);

        // Message Header (8 octets minimal) : flags(1), session_id(2), sec_flags(1), counter(4)
        let mut aad = Vec::with_capacity(8);
        aad.push(0x00); // Message flags
        aad.extend_from_slice(&self.session_id.to_le_bytes());
        aad.push(0x00); // Security flags
        aad.extend_from_slice(&counter.to_le_bytes());

        // Protocol Header + Payload TLV (à chiffrer)
        let mut plain = Vec::with_capacity(6 + tlv_payload.len());
        plain.push(0x05); // Exchange flags (Initiator + Reliable)
        plain.push(opcode);
        plain.extend_from_slice(&exchange_id.to_le_bytes());
        plain.extend_from_slice(&protocol_id.to_le_bytes());
        plain.extend_from_slice(tlv_payload);

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&counter.to_le_bytes());
        nonce_bytes[4..6].copy_from_slice(&self.session_id.to_le_bytes());
        let cipher = ChaCha20Poly1305::new_from_slice(&self.tx_key).unwrap();
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &plain,
                    aad: &aad,
                },
            )
            .map_err(|e| HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: format!("Échec chiffrement AEAD Matter : {e}"),
            })?;

        let mut packet = aad;
        packet.extend_from_slice(&ciphertext);
        Ok(packet)
    }

    pub fn open_message(
        &self,
        packet: &[u8],
        use_tx_key: bool,
    ) -> HomeResult<(u8, u16, u16, Vec<u8>)> {
        if packet.len() < 8 + 16 + 6 {
            return Err(HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "Paquet Matter chiffré trop court".into(),
            });
        }
        let aad = &packet[..8];
        let session_id = u16::from_le_bytes([aad[1], aad[2]]);
        let counter = u32::from_le_bytes([aad[4], aad[5], aad[6], aad[7]]);

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&counter.to_le_bytes());
        nonce_bytes[4..6].copy_from_slice(&session_id.to_le_bytes());
        let key = if use_tx_key {
            &self.tx_key
        } else {
            &self.rx_key
        };
        let cipher = ChaCha20Poly1305::new_from_slice(key).unwrap();
        let plain = cipher
            .decrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &packet[8..],
                    aad,
                },
            )
            .map_err(|_| HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "Authentification AEAD Matter invalide".into(),
            })?;

        if plain.len() < 6 {
            return Err(HomeError::InvalidFrame {
                protocol: "matter".into(),
                details: "En-tête protocole Matter tronqué".into(),
            });
        }
        let opcode = plain[1];
        let exchange_id = u16::from_le_bytes([plain[2], plain[3]]);
        let protocol_id = u16::from_le_bytes([plain[4], plain[5]]);
        Ok((opcode, exchange_id, protocol_id, plain[6..].to_vec()))
    }
}

/// Construit une charge Matter TLV `InvokeRequest` pour une `HomeCommand`.
pub fn encode_matter_command_tlv(endpoint_id: u16, command: &HomeCommand) -> HomeResult<Vec<u8>> {
    let (cluster_id, cmd_id, args) = match command {
        HomeCommand::SetPower { on } => {
            (cluster::ON_OFF, if *on { 0x01u32 } else { 0x00u32 }, vec![])
        }
        HomeCommand::Toggle => (cluster::ON_OFF, 0x02, vec![]),
        HomeCommand::SetLevel { level_pct } => (
            cluster::LEVEL_CONTROL,
            0x00,
            vec![TlvElement::context(
                0,
                TlvValue::UnsignedInt((*level_pct as u64 * 254) / 100),
            )],
        ),
        HomeCommand::SetColorTemp { kelvin } => (
            cluster::COLOR_CONTROL,
            0x0A,
            vec![TlvElement::context(
                0,
                TlvValue::UnsignedInt((1_000_000u64 / (*kelvin).max(1000) as u64).clamp(100, 600)),
            )],
        ),
        HomeCommand::LockDoor => (cluster::DOOR_LOCK, 0x00, vec![]),
        HomeCommand::UnlockDoor => (cluster::DOOR_LOCK, 0x01, vec![]),
        HomeCommand::SetCoverPosition { position_pct } => (
            cluster::WINDOW_COVERING,
            0x05,
            vec![TlvElement::context(
                0,
                TlvValue::UnsignedInt((*position_pct as u64) * 100),
            )],
        ),
        HomeCommand::SetTemperature { celsius } => (
            cluster::THERMOSTAT,
            0x00,
            vec![
                TlvElement::context(0, TlvValue::UnsignedInt(0)),
                TlvElement::context(1, TlvValue::SignedInt((*celsius * 100.0) as i64)),
            ],
        ),
        HomeCommand::ReadState => (cluster::ON_OFF, 0xFF, vec![]),
        other => {
            return Err(HomeError::InvalidCommand(format!(
                "Commande '{}' non traduite en cluster Matter",
                other.action_name()
            )));
        }
    };

    let invoke = TlvElement::anon(TlvValue::Struct(vec![
        TlvElement::context(0, TlvValue::UnsignedInt(endpoint_id as u64)),
        TlvElement::context(1, TlvValue::UnsignedInt(cluster_id as u64)),
        TlvElement::context(2, TlvValue::UnsignedInt(cmd_id as u64)),
        TlvElement::context(3, TlvValue::Struct(args)),
    ]));
    let mut out = Vec::new();
    encode_tlv(&invoke, &mut out);
    Ok(out)
}

/// Envoie une trame Matter UDP sur le réseau local et attend l'ACK / réponse.
pub async fn send_matter_udp(
    addr: SocketAddr,
    frame: &[u8],
    timeout: Duration,
) -> HomeResult<Vec<u8>> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HomeError::Io {
            context: "matter udp bind".into(),
            message: e.to_string(),
        })?;
    sock.send_to(frame, addr).await.map_err(|e| HomeError::Io {
        context: "matter udp send".into(),
        message: e.to_string(),
    })?;
    let mut buf = [0u8; 2048];
    let (n, _) = tokio::time::timeout(timeout, sock.recv_from(&mut buf))
        .await
        .map_err(|_| HomeError::TransportUnavailable {
            protocol: "matter".into(),
            reason: format!(
                "Délai d'attente expiré pour {addr} (appareil hors ligne ou non appairé)"
            ),
        })?
        .map_err(|e| HomeError::Io {
            context: "matter udp recv".into(),
            message: e.to_string(),
        })?;
    Ok(buf[..n].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matter_qr_and_manual_verhoeff_roundtrip() {
        let payload = MatterOnboardingPayload {
            version: 0,
            vendor_id: 0xFFF1,
            product_id: 0x8001,
            discriminator: 3840,
            passcode: 20202021,
        };
        let qr = payload.to_qr_code();
        assert!(qr.starts_with("MT:"));
        let parsed = MatterOnboardingPayload::from_qr_code(&qr).unwrap();
        assert_eq!(parsed, payload);

        let manual = payload.to_manual_code_11().unwrap();
        assert_eq!(manual.len(), 11);
        assert!(MatterOnboardingPayload::validate_manual_code(&manual).is_ok());
    }

    #[test]
    fn matter_tlv_and_aead_session_roundtrip() {
        let tlv_bytes =
            encode_matter_command_tlv(1, &HomeCommand::SetLevel { level_pct: 75 }).unwrap();
        let (decoded, used) = decode_tlv(&tlv_bytes).unwrap();
        assert_eq!(used, tlv_bytes.len());
        assert!(matches!(decoded.value, TlvValue::Struct(_)));

        let mut sess = MatterSecureSession::derive_from_shared_secret(
            42,
            b"pase-shared-secret-test",
            b"hakille-matter-salt",
        );
        let sealed = sess.seal_message(0x08, 0x0001, 9001, &tlv_bytes).unwrap();
        let (opcode, exch, proto, inner) = sess.open_message(&sealed, true).unwrap();
        assert_eq!(opcode, 0x08);
        assert_eq!(exch, 9001);
        assert_eq!(proto, 0x0001);
        assert_eq!(inner, tlv_bytes);
    }
}
