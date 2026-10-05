use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskDomain {
    Coding,
    Planning,
    Automation,
    Writing,
    Communication,
    Analytics,
    Other,
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
}

impl Default for GeneralityTracker {
    fn default() -> Self {
        Self::new()
    }
}
