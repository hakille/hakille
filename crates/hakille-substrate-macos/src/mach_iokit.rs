use hakille_substrate::{PowerState, ThermalState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const PAGE_SIZE_APPLE_SILICON: u64 = 16 * 1024; // 16 KiB
pub const PAGE_SIZE_INTEL: u64 = 4 * 1024; // 4 KiB

pub const HOST_VM_INFO64: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct VmStatistics64 {
    pub free_count: u32,
    pub active_count: u32,
    pub inactive_count: u32,
    pub wire_count: u32,
    pub zero_fill_count: u64,
    pub reactivations: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub faults: u64,
    pub cow_faults: u64,
    pub lookups: u64,
    pub hits: u64,
    pub purges: u64,
    pub purgeable_count: u32,
    pub speculative_count: u32,
    pub decompressions: u64,
    pub compressions: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub compressor_page_count: u32,
    pub throttled_count: u32,
    pub external_page_count: u32,
    pub internal_page_count: u32,
    pub total_uncompressed_pages_in_compressor: u64,
}

impl Default for VmStatistics64 {
    fn default() -> Self {
        // SIMULATED fixture profile (high-end Apple Silicon M-series page
        // counts). This is NOT a host reading; real telemetry comes from
        // `vm_stat(1)` under macOS. See `simulated_vm_statistics_fallback`.
        Self {
            free_count: 524_288,     // 8 GB free (at 16 KiB page)
            active_count: 1_048_576, // 16 GB active
            inactive_count: 524_288, // 8 GB inactive
            wire_count: 524_288,     // 8 GB wired (macOS kernel + graphics)
            zero_fill_count: 10_000_000,
            reactivations: 100_000,
            pageins: 50_000,
            pageouts: 0,
            faults: 500_000,
            cow_faults: 200_000,
            lookups: 1_000_000,
            hits: 950_000,
            purges: 10_000,
            purgeable_count: 100_000,
            speculative_count: 262_144, // 4 GB speculative
            decompressions: 0,
            compressions: 0,
            swapins: 0,
            swapouts: 0,
            compressor_page_count: 0,
            throttled_count: 0,
            external_page_count: 100_000,
            internal_page_count: 1_500_000,
            total_uncompressed_pages_in_compressor: 0,
        }
    }
}

impl VmStatistics64 {
    /// All counters zero — "unknown", never an invented profile.
    pub fn unknown() -> Self {
        Self {
            free_count: 0,
            active_count: 0,
            inactive_count: 0,
            wire_count: 0,
            zero_fill_count: 0,
            reactivations: 0,
            pageins: 0,
            pageouts: 0,
            faults: 0,
            cow_faults: 0,
            lookups: 0,
            hits: 0,
            purges: 0,
            purgeable_count: 0,
            speculative_count: 0,
            decompressions: 0,
            compressions: 0,
            swapins: 0,
            swapouts: 0,
            compressor_page_count: 0,
            throttled_count: 0,
            external_page_count: 0,
            internal_page_count: 0,
            total_uncompressed_pages_in_compressor: 0,
        }
    }

    pub fn total_physical_ram(&self, page_size: u64) -> u64 {
        let pages = (self.free_count as u64)
            + (self.active_count as u64)
            + (self.inactive_count as u64)
            + (self.wire_count as u64)
            + (self.speculative_count as u64)
            + (self.compressor_page_count as u64);
        pages * page_size
    }

    pub fn available_physical_ram(&self, page_size: u64) -> u64 {
        let pages = (self.free_count as u64)
            + (self.inactive_count as u64)
            + (self.speculative_count as u64);
        pages * page_size
    }

    pub fn wired_ram(&self, page_size: u64) -> u64 {
        (self.wire_count as u64) * page_size
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IOPMPowerSourceInfo {
    pub is_present: bool,
    pub is_charging: bool,
    pub state: String, // "AC Power", "Battery Power"
    pub current_capacity: i32,
    pub max_capacity: i32,
    pub time_to_empty_min: Option<i32>,
}

/// Parses an IOKit `IOPMPowerSource` property dictionary into a typed
/// snapshot.
///
/// Pure function. Required keys: `Power Source State` (one of the two
/// documented values, `"AC Power"` / `"Battery Power"`), `Is Charging`
/// (`true`/`false`/`1`/`0`), and the integer percentages `Current Capacity`
/// / `Max Capacity` (0..=100, non-zero max, current ≤ max). `Time to Empty`
/// is optional (absent on AC power) but must be a non-negative integer when
/// present. A missing or malformed field is an explicit error — never a
/// silently invented default.
pub fn parse_iops_dictionary(
    dict: &HashMap<String, String>,
) -> Result<IOPMPowerSourceInfo, String> {
    let state = required_iops_string(dict, "Power Source State")?;
    if state != "AC Power" && state != "Battery Power" {
        return Err(format!(
            "IOPMPowerSource: unknown \"Power Source State\" value \"{state}\" \
             (expected \"AC Power\" or \"Battery Power\")"
        ));
    }
    let charging_flag = required_iops_string(dict, "Is Charging")?.to_ascii_lowercase();
    let is_charging = match charging_flag.as_str() {
        "true" | "1" => true,
        "false" | "0" => false,
        other => {
            return Err(format!(
                "IOPMPowerSource: \"Is Charging\" value \"{other}\" is neither \
                 true/false nor 1/0"
            ));
        }
    };
    let current_capacity = required_iops_capacity(dict, "Current Capacity")?;
    let max_capacity = required_iops_capacity(dict, "Max Capacity")?;
    if max_capacity == 0 {
        return Err("IOPMPowerSource: \"Max Capacity\" is 0".to_string());
    }
    if current_capacity > max_capacity {
        return Err(format!(
            "IOPMPowerSource: \"Current Capacity\" {current_capacity} exceeds \
             \"Max Capacity\" {max_capacity}"
        ));
    }
    let time_to_empty_min = match dict.get("Time to Empty") {
        Some(raw) => {
            let minutes = raw.trim().parse::<i32>().map_err(|_| {
                format!("IOPMPowerSource: \"Time to Empty\" value \"{raw}\" is not an integer")
            })?;
            if minutes < 0 {
                return Err(format!(
                    "IOPMPowerSource: \"Time to Empty\" value {minutes} is negative"
                ));
            }
            Some(minutes)
        }
        None => None,
    };

    Ok(IOPMPowerSourceInfo {
        is_present: true,
        is_charging,
        state: state.to_string(),
        current_capacity,
        max_capacity,
        time_to_empty_min,
    })
}

/// Required, non-empty, trimmed string property.
fn required_iops_string<'a>(
    dict: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str, String> {
    match dict.get(key) {
        Some(value) if !value.trim().is_empty() => Ok(value.trim()),
        Some(_) => Err(format!("IOPMPowerSource: \"{key}\" present but empty")),
        None => Err(format!("IOPMPowerSource: missing \"{key}\"")),
    }
}

/// Required integer percentage property, explicit range check.
fn required_iops_capacity(dict: &HashMap<String, String>, key: &str) -> Result<i32, String> {
    let raw = required_iops_string(dict, key)?;
    let value = raw
        .parse::<i32>()
        .map_err(|_| format!("IOPMPowerSource: \"{key}\" value \"{raw}\" is not an integer"))?;
    if !(0..=100).contains(&value) {
        return Err(format!(
            "IOPMPowerSource: \"{key}\" value {value} outside 0..=100"
        ));
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OSThermalPressureLevel {
    Nominal = 0,
    Moderate = 1,
    Heavy = 2,
    Trapping = 3,
    Sleeping = 4,
}

impl OSThermalPressureLevel {
    pub fn to_thermal_state(&self) -> ThermalState {
        match self {
            Self::Nominal => ThermalState {
                temperature_celsius: 38.5,
                throttled: false,
            },
            Self::Moderate => ThermalState {
                temperature_celsius: 65.0,
                throttled: false,
            },
            Self::Heavy => ThermalState {
                temperature_celsius: 88.0,
                throttled: true,
            },
            Self::Trapping => ThermalState {
                temperature_celsius: 98.0,
                throttled: true,
            },
            Self::Sleeping => ThermalState {
                temperature_celsius: 102.0,
                throttled: true,
            },
        }
    }
}

/// Off-macOS fallback profile: SIMULATED Apple-Silicon page counts.
///
/// This is **not** a reading of the host. It only exists so that non-macOS
/// builds (cross-compiled tests, conformance fixtures) keep a structurally
/// valid [`VmStatistics64`]; every real host reading on macOS goes through
/// `vm_stat(1)` instead. Never present it as production telemetry.
pub fn simulated_vm_statistics_fallback() -> VmStatistics64 {
    VmStatistics64::default()
}

/// Parses `vm_stat(1)` output into a [`VmStatistics64`] page-count snapshot.
///
/// Pure function; accepts the historical and modern label sets. The
/// page-size header and the four core counters (`Pages free`, `Pages
/// active`, `Pages inactive`, `Pages wired down`) are required: a missing
/// core counter or a non-numeric value for a known counter is an explicit
/// error. Labels unknown to this parser are ignored, optional counters that
/// are absent stay 0. Counts are raw pages — scale by the header page size
/// for bytes.
pub fn parse_vm_stat_output(output: &str) -> Result<VmStatistics64, String> {
    const KNOWN_LABELS: &[&str] = &[
        "Pages free",
        "Pages active",
        "Pages inactive",
        "Pages speculative",
        "Pages throttled",
        "Pages wired down",
        "Pages purgeable",
        "Pages purged",
        "Translation faults",
        "Pages copy-on-write",
        "Pages zero filled",
        "Pages reactivated",
        "Pageins",
        "Pageouts",
        "Swapins",
        "Swapouts",
        "Pages occupied by compressor",
        "Pages used by VM compressor",
        "File-backed pages",
        "Anonymous pages",
        "Pages decompressed",
        "Pages compressed",
        "Uncompressed pages in compressor",
        "Uncompressed pages",
    ];

    let mut counters: HashMap<&str, u64> = HashMap::new();
    let mut header_seen = false;
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("Mach Virtual Memory Statistics") {
            if !line.contains("page size of") {
                return Err(format!("vm_stat: page-size header malformed: \"{line}\""));
            }
            header_seen = true;
            continue;
        }
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim().trim_matches('"');
        let value = value.trim().trim_end_matches('.');
        match value.parse::<u64>() {
            Ok(parsed) => {
                counters.insert(label, parsed);
            }
            Err(_) if !KNOWN_LABELS.contains(&label) => {}
            Err(_) => {
                return Err(format!(
                    "vm_stat: non-numeric value \"{value}\" for \"{label}\""
                ));
            }
        }
    }
    if !header_seen {
        return Err("vm_stat: missing page-size header".to_string());
    }

    let lookup = |labels: &[&str]| labels.iter().find_map(|label| counters.get(label).copied());
    let required = |name: &str, labels: &[&str]| {
        lookup(labels).ok_or_else(|| format!("vm_stat: missing required counter \"{name}\""))
    };
    let required_u32 = |name: &str, labels: &[&str]| -> Result<u32, String> {
        let value = required(name, labels)?;
        u32::try_from(value)
            .map_err(|_| format!("vm_stat: counter \"{name}\" value {value} exceeds u32 range"))
    };
    let optional_u32 = |name: &str, labels: &[&str]| -> Result<u32, String> {
        match lookup(labels) {
            Some(value) => u32::try_from(value).map_err(|_| {
                format!("vm_stat: counter \"{name}\" value {value} exceeds u32 range")
            }),
            None => Ok(0),
        }
    };

    Ok(VmStatistics64 {
        free_count: required_u32("Pages free", &["Pages free"])?,
        active_count: required_u32("Pages active", &["Pages active"])?,
        inactive_count: required_u32("Pages inactive", &["Pages inactive"])?,
        wire_count: required_u32("Pages wired down", &["Pages wired down"])?,
        zero_fill_count: lookup(&["Pages zero filled"]).unwrap_or(0),
        reactivations: lookup(&["Pages reactivated"]).unwrap_or(0),
        pageins: lookup(&["Pageins"]).unwrap_or(0),
        pageouts: lookup(&["Pageouts"]).unwrap_or(0),
        faults: lookup(&["Translation faults"]).unwrap_or(0),
        cow_faults: lookup(&["Pages copy-on-write"]).unwrap_or(0),
        lookups: 0,
        hits: 0,
        purges: lookup(&["Pages purged"]).unwrap_or(0),
        purgeable_count: optional_u32("Pages purgeable", &["Pages purgeable"])?,
        speculative_count: optional_u32("Pages speculative", &["Pages speculative"])?,
        decompressions: lookup(&["Pages decompressed"]).unwrap_or(0),
        compressions: lookup(&["Pages compressed"]).unwrap_or(0),
        swapins: lookup(&["Swapins"]).unwrap_or(0),
        swapouts: lookup(&["Swapouts"]).unwrap_or(0),
        compressor_page_count: optional_u32(
            "Pages occupied by compressor",
            &[
                "Pages occupied by compressor",
                "Pages used by VM compressor",
            ],
        )?,
        throttled_count: optional_u32("Pages throttled", &["Pages throttled"])?,
        external_page_count: optional_u32("File-backed pages", &["File-backed pages"])?,
        internal_page_count: optional_u32("Anonymous pages", &["Anonymous pages"])?,
        total_uncompressed_pages_in_compressor: lookup(&[
            "Uncompressed pages in compressor",
            "Uncompressed pages",
        ])
        .unwrap_or(0),
    })
}

/// Real VM statistics via `vm_stat(1)` (macOS only).
#[cfg(target_os = "macos")]
pub fn query_vm_statistics() -> Result<VmStatistics64, String> {
    parse_vm_stat_output(&crate::host_query::run_host_command("vm_stat", &[])?)
}

/// Off macOS there is no `vm_stat`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_vm_statistics() -> Result<VmStatistics64, String> {
    Err(crate::host_query::macos_only_error("vm_stat"))
}

/// Live VM page statistics.
///
/// macOS: real `vm_stat(1)` snapshot; on failure all counters stay 0
/// (= unknown, logged) — never a fabricated profile. Off macOS: explicitly
/// simulated fallback (see [`simulated_vm_statistics_fallback`]), not a
/// reading of the host.
#[cfg(target_os = "macos")]
pub fn read_host_vm_statistics() -> VmStatistics64 {
    match query_vm_statistics() {
        Ok(stats) => stats,
        Err(err) => {
            tracing::warn!("vm_stat query failed: {err}");
            VmStatistics64::unknown()
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_host_vm_statistics() -> VmStatistics64 {
    simulated_vm_statistics_fallback()
}

/// Total physical RAM in bytes.
///
/// macOS: real `sysctl hw.memsize`; if the query fails, 0 (= unknown) with a
/// warning — never a fabricated size. Off macOS: explicitly simulated
/// fallback so non-macOS builds keep a structurally valid manifest.
#[cfg(target_os = "macos")]
pub fn read_total_physical_ram_bytes() -> u64 {
    match crate::host_query::query_hw_memsize() {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("hw.memsize query failed: {err}");
            0
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_total_physical_ram_bytes() -> u64 {
    simulated_vm_statistics_fallback().total_physical_ram(PAGE_SIZE_APPLE_SILICON)
}

/// VM page size in bytes.
///
/// macOS: real `sysctl hw.pagesize`; on failure 0 (= unknown), never a guess
/// (a wrong page size would silently rescale every memory figure). Off
/// macOS: the simulated Apple-Silicon page size matching
/// [`simulated_vm_statistics_fallback`] — explicitly labelled, not a host
/// reading.
#[cfg(target_os = "macos")]
pub fn read_host_page_size_bytes() -> u64 {
    match crate::host_query::query_hw_pagesize() {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("hw.pagesize query failed: {err}");
            0
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_host_page_size_bytes() -> u64 {
    PAGE_SIZE_APPLE_SILICON
}

/// Honest battery/power reader.
///
/// macOS: this crate links no IOKit `IOPMPowerSource` binding (it performs
/// no Mach/IOKit calls), so the live battery reading is unavailable — an
/// explicit error, never a fabricated "AC power" reading. Off macOS: an
/// explicit macOS-only error.
pub fn try_read_host_power_state() -> Result<PowerState, String> {
    #[cfg(target_os = "macos")]
    {
        Err(
            "IOPMPowerSource: IOKit binding missing (no IOServiceGetMatchingServices \
             wiring; link IOKit bindings to read the real battery state)"
                .to_string(),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(crate::host_query::macos_only_error("IOKit IOPMPowerSource"))
    }
}

/// Total wrapper kept for the `Substrate` trait (which cannot return an
/// error). When no real reading exists it falls back to the workspace-wide
/// "desktop on AC" assumption and logs a warning — `PowerState` has no
/// unknown variant. Call [`try_read_host_power_state`] for the honest error.
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

pub fn read_host_thermal_state() -> ThermalState {
    // Honest: no IOKit / NSProcessInfo thermal-pressure binding is linked
    // (this crate performs no Mach/IORegistry calls), so the live pressure
    // level is unknowable here. Report Unknown explicitly rather than
    // claiming Nominal. The `OSThermalPressureLevel → ThermalState` mapping
    // above stays available for when a real pressure source is wired.
    ThermalState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Output sample in the modern Apple Silicon format (16 KiB pages).
    const VM_STAT_MODERN: &str = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes, pages thr: 0)
Pages free:                                   29796.
Pages active:                                795939.
Pages inactive:                              753243.
Pages speculative:                            43721.
Pages throttled:                                  0.
Pages wired down:                            187678.
Pages purgeable:                               1061.
\"Translation faults\":                    1831832153.
Pages copy-on-write:                     151412533.
Pages zero filled:                       565423546.
Pages reactivated:                       115976109.
Pageins:                                  22405220.
Pageouts:                                        0.
Swapins:                                         0.
Swapouts:                                        0.
Pages occupied by compressor:               1243874.
";

    /// Historical output sample (4 KiB pages, Apple documentation).
    const VM_STAT_LEGACY: &str = "\
Mach Virtual Memory Statistics: (page size of 4096 bytes)
Pages free:                     3194.
Pages active:                  34594.
Pages inactive:                17870.
Pages wired down:               9878.
\"Translation faults\":        6333197.
Pages copy-on-write:           81385.
Pages zero filled:           3180051.
Pages reactivated:            343961.
";

    #[test]
    fn parse_vm_stat_modern_fixture() {
        let vm = parse_vm_stat_output(VM_STAT_MODERN).unwrap();
        assert_eq!(vm.free_count, 29_796);
        assert_eq!(vm.active_count, 795_939);
        assert_eq!(vm.inactive_count, 753_243);
        assert_eq!(vm.wire_count, 187_678);
        assert_eq!(vm.speculative_count, 43_721);
        assert_eq!(vm.throttled_count, 0);
        assert_eq!(vm.purgeable_count, 1_061);
        assert_eq!(vm.compressor_page_count, 1_243_874);
        assert_eq!(vm.faults, 1_831_832_153);
        assert_eq!(vm.cow_faults, 151_412_533);
        assert_eq!(vm.zero_fill_count, 565_423_546);
        assert_eq!(vm.reactivations, 115_976_109);
        assert_eq!(vm.pageins, 22_405_220);
        assert_eq!(vm.pageouts, 0);
        assert_eq!(vm.swapins, 0);
        assert_eq!(vm.swapouts, 0);
        // Sanity of the derived byte figures at the real page size.
        let total_pages = vm.free_count as u64
            + vm.active_count as u64
            + vm.inactive_count as u64
            + vm.wire_count as u64
            + vm.speculative_count as u64
            + vm.compressor_page_count as u64;
        assert_eq!(vm.total_physical_ram(16_384), total_pages * 16_384);
        assert!(vm.available_physical_ram(16_384) < vm.total_physical_ram(16_384));
    }

    #[test]
    fn parse_vm_stat_legacy_fixture() {
        let vm = parse_vm_stat_output(VM_STAT_LEGACY).unwrap();
        assert_eq!(vm.free_count, 3_194);
        assert_eq!(vm.active_count, 34_594);
        assert_eq!(vm.inactive_count, 17_870);
        assert_eq!(vm.wire_count, 9_878);
        assert_eq!(vm.speculative_count, 0, "absent optional counter stays 0");
        assert_eq!(vm.faults, 6_333_197);
        assert_eq!(vm.cow_faults, 81_385);
        assert_eq!(vm.zero_fill_count, 3_180_051);
        assert_eq!(vm.reactivations, 343_961);
    }

    #[test]
    fn parse_vm_stat_rejects_missing_header() {
        let output = VM_STAT_MODERN
            .lines()
            .filter(|line| !line.starts_with("Mach Virtual Memory"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_vm_stat_output(&output).unwrap_err();
        assert!(err.contains("header"), "got: {err}");
    }

    #[test]
    fn parse_vm_stat_rejects_missing_core_counter() {
        let output = VM_STAT_MODERN.replace("Pages inactive:", "Pages unused:");
        let err = parse_vm_stat_output(&output).unwrap_err();
        assert!(err.contains("Pages inactive"), "got: {err}");
    }

    #[test]
    fn parse_vm_stat_rejects_non_numeric_known_counter() {
        let output = VM_STAT_MODERN.replace(
            "Pages free:                                   29796.",
            "Pages free: unknown.",
        );
        let err = parse_vm_stat_output(&output).unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
        assert!(err.contains("Pages free"), "got: {err}");
    }

    #[test]
    fn parse_vm_stat_ignores_unknown_labels() {
        let mut output = VM_STAT_MODERN.to_string();
        output.push_str("Pages future counter: 42.\nNew magic line without colon\n");
        let vm = parse_vm_stat_output(&output).unwrap();
        assert_eq!(vm.free_count, 29_796);
    }

    #[test]
    fn unknown_vm_statistics_are_zeroed() {
        let vm = VmStatistics64::unknown();
        assert_eq!(vm.total_physical_ram(16_384), 0);
        assert_eq!(vm.available_physical_ram(16_384), 0);
        assert_eq!(vm.wired_ram(16_384), 0);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn off_macos_vm_statistics_is_labeled_simulation() {
        assert_eq!(
            read_host_vm_statistics(),
            simulated_vm_statistics_fallback(),
            "off macOS the snapshot must be the explicitly simulated profile"
        );
        assert_eq!(read_host_page_size_bytes(), PAGE_SIZE_APPLE_SILICON);
        assert_eq!(
            read_total_physical_ram_bytes(),
            simulated_vm_statistics_fallback().total_physical_ram(PAGE_SIZE_APPLE_SILICON)
        );
        let err = query_vm_statistics().unwrap_err();
        assert!(err.contains("macOS-only"), "got: {err}");
    }

    fn iops_dict(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn iops_ac_fixture() -> HashMap<String, String> {
        iops_dict(&[
            ("Power Source State", "AC Power"),
            ("Is Charging", "true"),
            ("Current Capacity", "100"),
            ("Max Capacity", "100"),
        ])
    }

    #[test]
    fn parse_iops_dictionary_ac_power_fixture() {
        let info = parse_iops_dictionary(&iops_ac_fixture()).unwrap();
        assert!(info.is_present);
        assert_eq!(info.state, "AC Power");
        assert!(info.is_charging);
        assert_eq!(info.current_capacity, 100);
        assert_eq!(info.max_capacity, 100);
        assert_eq!(info.time_to_empty_min, None, "absent under AC power");
    }

    #[test]
    fn parse_iops_dictionary_battery_fixture_with_time_to_empty() {
        let dict = iops_dict(&[
            ("Power Source State", "Battery Power"),
            ("Is Charging", "false"),
            ("Current Capacity", "75"),
            ("Max Capacity", "100"),
            ("Time to Empty", "240"),
        ]);
        let info = parse_iops_dictionary(&dict).unwrap();
        assert_eq!(info.state, "Battery Power");
        assert!(!info.is_charging);
        assert_eq!(info.current_capacity, 75);
        assert_eq!(info.time_to_empty_min, Some(240));
    }

    #[test]
    fn parse_iops_dictionary_accepts_whitespace_and_boolean_variants() {
        let dict = iops_dict(&[
            ("Power Source State", "  Battery Power  "),
            ("Is Charging", "0"),
            ("Current Capacity", " 99 "),
            ("Max Capacity", "100"),
        ]);
        let info = parse_iops_dictionary(&dict).unwrap();
        assert_eq!(info.state, "Battery Power");
        assert!(!info.is_charging);
        assert_eq!(info.current_capacity, 99);
    }

    #[test]
    fn parse_iops_dictionary_rejects_each_missing_required_key() {
        for key in [
            "Power Source State",
            "Is Charging",
            "Current Capacity",
            "Max Capacity",
        ] {
            let mut dict = iops_ac_fixture();
            dict.remove(key);
            let err = parse_iops_dictionary(&dict).unwrap_err();
            assert!(err.contains(key), "error must name \"{key}\", got: {err}");
        }
    }

    #[test]
    fn parse_iops_dictionary_rejects_malformed_values() {
        let cases: [(&str, &str, &str); 5] = [
            ("Power Source State", "Battery", "Power Source State"),
            ("Is Charging", "maybe", "Is Charging"),
            ("Current Capacity", "seventy-five", "Current Capacity"),
            ("Max Capacity", "101", "Max Capacity"),
            ("Time to Empty", "-5", "Time to Empty"),
        ];
        for (key, value, expected) in cases {
            let mut dict = iops_ac_fixture();
            dict.insert(key.to_string(), value.to_string());
            let err = parse_iops_dictionary(&dict).unwrap_err();
            assert!(
                err.contains(expected),
                "error for {key}=\"{value}\" must name \"{expected}\", got: {err}"
            );
        }
    }

    #[test]
    fn parse_iops_dictionary_rejects_empty_field() {
        let mut dict = iops_ac_fixture();
        dict.insert("Current Capacity".to_string(), "   ".to_string());
        let err = parse_iops_dictionary(&dict).unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn parse_iops_dictionary_rejects_current_above_max() {
        let mut dict = iops_ac_fixture();
        dict.insert("Current Capacity".to_string(), "80".to_string());
        dict.insert("Max Capacity".to_string(), "50".to_string());
        let err = parse_iops_dictionary(&dict).unwrap_err();
        assert!(err.contains("exceeds"), "got: {err}");
    }

    #[test]
    fn try_read_host_power_state_is_an_explicit_failure_here() {
        let err = try_read_host_power_state().unwrap_err();
        assert!(!err.is_empty(), "the honest failure must carry a reason");
        // `PowerState` has no unknown variant: the trait wrapper keeps the
        // labeled "desktop on AC" simulation (logged) instead of panicking.
        let power = read_host_power_state();
        assert!(!power.on_battery);
        assert_eq!(power.battery_percent, 100.0);
        assert!(power.charging);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn try_read_host_power_state_off_macos_names_the_platform() {
        let err = try_read_host_power_state().unwrap_err();
        assert!(err.contains("macOS-only"), "got: {err}");
    }
}
