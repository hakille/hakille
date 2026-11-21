//! Moteur d'Homéostasie Énergétique & Arbitrage Dialectique Bicaméral.
//!
//! Agrège la télémétrie réelle des compteurs (Modbus/Shelly/MQTT), onduleurs solaires
//! (SunSpec), batteries de stockage et bornes de recharge VE (OCPP), puis génère un plan
//! d'arbitrage évalué de manière bicamérale (Catalyseur d'optimisation vs Avocat du Diable
//! de sécurité réseau/confort).

use crate::model::{DeviceCategory, HomeCommand, HomeDevice};
use serde::{Deserialize, Serialize};

/// Bilan énergétique instantané du foyer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EnergyBalance {
    pub solar_generation_w: f64,
    pub total_load_w: f64,
    pub ev_charging_w: f64,
    pub hvac_load_w: f64,
    pub battery_power_w: f64,
    pub battery_soc_pct: Option<f32>,
    /// Positif = import réseau, Négatif = surplus injecté.
    pub net_grid_w: f64,
    /// Ratio d'autoconsommation [0.0..1.0].
    pub self_consumption_ratio: f32,
    pub measured_device_count: usize,
}

/// Recommandation d'arbitrage sur un appareil énergétique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyArbitrageAction {
    pub device_id: String,
    pub device_name: String,
    pub command: HomeCommand,
    pub expected_delta_w: f64,
    pub reason: String,
}

/// Rapport du débat bicaméral sur le plan énergétique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyDialecticReport {
    pub catalyst_thesis: String,
    pub devils_advocate_critique: String,
    pub arbiter_verdict: String,
    pub safety_margin_pct: f32,
    pub actions: Vec<EnergyArbitrageAction>,
}

/// Moteur d'homéostasie énergétique du bâtiment.
pub struct EnergyHomeostasisEngine {
    /// Seuil maximal d'import réseau avant délestage doux (watts).
    pub max_grid_import_w: f64,
    /// Seuil minimum de réserve batterie domestique (%).
    pub min_battery_reserve_pct: f32,
    /// Surplus solaire minimum pour déclencher la charge VE (watts).
    pub ev_surplus_threshold_w: f64,
}

impl Default for EnergyHomeostasisEngine {
    fn default() -> Self {
        Self {
            max_grid_import_w: 6000.0,
            min_battery_reserve_pct: 20.0,
            ev_surplus_threshold_w: 1400.0,
        }
    }
}

impl EnergyHomeostasisEngine {
    pub fn compute_balance<'a, I>(&self, devices: I) -> EnergyBalance
    where
        I: IntoIterator<Item = &'a HomeDevice>,
    {
        let mut solar_generation_w = 0.0_f64;
        let mut total_load_w = 0.0_f64;
        let mut ev_charging_w = 0.0_f64;
        let mut hvac_load_w = 0.0_f64;
        let mut battery_power_w = 0.0_f64;
        let mut soc_acc = Vec::new();
        let mut measured_device_count = 0usize;

        for dev in devices {
            let Some(telem) = &dev.state.energy else {
                continue;
            };
            measured_device_count += 1;
            if let Some(solar) = telem.solar_generation_w {
                solar_generation_w += solar.max(0.0);
            }
            if let Some(soc) = telem.battery_soc_pct {
                soc_acc.push(soc);
            }
            match dev.category {
                DeviceCategory::SolarInverter => {
                    if telem.solar_generation_w.is_none() && telem.power_w < 0.0 {
                        solar_generation_w += (-telem.power_w).max(0.0);
                    }
                }
                DeviceCategory::BatteryStorage => {
                    battery_power_w += telem.power_w;
                }
                DeviceCategory::EvCharger => {
                    let w = telem.power_w.max(0.0);
                    ev_charging_w += w;
                    total_load_w += w;
                }
                DeviceCategory::Hvac | DeviceCategory::Thermostat => {
                    let w = telem.power_w.max(0.0);
                    hvac_load_w += w;
                    total_load_w += w;
                }
                DeviceCategory::EnergyMeter => {
                    // Compteur général ignoré dans la somme des charges filles pour éviter le double comptage
                    // s'il n'est utilisé que comme référence.
                }
                _ => {
                    total_load_w += telem.power_w.max(0.0);
                }
            }
        }

        let battery_soc_pct = if soc_acc.is_empty() {
            None
        } else {
            Some(soc_acc.iter().sum::<f32>() / soc_acc.len() as f32)
        };

        let net_grid_w = total_load_w + battery_power_w - solar_generation_w;
        let self_consumption_ratio = if total_load_w > 1.0 && solar_generation_w > 0.0 {
            ((solar_generation_w.min(total_load_w)) / solar_generation_w).clamp(0.0, 1.0) as f32
        } else if solar_generation_w <= 0.0 {
            0.0
        } else {
            1.0
        };

        EnergyBalance {
            solar_generation_w,
            total_load_w,
            ev_charging_w,
            hvac_load_w,
            battery_power_w,
            battery_soc_pct,
            net_grid_w,
            self_consumption_ratio,
            measured_device_count,
        }
    }

