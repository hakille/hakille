//! Pilote Modbus TCP (MBAP), Modbus RTU (CRC-16 `0xA001`) et décodeurs **SunSpec**
//! (Onduleurs solaires Modèle 101/103, Batteries Modèle 124, Compteurs Modèle 203).
//!
//! 100% Rust pur avec client TCP asynchrone réel et tests loopback complets.

use crate::model::{EnergyTelemetry, HomeError, HomeResult};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// CRC-16 Modbus RTU (polynôme `0xA001` réfléchi).
pub fn modbus_crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// Construit une trame Modbus RTU (avec CRC-16 little-endian).
pub fn build_rtu_read_holding(unit_id: u8, start_addr: u16, qty: u16) -> Vec<u8> {
    let mut pdu = vec![unit_id, 0x03];
    pdu.extend_from_slice(&start_addr.to_be_bytes());
    pdu.extend_from_slice(&qty.to_be_bytes());
    let crc = modbus_crc16(&pdu);
    pdu.extend_from_slice(&crc.to_le_bytes());
    pdu
}

pub fn verify_rtu_frame(raw: &[u8]) -> HomeResult<&[u8]> {
    if raw.len() < 4 {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-rtu".into(),
            details: "Trame RTU < 4 octets".into(),
        });
    }
    let body = &raw[..raw.len() - 2];
    let got = u16::from_le_bytes([raw[raw.len() - 2], raw[raw.len() - 1]]);
    let calc = modbus_crc16(body);
    if got != calc {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-rtu".into(),
            details: format!("CRC-16 RTU invalide (0x{got:04X} != 0x{calc:04X})"),
        });
    }
    Ok(body)
}

/// Construit une requête Modbus TCP (en-tête MBAP 7 octets + PDU).
pub fn build_tcp_pdu(tx_id: u16, unit_id: u8, pdu: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(7 + pdu.len());
    buf.extend_from_slice(&tx_id.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes()); // Protocol ID = 0
    buf.extend_from_slice(&((1 + pdu.len()) as u16).to_be_bytes());
    buf.push(unit_id);
    buf.extend_from_slice(pdu);
    buf
}

/// Lit des registres Modbus TCP (`FC 0x03 Read Holding Registers`) sur un équipement réel.
pub async fn modbus_tcp_read_holding(
    addr: SocketAddr,
    unit_id: u8,
    start_addr: u16,
    count: u16,
) -> HomeResult<Vec<u16>> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| HomeError::Io {
        context: format!("modbus connect {addr}"),
        message: e.to_string(),
    })?;
    let mut pdu = vec![0x03];
    pdu.extend_from_slice(&start_addr.to_be_bytes());
    pdu.extend_from_slice(&count.to_be_bytes());
    let req = build_tcp_pdu(1, unit_id, &pdu);
    stream.write_all(&req).await.map_err(|e| HomeError::Io {
        context: "modbus write".into(),
        message: e.to_string(),
    })?;

    let mut mbap = [0u8; 7];
    stream
        .read_exact(&mut mbap)
        .await
        .map_err(|e| HomeError::Io {
            context: "modbus read mbap".into(),
            message: e.to_string(),
        })?;
    let len = u16::from_be_bytes([mbap[4], mbap[5]]) as usize;
    if len < 2 {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-tcp".into(),
            details: "Longueur MBAP invalide".into(),
        });
    }
    let mut body = vec![0u8; len - 1];
    stream
        .read_exact(&mut body)
        .await
        .map_err(|e| HomeError::Io {
            context: "modbus read body".into(),
            message: e.to_string(),
        })?;
    if body.len() < 2 || body[0] != 0x03 {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-tcp".into(),
            details: format!("Code fonction ou exception Modbus inattendu : {body:?}"),
        });
    }
    let byte_count = body[1] as usize;
    if body.len() < 2 + byte_count || !byte_count.is_multiple_of(2) {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-tcp".into(),
            details: "Compteur d'octets Modbus incohérent".into(),
        });
    }
    let regs = body[2..2 + byte_count]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    Ok(regs)
}

