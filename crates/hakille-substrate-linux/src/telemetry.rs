use hakille_substrate::{PowerState, ThermalState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(target_os = "linux")]
use std::path::Path;

/// Origin of a [`MeminfoData`] reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeminfoSource {
    /// Parsed from the real `/proc/meminfo` of the running Linux host.
    Proc,
    /// Deterministic stand-in returned because no real meminfo could be read
    /// (non-Linux host, container without procfs, read/permission failure).
    /// The numbers are fabricated — callers must check
    /// [`MeminfoData::is_simulated`] before trusting them.
    #[default]
    Simulated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeminfoData {
    pub mem_total_bytes: u64,
    pub mem_free_bytes: u64,
    pub mem_available_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    /// Where this reading comes from. Defaults to `Simulated` when absent from
    /// serialized data (an unlabeled reading is never trusted as real host data).
    #[serde(default)]
    pub source: MeminfoSource,
}

impl MeminfoData {
    /// `true` when these values are fabricated (see [`MeminfoSource::Simulated`]).
    pub fn is_simulated(&self) -> bool {
        self.source == MeminfoSource::Simulated
    }

    pub fn used_ram_bytes(&self) -> u64 {
        self.mem_total_bytes
            .saturating_sub(self.mem_available_bytes)
    }

    pub fn used_swap_bytes(&self) -> u64 {
        self.swap_total_bytes.saturating_sub(self.swap_free_bytes)
    }
}

const MEMINFO_KEYS: [&str; 7] = [
    "MemTotal",
    "MemFree",
    "MemAvailable",
    "Buffers",
    "Cached",
    "SwapTotal",
    "SwapFree",
];

fn meminfo_unit_to_bytes(num: u64, unit: &str) -> Result<u64, String> {
    let factor: u64 = match unit {
        "" | "B" => 1,
        u if u.eq_ignore_ascii_case("kb") || u.eq_ignore_ascii_case("kib") => 1024,
        u if u.eq_ignore_ascii_case("mb") || u.eq_ignore_ascii_case("mib") => 1024 * 1024,
        u if u.eq_ignore_ascii_case("gb") || u.eq_ignore_ascii_case("gib") => 1024 * 1024 * 1024,
        other => return Err(format!("unsupported unit `{other}`")),
    };
    num.checked_mul(factor)
        .ok_or_else(|| format!("value {num} {unit} overflows u64 bytes"))
}

/// Pure parser for `/proc/meminfo` style content. Empty lines and unknown keys
/// are ignored; malformed or non-numeric values of the seven known keys, and
/// unknown units, are explicit errors — never a silently invented value.
/// Only `MemTotal` is mandatory; absent optional keys read as 0 (absent) and a
/// missing `MemAvailable` falls back to `MemFree + Buffers + Cached` (the
/// historical pre-3.14 kernel heuristic).
pub fn parse_meminfo(content: &str) -> Result<MeminfoData, String> {
    let mut map: HashMap<&str, u64> = HashMap::new();
    for (line_no, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !MEMINFO_KEYS.contains(&key) {
            continue;
        }
        let mut tokens = value.split_whitespace();
        let num_raw = tokens
            .next()
            .ok_or_else(|| format!("line {}: `{key}` has no value", line_no + 1))?;
        let num = num_raw.parse::<u64>().map_err(|e| {
            format!(
                "line {}: `{key}` value `{num_raw}` is not a number: {e}",
                line_no + 1
            )
        })?;
        let unit = tokens.next().unwrap_or("");
        let bytes = meminfo_unit_to_bytes(num, unit)
            .map_err(|e| format!("line {}: `{key}`: {e}", line_no + 1))?;
        map.insert(key, bytes);
    }

    let mem_total_bytes = *map
        .get("MemTotal")
        .ok_or("MemTotal not found in /proc/meminfo")?;
    let mem_free_bytes = *map.get("MemFree").unwrap_or(&0);
    let buffers_bytes = *map.get("Buffers").unwrap_or(&0);
    let cached_bytes = *map.get("Cached").unwrap_or(&0);
    // Checked addition: extreme (or hostile) fixture values must yield an
    // explicit error, never a debug-build overflow panic and never a wrapped
    // `MemAvailable`.
    let fallback_available = mem_free_bytes
        .checked_add(buffers_bytes)
        .and_then(|sum| sum.checked_add(cached_bytes))
        .ok_or("MemFree + Buffers + Cached overflows u64 (MemAvailable fallback)")?;
    let mem_available_bytes = *map.get("MemAvailable").unwrap_or(&fallback_available);
    let swap_total_bytes = *map.get("SwapTotal").unwrap_or(&0);
    let swap_free_bytes = *map.get("SwapFree").unwrap_or(&0);

    Ok(MeminfoData {
        mem_total_bytes,
        mem_free_bytes,
        mem_available_bytes,
        buffers_bytes,
        cached_bytes,
        swap_total_bytes,
        swap_free_bytes,
        source: MeminfoSource::Proc,
    })
}

