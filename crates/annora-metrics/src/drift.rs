use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuesDriftAlert {
    pub timestamp: Timestamp,
    pub cosine_drift: f64,
    pub threshold: f64,
    pub description: String,
}

pub struct ValuesDriftDetector {
    baseline_moral_vector: Vec<f64>,
    max_drift_threshold: f64,
}

/// Snapshot sérialisable de [`ValuesDriftDetector`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuesDriftSnapshot {
    pub baseline_moral_vector: Vec<f64>,
    pub max_drift_threshold: f64,
}

impl ValuesDriftDetector {
    pub fn new(baseline_moral_vector: Vec<f64>, max_drift_threshold: f64) -> Self {
        Self {
            baseline_moral_vector,
            max_drift_threshold,
        }
    }

    pub fn default_constitutional() -> Self {
        // 5 fundamental dimensions: Autonomy, Privacy, Honesty, Boundedness, Safety
        Self::new(vec![1.0, 1.0, 1.0, 1.0, 1.0], 0.15)
    }

    pub fn evaluate_current_vector(&self, current_vector: &[f64]) -> Option<ValuesDriftAlert> {
        if current_vector.len() != self.baseline_moral_vector.len() || current_vector.is_empty() {
            return None;
        }

        let dot_product: f64 = self
            .baseline_moral_vector
            .iter()
            .zip(current_vector)
            .map(|(&a, &b)| a * b)
            .sum();

        let norm_base: f64 = self
            .baseline_moral_vector
            .iter()
            .map(|&x| x * x)
            .sum::<f64>()
            .sqrt();
        let norm_curr: f64 = current_vector.iter().map(|&x| x * x).sum::<f64>().sqrt();

        if norm_base == 0.0 || norm_curr == 0.0 {
            return None;
        }

        let cosine_sim = (dot_product / (norm_base * norm_curr)).clamp(-1.0, 1.0);
        let cosine_drift = 1.0 - cosine_sim; // 0.0 = identical, >0.0 = drifted

        if cosine_drift > self.max_drift_threshold {
            Some(ValuesDriftAlert {
                timestamp: Timestamp::now(),
                cosine_drift,
                threshold: self.max_drift_threshold,
                description: format!(
                    "Constitutional values drift detected: drift {:.4} exceeded threshold {:.4}",
                    cosine_drift, self.max_drift_threshold
                ),
            })
        } else {
            None
        }
    }

    /// Snapshot mémoire simple (sérialisable en JSON).
    pub fn to_snapshot(&self) -> ValuesDriftSnapshot {
        ValuesDriftSnapshot {
            baseline_moral_vector: self.baseline_moral_vector.clone(),
            max_drift_threshold: self.max_drift_threshold,
        }
    }

    /// Persistance JSON simple via `std::fs`.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.to_snapshot())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let snap: ValuesDriftSnapshot = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            baseline_moral_vector: snap.baseline_moral_vector,
            max_drift_threshold: snap.max_drift_threshold,
        })
    }
}