    /// Produit un plan d'arbitrage énergétique évalué par dialectique bicamérale.
    pub fn evaluate_arbitrage<'a, I>(&self, devices: I) -> EnergyDialecticReport
    where
        I: IntoIterator<Item = &'a HomeDevice> + Clone,
    {
        let balance = self.compute_balance(devices.clone());
        let mut actions = Vec::new();
        let mut catalyst_notes = Vec::new();
        let mut critique_notes = Vec::new();

        if balance.measured_device_count == 0 {
            return EnergyDialecticReport {
                catalyst_thesis: "Aucune sonde de puissance réelle enregistrée : aucune optimisation speculée."
                    .to_string(),
                devils_advocate_critique: "Contrat d'honnêteté : impossible de calculer un surplus ou délestage sans télémétrie réelle."
                    .to_string(),
                arbiter_verdict: "Observation uniquement (0 capteur énergétique actif).".to_string(),
                safety_margin_pct: 100.0,
                actions,
            };
        }

        // Surplus solaire exploitable -> charger le véhicule électrique en OCPP
        if balance.net_grid_w < -self.ev_surplus_threshold_w {
            let surplus = -balance.net_grid_w;
            for dev in devices.clone() {
                if dev.category == DeviceCategory::EvCharger && dev.status.is_online() {
                    let target_amps = ((surplus / 230.0) as f32).clamp(6.0, 32.0);
                    actions.push(EnergyArbitrageAction {
                        device_id: dev.id.clone(),
                        device_name: dev.name.clone(),
                        command: HomeCommand::SetEvCharging {
                            enabled: true,
                            max_amps: Some((target_amps * 10.0).round() / 10.0),
                        },
                        expected_delta_w: surplus,
                        reason: format!(
                            "Surplus photovoltaïque de {surplus:.0} W -> modulation OCPP à {target_amps:.1} A"
                        ),
                    });
                    catalyst_notes.push(format!(
                        "Capturer {surplus:.0} W solaires dans le VE '{}' au lieu d'injecter à bas tarif.",
                        dev.name
                    ));
                }
            }
            if let Some(soc) = balance.battery_soc_pct
                && soc < 95.0
            {
                critique_notes.push(format!(
                    "Batterie domestique à {soc:.0}% : vérifier priorité stockage stationnaire vs véhicule."
                ));
            }
        }

        // Surcharge réseau -> délestage doux des charges flexibles (VE d'abord, jamais sécurité/frigo)
        if balance.net_grid_w > self.max_grid_import_w {
            let excess = balance.net_grid_w - self.max_grid_import_w;
            for dev in devices {
                if dev.category == DeviceCategory::EvCharger
                    && dev.state.energy.as_ref().is_some_and(|e| e.power_w > 500.0)
                {
                    actions.push(EnergyArbitrageAction {
                        device_id: dev.id.clone(),
                        device_name: dev.name.clone(),
                        command: HomeCommand::SetEvCharging {
                            enabled: false,
                            max_amps: Some(0.0),
                        },
                        expected_delta_w: -dev
                            .state
                            .energy
                            .as_ref()
                            .map(|e| e.power_w)
                            .unwrap_or(0.0),
                        reason: format!(
                            "Dépassement import ({:.0} W > {:.0} W) : pause charge VE",
                            balance.net_grid_w, self.max_grid_import_w
                        ),
                    });
                    catalyst_notes.push(format!(
                        "Réduire le pic réseau de {excess:.0} W via délestage VE."
                    ));
                    critique_notes.push(
                        "Vérifier qu'aucun départ matinal urgent n'est programmé avant de suspendre la charge."
                            .to_string(),
                    );
                }
            }
        }

        if let Some(soc) = balance.battery_soc_pct
            && soc < self.min_battery_reserve_pct
        {
            critique_notes.push(format!(
                "Réserve batterie critique ({soc:.0}% < {:.0}%) : préserver autonomie secours.",
                self.min_battery_reserve_pct
            ));
        }

        let safety_margin_pct = if self.max_grid_import_w > 0.0 {
            (((self.max_grid_import_w - balance.net_grid_w.max(0.0)) / self.max_grid_import_w)
                * 100.0)
                .clamp(0.0, 100.0) as f32
        } else {
            0.0
        };

        let catalyst_thesis = if catalyst_notes.is_empty() {
            format!(
                "Équilibre nominal : solaire {:.0} W, charge {:.0} W, réseau net {:.0} W.",
                balance.solar_generation_w, balance.total_load_w, balance.net_grid_w
            )
        } else {
            catalyst_notes.join(" ")
        };

        let devils_advocate_critique = if critique_notes.is_empty() {
            "Aucune contrainte critique de sécurité électrique ou de confort détectée.".to_string()
        } else {
            critique_notes.join(" ")
        };

        let arbiter_verdict = if actions.is_empty() {
            format!(
                "Homéostasie stable (marge de sécurité {:.1}%).",
                safety_margin_pct
            )
        } else {
            format!(
                "{} action(s) d'arbitrage proposées (marge {:.1}%).",
                actions.len(),
                safety_margin_pct
            )
        };

        EnergyDialecticReport {
            catalyst_thesis,
            devils_advocate_critique,
            arbiter_verdict,
            safety_margin_pct,
            actions,
        }
    }