/// Deterministic stand-in used only when the real `/proc/meminfo` is
/// unavailable. Values are fabricated (64 GiB class machine) and tagged
/// [`MeminfoSource::Simulated`] — never presented as host truth.
fn simulated_meminfo(reason: &str) -> MeminfoData {
    tracing::warn!(
        "meminfo unavailable ({reason}); returning SIMULATED values tagged source=Simulated"
    );
    MeminfoData {
        mem_total_bytes: 64 * 1024 * 1024 * 1024,
        mem_free_bytes: 32 * 1024 * 1024 * 1024,
        mem_available_bytes: 48 * 1024 * 1024 * 1024,
        buffers_bytes: 1024 * 1024 * 1024,
        cached_bytes: 15 * 1024 * 1024 * 1024,
        swap_total_bytes: 8 * 1024 * 1024 * 1024,
        swap_free_bytes: 8 * 1024 * 1024 * 1024,
        source: MeminfoSource::Simulated,
    }
}

/// Honest host memory reader. On Linux this reads and parses the real
/// `/proc/meminfo` (explicit error on unreadable file or malformed content);
/// elsewhere it returns an explicit error — no fabricated success.
pub fn try_read_host_meminfo() -> Result<MeminfoData, String> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/meminfo")
            .map_err(|e| format!("/proc/meminfo unreadable: {e}"))?;
        parse_meminfo(&content)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("not a Linux host: /proc/meminfo does not exist".to_string())
    }
}

/// Total wrapper kept for callers that cannot handle a `Result`. When the real
/// reading fails, returns simulated data explicitly tagged [`MeminfoSource::Simulated`]
/// (and logs a warning) instead of an unlabeled fabricated default.
pub fn read_host_meminfo() -> MeminfoData {
    match try_read_host_meminfo() {
        Ok(data) => data,
        Err(reason) => simulated_meminfo(&reason),
    }
}

/// Real `/proc/loadavg` reading: three load averages plus the
/// `runnable/total` process counts and the last allocated PID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadAverage {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
    pub runnable_processes: Option<u32>,
    pub total_processes: Option<u32>,
    pub last_pid: Option<u64>,
}

/// Pure parser for `/proc/loadavg` style content
/// (`0.42 0.31 0.25 2/534 12345`). The three load averages are mandatory;
/// non-numeric or missing fields are explicit errors. The `run/total` pair and
/// the last PID are optional (older/trimmed fixtures may omit them).
pub fn parse_loadavg(content: &str) -> Result<LoadAverage, String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "/proc/loadavg: empty content".to_string())?;
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(format!(
            "/proc/loadavg: expected at least 3 fields (1/5/15 min load), got {} in `{line}`",
            tokens.len()
        ));
    }
    let parse_load = |raw: &str, field: &str| {
        raw.parse::<f64>()
            .map_err(|e| format!("/proc/loadavg: {field} `{raw}` is not a number: {e}"))
    };
    let load_1min = parse_load(tokens[0], "load_1min")?;
    let load_5min = parse_load(tokens[1], "load_5min")?;
    let load_15min = parse_load(tokens[2], "load_15min")?;

    let (runnable_processes, total_processes) = match tokens.get(3) {
        Some(pair) => {
            let (runnable_raw, total_raw) = pair.split_once('/').ok_or_else(|| {
                format!("/proc/loadavg: process field `{pair}` is not in `run/total` form")
            })?;
            let runnable = runnable_raw
                .parse::<u32>()
                .map_err(|e| format!("/proc/loadavg: runnable count `{runnable_raw}`: {e}"))?;
            let total = total_raw
                .parse::<u32>()
                .map_err(|e| format!("/proc/loadavg: total count `{total_raw}`: {e}"))?;
            (Some(runnable), Some(total))
        }
        None => (None, None),
    };
    let last_pid = match tokens.get(4) {
        Some(raw) => Some(
            raw.parse::<u64>()
                .map_err(|e| format!("/proc/loadavg: last pid `{raw}`: {e}"))?,
        ),
        None => None,
    };

    Ok(LoadAverage {
        load_1min,
        load_5min,
        load_15min,
        runnable_processes,
        total_processes,
        last_pid,
    })
}

