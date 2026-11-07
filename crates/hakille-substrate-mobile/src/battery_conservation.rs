//! Mesures fournies par l'hôte et décisions de budget ; aucune lecture de
//! batterie ni mesure d'énergie n'est prétendue sans fournisseur natif.
use crate::lifecycle::MobileRuntimePolicy;
use hakille_core::{HakilleError, Result};
use hakille_substrate::PowerState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryTier {
    Unknown,
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
    observation: Option<PowerState>,
    tier: BatteryTier,
    agent_energy_ledger: Mutex<HashMap<String, u64>>,
}

fn invalid(message: &str) -> HakilleError {
    HakilleError::SubstrateError(message.into())
}

impl BatteryConservationEngine {
    /// Compatibilité : une valeur invalide devient Unknown, jamais un budget
    /// normal/charging. `try_new` expose l'erreur de validation à l'appelant.
    pub fn new(battery_percent: f32, charging: bool) -> Self {
        Self::try_new(battery_percent, charging).unwrap_or_default()
    }

    pub fn try_new(battery_percent: f32, charging: bool) -> Result<Self> {
        let mut engine = Self::default();
        engine.update_power_state(&PowerState {
            on_battery: !charging,
            battery_percent,
            charging,
        })?;
        Ok(engine)
    }

    pub fn observation(&self) -> Option<PowerState> {
        self.observation.clone()
    }

    pub fn clear_observation(&mut self) {
        self.observation = None;
        self.tier = BatteryTier::Unknown;
    }

    pub fn evaluate_tier(&mut self) {
        self.tier = match &self.observation {
            None => BatteryTier::Unknown,
            Some(state) if state.charging => BatteryTier::Charging,
            Some(state) if state.battery_percent > 30.0 => BatteryTier::Normal,
            Some(state) if state.battery_percent >= 15.0 => BatteryTier::Conserve,
            Some(_) => BatteryTier::Critical,
        };
    }

    /// Un relevé invalide invalide aussi l'ancien budget : pas de mesure périmée
    /// conservée comme si elle provenait toujours d'une source saine.
    pub fn update_power_state(&mut self, state: &PowerState) -> Result<()> {
        if !state.battery_percent.is_finite()
            || !(0.0..=100.0).contains(&state.battery_percent)
            || (state.on_battery && state.charging)
        {
            self.clear_observation();
            return Err(invalid(
                "batterie invalide : pourcentage fini 0–100 et charging incompatible avec on_battery",
            ));
        }
        self.observation = Some(state.clone());
        self.evaluate_tier();
        Ok(())
    }

    pub fn current_tier(&self) -> BatteryTier {
        self.tier
    }

    pub fn perception_fps(&self) -> u32 {
        match self.tier {
            BatteryTier::Charging | BatteryTier::Normal => 30,
            BatteryTier::Conserve => 1,
            BatteryTier::Critical | BatteryTier::Unknown => 0,
        }
    }

    pub fn perception_fps_with_policy(&self, policy: &MobileRuntimePolicy) -> u32 {
        self.perception_fps().min(policy.perception_fps_limit)
    }

    /// Recommandation de calcul uniquement, pas une connexion ni un consentement
    /// réseau : l'hôte doit appliquer ses capabilities et sa politique d'egress.
    pub fn should_offload_inference_to_shadow(&self) -> bool {
        matches!(self.tier, BatteryTier::Conserve | BatteryTier::Critical)
    }

    pub fn should_offload_inference_with_network(&self, connected: bool) -> bool {
        connected && self.should_offload_inference_to_shadow()
    }

    /// Zéro signifie suspendu, jamais une boucle de synchronisation sans délai.
    pub fn background_sync_interval_secs(&self) -> u32 {
        match self.tier {
            BatteryTier::Charging => 5,
            BatteryTier::Normal => 15,
            BatteryTier::Conserve => 900,
            BatteryTier::Critical | BatteryTier::Unknown => 0,
        }
    }

