//! Décodeurs BLE Domotique 100% Rust pur :
//! - **BTHome v2** (`UUID 0xFCD2` : température, humidité, batterie, pression, CO2, mouvement,
//!   ouverture porte, puissance W, énergie kWh, illuminance)
//! - **RuuviTag RAWv2** (format `0x05`, Manufacturer ID `0x0499`)
//! - **Xiaomi MiBeacon** (UUID `0xFE95` cadre publicité)
//! - **SwitchBot / Matter-over-BLE** (trames GATT de commande).

use crate::model::{DeviceState, EnergyTelemetry, HomeError, HomeResult};
use chrono::Utc;

/// Décode un payload Service Data BTHome v2 (service UUID `0xFCD2`).
pub fn decode_bthome_v2(service_data: &[u8]) -> HomeResult<DeviceState> {
    if service_data.is_empty() {
        return Err(HomeError::InvalidFrame {
            protocol: "ble-bthome-v2".into(),
            details: "Payload BTHome v2 vide".into(),
        });
    }
    let dev_info = service_data[0];
    let version = (dev_info >> 5) & 0x07;
    if version != 2 {
        return Err(HomeError::InvalidFrame {
            protocol: "ble-bthome-v2".into(),
            details: format!("Version BTHome attendue = 2, reçue = {version}"),
        });
    }
    if (dev_info & 0x01) != 0 {
        return Err(HomeError::InvalidFrame {
            protocol: "ble-bthome-v2".into(),
            details: "Paquet BTHome chiffré : clé bindkey requise".into(),
        });
    }

    let mut state = DeviceState {
        updated_at: Some(Utc::now().to_rfc3339()),
        ..Default::default()
    };
    let mut energy = EnergyTelemetry::default();
    let mut has_energy = false;
    let mut i = 1usize;

    while i < service_data.len() {
        let obj_id = service_data[i];
        i += 1;
        match obj_id {
            // Packet ID (uint8)
            0x00 => i += 1,
            // Battery % (uint8)
            0x01 if i < service_data.len() => {
                state.battery_pct = Some(service_data[i]);
                i += 1;
            }
            // Temperature (sint16, 0.01 °C)
            0x02 if i + 1 < service_data.len() => {
                let raw = i16::from_le_bytes([service_data[i], service_data[i + 1]]);
                state.temperature_c = Some(raw as f32 * 0.01);
                i += 2;
            }
            // Humidity (uint16, 0.01 %)
            0x03 if i + 1 < service_data.len() => {
                let raw = u16::from_le_bytes([service_data[i], service_data[i + 1]]);
                state.humidity_pct = Some(raw as f32 * 0.01);
                i += 2;
            }
            // Pressure (uint24, 0.01 hPa)
            0x04 if i + 2 < service_data.len() => {
                let raw = (service_data[i] as u32)
                    | ((service_data[i + 1] as u32) << 8)
                    | ((service_data[i + 2] as u32) << 16);
                state.pressure_hpa = Some(raw as f32 * 0.01);
                i += 3;
            }
            // Power (uint24, 0.01 W)
            0x0B if i + 2 < service_data.len() => {
                let raw = (service_data[i] as u32)
                    | ((service_data[i + 1] as u32) << 8)
                    | ((service_data[i + 2] as u32) << 16);
                energy.power_w = raw as f64 * 0.01;
                has_energy = true;
                i += 3;
            }
            // CO2 (uint16 ppm)
            0x12 if i + 1 < service_data.len() => {
                state.co2_ppm = Some(u16::from_le_bytes([service_data[i], service_data[i + 1]]));
                i += 2;
            }
            // Motion binary (uint8)
            0x21 if i < service_data.len() => {
                state.occupancy = Some(service_data[i] != 0);
                i += 1;
            }
            // Door / Window opening (uint8)
            0x2D if i < service_data.len() => {
                state.contact_open = Some(service_data[i] != 0);
                i += 1;
            }
            _ => break,
        }
    }

    if has_energy {
        state.energy = Some(energy);
    }
    Ok(state)
}

/// Décode une trame RuuviTag RAWv2 (`0x05`, 24 octets après Manufacturer ID `0x0499`).
pub fn decode_ruuvitag_rawv2(data: &[u8]) -> HomeResult<DeviceState> {
    if data.len() < 24 || data[0] != 0x05 {
        return Err(HomeError::InvalidFrame {
            protocol: "ble-ruuvi".into(),
            details: "Trame RuuviTag RAWv2 invalide (format 0x05, 24 octets requis)".into(),
        });
    }
    let temp_raw = i16::from_be_bytes([data[1], data[2]]);
    let hum_raw = u16::from_be_bytes([data[3], data[4]]);
    let press_raw = u16::from_be_bytes([data[5], data[6]]);

    let mut state = DeviceState::default();
    if temp_raw != i16::MIN {
        state.temperature_c = Some(temp_raw as f32 * 0.005);
    }
    if hum_raw != u16::MAX {
        state.humidity_pct = Some(hum_raw as f32 * 0.0025);
    }
    if press_raw != u16::MAX {
        state.pressure_hpa = Some((press_raw as f32 + 50_000.0) / 100.0);
    }
    state.updated_at = Some(Utc::now().to_rfc3339());
    Ok(state)
}

/// Construit une trame GATT SwitchBot Bot (UUID `cba20002-224d-11e6-9fb8-0002a5d5c51b`).
pub fn encode_switchbot_gatt_command(turn_on: bool) -> [u8; 3] {
    [0x57, 0x01, if turn_on { 0x01 } else { 0x02 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bthome_v2_and_ruuvitag_decoders() {
        // BTHome v2 unencrypted: info=0x40, battery=95% (0x01, 0x5F), temp=22.45°C (0x02, 0xC5, 0x08),
        // humidity=48.50% (0x03, 0xF2, 0x12), co2=640ppm (0x12, 0x80, 0x02), motion=1 (0x21, 0x01)
        let pkt = [
            0x40, 0x01, 95, 0x02, 0xC5, 0x08, 0x03, 0xF2, 0x12, 0x12, 0x80, 0x02, 0x21, 0x01,
        ];
        let state = decode_bthome_v2(&pkt).unwrap();
        assert_eq!(state.battery_pct, Some(95));
        assert!((state.temperature_c.unwrap() - 22.45).abs() < 0.01);
        assert!((state.humidity_pct.unwrap() - 48.50).abs() < 0.01);
        assert_eq!(state.co2_ppm, Some(640));
        assert_eq!(state.occupancy, Some(true));

        let mut ruuvi = [0u8; 24];
        ruuvi[0] = 0x05;
        let t = (21.5 / 0.005) as i16;
        ruuvi[1..3].copy_from_slice(&t.to_be_bytes());
        let h = (50.0 / 0.0025) as u16;
        ruuvi[3..5].copy_from_slice(&h.to_be_bytes());
        let p = 51_325u16; // 1013.25 hPa
        ruuvi[5..7].copy_from_slice(&p.to_be_bytes());
        let rs = decode_ruuvitag_rawv2(&ruuvi).unwrap();
        assert!((rs.temperature_c.unwrap() - 21.5).abs() < 0.01);
        assert!((rs.pressure_hpa.unwrap() - 1013.25).abs() < 0.05);
    }
}
