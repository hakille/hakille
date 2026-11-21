//! Pilote Zigbee 3.0 en Rust pur :
//! - Codec Zigbee Cluster Library (ZCL) : On/Off (`0x0006`), LevelControl (`0x0008`),
//!   ColorControl (`0x0300`), DoorLock (`0x0101`), Thermostat (`0x0201`),
//!   TemperatureMeasurement (`0x0402`), IAS Zone (`0x0500`)
//! - Encapsulation série Silicon Labs ASH/EZSP (`0x7E` + byte-stuffing + CRC-CCITT)
//! - Encapsulation série Texas Instruments ZNP / MT (`0xFE` + longueur + cmd + FCS XOR)
//! - Traduction Zigbee2MQTT bridge.

use crate::model::{HomeCommand, HomeError, HomeResult};

/// Calcule le CRC-16 CCITT (polynôme `0x1021`, graine `0xFFFF`) utilisé par ASH/EZSP.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Trame ZCL (Zigbee Cluster Library).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZclFrame {
    pub frame_control: u8,
    pub transaction_seq: u8,
    pub command_id: u8,
    pub payload: Vec<u8>,
}

impl ZclFrame {
    pub fn cluster_command(seq: u8, command_id: u8, payload: Vec<u8>) -> Self {
        Self {
            frame_control: 0x01, // Cluster-specific, Client->Server, Default response enabled
            transaction_seq: seq,
            command_id,
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(3 + self.payload.len());
        out.push(self.frame_control);
        out.push(self.transaction_seq);
        out.push(self.command_id);
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn from_bytes(raw: &[u8]) -> HomeResult<Self> {
        if raw.len() < 3 {
            return Err(HomeError::InvalidFrame {
                protocol: "zigbee-zcl".into(),
                details: "Trame ZCL < 3 octets".into(),
            });
        }
        Ok(Self {
            frame_control: raw[0],
            transaction_seq: raw[1],
            command_id: raw[2],
            payload: raw[3..].to_vec(),
        })
    }

    pub fn from_home_command(seq: u8, cmd: &HomeCommand) -> HomeResult<(u16, Self)> {
        match cmd {
            HomeCommand::SetPower { on: true } => {
                Ok((0x0006, Self::cluster_command(seq, 0x01, vec![])))
            }
            HomeCommand::SetPower { on: false } => {
                Ok((0x0006, Self::cluster_command(seq, 0x00, vec![])))
            }
            HomeCommand::Toggle => Ok((0x0006, Self::cluster_command(seq, 0x02, vec![]))),
            HomeCommand::SetLevel { level_pct } => {
                let lvl = ((*level_pct as u16 * 254) / 100) as u8;
                Ok((
                    0x0008,
                    Self::cluster_command(seq, 0x04, vec![lvl, 0x05, 0x00]),
                ))
            }
            HomeCommand::LockDoor => Ok((0x0101, Self::cluster_command(seq, 0x00, vec![]))),
            HomeCommand::UnlockDoor => Ok((0x0101, Self::cluster_command(seq, 0x01, vec![]))),
            HomeCommand::SetCoverPosition { position_pct } => Ok((
                0x0102,
                Self::cluster_command(seq, 0x05, vec![*position_pct]),
            )),
            other => Err(HomeError::InvalidCommand(format!(
                "Commande '{}' non traduite en trame ZCL",
                other.action_name()
            ))),
        }
    }
}

/// Encode une trame Silicon Labs ASHv2 (byte-stuffing `0x7D` + CRC-CCITT + délimiteur `0x7E`).
pub fn encode_ash_frame(control_byte: u8, ezsp_payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(1 + ezsp_payload.len());
    body.push(control_byte);
    body.extend_from_slice(ezsp_payload);
    let crc = crc16_ccitt(&body);
    body.push((crc >> 8) as u8);
    body.push((crc & 0xFF) as u8);

    let mut out = Vec::with_capacity(body.len() + 4);
    for b in body {
        if matches!(b, 0x7E | 0x7D | 0x11 | 0x13 | 0x18 | 0x1A) {
            out.push(0x7D);
            out.push(b ^ 0x20);
        } else {
            out.push(b);
        }
    }
    out.push(0x7E);
    out
}

/// Décode une trame Silicon Labs ASHv2 et vérifie son CRC-CCITT.
pub fn decode_ash_frame(raw: &[u8]) -> HomeResult<(u8, Vec<u8>)> {
    let stripped = raw.strip_suffix(&[0x7E]).unwrap_or(raw);
    let mut unescaped = Vec::with_capacity(stripped.len());
    let mut i = 0;
    while i < stripped.len() {
        if stripped[i] == 0x7D {
            i += 1;
            let next = *stripped.get(i).ok_or_else(|| HomeError::InvalidFrame {
                protocol: "zigbee-ash".into(),
                details: "Échappement ASH incomplet".into(),
            })?;
            unescaped.push(next ^ 0x20);
        } else {
            unescaped.push(stripped[i]);
        }
        i += 1;
    }
    if unescaped.len() < 3 {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-ash".into(),
            details: "Trame ASH trop courte".into(),
        });
    }
    let data_end = unescaped.len() - 2;
    let got_crc = u16::from_be_bytes([unescaped[data_end], unescaped[data_end + 1]]);
    let calc_crc = crc16_ccitt(&unescaped[..data_end]);
    if got_crc != calc_crc {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-ash".into(),
            details: format!("CRC ASH invalide (0x{got_crc:04X} != 0x{calc_crc:04X})"),
        });
    }
    Ok((unescaped[0], unescaped[1..data_end].to_vec()))
}

