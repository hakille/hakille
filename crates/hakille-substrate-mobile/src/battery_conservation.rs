use hakille_substrate::PowerState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryTier {
    Charging,
    Normal,
    Conserve,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyProfileRecord {
    pub agent_name: String,
    pub energy_micro_watt_hours: u64,
    pub duration_ms: u64,
}

pub struct BatteryConservationEngine {
    battery_percent: f32,
    charging: bool,
    tier: BatteryTier,
    agent_energy_ledger: Mutex<HashMap<String, u64>>,
}

impl BatteryConservationEngine {
    pub fn new(battery_percent: f32, charging: bool) -> Self {
        let mut engine = Self {
            battery_percent,
            charging,
            tier: BatteryTier::Normal,
            agent_energy_ledger: Mutex::new(HashMap::new()),
        };
        engine.evaluate_tier();
        engine
    }

    pub fn evaluate_tier(&mut self) {
        if self.charging {
            self.tier = BatteryTier::Charging;
        } else if self.battery_percent > 30.0 {
            self.tier = BatteryTier::Normal;
        } else if self.battery_percent >= 15.0 {
            self.tier = BatteryTier::Conserve;
        } else {
            self.tier = BatteryTier::Critical;
        }
    }

    pub fn update_power_state(&mut self, state: &PowerState) {
        self.battery_percent = state.battery_percent;
        self.charging = state.charging;
        self.evaluate_tier();
    }

    pub fn current_tier(&self) -> BatteryTier {
        self.tier
    }

    pub fn perception_fps(&self) -> u32 {
        match self.tier {
            BatteryTier::Charging => 30,
            BatteryTier::Normal => 30,
            BatteryTier::Conserve => 1,
            BatteryTier::Critical => 0, // Event-driven only
        }
    }

    pub fn should_offload_inference_to_shadow(&self) -> bool {
        match self.tier {
            BatteryTier::Charging | BatteryTier::Normal => false,
            BatteryTier::Conserve | BatteryTier::Critical => true,
        }
    }

    pub fn should_offload_inference_with_network(&self, is_network_connected: bool) -> bool {
        if !is_network_connected {
            // Offline fallback: force local low-power execution
            false
        } else {
            self.should_offload_inference_to_shadow()
        }
    }

    pub fn background_sync_interval_secs(&self) -> u32 {
        match self.tier {
            BatteryTier::Charging => 5,
            BatteryTier::Normal => 15,
            BatteryTier::Conserve => 900, // 15 minutes
            BatteryTier::Critical => 0,   // Paused until charging
        }
    }

    pub fn record_agent_energy(&self, agent_name: &str, micro_watt_hours: u64) {
        let mut map = self.agent_energy_ledger.lock().unwrap();
        *map.entry(agent_name.to_string()).or_default() += micro_watt_hours;
    }

    pub fn get_agent_energy(&self, agent_name: &str) -> u64 {
        self.agent_energy_ledger
            .lock()
            .unwrap()
            .get(agent_name)
            .copied()
            .unwrap_or(0)
    }
}

impl Default for BatteryConservationEngine {
    fn default() -> Self {
        Self::new(85.0, false)
    }
}
