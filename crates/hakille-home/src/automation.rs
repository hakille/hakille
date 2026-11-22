//! Moteur d'automatisation réactive souverain (`AutomationEngine`).
//!
//! Principes de sécurité et d'honnêteté :
//! - Évaluation 100 % locale sur les états et mesures réels enregistrés dans le hub.
//! - Anti-oscillation (hystérésis / cooldown par règle) pour éviter les boucles d'actionneurs.
//! - **Invariant constitutionnel Tier 3** : une règle autonome ne peut JAMAIS exécuter
//!   silencieusement une action physique irréversible (`UnlockDoor`, `DisarmAlarm`,
//!   `SetWaterValve`, `SetMainBreaker`). Toute action Tier 3 est obligatoirement
//!   escaladée en `AutomationEscalation` exigeant une Carte d'Action et un maintien
//!   physique Braise de 600 ms.

use crate::energy::EnergyBalance;
use crate::model::{HomeCommand, HomeDevice, HomeRiskTier};
use crate::security::HomeRiskClassifier;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Condition déclenchante d'une règle domotique locale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationTrigger {
    /// Déclenchement quand un capteur de présence change d'état dans une pièce.
    OccupancyChanged { room_id: String, occupied: bool },
    /// Déclenchement quand la température mesurée d'un appareil franchit un seuil.
    TemperatureBelow { device_id: String, threshold_c: f32 },
    /// Déclenchement quand la température mesurée dépasse un seuil.
    TemperatureAbove { device_id: String, threshold_c: f32 },
    /// Déclenchement quand le surplus solaire mesuré dépasse un seuil (W).
    SolarSurplusAbove { min_surplus_w: f64 },
    /// Déclenchement quand l'import réseau net dépasse un plafond (W).
    GridImportAbove { max_import_w: f64 },
    /// Déclenchement quand l'état de charge batterie passe sous un seuil (%).
    BatterySocBelow { min_soc_pct: f32 },
    /// Déclenchement quand un appareil est dans un état d'alimentation donné.
    DevicePowerIs { device_id: String, on: bool },
}

/// Condition additionnelle optionnelle pour filtrer l'exécution d'une règle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationCondition {
    /// Plage horaire UTC (heure de début incluse, heure de fin exclue ; gère le passage minuit).
    HourWindowUtc { start_hour: u32, end_hour: u32 },
    /// Exige que la pièce soit occupée (ou inoccupée).
    RoomOccupied { room_id: String, occupied: bool },
    /// Exige qu'au moins une sonde énergétique réelle soit active.
    HasMeasuredEnergy,
}

/// Action ciblée par une règle d'automatisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationAction {
    pub device_id: String,
    pub command: HomeCommand,
}

/// Règle d'automatisation réactive persistée dans `home.redb`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub trigger: AutomationTrigger,
    #[serde(default)]
    pub conditions: Vec<AutomationCondition>,
    pub actions: Vec<AutomationAction>,
    /// Délai minimal entre deux déclenchements consécutifs (anti-oscillation).
    pub cooldown_secs: u64,
    #[serde(default)]
    pub last_triggered_at: Option<String>,
}

/// Action prête à être exécutée automatiquement (Tier 0, 1 ou 2 uniquement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedAutoDispatch {
    pub rule_id: String,
    pub rule_name: String,
    pub device_id: String,
    pub command: HomeCommand,
    pub risk_tier: HomeRiskTier,
    pub reason: String,
}

/// Escalade obligatoire vers Carte d'Action + maintien physique 600 ms pour toute action Tier 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationEscalation {
    pub rule_id: String,
    pub rule_name: String,
    pub device_id: String,
    pub command: HomeCommand,
    pub risk_tier: HomeRiskTier,
    pub requires_600ms_hold: bool,
    pub reason: String,
}

/// Résultat déterministe d'un cycle d'évaluation du moteur d'automatisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AutomationEvaluationReport {
    pub evaluated_rules: usize,
    pub triggered_rule_ids: Vec<String>,
    pub auto_dispatches: Vec<PlannedAutoDispatch>,
    pub escalations: Vec<AutomationEscalation>,
    pub skipped_cooldown: Vec<String>,
}

