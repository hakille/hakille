//! Pilote Z-Wave SerialAPI en Rust pur.
//!
//! Implémente le cadrage série Z-Wave (`SOF 0x01`, `ACK 0x06`, `NAK 0x15`, `CAN 0x18`,
//! checksum LRC XOR inversé `0xFF`) et les Command Classes standards :
//! - `0x25` Switch Binary
//! - `0x26` Switch Multilevel
//! - `0x31` Sensor Multilevel
//! - `0x32` Meter (W / kWh)
//! - `0x43` Thermostat Setpoint
//! - `0x62` Door Lock
//! - `0x71` Notification

use crate::model::{HomeCommand, HomeError, HomeResult};

pub const ZWAVE_SOF: u8 = 0x01;
pub const ZWAVE_ACK: u8 = 0x06;
pub const ZWAVE_NAK: u8 = 0x15;
pub const ZWAVE_CAN: u8 = 0x18;

pub mod command_class {
    pub const SWITCH_BINARY: u8 = 0x25;
    pub const SWITCH_MULTILEVEL: u8 = 0x26;
    pub const SENSOR_MULTILEVEL: u8 = 0x31;
    pub const METER: u8 = 0x32;
    pub const THERMOSTAT_SETPOINT: u8 = 0x43;
    pub const DOOR_LOCK: u8 = 0x62;
    pub const NOTIFICATION: u8 = 0x71;
}

/// Trame SerialAPI `FUNC_ID_ZW_SEND_DATA` (0x13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZWaveSerialFrame {
    pub frame_type: u8, // 0x00 = Request, 0x01 = Response
    pub func_id: u8,
    pub payload: Vec<u8>,
}

impl ZWaveSerialFrame {
    pub fn send_data(node_id: u8, command_class_payload: &[u8], callback_id: u8) -> Self {
        let mut payload = Vec::with_capacity(4 + command_class_payload.len());
        payload.push(node_id);
        payload.push(command_class_payload.len() as u8);
        payload.extend_from_slice(command_class_payload);
        payload.push(0x25); // Transmit options: ACK | AutoRoute | Explore
        payload.push(callback_id);
        Self {
            frame_type: 0x00,
            func_id: 0x13, // FUNC_ID_ZW_SEND_DATA
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let len = (3 + self.payload.len()) as u8;
        let mut frame = Vec::with_capacity(2 + len as usize);
        frame.push(ZWAVE_SOF);
        frame.push(len);
        frame.push(self.frame_type);
        frame.push(self.func_id);
        frame.extend_from_slice(&self.payload);

        // LRC : 0xFF XOR (tous les octets entre SOF exclu et Checksum exclu)
        let mut checksum = 0xFFu8;
        for &b in &frame[1..] {
            checksum ^= b;
        }
        frame.push(checksum);
        frame
    }

    pub fn from_bytes(raw: &[u8]) -> HomeResult<Self> {
        if raw.len() < 5 || raw[0] != ZWAVE_SOF {
            return Err(HomeError::InvalidFrame {
                protocol: "zwave".into(),
                details: "Trame Z-Wave SOF invalide".into(),
            });
        }
        let len = raw[1] as usize;
        if raw.len() < len + 2 {
            return Err(HomeError::InvalidFrame {
                protocol: "zwave".into(),
                details: "Longueur trame Z-Wave tronquée".into(),
            });
        }
        let mut calc = 0xFFu8;
        for &b in &raw[1..1 + len] {
            calc ^= b;
        }
        let got = raw[1 + len];
        if calc != got {
            return Err(HomeError::InvalidFrame {
                protocol: "zwave".into(),
                details: format!("Checksum LRC Z-Wave invalide (0x{got:02X} != 0x{calc:02X})"),
            });
        }
        Ok(Self {
            frame_type: raw[2],
            func_id: raw[3],
            payload: raw[4..1 + len].to_vec(),
        })
    }
}

/// Encode une `HomeCommand` en charge utile Command Class Z-Wave.
pub fn encode_zwave_command_class(cmd: &HomeCommand) -> HomeResult<Vec<u8>> {
    match cmd {
        HomeCommand::SetPower { on } => Ok(vec![
            command_class::SWITCH_BINARY,
            0x01, // SWITCH_BINARY_SET
            if *on { 0xFF } else { 0x00 },
        ]),
        HomeCommand::SetLevel { level_pct } => Ok(vec![
            command_class::SWITCH_MULTILEVEL,
            0x01, // SWITCH_MULTILEVEL_SET
            (*level_pct).min(99),
        ]),
        HomeCommand::LockDoor => Ok(vec![command_class::DOOR_LOCK, 0x01, 0xFF]),
        HomeCommand::UnlockDoor => Ok(vec![command_class::DOOR_LOCK, 0x01, 0x00]),
        HomeCommand::SetTemperature { celsius } => {
            let tenths = (*celsius * 10.0).round() as i16;
            let bytes = tenths.to_be_bytes();
            Ok(vec![
                command_class::THERMOSTAT_SETPOINT,
                0x01,
                0x01, // Heating
                0x22, // precision=1, scale=Celsius, size=2
                bytes[0],
                bytes[1],
            ])
        }
        HomeCommand::ReadState => Ok(vec![command_class::SWITCH_BINARY, 0x02]),
        other => Err(HomeError::InvalidCommand(format!(
            "Commande '{}' non traduite en Command Class Z-Wave",
            other.action_name()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zwave_serial_frame_lrc_roundtrip() {
        let cc = encode_zwave_command_class(&HomeCommand::LockDoor).unwrap();
        let frame = ZWaveSerialFrame::send_data(12, &cc, 3);
        let wire = frame.to_bytes();
        let decoded = ZWaveSerialFrame::from_bytes(&wire).unwrap();
        assert_eq!(decoded, frame);

        // Corruption détectée par le LRC XOR
        let mut corrupted = wire.clone();
        corrupted[4] ^= 0xFF;
        assert!(ZWaveSerialFrame::from_bytes(&corrupted).is_err());
    }
}
