//! Politique pure de cycle de vie et pression thermique. Les effets retournés
//! sont des ORDRES à appliquer par l'hôte, jamais une preuve d'arrêt du micro,
//! d'annulation OS ou d'exécution en arrière-plan.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppLifecycleState {
    #[default]
    Unknown,
    Foreground,
    Inactive,
    Background,
    Terminated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalPressure {
    #[default]
    Unknown,
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEffect {
    StopMicrophone,
    CancelInference,
    PauseBackgroundWork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileRuntimePolicy {
    pub microphone_allowed: bool,
    pub inference_allowed: bool,
    pub perception_fps_limit: u32,
    pub background_sync_allowed: bool,
}

#[derive(Debug, Default, Clone)]
pub struct MobileLifecyclePolicy {
    lifecycle: AppLifecycleState,
    thermal: ThermalPressure,
}

impl MobileLifecyclePolicy {
    pub fn lifecycle(&self) -> AppLifecycleState {
        self.lifecycle
    }
    pub fn thermal(&self) -> ThermalPressure {
        self.thermal
    }

    /// Pas de StartMicrophone automatique au retour foreground : une activation
    /// utilisateur et la permission native restent nécessaires.
    pub fn update_lifecycle(&mut self, state: AppLifecycleState) -> Vec<LifecycleEffect> {
        self.lifecycle = state;
        self.restriction_effects()
    }

    pub fn update_thermal(&mut self, pressure: ThermalPressure) -> Vec<LifecycleEffect> {
        self.thermal = pressure;
        self.restriction_effects()
    }

    pub fn policy(&self) -> MobileRuntimePolicy {
        let foreground = self.lifecycle == AppLifecycleState::Foreground;
        let thermal_budget = match self.thermal {
            ThermalPressure::Nominal => 30,
            ThermalPressure::Fair => 10,
            ThermalPressure::Unknown => 1,
            ThermalPressure::Serious | ThermalPressure::Critical => 0,
        };
        MobileRuntimePolicy {
            microphone_allowed: foreground && thermal_budget > 0,
            inference_allowed: foreground
                && matches!(
                    self.thermal,
                    ThermalPressure::Nominal | ThermalPressure::Fair
                ),
            perception_fps_limit: if foreground { thermal_budget } else { 0 },
            background_sync_allowed: foreground
                && matches!(
                    self.thermal,
                    ThermalPressure::Nominal | ThermalPressure::Fair
                ),
        }
    }

    fn restriction_effects(&self) -> Vec<LifecycleEffect> {
        let policy = self.policy();
        let mut effects = Vec::new();
        if !policy.microphone_allowed {
            effects.push(LifecycleEffect::StopMicrophone);
        }
        if !policy.inference_allowed {
            effects.push(LifecycleEffect::CancelInference);
        }
        if !policy.background_sync_allowed {
            effects.push(LifecycleEffect::PauseBackgroundWork);
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_and_thermal_require_stop_without_claiming_os_effect() {
        let mut policy = MobileLifecyclePolicy::default();
        assert!(!policy.policy().microphone_allowed);
        policy.update_thermal(ThermalPressure::Nominal);
        assert!(
            policy
                .update_lifecycle(AppLifecycleState::Foreground)
                .is_empty()
        );
        assert!(policy.policy().microphone_allowed && policy.policy().inference_allowed);
        for state in [
            AppLifecycleState::Inactive,
            AppLifecycleState::Background,
            AppLifecycleState::Terminated,
            AppLifecycleState::Unknown,
        ] {
            assert_eq!(
                policy.update_lifecycle(state),
                [
                    LifecycleEffect::StopMicrophone,
                    LifecycleEffect::CancelInference,
                    LifecycleEffect::PauseBackgroundWork
                ]
            );
            assert!(!policy.policy().microphone_allowed);
        }
        assert!(
            policy
                .update_lifecycle(AppLifecycleState::Foreground)
                .is_empty()
        );
        assert!(
            policy
                .update_thermal(ThermalPressure::Critical)
                .contains(&LifecycleEffect::StopMicrophone)
        );
        assert!(!policy.policy().inference_allowed);
        policy.update_thermal(ThermalPressure::Fair);
        assert_eq!(policy.policy().perception_fps_limit, 10);
    }

    #[test]
    fn unknown_thermal_does_not_invent_full_compute_budget() {
        let mut policy = MobileLifecyclePolicy::default();
        let effects = policy.update_lifecycle(AppLifecycleState::Foreground);
        assert!(effects.contains(&LifecycleEffect::CancelInference));
        assert!(!policy.policy().inference_allowed);
        assert_eq!(policy.thermal(), ThermalPressure::Unknown);
    }
}