/// Encode une trame Texas Instruments ZNP (MT Serial Format : `0xFE | Len | Cmd0 | Cmd1 | Data | FCS`).
pub fn encode_ti_znp_frame(cmd0: u8, cmd1: u8, data: &[u8]) -> HomeResult<Vec<u8>> {
    if data.len() > 250 {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-znp".into(),
            details: "Charge TI ZNP > 250 octets".into(),
        });
    }
    let len = data.len() as u8;
    let mut fcs = len ^ cmd0 ^ cmd1;
    for &b in data {
        fcs ^= b;
    }
    let mut out = Vec::with_capacity(5 + data.len());
    out.push(0xFE);
    out.push(len);
    out.push(cmd0);
    out.push(cmd1);
    out.extend_from_slice(data);
    out.push(fcs);
    Ok(out)
}

/// Décode et vérifie le FCS d'une trame TI ZNP.
pub fn decode_ti_znp_frame(raw: &[u8]) -> HomeResult<(u8, u8, Vec<u8>)> {
    if raw.len() < 5 || raw[0] != 0xFE {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-znp".into(),
            details: "SOF 0xFE manquant ou trame trop courte".into(),
        });
    }
    let len = raw[1] as usize;
    if raw.len() < 5 + len {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-znp".into(),
            details: "Trame TI ZNP tronquée".into(),
        });
    }
    let cmd0 = raw[2];
    let cmd1 = raw[3];
    let data = &raw[4..4 + len];
    let got_fcs = raw[4 + len];
    let mut calc = (len as u8) ^ cmd0 ^ cmd1;
    for &b in data {
        calc ^= b;
    }
    if got_fcs != calc {
        return Err(HomeError::InvalidFrame {
            protocol: "zigbee-znp".into(),
            details: "Checksum FCS TI ZNP invalide".into(),
        });
    }
    Ok((cmd0, cmd1, data.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigbee_zcl_ash_and_znp_codecs_roundtrip() {
        let (cluster, zcl) =
            ZclFrame::from_home_command(7, &HomeCommand::SetLevel { level_pct: 50 }).unwrap();
        assert_eq!(cluster, 0x0008);
        let zcl_bytes = zcl.to_bytes();
        assert_eq!(ZclFrame::from_bytes(&zcl_bytes).unwrap(), zcl);

        // Payload contenant des octets réservés (0x7E, 0x7D) pour tester le byte-stuffing ASH
        let ezsp_payload = vec![0x01, 0x7E, 0x7D, 0x11, 0xAB];
        let ash = encode_ash_frame(0x00, &ezsp_payload);
        let (ctrl, decoded) = decode_ash_frame(&ash).unwrap();
        assert_eq!(ctrl, 0x00);
        assert_eq!(decoded, ezsp_payload);

        let znp = encode_ti_znp_frame(0x24, 0x01, &zcl_bytes).unwrap();
        let (c0, c1, zdata) = decode_ti_znp_frame(&znp).unwrap();
        assert_eq!((c0, c1), (0x24, 0x01));
        assert_eq!(zdata, zcl_bytes);
    }
}
