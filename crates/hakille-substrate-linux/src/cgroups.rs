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
            cpu_max_quota_us: Some(100_000),            // 100ms per 100ms (1 full CPU core)
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

/// Validate a cgroup v2 `io.max` target device. The kernel expects
/// `MAJOR:MINOR` (or the keyword `default` in recent kernels) as the first
/// whitespace-separated token of the line. Anything empty or containing
/// whitespace would corrupt that single-line format and is an explicit error.
pub fn validate_io_max_target(device: &str) -> Result<(), String> {
    let trimmed = device.trim();
    if trimmed.is_empty() {
        return Err("io.max target device is empty".to_string());
    }
    if trimmed.split_whitespace().count() != 1 {
        return Err(format!(
            "io.max target `{device}` must not contain whitespace"
        ));
    }
    if trimmed == "default" {
        return Ok(());
    }
    let Some((major, minor)) = trimmed.split_once(':') else {
        return Err(format!(
            "io.max target `{device}` must be `MAJOR:MINOR` (or `default`)"
        ));
    };
    for (part, field) in [(major, "major"), (minor, "minor")] {
        part.parse::<u32>()
            .map_err(|e| format!("io.max target `{device}`: {field} `{part}` is not a u32: {e}"))?;
    }
    Ok(())
}

/// Parse a single-number cgroup file (`memory.current`, `memory.usage_in_bytes`,
/// `pids.current`, …). An empty file or the v2 `max` sentinel (no limit, not a
/// measurement) are explicit errors — never silently coerced to 0.
pub fn parse_cgroup_single_u64(content: &str, field: &str) -> Result<u64, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(format!("{field}: empty content"));
    }
    if trimmed.eq_ignore_ascii_case("max") {
        return Err(format!("{field}: `max` means no limit, not a measurement"));
    }
    trimmed
        .parse::<u64>()
        .map_err(|e| format!("{field}: `{trimmed}` is not a u64: {e}"))
}

fn parse_keyed_u64(value: &str, field: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|e| format!("{field}: `{value}` is not a u64: {e}"))
}

/// Parse CPU usage in microseconds from either cgroup format:
/// - v2 `cpu.stat`: `usage_usec <n>`, or `user_usec` + `system_usec`;
/// - v1 `cpuacct.usage`: a bare nanosecond counter (converted to µs).
///
/// Unknown keys are ignored; a missing usage counter is an explicit error.
pub fn parse_cpu_stat_usage_usec(content: &str) -> Result<u64, String> {
    let mut usage_usec: Option<u64> = None;
    let mut user_usec: Option<u64> = None;
    let mut system_usec: Option<u64> = None;

    for line in content.lines() {
        let mut tokens = line.split_whitespace();
        let (Some(key), Some(value)) = (tokens.next(), tokens.next()) else {
            continue;
        };
        match key {
            "usage_usec" => usage_usec = Some(parse_keyed_u64(value, "cpu.stat usage_usec")?),
            "user_usec" => user_usec = Some(parse_keyed_u64(value, "cpu.stat user_usec")?),
            "system_usec" => system_usec = Some(parse_keyed_u64(value, "cpu.stat system_usec")?),
            _ => {}
        }
    }

    if let Some(usage) = usage_usec {
        return Ok(usage);
    }
    if let (Some(user), Some(system)) = (user_usec, system_usec) {
        return user
            .checked_add(system)
            .ok_or_else(|| "cpu.stat: user_usec + system_usec overflows u64".to_string());
    }
    let trimmed = content.trim();
    if !trimmed.is_empty()
        && trimmed.split_whitespace().count() == 1
        && let Ok(ns) = trimmed.parse::<u64>()
    {
        return Ok(ns / 1_000);
    }
    Err(
        "cpu.stat: no `usage_usec` (v2), no `user_usec`+`system_usec` pair (v2) \
         and no bare nanosecond counter (v1 cpuacct.usage)"
            .to_string(),
    )
}

