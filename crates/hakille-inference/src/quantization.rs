use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedTensorInt8 {
    pub data: Vec<i8>,
    pub scale: f32,
    pub shape: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedTensorQ4 {
    pub packed_nibbles: Vec<u8>,
    pub scale: f32,
    pub count: usize,
    pub shape: Vec<usize>,
}

pub fn quantize_f32_to_int8(data: &[f32], shape: Vec<usize>) -> QuantizedTensorInt8 {
    let max_abs = data
        .iter()
        .copied()
        .filter(|x| x.is_finite())
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    let scale = if max_abs > 0.0 && max_abs.is_finite() {
        max_abs / 127.0
    } else {
        1.0
    };

    let quantized_data = data
        .iter()
        .map(|&x| {
            let v = if x.is_finite() { x } else { 0.0 };
            (v / scale).round().clamp(-128.0, 127.0) as i8
        })
        .collect();

    QuantizedTensorInt8 {
        data: quantized_data,
        scale,
        shape,
    }
}

pub fn dequantize_int8_to_f32(tensor: &QuantizedTensorInt8) -> Vec<f32> {
    let scale = if tensor.scale.is_finite() {
        tensor.scale
    } else {
        0.0
    };
    tensor.data.iter().map(|&x| x as f32 * scale).collect()
}

pub fn quantize_f32_to_q4(data: &[f32], shape: Vec<usize>) -> QuantizedTensorQ4 {
    let max_abs = data
        .iter()
        .copied()
        .filter(|x| x.is_finite())
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    let scale = if max_abs > 0.0 && max_abs.is_finite() {
        max_abs / 7.0
    } else {
        1.0
    };

    let mut packed = Vec::with_capacity(data.len().div_ceil(2));
    for chunk in data.chunks(2) {
        let v0 = if chunk[0].is_finite() { chunk[0] } else { 0.0 };
        let q0 = (v0 / scale).round().clamp(-8.0, 7.0) as i8;
        let u0 = (q0 & 0x0F) as u8;

        let u1 = if chunk.len() > 1 {
            let v1 = if chunk[1].is_finite() { chunk[1] } else { 0.0 };
            let q1 = (v1 / scale).round().clamp(-8.0, 7.0) as i8;
            (q1 & 0x0F) as u8
        } else {
            0
        };

        packed.push((u1 << 4) | u0);
    }

    QuantizedTensorQ4 {
        packed_nibbles: packed,
        scale,
        count: data.len(),
        shape,
    }
}

pub fn dequantize_q4_to_f32(tensor: &QuantizedTensorQ4) -> Vec<f32> {
    let max_available = tensor.packed_nibbles.len().saturating_mul(2);
    let effective_count = tensor.count.min(max_available);
    let scale = if tensor.scale.is_finite() {
        tensor.scale
    } else {
        0.0
    };
    let mut out = Vec::with_capacity(effective_count);
    for &byte in &tensor.packed_nibbles {
        if out.len() < effective_count {
            let mut q0 = (byte & 0x0F) as i8;
            if q0 >= 8 {
                q0 -= 16;
            }
            out.push(q0 as f32 * scale);
        }
        if out.len() < effective_count {
            let mut q1 = ((byte >> 4) & 0x0F) as i8;
            if q1 >= 8 {
                q1 -= 16;
            }
            out.push(q1 as f32 * scale);
        }
    }
    out
}