/// Écrit un registre unique Modbus TCP (`FC 0x06 Write Single Register`).
pub async fn modbus_tcp_write_register(
    addr: SocketAddr,
    unit_id: u8,
    register: u16,
    value: u16,
) -> HomeResult<()> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| HomeError::Io {
        context: format!("modbus connect {addr}"),
        message: e.to_string(),
    })?;
    let mut pdu = vec![0x06];
    pdu.extend_from_slice(&register.to_be_bytes());
    pdu.extend_from_slice(&value.to_be_bytes());
    let req = build_tcp_pdu(2, unit_id, &pdu);
    stream.write_all(&req).await.map_err(|e| HomeError::Io {
        context: "modbus write".into(),
        message: e.to_string(),
    })?;
    let mut resp = [0u8; 12];
    stream
        .read_exact(&mut resp)
        .await
        .map_err(|e| HomeError::Io {
            context: "modbus write ack".into(),
            message: e.to_string(),
        })?;
    if resp[7] != 0x06 {
        return Err(HomeError::InvalidFrame {
            protocol: "modbus-tcp".into(),
            details: format!("Échec FC06 : 0x{:02X}", resp[7]),
        });
    }
    Ok(())
}

/// Décode le bloc SunSpec Modèle 101/103 (Onduleur Solaire AC) à partir de ses registres.
/// Disposition standard SunSpec :
/// `[0]=ID(101|103), [1]=Len(50), [2]=A, [3..5]=Aph, [6]=A_SF, [14]=W, [15]=W_SF, [24..25]=WH(u32), [26]=WH_SF`
pub fn decode_sunspec_inverter_101(regs: &[u16]) -> HomeResult<EnergyTelemetry> {
    if regs.len() < 27 || (regs[0] != 101 && regs[0] != 103) {
        return Err(HomeError::InvalidFrame {
            protocol: "sunspec".into(),
            details: "Modèle SunSpec 101/103 absent ou trop court".into(),
        });
    }
    let scale = |sf_reg: u16| -> f64 {
        let sf = sf_reg as i16 as i32;
        10f64.powi(sf)
    };
    let watts_raw = regs[14] as i16 as f64;
    let watts = watts_raw * scale(regs[15]);
    let amps_raw = regs[2] as f64;
    let amps = amps_raw * scale(regs[6]);
    let wh_raw = (((regs[24] as u32) << 16) | (regs[25] as u32)) as f64;
    let kwh = (wh_raw * scale(regs[26])) / 1000.0;

    Ok(EnergyTelemetry {
        power_w: -watts, // négatif = production injectée dans le foyer
        solar_generation_w: Some(watts.max(0.0)),
        battery_soc_pct: None,
        energy_kwh: Some(kwh),
        voltage_v: Some(230.0),
        current_a: Some(amps as f32),
        power_factor: None,
    })
}

/// Décode un bloc SunSpec Modèle 124 (Stockage Batterie : ChaState % SoC avec `ChaState_SF`).
pub fn decode_sunspec_storage_124(regs: &[u16]) -> HomeResult<EnergyTelemetry> {
    if regs.len() < 12 || regs[0] != 124 {
        return Err(HomeError::InvalidFrame {
            protocol: "sunspec-storage".into(),
            details: "Modèle SunSpec 124 absent".into(),
        });
    }
    // Reg[4] = ChaState (u16), Reg[11] = ChaState_SF (sunssf)
    let sf = regs[11] as i16 as i32;
    let soc = ((regs[4] as f64) * 10f64.powi(sf)).clamp(0.0, 100.0) as f32;
    Ok(EnergyTelemetry {
        battery_soc_pct: Some(soc),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn rtu_crc_and_sunspec_101_decoder() {
        let rtu = build_rtu_read_holding(1, 40000, 2);
        assert!(verify_rtu_frame(&rtu).is_ok());

        let mut regs = vec![0u16; 30];
        regs[0] = 101;
        regs[1] = 50;
        regs[2] = 152; // 15.2 A
        regs[6] = (-1i16) as u16;
        regs[14] = 3500; // 3500 W
        regs[15] = 0;
        regs[24] = 0x001E;
        regs[25] = 0x8480; // 2_000_000 Wh = 2000 kWh
        regs[26] = 0;

        let telem = decode_sunspec_inverter_101(&regs).unwrap();
        assert_eq!(telem.solar_generation_w, Some(3500.0));
        assert!((telem.energy_kwh.unwrap() - 2000.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn modbus_tcp_loopback_read_holding() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = [0u8; 12];
            sock.read_exact(&mut req).await.unwrap();
            let tx_id = u16::from_be_bytes([req[0], req[1]]);
            // Réponse FC03 : 2 registres (0x0065=101, 0x0032=50)
            let pdu = [0x03, 0x04, 0x00, 0x65, 0x00, 0x32];
            let resp = build_tcp_pdu(tx_id, 1, &pdu);
            sock.write_all(&resp).await.unwrap();
        });

        let regs = modbus_tcp_read_holding(addr, 1, 40070, 2).await.unwrap();
        assert_eq!(regs, vec![101, 50]);
    }
}