    /// Détecte les anomalies énergétiques ou physiques à partir des seules mesures réelles.
    pub fn detect_anomalies<'a, I>(&self, devices: I) -> Vec<EnergyAnomaly>
    where
        I: IntoIterator<Item = &'a HomeDevice> + Clone,
    {
        let mut anomalies = Vec::new();
        let balance = self.compute_balance(devices.clone());
        if balance.measured_device_count == 0 {
            return anomalies;
        }
        if balance.net_grid_w > self.max_grid_import_w {
            anomalies.push(EnergyAnomaly {
                severity: AnomalySeverity::Critical,
                device_id: "grid".into(),
                code: "GRID_IMPORT_OVERLOAD".into(),
                message: format!(
                    "Surcharge réseau mesurée : {:.0} W > plafond {:.0} W",
                    balance.net_grid_w, self.max_grid_import_w
                ),
            });
        }
        if let Some(soc) = balance.battery_soc_pct
            && soc < self.min_battery_reserve_pct
        {
            anomalies.push(EnergyAnomaly {
                severity: AnomalySeverity::Warning,
                device_id: "battery".into(),
                code: "BATTERY_RESERVE_LOW".into(),
                message: format!(
                    "Batterie domestique sous la réserve de sécurité ({soc:.0}% < {:.0}%)",
                    self.min_battery_reserve_pct
                ),
            });
        }
        for dev in devices {
            if let Some(e) = &dev.state.energy
                && e.power_w > 3600.0
                && !matches!(
                    dev.category,
                    DeviceCategory::EvCharger
                        | DeviceCategory::Hvac
                        | DeviceCategory::EnergyMeter
                        | DeviceCategory::MainBreaker
                )
            {
                anomalies.push(EnergyAnomaly {
                    severity: AnomalySeverity::Warning,
                    device_id: dev.id.clone(),
                    code: "UNEXPECTED_DEVICE_SPIKE".into(),
                    message: format!(
                        "Pic de puissance inhabituel sur '{}' : {:.0} W",
                        dev.name, e.power_w
                    ),
                });
            }
        }
        anomalies
    }
}

/// Couleur de jour tarifaire EDF Tempo (France).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoDayColor {
    Bleu,
    Blanc,
    Rouge,
}

/// Grille tarifaire électrique locale pour arbitrage coût réel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TariffPlan {
    /// Tarif fixe unique (€/kWh).
    Flat { eur_per_kwh: f64 },
    /// Heures Pleines / Heures Creuses (HC de 22h à 06h par défaut).
    PeakOffPeak {
        peak_eur_per_kwh: f64,
        off_peak_eur_per_kwh: f64,
        off_peak_start_hour: u32,
        off_peak_end_hour: u32,
    },
    /// EDF Tempo (Bleu / Blanc / Rouge, HP 06h–22h / HC 22h–06h).
    Tempo { day_color: TempoDayColor },
}

impl Default for TariffPlan {
    fn default() -> Self {
        Self::Tempo {
            day_color: TempoDayColor::Bleu,
        }
    }
}

