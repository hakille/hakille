use annora_core::{AnnoraError, Result};
use annora_substrate::ComputeBackend;
use serde::{Deserialize, Serialize};

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
                max_idx = i;
            }
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
