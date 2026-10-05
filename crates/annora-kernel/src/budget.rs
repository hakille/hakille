use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineBudgetConfig {
    pub max_perception_ms: u64,
    pub max_routing_ms: u64,
    pub max_inference_ms: u64,
    pub max_action_ms: u64,
    pub max_total_ms: u64,
}

impl Default for PipelineBudgetConfig {
    fn default() -> Self {
        Self {
            max_perception_ms: 60,
            max_routing_ms: 20,
            max_inference_ms: 100,
            max_action_ms: 40,
            max_total_ms: 220,
        }
    }
}

pub struct LatencyTracker {
    config: PipelineBudgetConfig,
    start_time: Instant,
    stage_durations: Vec<(&'static str, Duration)>,
}

impl LatencyTracker {
    pub fn start(config: PipelineBudgetConfig) -> Self {
        Self {
            config,
            start_time: Instant::now(),
            stage_durations: Vec::new(),
        }
    }

    pub fn record_stage(&mut self, stage: &'static str, duration: Duration) {
        self.stage_durations.push((stage, duration));
    }

    pub fn total_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn is_within_budget(&self) -> bool {
        self.total_elapsed().as_millis() as u64 <= self.config.max_total_ms
    }

    pub fn remaining_budget_ms(&self) -> i64 {
        let elapsed = self.total_elapsed().as_millis() as i64;
        self.config.max_total_ms as i64 - elapsed
    }
}