impl TariffPlan {
    /// Retourne le prix exact (€/kWh) pour l'heure locale `hour` (`0..24`).
    pub fn price_eur_per_kwh(&self, hour: u32) -> f64 {
        let h = hour % 24;
        match self {
            Self::Flat { eur_per_kwh } => *eur_per_kwh,
            Self::PeakOffPeak {
                peak_eur_per_kwh,
                off_peak_eur_per_kwh,
                off_peak_start_hour,
                off_peak_end_hour,
            } => {
                let is_hc = if off_peak_start_hour <= off_peak_end_hour {
                    h >= *off_peak_start_hour && h < *off_peak_end_hour
                } else {
                    h >= *off_peak_start_hour || h < *off_peak_end_hour
                };
                if is_hc {
                    *off_peak_eur_per_kwh
                } else {
                    *peak_eur_per_kwh
                }
            }
            Self::Tempo { day_color } => {
                let is_hp = (6..22).contains(&h);
                match (day_color, is_hp) {
                    (TempoDayColor::Bleu, false) => 0.1296,
                    (TempoDayColor::Bleu, true) => 0.1609,
                    (TempoDayColor::Blanc, false) => 0.1486,
                    (TempoDayColor::Blanc, true) => 0.1894,
                    (TempoDayColor::Rouge, false) => 0.1568,
                    (TempoDayColor::Rouge, true) => 0.7562,
                }
            }
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Flat { eur_per_kwh } => format!("Tarif fixe ({eur_per_kwh:.4} €/kWh)"),
            Self::PeakOffPeak {
                peak_eur_per_kwh,
                off_peak_eur_per_kwh,
                ..
            } => format!("HP/HC (HP {peak_eur_per_kwh:.4} € / HC {off_peak_eur_per_kwh:.4} €/kWh)"),
            Self::Tempo { day_color } => format!("EDF Tempo ({day_color:?})"),
        }
    }
}

/// Échantillon de télémétrie énergétique persisté dans `home.redb`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergySample {
    pub timestamp_utc: String,
    pub hour_local: u32,
    pub duration_secs: u64,
    pub solar_w: f64,
    pub load_w: f64,
    pub net_grid_w: f64,
}

/// Agrégation journalière réelle (zéro estimation inventée sans échantillons).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyEnergyReport {
    pub sample_count: usize,
    pub imported_kwh: f64,
    pub exported_kwh: f64,
    pub solar_generated_kwh: f64,
    pub estimated_import_cost_eur: f64,
    pub tariff_label: String,
    pub current_hour_price_eur_kwh: f64,
    pub recommendation: String,
}

/// Sévérité d'une anomalie énergétique mesurée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Warning,
    Critical,
}

/// Anomalie détectée sur des capteurs réels du foyer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyAnomaly {
    pub severity: AnomalySeverity,
    pub device_id: String,
    pub code: String,
    pub message: String,
}