/// Honest `/proc/loadavg` reader: real read on Linux with explicit errors,
/// explicit "not a Linux host" error elsewhere.
pub fn try_read_host_loadavg() -> Result<LoadAverage, String> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/loadavg")
            .map_err(|e| format!("/proc/loadavg unreadable: {e}"))?;
        parse_loadavg(&content)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("not a Linux host: /proc/loadavg does not exist".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinuxPowerSupplyData {
    pub name: String,
    pub supply_type: String, // "Battery", "Mains", "USB"
    pub status: String,      // "Charging", "Discharging", "Full", "Not charging"
    pub capacity_pct: f32,
    pub online: bool,
}

/// Pure parser for sysfs power-supply fields. Non-numeric or out-of-range
/// capacity, and an `online` flag that is neither `0` nor `1`, are explicit
/// errors. When `online` is absent it is inferred from the status
/// (Charging/Full ⇒ online).
pub fn parse_power_supply_fields(
    name: &str,
    status: &str,
    capacity: &str,
    supply_type: &str,
    online: Option<&str>,
) -> Result<LinuxPowerSupplyData, String> {
    let capacity_raw = capacity.trim();
    let capacity_pct = capacity_raw.parse::<f32>().map_err(|e| {
        format!("power supply `{name}`: capacity `{capacity_raw}` is not a number: {e}")
    })?;
    if !capacity_pct.is_finite() || !(0.0..=100.0).contains(&capacity_pct) {
        return Err(format!(
            "power supply `{name}`: capacity {capacity_pct} outside 0..=100"
        ));
    }
    let status = status.trim();
    let is_online = match online {
        Some(raw) => match raw.trim() {
            "1" => true,
            "0" => false,
            other => {
                return Err(format!(
                    "power supply `{name}`: online flag `{other}` is neither 0 nor 1"
                ));
            }
        },
        None => status.eq_ignore_ascii_case("Charging") || status.eq_ignore_ascii_case("Full"),
    };

    Ok(LinuxPowerSupplyData {
        name: name.to_string(),
        supply_type: supply_type.trim().to_string(),
        status: status.to_string(),
        capacity_pct,
        online: is_online,
    })
}

/// Honest power-supply reader. On Linux this scans `/sys/class/power_supply`
/// and returns the real battery reading, or an explicit error when no supply
/// is readable; elsewhere it is an explicit error.
pub fn try_read_host_power_state() -> Result<PowerState, String> {
    #[cfg(target_os = "linux")]
    {
        let power_supply_dir = Path::new("/sys/class/power_supply");
        if !power_supply_dir.is_dir() {
            return Err("/sys/class/power_supply does not exist on this host".to_string());
        }
        let entries = std::fs::read_dir(power_supply_dir)
            .map_err(|e| format!("/sys/class/power_supply unreadable: {e}"))?;
        let mut found_battery: Option<LinuxPowerSupplyData> = None;
        let mut ac_online = false;

        for entry in entries.flatten() {
            let path = entry.path();
            let supply_name = entry.file_name().to_string_lossy().to_string();
            let supply_type = std::fs::read_to_string(path.join("type")).unwrap_or_default();
            let supply_type = supply_type.trim();

            if supply_type.eq_ignore_ascii_case("Mains") {
                if let Ok(on_str) = std::fs::read_to_string(path.join("online")) {
                    if on_str.trim() == "1" {
                        ac_online = true;
                    }
                }
            } else if supply_type.eq_ignore_ascii_case("Battery") {
                let status = std::fs::read_to_string(path.join("status"))
                    .unwrap_or_else(|_| "Unknown".to_string());
                match std::fs::read_to_string(path.join("capacity")) {
                    Ok(capacity) => {
                        match parse_power_supply_fields(
                            &supply_name,
                            &status,
                            &capacity,
                            supply_type,
                            None,
                        ) {
                            Ok(battery) => found_battery = Some(battery),
                            Err(e) => tracing::warn!("ignoring battery `{supply_name}`: {e}"),
                        }
                    }
                    Err(e) => {
                        tracing::warn!("ignoring battery `{supply_name}`: capacity unreadable: {e}")
                    }
                }
            }
        }

        if let Some(bat) = found_battery {
            let is_charging = bat.status.eq_ignore_ascii_case("Charging") || ac_online;
            let on_battery = bat.status.eq_ignore_ascii_case("Discharging") || !ac_online;
            return Ok(PowerState {
                on_battery,
                battery_percent: bat.capacity_pct,
                charging: is_charging,
            });
        }
        if ac_online {
            // Mains-only machine (desktop/server): no battery exists to report.
            tracing::debug!(
                "no battery in /sys/class/power_supply; AC supply online, reporting desktop-on-AC"
            );
            return Ok(PowerState {
                on_battery: false,
                battery_percent: 100.0,
                charging: true,
            });
        }
        Err("no readable battery and no online AC supply in /sys/class/power_supply".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("not a Linux host: /sys/class/power_supply does not exist".to_string())
    }
}

/// Total wrapper kept for the `Substrate` trait (which cannot return an error).
/// When no real reading exists it falls back to the workspace-wide
/// "desktop on AC" assumption and logs a warning — `PowerState` has no unknown
/// variant. Call [`try_read_host_power_state`] for the honest error.
pub fn read_host_power_state() -> PowerState {
    match try_read_host_power_state() {
        Ok(state) => state,
        Err(reason) => {
            tracing::warn!(
                "power state unavailable ({reason}); assuming desktop on AC \
                 (battery_percent=100, charging=true) — SIMULATED, not a host measurement"
            );
            PowerState {
                on_battery: false,
                battery_percent: 100.0,
                charging: true,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinuxThermalZoneData {
    pub zone_name: String,
    pub temp_celsius: f32,
    pub zone_type: String,
    pub is_throttled: bool,
}

/// Conventional package thermal trip point used when sysfs trip points are
/// not sampled.
pub const LINUX_THERMAL_THROTTLE_CELSIUS: f32 = 85.0;

/// Pure parser for sysfs `thermal_zone*/temp` (millidegrees Celsius).
/// Non-numeric, non-finite or implausible temperatures are explicit errors —
/// never a fabricated 42 °C fallback.
pub fn parse_thermal_zone(
    temp_raw: &str,
    zone_type: &str,
    name: &str,
) -> Result<LinuxThermalZoneData, String> {
    let raw = temp_raw.trim();
    let millicelsius = raw
        .parse::<f32>()
        .map_err(|e| format!("thermal zone `{name}`: `{raw}` is not a number: {e}"))?;
    if !millicelsius.is_finite() {
        return Err(format!(
            "thermal zone `{name}`: non-finite temperature `{raw}`"
        ));
    }
    let celsius = millicelsius / 1000.0;
    if !(-100.0..=250.0).contains(&celsius) {
        return Err(format!(
            "thermal zone `{name}`: {celsius} °C outside plausible range -100..=250"
        ));
    }
    Ok(LinuxThermalZoneData {
        zone_name: name.to_string(),
        temp_celsius: celsius,
        zone_type: zone_type.trim().to_string(),
        is_throttled: celsius >= LINUX_THERMAL_THROTTLE_CELSIUS,
    })
}

/// Real sysfs thermal reading on Linux; `ThermalState::Unknown` elsewhere.
/// Unreadable or malformed zones are skipped with a warning — never an
/// invented temperature.
pub fn read_host_thermal_state() -> ThermalState {
    #[cfg(target_os = "linux")]
    {
        let thermal_dir = Path::new("/sys/class/thermal");
        if thermal_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(thermal_dir) {
                let mut highest_temp: f32 = 0.0;
                let mut any_throttled = false;
                let mut found_any = false;

                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with("thermal_zone") {
                        continue;
                    }
                    let path = entry.path();
                    let Ok(temp_str) = std::fs::read_to_string(path.join("temp")) else {
                        continue;
                    };
                    let type_str = std::fs::read_to_string(path.join("type"))
                        .unwrap_or_else(|_| "unknown".to_string());
                    match parse_thermal_zone(&temp_str, &type_str, &name) {
                        Ok(data) => {
                            found_any = true;
                            if data.temp_celsius > highest_temp {
                                highest_temp = data.temp_celsius;
                            }
                            if data.is_throttled {
                                any_throttled = true;
                            }
                        }
                        Err(e) => tracing::warn!("ignoring thermal zone `{name}`: {e}"),
                    }
                }

                if found_any {
                    return ThermalState {
                        temperature_celsius: highest_temp,
                        throttled: any_throttled,
                    };
                }
            }
        }
        ThermalState::Unknown
    }
    #[cfg(not(target_os = "linux"))]
    {
        ThermalState::Unknown
    }
}

/// Detect dedicated VRAM by summing `mem_info_vram_total` across DRM render
/// devices (`/sys/class/drm/cardN/device/…`, amdgpu et al.).
/// Returns `None` when no discrete VRAM is discoverable (Intel iGPU with
/// shared memory, VM without GPU passthrough, sysfs unreadable) — the
/// honest "unknown" signal, never an invented size. std only, no new deps.
pub fn detect_linux_vram_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let drm_dir = Path::new("/sys/class/drm");
        let entries = std::fs::read_dir(drm_dir).ok()?;
        let mut total: u64 = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Physical cards only (`card0`, `card1` …), not connectors (`card0-DP-1`).
            if !name.starts_with("card") || !name[4..].bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let device_dir = entry.path().join("device");
            for candidate in ["mem_info_vram_total", "mem_info_vis_vram_total"] {
                if let Ok(raw) = std::fs::read_to_string(device_dir.join(candidate)) {
                    if let Ok(bytes) = raw.trim().parse::<u64>() {
                        total = total.saturating_add(bytes);
                        break;
                    }
                }
            }
        }
        if total > 0 { Some(total) } else { None }
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPICAL_MEMINFO: &str = "\
MemTotal:       32890624 kB
MemFree:         1536000 kB
MemAvailable:   20971520 kB
Buffers:          208896 kB
Cached:          9437184 kB
SwapCached:            0 kB
Active:         14000000 kB
Inactive:        6000000 kB
SwapTotal:       8388608 kB
SwapFree:        8388608 kB
Dirty:               128 kB
Writeback:             0 kB
Hugepagesize:       2048 kB
DirectMap4k:      262144 kB
";

    #[test]
    fn meminfo_typical_fixture_parses_kb_units() {
        let parsed = parse_meminfo(TYPICAL_MEMINFO).expect("typical meminfo must parse");
        assert_eq!(parsed.mem_total_bytes, 32890624 * 1024);
        assert_eq!(parsed.mem_free_bytes, 1536000 * 1024);
        assert_eq!(parsed.mem_available_bytes, 20971520 * 1024);
        assert_eq!(parsed.buffers_bytes, 208896 * 1024);
        assert_eq!(parsed.cached_bytes, 9437184 * 1024);
        assert_eq!(parsed.swap_total_bytes, 8388608 * 1024);
        assert_eq!(parsed.swap_free_bytes, 8388608 * 1024);
        assert_eq!(parsed.used_ram_bytes(), (32890624 - 20971520) * 1024);
        assert_eq!(parsed.source, MeminfoSource::Proc);
        assert!(!parsed.is_simulated());
    }

    #[test]
    fn meminfo_missing_mem_total_is_explicit_error() {
        let err = parse_meminfo("MemFree: 1024 kB\n").expect_err("MemTotal is mandatory");
        assert!(err.contains("MemTotal"), "got: {err}");
    }

    #[test]
    fn meminfo_empty_content_is_explicit_error() {
        assert!(parse_meminfo("").is_err());
        assert!(parse_meminfo("\n  \n").is_err());
    }

    #[test]
    fn meminfo_non_numeric_known_value_is_explicit_error() {
        let err = parse_meminfo("MemTotal: lots kB\n").expect_err("non-numeric value must fail");
        assert!(err.contains("MemTotal"), "got: {err}");
        assert!(err.contains("not a number"), "got: {err}");
    }

    #[test]
    fn meminfo_unknown_unit_is_explicit_error() {
        let err =
            parse_meminfo("MemTotal: 100 xB\n").expect_err("unknown unit must fail explicitly");
        assert!(err.contains("unsupported unit"), "got: {err}");
    }

    #[test]
    fn meminfo_overflow_units_is_explicit_error() {
        let err = parse_meminfo("MemTotal: 18446744073709551615 kB\n")
            .expect_err("overflowing unit conversion must fail");
        assert!(err.contains("overflows"), "got: {err}");
    }

    #[test]
    fn meminfo_accepts_bare_bytes_and_b_unit() {
        let bare = parse_meminfo("MemTotal: 1024\n").unwrap();
        assert_eq!(bare.mem_total_bytes, 1024);
        let with_b = parse_meminfo("MemTotal: 2048 B\n").unwrap();
        assert_eq!(with_b.mem_total_bytes, 2048);
    }

    #[test]
    fn meminfo_unknown_keys_and_empty_lines_are_ignored() {
        let content =
            "\n# comment-like decoration\nMemTotal: 1048576 kB\nSomethingElse: not-a-number zz\n\n";
        let parsed = parse_meminfo(content).expect("unknown keys must not fail the parse");
        assert_eq!(parsed.mem_total_bytes, 1024 * 1024 * 1024);
    }

    #[test]
    fn meminfo_handles_crlf_and_extra_whitespace() {
        let content = "MemTotal:\t\t4194304   kB\r\nMemFree:  1024 kB\r\n";
        let parsed = parse_meminfo(content).unwrap();
        assert_eq!(parsed.mem_total_bytes, 4194304 * 1024);
        assert_eq!(parsed.mem_free_bytes, 1024 * 1024);
    }

    #[test]
    fn meminfo_mem_available_falls_back_to_free_buffers_cached() {
        let content =
            "MemTotal: 8388608 kB\nMemFree: 1048576 kB\nBuffers: 262144 kB\nCached: 2097152 kB\n";
        let parsed = parse_meminfo(content).unwrap();
        assert_eq!(
            parsed.mem_available_bytes,
            (1048576 + 262144 + 2097152) * 1024
        );
    }

    #[test]
    fn meminfo_atypical_old_kernel_without_mem_available_parses() {
        // Kernel 3.x era fixture (no MemAvailable): many unrelated state keys,
        // CRLF line endings, tabs, and swap present.
        let content = concat!(
            "MemTotal:        1938952 kB\r\n",
            "MemFree:          184740 kB\r\n",
            "Buffers:           10240 kB\r\n",
            "Cached:           919412 kB\r\n",
            "SwapCached:            0 kB\r\n",
            "Active:           927748 kB\r\n",
            "Inactive:         393640 kB\r\n",
            "HighTotal:             0 kB\r\n",
            "LowTotal:        1938952 kB\r\n",
            "SwapTotal:        102396 kB\r\n",
            "SwapFree:\t\t 102396 kB\r\n",
            "Dirty:               116 kB\r\n",
            "Writeback:             0 kB\r\n",
            "AnonPages:        415084 kB\r\n",
            "Mapped:            80972 kB\r\n",
            "Shmem:              8408 kB\r\n",
            "Slab:              71704 kB\r\n",
            "SReclaimable:      55748 kB\r\n",
            "SUnreclaim:        15956 kB\r\n",
            "Hugepagesize:       2048 kB\r\n",
        );
        let parsed = parse_meminfo(content).expect("atypical meminfo fixture must parse");
        assert_eq!(parsed.mem_total_bytes, 1_938_952 * 1024);
        assert_eq!(
            parsed.mem_available_bytes,
            (184_740 + 10_240 + 919_412) * 1024,
            "absent MemAvailable must use the free+buffers+cached heuristic"
        );
        assert_eq!(parsed.used_ram_bytes(), (1_938_952 - 1_114_392) * 1024);
        assert_eq!(parsed.swap_total_bytes, 102_396 * 1024);
        assert_eq!(parsed.swap_free_bytes, 102_396 * 1024);
        assert_eq!(parsed.used_swap_bytes(), 0);
        assert_eq!(parsed.source, MeminfoSource::Proc);
    }

    #[test]
    fn meminfo_mem_available_fallback_overflow_is_explicit_error() {
        let content = "MemTotal: 1\nMemFree: 18446744073709551615\nBuffers: 18446744073709551615\nCached: 1\n";
        let err =
            parse_meminfo(content).expect_err("fallback overflow must fail, not wrap or panic");
        assert!(err.contains("overflows"), "got: {err}");
    }

    #[test]
    fn meminfo_optional_keys_absent_read_as_zero() {
        let parsed = parse_meminfo("MemTotal: 4096 kB\n").unwrap();
        assert_eq!(parsed.mem_free_bytes, 0);
        assert_eq!(parsed.swap_total_bytes, 0);
        assert_eq!(parsed.mem_available_bytes, 0);
    }

    #[test]
    fn loadavg_typical_fixture_parses_all_fields() {
        let load = parse_loadavg("0.42 0.31 0.25 2/534 12345\n").expect("typical loadavg");
        assert_eq!(load.load_1min, 0.42);
        assert_eq!(load.load_5min, 0.31);
        assert_eq!(load.load_15min, 0.25);
        assert_eq!(load.runnable_processes, Some(2));
        assert_eq!(load.total_processes, Some(534));
        assert_eq!(load.last_pid, Some(12345));
    }

    #[test]
    fn loadavg_handles_crlf_extra_spaces_and_leading_blank_lines() {
        let load = parse_loadavg("\r\n   1.50   1.25   0.75   3/201   9999  \r\n").unwrap();
        assert_eq!(load.load_1min, 1.5);
        assert_eq!(load.load_15min, 0.75);
        assert_eq!(load.total_processes, Some(201));
    }

    #[test]
    fn loadavg_accepts_three_fields_without_process_counts() {
        let load = parse_loadavg("0.10 0.20 0.30\n").unwrap();
        assert_eq!(load.runnable_processes, None);
        assert_eq!(load.total_processes, None);
        assert_eq!(load.last_pid, None);
    }

    #[test]
    fn loadavg_empty_or_short_content_is_explicit_error() {
        assert!(parse_loadavg("").is_err());
        assert!(parse_loadavg("   \n").is_err());
        let err = parse_loadavg("0.10 0.20\n").expect_err("two fields must fail");
        assert!(err.contains("at least 3"), "got: {err}");
    }

    #[test]
    fn loadavg_non_numeric_load_is_explicit_error() {
        let err = parse_loadavg("many 0.20 0.30 1/2 3\n").expect_err("non-numeric load");
        assert!(err.contains("load_1min"), "got: {err}");
    }

    #[test]
    fn loadavg_malformed_process_field_is_explicit_error() {
        let err = parse_loadavg("0.1 0.2 0.3 abc 4\n").expect_err("run/total malformed");
        assert!(err.contains("run/total"), "got: {err}");
        let err = parse_loadavg("0.1 0.2 0.3 1/x 4\n").expect_err("non-numeric total");
        assert!(err.contains("total count"), "got: {err}");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn loadavg_reader_off_linux_is_explicit_error() {
        let err = try_read_host_loadavg().expect_err("non-Linux host has no /proc/loadavg");
        assert!(err.contains("Linux"), "got: {err}");
    }

    #[test]
    fn power_supply_valid_reading_parses() {
        let power =
            parse_power_supply_fields("BAT0", "Charging", "88", "Battery", Some("1")).unwrap();
        assert_eq!(power.capacity_pct, 88.0);
        assert_eq!(power.status, "Charging");
        assert_eq!(power.supply_type, "Battery");
        assert!(power.online);
    }

    #[test]
    fn power_supply_infers_online_from_charging_status() {
        let charging =
            parse_power_supply_fields("BAT0", "Charging", "50", "Battery", None).unwrap();
        assert!(charging.online);
        let full = parse_power_supply_fields("BAT0", "Full", "100", "Battery", None).unwrap();
        assert!(full.online);
        let discharging =
            parse_power_supply_fields("BAT0", "Discharging", "50", "Battery", None).unwrap();
        assert!(!discharging.online);
    }

    #[test]
    fn power_supply_invalid_capacity_is_explicit_error() {
        let err = parse_power_supply_fields("BAT0", "Charging", "Unknown", "Battery", None)
            .expect_err("non-numeric capacity must fail");
        assert!(err.contains("capacity"), "got: {err}");
        let err = parse_power_supply_fields("BAT0", "Charging", "150", "Battery", None)
            .expect_err("out-of-range capacity must fail");
        assert!(err.contains("0..=100"), "got: {err}");
    }

    #[test]
    fn power_supply_invalid_online_flag_is_explicit_error() {
        let err = parse_power_supply_fields("AC", "Unknown", "100", "Mains", Some("yes"))
            .expect_err("online must be 0 or 1");
        assert!(err.contains("neither 0 nor 1"), "got: {err}");
    }

    #[test]
    fn thermal_zone_valid_readings_and_threshold() {
        let normal = parse_thermal_zone("48500", "x86_pkg_temp", "thermal_zone0").unwrap();
        assert_eq!(normal.temp_celsius, 48.5);
        assert!(!normal.is_throttled);
        let hot = parse_thermal_zone("92000", "acpitz", "thermal_zone1").unwrap();
        assert_eq!(hot.temp_celsius, 92.0);
        assert!(hot.is_throttled);
        let below = parse_thermal_zone("84999", "acpitz", "thermal_zone2").unwrap();
        assert!(!below.is_throttled);
        let at = parse_thermal_zone("85000", "acpitz", "thermal_zone3").unwrap();
        assert!(at.is_throttled);
    }

    #[test]
    fn thermal_zone_invalid_temperature_is_explicit_error() {
        let err = parse_thermal_zone("not-a-temp", "acpitz", "thermal_zone0")
            .expect_err("non-numeric temp must fail");
        assert!(err.contains("not a number"), "got: {err}");
        let err = parse_thermal_zone("NaN", "acpitz", "thermal_zone0")
            .expect_err("non-finite temp must fail");
        assert!(err.contains("non-finite"), "got: {err}");
        let err = parse_thermal_zone("500000", "acpitz", "thermal_zone0")
            .expect_err("implausible temp must fail");
        assert!(err.contains("plausible"), "got: {err}");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn meminfo_reader_off_linux_is_tagged_simulated() {
        let data = read_host_meminfo();
        assert!(
            data.is_simulated(),
            "non-Linux fallback must be tagged Simulated"
        );
        assert_eq!(data.source, MeminfoSource::Simulated);
        assert!(data.mem_total_bytes > 0);
        assert!(data.mem_available_bytes > 0);
        assert!(try_read_host_meminfo().is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn meminfo_reader_on_linux_is_real() {
        let data = read_host_meminfo();
        assert_eq!(data.source, MeminfoSource::Proc);
        assert!(!data.is_simulated());
        assert!(data.mem_total_bytes > 0);
        assert!(try_read_host_meminfo().is_ok());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn power_reader_off_linux_is_explicitly_documented_fallback() {
        assert!(try_read_host_power_state().is_err());
        let power = read_host_power_state();
        assert_eq!(power.battery_percent, 100.0);
        assert!(!power.on_battery);
        assert!(power.charging);
    }

    #[test]
    fn meminfo_simulated_source_serde_default_is_conservative() {
        let json = r#"{"mem_total_bytes":1024,"mem_free_bytes":0,"mem_available_bytes":0,"buffers_bytes":0,"cached_bytes":0,"swap_total_bytes":0,"swap_free_bytes":0}"#;
        let data: MeminfoData = serde_json::from_str(json).unwrap();
        assert_eq!(data.source, MeminfoSource::Simulated);
        assert!(data.is_simulated());
    }
}