/// Évaluateur pur et déterministe des règles domotiques locales.
pub struct AutomationEngine;

impl AutomationEngine {
    /// Évalue toutes les règles actives sur l'état réel courant des appareils et du bilan énergétique.
    ///
    /// - Les actions Tier 0/1/2 sont placées dans `auto_dispatches`.
    /// - **Toute** action Tier 3 est systématiquement bloquée de l'exécution silencieuse
    ///   et routée vers `escalations` (`requires_600ms_hold = true`).
    pub fn evaluate(
        rules: &mut [AutomationRule],
        devices: &BTreeMap<String, HomeDevice>,
        energy: &EnergyBalance,
        now: DateTime<Utc>,
    ) -> AutomationEvaluationReport {
        let mut report = AutomationEvaluationReport {
            evaluated_rules: rules.len(),
            ..Default::default()
        };

        for rule in rules.iter_mut() {
            if !rule.enabled {
                continue;
            }
            if is_in_cooldown(rule, now) {
                report.skipped_cooldown.push(rule.id.clone());
                continue;
            }
            if !trigger_matches(&rule.trigger, devices, energy) {
                continue;
            }
            if !rule
                .conditions
                .iter()
                .all(|c| condition_matches(c, devices, energy, now))
            {
                continue;
            }

            let mut rule_fired = false;
            for action in &rule.actions {
                let Some(device) = devices.get(&action.device_id) else {
                    continue;
                };
                let tier = HomeRiskClassifier::classify(device.category, &action.command);
                rule_fired = true;
                if tier.requires_physical_hold() {
                    report.escalations.push(AutomationEscalation {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        device_id: action.device_id.clone(),
                        command: action.command.clone(),
                        risk_tier: tier,
                        requires_600ms_hold: true,
                        reason: format!(
                            "Règle '{}' bloquée en exécution autonome : action Tier 3 '{}' sur '{}' exige maintien physique 600 ms",
                            rule.name,
                            action.command.action_name(),
                            device.name
                        ),
                    });
                } else {
                    report.auto_dispatches.push(PlannedAutoDispatch {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        device_id: action.device_id.clone(),
                        command: action.command.clone(),
                        risk_tier: tier,
                        reason: format!(
                            "Règle '{}' déclenchée -> '{}' sur '{}'",
                            rule.name,
                            action.command.action_name(),
                            device.name
                        ),
                    });
                }
            }

            if rule_fired {
                rule.last_triggered_at = Some(now.to_rfc3339());
                report.triggered_rule_ids.push(rule.id.clone());
            }
        }

        report
    }
}

fn is_in_cooldown(rule: &AutomationRule, now: DateTime<Utc>) -> bool {
    if rule.cooldown_secs == 0 {
        return false;
    }
    let Some(raw) = &rule.last_triggered_at else {
        return false;
    };
    let Ok(last) = DateTime::parse_from_rfc3339(raw) else {
        return false;
    };
    let elapsed = now
        .signed_duration_since(last.with_timezone(&Utc))
        .num_seconds();
    elapsed >= 0 && (elapsed as u64) < rule.cooldown_secs
}

fn trigger_matches(
    trigger: &AutomationTrigger,
    devices: &BTreeMap<String, HomeDevice>,
    energy: &EnergyBalance,
) -> bool {
    match trigger {
        AutomationTrigger::OccupancyChanged { room_id, occupied } => devices
            .values()
            .any(|d| d.room.eq_ignore_ascii_case(room_id) && d.state.occupancy == Some(*occupied)),
        AutomationTrigger::TemperatureBelow {
            device_id,
            threshold_c,
        } => devices
            .get(device_id)
            .and_then(|d| d.state.temperature_c)
            .is_some_and(|t| t < *threshold_c),
        AutomationTrigger::TemperatureAbove {
            device_id,
            threshold_c,
        } => devices
            .get(device_id)
            .and_then(|d| d.state.temperature_c)
            .is_some_and(|t| t > *threshold_c),
        AutomationTrigger::SolarSurplusAbove { min_surplus_w } => {
            energy.measured_device_count > 0 && (-energy.net_grid_w) >= *min_surplus_w
        }
        AutomationTrigger::GridImportAbove { max_import_w } => {
            energy.measured_device_count > 0 && energy.net_grid_w >= *max_import_w
        }
        AutomationTrigger::BatterySocBelow { min_soc_pct } => {
            energy.battery_soc_pct.is_some_and(|s| s < *min_soc_pct)
        }
        AutomationTrigger::DevicePowerIs { device_id, on } => devices
            .get(device_id)
            .and_then(|d| d.state.on)
            .is_some_and(|state| state == *on),
    }
}

