use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskDomain {
    Coding,
    Planning,
    Automation,
    Writing,
    Communication,
    Analytics,
    Other,
}

/// Snapshot sérialisable de [`GeneralityTracker`] pour persistance JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralitySnapshot {
    pub domain_counts: HashMap<TaskDomain, usize>,
    pub total_tasks: usize,
    pub generality_score: f64,
}

pub struct GeneralityTracker {
    domain_counts: HashMap<TaskDomain, usize>,
    total_tasks: usize,
}

impl GeneralityTracker {
    pub fn new() -> Self {
        Self {
            domain_counts: HashMap::new(),
            total_tasks: 0,
        }
    }

    pub fn record_task(&mut self, domain: TaskDomain) {
        *self.domain_counts.entry(domain).or_insert(0) += 1;
        self.total_tasks += 1;
    }

    /// Computes Shannon entropy of tasks handled across different domains,
    /// normalized by ln(num_domains). Returns 0.0 (only 1 domain) to 1.0 (perfectly balanced).
    pub fn generality_score(&self) -> f64 {
        if self.total_tasks == 0 || self.domain_counts.len() <= 1 {
            return 0.0;
        }

        let n = self.domain_counts.len() as f64;
        let max_entropy = n.ln();
        if max_entropy == 0.0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &count in self.domain_counts.values() {
            let p = count as f64 / self.total_tasks as f64;
            if p > 0.0 {
                entropy -= p * p.ln();
            }
        }

        (entropy / max_entropy).clamp(0.0, 1.0)
    }

    pub fn domain_distribution(&self) -> HashMap<TaskDomain, f64> {
        let mut dist = HashMap::new();
        if self.total_tasks == 0 {
            return dist;
        }

        for (&domain, &count) in &self.domain_counts {
            dist.insert(domain, count as f64 / self.total_tasks as f64);
        }
        dist
    }

    /// Snapshot mémoire simple (sérialisable en JSON).
    pub fn to_snapshot(&self) -> GeneralitySnapshot {
        GeneralitySnapshot {
            domain_counts: self.domain_counts.clone(),
            total_tasks: self.total_tasks,
            generality_score: self.generality_score(),
        }
    }

    /// Persistance JSON simple via `std::fs`.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.to_snapshot())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Recharge depuis un fichier JSON écrit par [`Self::save_to_file`].
    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let snap: GeneralitySnapshot = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            domain_counts: snap.domain_counts,
            total_tasks: snap.total_tasks,
        })
    }
}

impl Default for GeneralityTracker {
    fn default() -> Self {
        Self::new()
    }
}