    /// Consigne une mesure fournie par le profileur hôte. Ne calcule pas une
    /// consommation supposée depuis la durée du traitement.
    pub fn record_agent_energy(&self, agent_name: &str, micro_watt_hours: u64) -> Result<()> {
        if agent_name.is_empty()
            || agent_name.len() > 128
            || agent_name.chars().any(char::is_control)
        {
            return Err(invalid("nom d'agent énergétique invalide"));
        }
        let mut map = self
            .agent_energy_ledger
            .lock()
            .map_err(|_| invalid("registre énergétique indisponible"))?;
        if !map.contains_key(agent_name) && map.len() >= 256 {
            return Err(invalid("registre énergétique plein (256 agents maximum)"));
        }
        let next = map
            .get(agent_name)
            .copied()
            .unwrap_or(0)
            .checked_add(micro_watt_hours)
            .ok_or_else(|| invalid("compteur énergétique saturé, mesure non ajoutée"))?;
        map.insert(agent_name.to_owned(), next);
        Ok(())
    }

    pub fn get_agent_energy(&self, agent_name: &str) -> Result<u64> {
        Ok(self
            .agent_energy_ledger
            .lock()
            .map_err(|_| invalid("registre énergétique indisponible"))?
            .get(agent_name)
            .copied()
            .unwrap_or(0))
    }
}

impl Default for BatteryConservationEngine {
    fn default() -> Self {
        Self {
            observation: None,
            tier: BatteryTier::Unknown,
            agent_energy_ledger: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::{AppLifecycleState, MobileLifecyclePolicy, ThermalPressure};

    #[test]
    fn unknown_and_invalid_battery_are_conservative() {
        let mut engine = BatteryConservationEngine::default();
        assert!(engine.observation().is_none());
        assert_eq!(engine.current_tier(), BatteryTier::Unknown);
        assert_eq!(engine.perception_fps(), 0);
        assert_eq!(engine.background_sync_interval_secs(), 0);
        for percent in [f32::NAN, f32::INFINITY, -1.0, 100.1] {
            assert!(BatteryConservationEngine::try_new(percent, true).is_err());
            assert_eq!(
                BatteryConservationEngine::new(percent, true).current_tier(),
                BatteryTier::Unknown
            );
        }
        engine
            .update_power_state(&PowerState {
                on_battery: false,
                charging: true,
                battery_percent: 50.0,
            })
            .unwrap();
        assert_eq!(engine.current_tier(), BatteryTier::Charging);
        assert!(
            engine
                .update_power_state(&PowerState {
                    on_battery: true,
                    charging: true,
                    battery_percent: 50.0
                })
                .is_err()
        );
        assert!(engine.observation().is_none());
    }

    #[test]
    fn thresholds_and_thermal_lifecycle_budget() {
        for (percent, tier) in [
            (0.0, BatteryTier::Critical),
            (14.99, BatteryTier::Critical),
            (15.0, BatteryTier::Conserve),
            (30.0, BatteryTier::Conserve),
            (30.01, BatteryTier::Normal),
            (100.0, BatteryTier::Normal),
        ] {
            assert_eq!(
                BatteryConservationEngine::try_new(percent, false)
                    .unwrap()
                    .current_tier(),
                tier
            );
        }
        let engine = BatteryConservationEngine::new(80.0, false);
        let mut lifecycle = MobileLifecyclePolicy::default();
        lifecycle.update_thermal(ThermalPressure::Nominal);
        lifecycle.update_lifecycle(AppLifecycleState::Foreground);
        assert_eq!(engine.perception_fps_with_policy(&lifecycle.policy()), 30);
        lifecycle.update_thermal(ThermalPressure::Serious);
        assert_eq!(engine.perception_fps_with_policy(&lifecycle.policy()), 0);
        lifecycle.update_thermal(ThermalPressure::Nominal);
        lifecycle.update_lifecycle(AppLifecycleState::Background);
        assert_eq!(engine.perception_fps_with_policy(&lifecycle.policy()), 0);
    }

    #[test]
    fn energy_ledger_rejects_overflow_and_unbounded_cardinality() {
        let engine = BatteryConservationEngine::default();
        engine.record_agent_energy("agent", u64::MAX).unwrap();
        assert!(engine.record_agent_energy("agent", 1).is_err());
        assert_eq!(engine.get_agent_energy("agent").unwrap(), u64::MAX);
        for index in 0..255 {
            engine
                .record_agent_energy(&format!("agent-{index}"), 1)
                .unwrap();
        }
        assert!(engine.record_agent_energy("one-too-many", 1).is_err());
        assert!(engine.record_agent_energy("", 1).is_err());
        assert!(engine.record_agent_energy(&"a".repeat(129), 1).is_err());
    }
}
