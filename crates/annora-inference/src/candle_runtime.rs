use annora_core::{AnnoraError, Result};
use annora_substrate::ComputeBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorBuffer {
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}

impl TensorBuffer {
    pub fn new(shape: Vec<usize>, data: Vec<f32>) -> Self {
        Self { shape, data }
    }

    pub fn zeros(shape: Vec<usize>) -> Self {
        let size = shape.iter().product();
        Self {
            shape,
            data: vec![0.0; size],
        }
    }

    pub fn argmax(&self) -> usize {
        let mut max_idx = 0;
        let mut max_val = f32::NEG_INFINITY;
        for (i, &val) in self.data.iter().enumerate() {
            if val > max_val {
                max_val = val;
            }
            max_idx = i;
        }
        max_idx
    }

    pub fn softmax(&self) -> Self {
        let max_val = self.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = self.data.iter().map(|&x| (x - max_val).exp()).collect();
        let sum_exp: f32 = exps.iter().sum();
        let data = exps.into_iter().map(|x| x / sum_exp.max(1e-12)).collect();
        Self {
            shape: self.shape.clone(),
            data,
        }
    }

    pub fn matmul(&self, other: &Self) -> Result<Self> {
        if self.shape.len() != 2 || other.shape.len() != 2 {
            return Err(AnnoraError::InferenceError("Matmul requires 2D tensors".into()));
        }
        let (m, k) = (self.shape[0], self.shape[1]);
        let (k2, n) = (other.shape[0], other.shape[1]);
        if k != k2 {
            return Err(AnnoraError::InferenceError(format!("Dimension mismatch: {} vs {}", k, k2)));
        }

        let mut out = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for l in 0..k {
                    sum += self.data[i * k + l] * other.data[l * n + j];
                }
                out[i * n + j] = sum;
            }
        }

        Ok(Self {
            shape: vec![m, n],
            data: out,
        })
    }
}

pub struct CandleRuntime {
    pub backend: ComputeBackend,
    pub model_id: String,
}

impl CandleRuntime {
    pub fn new(backend: ComputeBackend, model_id: impl Into<String>) -> Self {
        Self {
            backend,
            model_id: model_id.into(),
        }
    }

    pub fn forward_linear(
        &self,
        input: &TensorBuffer,
        weights: &TensorBuffer,
        bias: Option<&TensorBuffer>,
    ) -> Result<TensorBuffer> {
        let mut result = input.matmul(weights)?;
        if let Some(b) = bias {
            if b.data.len() == result.data.len() {
                for (r, bi) in result.data.iter_mut().zip(&b.data) {
                    *r += bi;
                }
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetensorsTensorInfo {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub data_offsets: (u64, u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetensorsModel {
    pub header_size: u64,
    pub metadata: HashMap<String, String>,
    pub tensors: HashMap<String, SafetensorsTensorInfo>,
    pub total_parameters: usize,
    pub data_size_bytes: u64,
}

impl SafetensorsModel {
    /// Parse and inspect a HuggingFace / Candle Safetensors model from disk.
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(|e| {
            AnnoraError::InferenceError(format!("Failed to open safetensors file {}: {}", path.display(), e))
        })?;

        let mut len_buf = [0u8; 8];
        file.read_exact(&mut len_buf).map_err(|e| {
            AnnoraError::InferenceError(format!("Failed to read safetensors header length: {}", e))
        })?;

        let header_size = u64::from_le_bytes(len_buf);
        if header_size > 50 * 1024 * 1024 {
            return Err(AnnoraError::InferenceError(format!(
                "Safetensors header size {} exceeds maximum threshold", header_size
            )));
        }

        let mut json_buf = vec![0u8; header_size as usize];
        file.read_exact(&mut json_buf).map_err(|e| {
            AnnoraError::InferenceError(format!("Failed to read safetensors JSON header: {}", e))
        })?;

        let json_val: serde_json::Value = serde_json::from_slice(&json_buf).map_err(|e| {
            AnnoraError::InferenceError(format!("Failed to parse safetensors JSON header: {}", e))
        })?;

        let obj = json_val.as_object().ok_or_else(|| {
            AnnoraError::InferenceError("Safetensors root JSON must be an object".into())
        })?;

        let mut metadata = HashMap::new();
        let mut tensors = HashMap::new();
        let mut total_parameters = 0usize;
        let mut data_size_bytes = 0u64;

        for (k, v) in obj {
            if k == "__metadata__" {
                if let Some(meta_obj) = v.as_object() {
                    for (mk, mv) in meta_obj {
                        if let Some(s) = mv.as_str() {
                            metadata.insert(mk.clone(), s.to_string());
                        } else {
                            metadata.insert(mk.clone(), mv.to_string());
                        }
                    }
                }
            } else {
                let dtype = v.get("dtype").and_then(|d| d.as_str()).unwrap_or("F32").to_string();
                let shape: Vec<usize> = v.get("shape")
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_u64().map(|n| n as usize)).collect())
                    .unwrap_or_default();
                let data_offsets = v.get("data_offsets")
                    .and_then(|o| o.as_array())
                    .map(|arr| {
                        let start = arr.get(0).and_then(|x| x.as_u64()).unwrap_or(0);
                        let end = arr.get(1).and_then(|x| x.as_u64()).unwrap_or(0);
                        (start, end)
                    })
                    .unwrap_or((0, 0));

                let params: usize = if shape.is_empty() { 1 } else { shape.iter().product() };
                total_parameters += params;
                data_size_bytes = data_size_bytes.max(data_offsets.1);

                tensors.insert(
                    k.clone(),
                    SafetensorsTensorInfo {
                        name: k.clone(),
                        dtype,
                        shape,
                        data_offsets,
                    },
                );
            }
        }

        Ok(Self {
            header_size,
            metadata,
            tensors,
            total_parameters,
            data_size_bytes,
        })
    }

    pub fn tensor_count(&self) -> usize {
        self.tensors.len()
    }

    pub fn codebook_count(&self) -> usize {
        self.tensors.keys()
            .filter(|k| k.contains("quantizer") || k.contains("vq") || k.contains("codebook"))
            .count()
    }

    pub fn find_tensor(&self, name_part: &str) -> Option<&SafetensorsTensorInfo> {
        self.tensors.iter().find(|(k, _)| k.contains(name_part)).map(|(_, v)| v)
    }
}