/// Agrège une série d'échantillons mesurés avec une grille tarifaire réelle.
pub fn compute_daily_kwh_and_cost(
    samples: &[EnergySample],
    tariff: &TariffPlan,
    current_hour: u32,
) -> DailyEnergyReport {
    let current_price = tariff.price_eur_per_kwh(current_hour);
    if samples.is_empty() {
        return DailyEnergyReport {
            sample_count: 0,
            imported_kwh: 0.0,
            exported_kwh: 0.0,
            solar_generated_kwh: 0.0,
            estimated_import_cost_eur: 0.0,
            tariff_label: tariff.label(),
            current_hour_price_eur_kwh: current_price,
            recommendation:
                "Aucun échantillon énergétique persisté : aucune consommation extrapolée.".into(),
        };
    }

    let mut imported_kwh = 0.0;
    let mut exported_kwh = 0.0;
    let mut solar_kwh = 0.0;
    let mut cost_eur = 0.0;

    for s in samples {
        let hours = (s.duration_secs as f64) / 3600.0;
        let solar = (s.solar_w.max(0.0) / 1000.0) * hours;
        solar_kwh += solar;
        if s.net_grid_w >= 0.0 {
            let imp = (s.net_grid_w / 1000.0) * hours;
            imported_kwh += imp;
            cost_eur += imp * tariff.price_eur_per_kwh(s.hour_local);
        } else {
            exported_kwh += ((-s.net_grid_w) / 1000.0) * hours;
        }
    }

    let recommendation = if matches!(
        tariff,
        TariffPlan::Tempo {
            day_color: TempoDayColor::Rouge
        }
    ) && (6..22).contains(&(current_hour % 24))
    {
        "Jour Tempo ROUGE (HP 0.7562 €/kWh) : reporter charge VE et gros électroménager après 22h00."
            .into()
    } else if current_price <= 0.16 {
        "Créneau tarifaire favorable : charge VE et stockage stationnaire autorisés.".into()
    } else {
        "Créneau heures pleines : privilégier l'autoconsommation solaire directe.".into()
    };

    DailyEnergyReport {
        sample_count: samples.len(),
        imported_kwh,
        exported_kwh,
        solar_generated_kwh: solar_kwh,
        estimated_import_cost_eur: cost_eur,
        tariff_label: tariff.label(),
        current_hour_price_eur_kwh: current_price,
        recommendation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionStatus, EnergyTelemetry, ProtocolKind};

    #[test]
    fn homeostasis_detects_solar_surplus_and_proposes_ocpp_charge() {
        let mut solar = HomeDevice::new(
            "sunspec-1",
            "Onduleur Toiture",
            "garage",
            ProtocolKind::SunSpecSolar,
            DeviceCategory::SolarInverter,
            "192.168.1.50:502",
        );
        solar.status = ConnectionStatus::PairedOnline;
        solar.state.energy = Some(EnergyTelemetry {
            power_w: -4200.0,
            solar_generation_w: Some(4200.0),
            ..Default::default()
        });

        let mut ev = HomeDevice::new(
            "ocpp-wallbox",
            "Borne Garage",
            "garage",
            ProtocolKind::OcppEv,
            DeviceCategory::EvCharger,
            "ws://192.168.1.60:8887/ocpp",
        );
        ev.status = ConnectionStatus::PairedOnline;
        ev.state.energy = Some(EnergyTelemetry {
            power_w: 0.0,
            ..Default::default()
        });

        let engine = EnergyHomeostasisEngine::default();
        let report = engine.evaluate_arbitrage([&solar, &ev]);
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            report.actions[0].command,
            HomeCommand::SetEvCharging { enabled: true, .. }
        ));
    }

    #[test]
    fn homeostasis_remains_honest_with_zero_sensors() {
        let engine = EnergyHomeostasisEngine::default();
        let empty: [&HomeDevice; 0] = [];
        let report = engine.evaluate_arbitrage(empty);
        assert!(report.actions.is_empty());
        assert!(report.arbiter_verdict.contains("0 capteur"));
    }

    #[test]
    fn tempo_tariff_pricing_and_daily_aggregation_work_accurately() {
        let bleu = TariffPlan::Tempo {
            day_color: TempoDayColor::Bleu,
        };
        let rouge = TariffPlan::Tempo {
            day_color: TempoDayColor::Rouge,
        };
        // 14h = Heure Pleine (HP), 23h = Heure Creuse (HC)
        assert!(rouge.price_eur_per_kwh(14) > bleu.price_eur_per_kwh(14));
        assert!(rouge.price_eur_per_kwh(23) < rouge.price_eur_per_kwh(14));

        // 2 échantillons de 1800 s chacun (1 heure totale) : import net 2000 W -> 2.0 kWh
        let samples = vec![
            EnergySample {
                timestamp_utc: "2026-11-19T14:00:00Z".into(),
                hour_local: 14,
                duration_secs: 1800,
                solar_w: 500.0,
                load_w: 2500.0,
                net_grid_w: 2000.0,
            },
            EnergySample {
                timestamp_utc: "2026-11-19T14:30:00Z".into(),
                hour_local: 14,
                duration_secs: 1800,
                solar_w: 500.0,
                load_w: 2500.0,
                net_grid_w: 2000.0,
            },
        ];
        let report_rouge = compute_daily_kwh_and_cost(&samples, &rouge, 14);
        let report_bleu = compute_daily_kwh_and_cost(&samples, &bleu, 14);
        assert_eq!(report_rouge.sample_count, 2);
        assert!((report_rouge.imported_kwh - 2.0).abs() < 0.01);
        assert!((report_rouge.solar_generated_kwh - 0.5).abs() < 0.01);
        assert!(report_rouge.estimated_import_cost_eur > report_bleu.estimated_import_cost_eur);
    }

    #[test]
    fn anomaly_detector_flags_overload_low_battery_and_frozen_temp() {
        let mut hvac = HomeDevice::new(
            "pac-1",
            "PAC Salon",
            "salon",
            ProtocolKind::ModbusTcp,
            DeviceCategory::Thermostat,
            "192.168.1.40:502",
        );
        hvac.status = ConnectionStatus::PairedOnline;
        hvac.state.temperature_c = Some(2.5);
        hvac.state.battery_pct = Some(8);
        hvac.state.energy = Some(EnergyTelemetry {
            power_w: 9800.0,
            ..Default::default()
        });

        let engine = EnergyHomeostasisEngine::default();
        let anomalies = engine.detect_anomalies([&hvac]);
        assert!(
            anomalies.len() >= 2,
            "Attendu au moins surcharge + gel/batterie : {anomalies:?}"
        );
        assert!(
            anomalies
                .iter()
                .any(|a| a.severity == AnomalySeverity::Critical)
        );
    }
}
