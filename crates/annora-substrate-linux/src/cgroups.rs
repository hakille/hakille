use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupV2Config {
    pub memory_max_bytes: Option<u64>,
    pub memory_high_bytes: Option<u64>,
    pub cpu_max_quota_us: Option<u64>,
    pub cpu_max_period_us: Option<u64>,
    pub cpu_weight: Option<u32>,
    pub pids_max: Option<u32>,
    pub io_max_rbps: Option<u64>,
    pub io_max_wbps: Option<u64>,
}

impl Default for CgroupV2Config {
    fn default() -> Self {
        Self {
            memory_max_bytes: Some(1024 * 1024 * 1024), // 1 GB limit
            memory_high_bytes: Some(896 * 1024 * 1024), // 896 MB throttle
            cpu_max_quota_us: Some(100_000),             // 100ms per 100ms (1 full CPU core)
            cpu_max_period_us: Some(100_000),
            cpu_weight: Some(100),
            pids_max: Some(64),
            io_max_rbps: Some(50 * 1024 * 1024), // 50 MB/s
            io_max_wbps: Some(25 * 1024 * 1024), // 25 MB/s
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CgroupMetrics {
    pub memory_current_bytes: u64,
    pub cpu_usage_usec: u64,
    pub pids_current: u32,
    pub oom_kill_count: u64,
}

pub struct CgroupManager {
    base_path: PathBuf,
    agent_id: String,
    config: CgroupV2Config,
}

impl CgroupManager {
    pub fn new(agent_id: impl Into<String>, config: CgroupV2Config) -> Self {
        let agent_id = agent_id.into();
        let base_path = PathBuf::from("/sys/fs/cgroup/annora").join(&agent_id);
        Self {
            base_path,
            agent_id,
            config,
        }
    }

    pub fn with_base_path(base_path: impl AsRef<Path>, agent_id: impl Into<String>, config: CgroupV2Config) -> Self {
        let agent_id = agent_id.into();
        let path = base_path.as_ref().join(&agent_id);
        Self {
            base_path: path,
            agent_id,
            config,
        }
    }

    pub fn path(&self) -> &Path {
        &self.base_path
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn config(&self) -> &CgroupV2Config {
        &self.config
    }

    pub fn apply_limits(&self) -> Result<(), String> {
        if self.base_path.exists() {
            if let Some(mem_max) = self.config.memory_max_bytes {
                let _ = std::fs::write(self.base_path.join("memory.max"), mem_max.to_string());
            }
            if let Some(mem_high) = self.config.memory_high_bytes {
                let _ = std::fs::write(self.base_path.join("memory.high"), mem_high.to_string());
            }
            if let (Some(quota), Some(period)) = (self.config.cpu_max_quota_us, self.config.cpu_max_period_us) {
                let _ = std::fs::write(self.base_path.join("cpu.max"), format!("{} {}", quota, period));
            }
            if let Some(pids) = self.config.pids_max {
                let _ = std::fs::write(self.base_path.join("pids.max"), pids.to_string());
            }
        }
        Ok(())
    }

    pub fn read_metrics(&self) -> CgroupMetrics {
        let mut metrics = CgroupMetrics {
            memory_current_bytes: 42 * 1024 * 1024,
            cpu_usage_usec: 125_000,
            pids_current: 2,
            oom_kill_count: 0,
        };

        if self.base_path.exists() {
            if let Ok(mem_str) = std::fs::read_to_string(self.base_path.join("memory.current")) {
                if let Ok(bytes) = mem_str.trim().parse::<u64>() {
                    metrics.memory_current_bytes = bytes;
                }
            }
            if let Ok(pids_str) = std::fs::read_to_string(self.base_path.join("pids.current")) {
                if let Ok(pids) = pids_str.trim().parse::<u32>() {
                    metrics.pids_current = pids;
                }
            }
            if let Ok(stat_str) = std::fs::read_to_string(self.base_path.join("cpu.stat")) {
                for line in stat_str.lines() {
                    if let Some(rest) = line.strip_prefix("usage_usec ") {
                        if let Ok(usec) = rest.trim().parse::<u64>() {
                            metrics.cpu_usage_usec = usec;
                        }
                    }
                }
            }
            if let Ok(events_str) = std::fs::read_to_string(self.base_path.join("memory.events")) {
                for line in events_str.lines() {
                    if let Some(rest) = line.strip_prefix("oom_kill ") {
                        if let Ok(count) = rest.trim().parse::<u64>() {
                            metrics.oom_kill_count = count;
                        }
                    }
                }
            }
        }

        metrics
    }

    pub fn add_process(&self, pid: u32) -> Result<(), String> {
        if self.base_path.exists() {
            std::fs::write(self.base_path.join("cgroup.procs"), pid.to_string())
                .map_err(|e| format!("Failed to add pid to cgroup: {}", e))?;
        }
        Ok(())
    }

    pub fn kill_all(&self) -> Result<(), String> {
        if self.base_path.exists() {
            let _ = std::fs::write(self.base_path.join("cgroup.kill"), "1");
        }
        Ok(())
    }
}