fn condition_matches(
    cond: &AutomationCondition,
    devices: &BTreeMap<String, HomeDevice>,
    energy: &EnergyBalance,
    now: DateTime<Utc>,
) -> bool {
    match cond {
        AutomationCondition::HourWindowUtc {
            start_hour,
            end_hour,
        } => {
            let h = now.hour();
            if start_hour <= end_hour {
                h >= *start_hour && h < *end_hour
            } else {
                h >= *start_hour || h < *end_hour
            }
        }
        AutomationCondition::RoomOccupied { room_id, occupied } => devices
            .values()
            .any(|d| d.room.eq_ignore_ascii_case(room_id) && d.state.occupancy == Some(*occupied)),
        AutomationCondition::HasMeasuredEnergy => energy.measured_device_count > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceCategory, ProtocolKind};

    #[test]
    fn tier3_automation_is_always_escalated_never_auto_dispatched() {
        let mut devices = BTreeMap::new();
        let mut sensor = HomeDevice::new(
            "sensor-entree",
            "Présence Entrée",
            "entree",
            ProtocolKind::Zigbee,
            DeviceCategory::OccupancySensor,
            "mqtt://192.168.1.10:1883",
        );
        sensor.state.occupancy = Some(true);
        devices.insert(sensor.id.clone(), sensor);

        let lamp = HomeDevice::new(
            "lampe-entree",
            "Lampe Entrée",
            "entree",
            ProtocolKind::Matter,
            DeviceCategory::Light,
            "192.168.1.20:5540",
        );
        devices.insert(lamp.id.clone(), lamp);

        let lock = HomeDevice::new(
            "serrure-entree",
            "Serrure Entrée",
            "entree",
            ProtocolKind::Matter,
            DeviceCategory::DoorLock,
            "192.168.1.21:5540",
        );
        devices.insert(lock.id.clone(), lock);

        let mut rules = vec![AutomationRule {
            id: "arrivee-maison".into(),
            name: "Arrivée Maison".into(),
            description: "Allume l'entrée et propose le déverrouillage".into(),
            enabled: true,
            trigger: AutomationTrigger::OccupancyChanged {
                room_id: "entree".into(),
                occupied: true,
            },
            conditions: vec![],
            actions: vec![
                AutomationAction {
                    device_id: "lampe-entree".into(),
                    command: HomeCommand::SetPower { on: true },
                },
                AutomationAction {
                    device_id: "serrure-entree".into(),
                    command: HomeCommand::UnlockDoor,
                },
            ],
            cooldown_secs: 60,
            last_triggered_at: None,
        }];

        let now = Utc::now();
        let report =
            AutomationEngine::evaluate(&mut rules, &devices, &EnergyBalance::default(), now);
        assert_eq!(report.auto_dispatches.len(), 1);
        assert_eq!(report.auto_dispatches[0].device_id, "lampe-entree");
        assert_eq!(report.escalations.len(), 1);
        assert_eq!(report.escalations[0].device_id, "serrure-entree");
        assert!(report.escalations[0].requires_600ms_hold);

        // Cooldown respecté immédiatement après
        let second =
            AutomationEngine::evaluate(&mut rules, &devices, &EnergyBalance::default(), now);
        assert!(second.auto_dispatches.is_empty());
        assert_eq!(second.skipped_cooldown, vec!["arrivee-maison"]);
    }
}
