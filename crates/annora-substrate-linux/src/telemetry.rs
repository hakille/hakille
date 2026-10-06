use annora_substrate::{PowerState, ThermalState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeminfoData {
    pub mem_total_bytes: u64,
    pub mem_free_bytes: u64,
    pub mem_available_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
}

impl MeminfoData {
    pub fn used_ram_bytes(&self) -> u64 {
        self.mem_total_bytes.saturating_sub(self.mem_available_bytes)
    }

    pub fn used_swap_bytes(&self) -> u64 {
        self.swap_total_bytes.saturating_sub(self.swap_free_bytes)
    }
}

pub fn parse_meminfo(content: &str) -> Result<MeminfoData, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, val_str)) = line.split_once(':') {
            let key = key.trim();
            let mut val_tokens = val_str.trim().split_whitespace();
            if let Some(val_num_str) = val_tokens.next() {
                if let Ok(num) = val_num_str.parse::<u64>() {
                    // Check if unit is kB
                    let unit = val_tokens.next().unwrap_or("");
                    let bytes = if unit.eq_ignore_ascii_case("kb") {
                        num * 1024
                    } else if unit.eq_ignore_ascii_case("mb") {
                        num * 1024 * 1024
                    } else if unit.eq_ignore_ascii_case("gb") {
                        num * 1024 * 1024 * 1024
                    } else {
                        num
                    };
                    map.insert(key.to_string(), bytes);
                }
            }
        }
    }

    let mem_total_bytes = *map.get("MemTotal").ok_or("MemTotal not found in /proc/meminfo")?;
    let mem_free_bytes = *map.get("MemFree").unwrap_or(&0);
    let buffers_bytes = *map.get("Buffers").unwrap_or(&0);
    let cached_bytes = *map.get("Cached").unwrap_or(&0);
    let fallback_available = mem_free_bytes + buffers_bytes + cached_bytes;
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
    })
}

pub fn read_host_meminfo() -> MeminfoData {
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        if let Ok(data) = parse_meminfo(&content) {
            return data;
        }
    }
    // High-performance default when /proc/meminfo is unavailable (e.g. non-Linux host or test env)
    MeminfoData {
        mem_total_bytes: 64 * 1024 * 1024 * 1024,
        mem_free_bytes: 32 * 1024 * 1024 * 1024,
        mem_available_bytes: 48 * 1024 * 1024 * 1024,
        buffers_bytes: 1024 * 1024 * 1024,
        cached_bytes: 15 * 1024 * 1024 * 1024,
        swap_total_bytes: 8 * 1024 * 1024 * 1024,
        swap_free_bytes: 8 * 1024 * 1024 * 1024,
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

pub fn parse_power_supply_fields(
    name: &str,
    status: &str,
    capacity: &str,
    supply_type: &str,
    online: Option<&str>,
) -> LinuxPowerSupplyData {
    let capacity_pct = capacity.trim().parse::<f32>().unwrap_or(100.0);
    let is_online = online
        .map(|s| s.trim() == "1")
        .unwrap_or_else(|| status.trim().eq_ignore_ascii_case("Charging") || status.trim().eq_ignore_ascii_case("Full"));

    LinuxPowerSupplyData {
        name: name.to_string(),
        supply_type: supply_type.trim().to_string(),
        status: status.trim().to_string(),
        capacity_pct,
        online: is_online,
    }
}

pub fn read_host_power_state() -> PowerState {
    let power_supply_dir = Path::new("/sys/class/power_supply");
    if power_supply_dir.exists() && power_supply_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(power_supply_dir) {
            let mut found_battery: Option<LinuxPowerSupplyData> = None;
            let mut ac_online = false;

            for entry in entries.flatten() {
                let path = entry.path();
                let type_path = path.join("type");
                let status_path = path.join("status");
                let capacity_path = path.join("capacity");
                let online_path = path.join("online");

                let supply_type = std::fs::read_to_string(&type_path).unwrap_or_default();
                let supply_name = entry.file_name().to_string_lossy().to_string();

                if supply_type.trim().eq_ignore_ascii_case("Mains") {
                    if let Ok(on_str) = std::fs::read_to_string(&online_path) {
                        if on_str.trim() == "1" {
                            ac_online = true;
                        }
                    }
                } else if supply_type.trim().eq_ignore_ascii_case("Battery") {
                    let status = std::fs::read_to_string(&status_path).unwrap_or_else(|_| "Unknown".to_string());
                    let capacity = std::fs::read_to_string(&capacity_path).unwrap_or_else(|_| "100".to_string());
                    let data = parse_power_supply_fields(&supply_name, &status, &capacity, &supply_type, None);
                    found_battery = Some(data);
                }
            }

            if let Some(bat) = found_battery {
                let is_charging = bat.status.eq_ignore_ascii_case("Charging") || ac_online;
                let on_battery = bat.status.eq_ignore_ascii_case("Discharging") || !ac_online;
                return PowerState {
                    on_battery,
                    battery_percent: bat.capacity_pct,
                    charging: is_charging,
                };
            }
        }
    }

    PowerState {
        on_battery: false,
        battery_percent: 100.0,
        charging: true,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinuxThermalZoneData {
    pub zone_name: String,
    pub temp_celsius: f32,
    pub zone_type: String,
    pub is_throttled: bool,
}

pub fn parse_thermal_zone(temp_raw: &str, zone_type: &str, name: &str) -> LinuxThermalZoneData {
    let millicelsius = temp_raw.trim().parse::<f32>().unwrap_or(42000.0);
    let celsius = millicelsius / 1000.0;
    let is_throttled = celsius >= 85.0; // Standard thermal trip point

    LinuxThermalZoneData {
        zone_name: name.to_string(),
        temp_celsius: celsius,
        zone_type: zone_type.trim().to_string(),
        is_throttled,
    }
}

pub fn read_host_thermal_state() -> ThermalState {
    let thermal_dir = Path::new("/sys/class/thermal");
    if thermal_dir.exists() && thermal_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(thermal_dir) {
            let mut highest_temp: f32 = 0.0;
            let mut any_throttled = false;
            let mut found_any = false;

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("thermal_zone") {
                    let path = entry.path();
                    let temp_path = path.join("temp");
                    let type_path = path.join("type");

                    if let Ok(temp_str) = std::fs::read_to_string(&temp_path) {
                        let type_str = std::fs::read_to_string(&type_path).unwrap_or_else(|_| "unknown".to_string());
                        let data = parse_thermal_zone(&temp_str, &type_str, &name);
                        found_any = true;
                        if data.temp_celsius > highest_temp {
                            highest_temp = data.temp_celsius;
                        }
                        if data.is_throttled {
                            any_throttled = true;
                        }
                    }
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

    ThermalState {
        temperature_celsius: 42.0,
        throttled: false,
    }
}