/// Parse the OOM-kill counter from cgroup v2 `memory.events` or cgroup v1
/// `memory.oom_control` (both expose `oom_kill <n>`). A missing counter is an
/// explicit error — an absent file must not read as "zero kills".
pub fn parse_memory_events_oom_kill(content: &str) -> Result<u64, String> {
    for line in content.lines() {
        let mut tokens = line.split_whitespace();
        if tokens.next() != Some("oom_kill") {
            continue;
        }
        let value = tokens
            .next()
            .ok_or_else(|| "oom_kill: missing value".to_string())?;
        return parse_keyed_u64(value, "oom_kill");
    }
    Err("memory.events / memory.oom_control: no `oom_kill` counter found".to_string())
}

pub struct CgroupManager {
    base_path: PathBuf,
    agent_id: String,
    config: CgroupV2Config,
}

impl CgroupManager {
    pub fn new(agent_id: impl Into<String>, config: CgroupV2Config) -> Self {
        let agent_id = agent_id.into();
        let base_path = PathBuf::from("/sys/fs/cgroup/hakille").join(&agent_id);
        Self {
            base_path,
            agent_id,
            config,
        }
    }

    pub fn with_base_path(
        base_path: impl AsRef<Path>,
        agent_id: impl Into<String>,
        config: CgroupV2Config,
    ) -> Self {
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

    fn require_existing(&self, operation: &str) -> Result<(), String> {
        if self.base_path.exists() {
            Ok(())
        } else {
            Err(format!(
                "cgroup path {} does not exist: {operation} refused (no silent no-op)",
                self.base_path.display()
            ))
        }
    }

    fn write_file(&self, name: &str, value: &str) -> Result<(), String> {
        let path = self.base_path.join(name);
        std::fs::write(&path, value).map_err(|e| format!("write {} failed: {e}", path.display()))
    }

    /// Apply the configured limits to real cgroup files. Every unreadable or
    /// missing target is an explicit error — a missing cgroup is never a
    /// silent success.
    ///
    /// I/O bandwidth limits are device-scoped by the kernel (`io.max` needs a
    /// `MAJOR:MINOR` target per line), so they are applied by
    /// [`Self::apply_io_limits`]; when configured they are never dropped
    /// silently — a warning names the required call.
    pub fn apply_limits(&self) -> Result<(), String> {
        self.require_existing("apply_limits")?;
        if let Some(mem_max) = self.config.memory_max_bytes {
            self.write_file("memory.max", &mem_max.to_string())?;
        }
        if let Some(mem_high) = self.config.memory_high_bytes {
            self.write_file("memory.high", &mem_high.to_string())?;
        }
        if let (Some(quota), Some(period)) =
            (self.config.cpu_max_quota_us, self.config.cpu_max_period_us)
        {
            self.write_file("cpu.max", &format!("{quota} {period}"))?;
        }
        if let Some(weight) = self.config.cpu_weight {
            self.write_file("cpu.weight", &weight.to_string())?;
        }
        if let Some(pids) = self.config.pids_max {
            self.write_file("pids.max", &pids.to_string())?;
        }
        if self.config.io_max_rbps.is_some() || self.config.io_max_wbps.is_some() {
            tracing::warn!(
                "cgroup {}: io.max limits configured but not applied here (device-scoped); \
                 call apply_io_limits with a MAJOR:MINOR target to enforce them",
                self.base_path.display()
            );
        }
        Ok(())
    }

    /// Apply the configured I/O bandwidth limits to the real `io.max` file for
    /// one target device. A malformed target is an explicit error (never a
    /// silently skipped limit); a config without I/O limits is a no-op.
    pub fn apply_io_limits(&self, device: &str) -> Result<(), String> {
        validate_io_max_target(device)?;
        let rbps = self.config.io_max_rbps;
        let wbps = self.config.io_max_wbps;
        if rbps.is_none() && wbps.is_none() {
            return Ok(());
        }
        self.require_existing("apply_io_limits")?;
        let mut line = device.trim().to_string();
        if let Some(rbps) = rbps {
            line.push_str(&format!(" rbps={rbps}"));
        }
        if let Some(wbps) = wbps {
            line.push_str(&format!(" wbps={wbps}"));
        }
        self.write_file("io.max", &line)
    }

    fn read_first_u64(&self, candidates: &[&str], metric: &str) -> Result<u64, String> {
        let mut errors = Vec::new();
        for name in candidates {
            let path = self.base_path.join(name);
            match std::fs::read_to_string(&path) {
                Ok(content) => match parse_cgroup_single_u64(&content, name) {
                    Ok(value) => return Ok(value),
                    Err(e) => errors.push(e),
                },
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        Err(format!(
            "{metric}: no readable value in {} ({})",
            self.base_path.display(),
            errors.join("; ")
        ))
    }

    fn read_cpu_usage_usec(&self) -> Result<u64, String> {
        let mut errors = Vec::new();
        for name in ["cpu.stat", "cpuacct.usage"] {
            let path = self.base_path.join(name);
            match std::fs::read_to_string(&path) {
                Ok(content) => match parse_cpu_stat_usage_usec(&content) {
                    Ok(value) => return Ok(value),
                    Err(e) => errors.push(format!("{name}: {e}")),
                },
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        Err(format!(
            "cpu usage unavailable in {} ({})",
            self.base_path.display(),
            errors.join("; ")
        ))
    }

    fn read_oom_kill_count(&self) -> Result<u64, String> {
        let mut errors = Vec::new();
        for name in ["memory.events", "memory.oom_control"] {
            let path = self.base_path.join(name);
            match std::fs::read_to_string(&path) {
                Ok(content) => match parse_memory_events_oom_kill(&content) {
                    Ok(value) => return Ok(value),
                    Err(e) => errors.push(format!("{name}: {e}")),
                },
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        Err(format!(
            "OOM-kill count unavailable in {} ({})",
            self.base_path.display(),
            errors.join("; ")
        ))
    }

    /// Read real cgroup metrics (v2 files, v1 fallbacks). Missing cgroup or
    /// unreadable/malformed counters are explicit errors: quoted placeholder
    /// metrics are never returned.
    pub fn read_metrics(&self) -> Result<CgroupMetrics, String> {
        if !self.base_path.exists() {
            return Err(format!(
                "cgroup path {} does not exist: metrics unavailable (never simulated)",
                self.base_path.display()
            ));
        }
        let memory_current_bytes = self.read_first_u64(
            &["memory.current", "memory.usage_in_bytes"],
            "memory.current",
        )?;
        let pids_raw = self.read_first_u64(&["pids.current"], "pids.current")?;
        let pids_current = u32::try_from(pids_raw)
            .map_err(|_| format!("pids.current: {pids_raw} overflows u32"))?;
        let cpu_usage_usec = self.read_cpu_usage_usec()?;
        let oom_kill_count = self.read_oom_kill_count()?;

        Ok(CgroupMetrics {
            memory_current_bytes,
            cpu_usage_usec,
            pids_current,
            oom_kill_count,
        })
    }

    pub fn add_process(&self, pid: u32) -> Result<(), String> {
        self.require_existing("add_process")?;
        self.write_file("cgroup.procs", &pid.to_string())
    }

    pub fn kill_all(&self) -> Result<(), String> {
        self.require_existing("kill_all")?;
        self.write_file("cgroup.kill", "1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fixture_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "hakille-cgroup-test-{}-{name}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        dir
    }

    fn write_fixture(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).expect("write fixture");
    }

    fn manager_for(dir: &Path) -> CgroupManager {
        CgroupManager::with_base_path(dir, "agent-fixture", CgroupV2Config::default())
    }

    #[test]
    fn parse_single_u64_rejects_max_empty_and_garbage() {
        assert_eq!(
            parse_cgroup_single_u64("104857600\n", "memory.current").unwrap(),
            104857600
        );
        assert!(parse_cgroup_single_u64("max\n", "memory.max").is_err());
        assert!(parse_cgroup_single_u64("   \n", "memory.current").is_err());
        assert!(parse_cgroup_single_u64("12.5", "memory.current").is_err());
        assert!(parse_cgroup_single_u64("-1", "memory.current").is_err());
    }

    #[test]
    fn parse_cpu_stat_v2_usage_usec() {
        let fixture = "usage_usec 125000\nuser_usec 100000\nsystem_usec 25000\nnr_periods 12\nnr_throttled 1\nthrottled_usec 5000\n";
        assert_eq!(parse_cpu_stat_usage_usec(fixture).unwrap(), 125_000);
    }

    #[test]
    fn parse_cpu_stat_v2_user_plus_system_fallback() {
        let fixture = "user_usec 100000\nsystem_usec 25000\n";
        assert_eq!(parse_cpu_stat_usage_usec(fixture).unwrap(), 125_000);
    }

    #[test]
    fn parse_cpu_stat_v1_bare_nanoseconds() {
        assert_eq!(parse_cpu_stat_usage_usec("250000000\n").unwrap(), 250_000);
    }

    #[test]
    fn parse_cpu_stat_missing_usage_is_explicit_error() {
        let err =
            parse_cpu_stat_usage_usec("nr_periods 0\nnr_throttled 0\n").expect_err("no usage");
        assert!(err.contains("usage_usec"), "got: {err}");
        assert!(parse_cpu_stat_usage_usec("").is_err());
        assert!(parse_cpu_stat_usage_usec("usage_usec not-a-number\n").is_err());
    }

    #[test]
    fn parse_memory_events_v2_oom_kill() {
        let fixture = "low 0\nhigh 0\nmax 1\noom 0\noom_kill 3\noom_group_kill 0\n";
        assert_eq!(parse_memory_events_oom_kill(fixture).unwrap(), 3);
    }

    #[test]
    fn parse_memory_oom_control_v1_oom_kill() {
        let fixture = "oom_kill_disable 0\nunder_oom 0\noom_kill 2\n";
        assert_eq!(parse_memory_events_oom_kill(fixture).unwrap(), 2);
    }

    #[test]
    fn parse_memory_events_without_oom_kill_is_explicit_error() {
        let err = parse_memory_events_oom_kill("low 0\nhigh 0\n").expect_err("no counter");
        assert!(err.contains("oom_kill"), "got: {err}");
        assert!(parse_memory_events_oom_kill("").is_err());
        assert!(parse_memory_events_oom_kill("oom_kill x\n").is_err());
    }

    #[test]
    fn read_metrics_v2_fixture_returns_real_values() {
        let dir = fixture_dir("v2");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();
        write_fixture(&root, "memory.current", "104857600\n");
        write_fixture(&root, "pids.current", "7\n");
        write_fixture(
            &root,
            "cpu.stat",
            "usage_usec 125000\nuser_usec 100000\nsystem_usec 25000\n",
        );
        write_fixture(
            &root,
            "memory.events",
            "low 0\nhigh 0\nmax 1\noom 0\noom_kill 3\noom_group_kill 0\n",
        );

        let metrics = manager_for(&dir).read_metrics().expect("v2 fixture reads");
        assert_eq!(
            metrics,
            CgroupMetrics {
                memory_current_bytes: 104_857_600,
                cpu_usage_usec: 125_000,
                pids_current: 7,
                oom_kill_count: 3,
            }
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_metrics_v1_fixture_returns_real_values() {
        let dir = fixture_dir("v1");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();
        write_fixture(&root, "memory.usage_in_bytes", "52428800\n");
        write_fixture(&root, "pids.current", "3\n");
        write_fixture(&root, "cpuacct.usage", "250000000\n");
        write_fixture(
            &root,
            "memory.oom_control",
            "oom_kill_disable 0\nunder_oom 0\noom_kill 2\n",
        );

        let metrics = manager_for(&dir).read_metrics().expect("v1 fixture reads");
        assert_eq!(metrics.memory_current_bytes, 52_428_800);
        assert_eq!(metrics.cpu_usage_usec, 250_000);
        assert_eq!(metrics.pids_current, 3);
        assert_eq!(metrics.oom_kill_count, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_metrics_missing_cgroup_path_is_explicit_error() {
        let dir = fixture_dir("missing");
        let err = manager_for(&dir)
            .read_metrics()
            .expect_err("missing cgroup must not fabricate metrics");
        assert!(err.contains("does not exist"), "got: {err}");
        assert!(err.contains("never simulated"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_metrics_missing_field_is_explicit_error() {
        let dir = fixture_dir("partial");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();
        write_fixture(&root, "memory.current", "104857600\n");
        write_fixture(&root, "pids.current", "7\n");
        write_fixture(&root, "cpu.stat", "usage_usec 125000\n");

        let err = manager_for(&dir)
            .read_metrics()
            .expect_err("missing memory.events must fail, not read as 0 kills");
        assert!(err.contains("OOM-kill count unavailable"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_limits_writes_real_files_and_missing_path_is_error() {
        let dir = fixture_dir("apply");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();

        manager_for(&dir)
            .apply_limits()
            .expect("apply on existing cgroup");
        assert_eq!(
            std::fs::read_to_string(root.join("memory.max")).unwrap(),
            (1024 * 1024 * 1024u64).to_string()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("memory.high")).unwrap(),
            (896 * 1024 * 1024u64).to_string()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("cpu.max")).unwrap(),
            "100000 100000"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("cpu.weight")).unwrap(),
            "100"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("pids.max")).unwrap(),
            "64"
        );

        let missing = fixture_dir("apply-missing");
        let err = manager_for(&missing)
            .apply_limits()
            .expect_err("missing cgroup path must fail, not silently no-op");
        assert!(err.contains("does not exist"), "got: {err}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&missing);
    }

    #[test]
    fn validate_io_max_target_accepts_real_devices_and_rejects_junk() {
        assert!(validate_io_max_target("8:0").is_ok());
        assert!(validate_io_max_target("259:4").is_ok());
        assert!(validate_io_max_target("default").is_ok());
        assert!(validate_io_max_target("  8:0  ").is_ok());
        assert!(validate_io_max_target("").is_err());
        assert!(validate_io_max_target("8").is_err());
        assert!(validate_io_max_target("8:0 rbps=1").is_err());
        assert!(validate_io_max_target("x:0").is_err());
        assert!(validate_io_max_target("8:y").is_err());
        assert!(validate_io_max_target("8:").is_err());
    }

    #[test]
    fn apply_io_limits_writes_device_scoped_line_and_fails_honestly() {
        let dir = fixture_dir("io");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();

        manager_for(&dir)
            .apply_io_limits("8:0")
            .expect("io.max write");
        assert_eq!(
            std::fs::read_to_string(root.join("io.max")).unwrap(),
            "8:0 rbps=52428800 wbps=26214400"
        );

        let err = manager_for(&dir)
            .apply_io_limits("not-a-device")
            .expect_err("malformed target must fail explicitly");
        assert!(err.contains("MAJOR:MINOR"), "got: {err}");

        let missing = fixture_dir("io-missing");
        let err = manager_for(&missing)
            .apply_io_limits("8:0")
            .expect_err("missing cgroup must fail, not silently no-op");
        assert!(err.contains("does not exist"), "got: {err}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&missing);
    }

    #[test]
    fn apply_io_limits_without_configured_bounds_is_noop() {
        let dir = fixture_dir("io-none");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();
        let config = CgroupV2Config {
            io_max_rbps: None,
            io_max_wbps: None,
            ..CgroupV2Config::default()
        };
        let manager = CgroupManager::with_base_path(&dir, "agent-fixture", config);
        manager
            .apply_io_limits("8:0")
            .expect("no-op without io bounds");
        assert!(!root.join("io.max").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_process_and_kill_all_write_and_fail_honestly() {
        let dir = fixture_dir("procs");
        let root = dir.join("agent-fixture");
        std::fs::create_dir_all(&root).unwrap();

        manager_for(&dir).add_process(4242).expect("add pid");
        assert_eq!(
            std::fs::read_to_string(root.join("cgroup.procs")).unwrap(),
            "4242"
        );
        manager_for(&dir).kill_all().expect("kill");
        assert_eq!(
            std::fs::read_to_string(root.join("cgroup.kill")).unwrap(),
            "1"
        );

        let missing = fixture_dir("procs-missing");
        assert!(manager_for(&missing).add_process(1).is_err());
        assert!(manager_for(&missing).kill_all().is_err());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&missing);
    }
}
