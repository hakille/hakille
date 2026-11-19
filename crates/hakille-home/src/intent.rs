//! Résolveur d'intention domotique hors-ligne (FR / EN) déterministe et sans hallucination.
//!
//! Permet de traduire une phrase naturelle (« allume la lampe du salon »,
//! « règle le thermostat à 20.5°C », « déverrouille la porte d'entrée »,
//! « quel est le bilan énergétique ? ») en intention structurée vérifiée
//! uniquement contre les appareils **réellement enregistrés** dans `SovereignHomeHub`.

use crate::model::{DeviceCategory, HomeCommand, HomeDevice, HomeRiskTier, RoomZone};
use crate::security::HomeRiskClassifier;
use serde::{Deserialize, Serialize};

/// Intention domotique résolue à partir d'une phrase utilisateur FR/EN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedHomeIntent {
    /// Commande ciblant un appareil réellement présent dans le hub.
    DeviceCommand {
        device_id: String,
        device_name: String,
        room: String,
        command: HomeCommand,
        risk_tier: HomeRiskTier,
        requires_600ms_hold: bool,
        confidence: f32,
        explanation: String,
    },
    /// Commande groupée sur tous les appareils pertinents d'une pièce enregistrée.
    RoomCommand {
        room_id: String,
        device_ids: Vec<String>,
        command: HomeCommand,
        max_risk_tier: HomeRiskTier,
        requires_600ms_hold: bool,
        explanation: String,
    },
    /// Demande de bilan énergétique réel (zéro effet physique).
    QueryEnergy,
    /// Demande d'inventaire / statut général de la maison (zéro effet physique).
    QueryStatus,
    /// Aucun appareil ou pièce réel ne correspond (zéro hallucination).
    Unmatched { reason: String },
}

/// Normalise une chaîne (minuscules + repli simple des accents FR les plus fréquents).
fn normalize_fr(input: &str) -> String {
    input
        .to_lowercase()
        .replace(['é', 'è', 'ê', 'ë'], "e")
        .replace(['à', 'â', 'ä'], "a")
        .replace(['î', 'ï'], "i")
        .replace(['ô', 'ö'], "o")
        .replace(['ù', 'û', 'ü'], "u")
        .replace('ç', "c")
        .replace(['\'', '’', '?', '!', ',', ';'], " ")
}

/// Extrait le premier nombre flottant positif d'un texte normalisé.
fn extract_first_number(norm: &str) -> Option<f32> {
    for token in norm.split_whitespace() {
        let cleaned = token
            .trim_end_matches("°c")
            .trim_end_matches("c")
            .trim_end_matches('%')
            .trim_end_matches('a')
            .trim_end_matches("degres")
            .trim_end_matches("degre")
            .replace(',', ".");
        if let Ok(v) = cleaned.parse::<f32>()
            && v.is_finite()
        {
            return Some(v);
        }
    }
    None
}

/// Déduit la commande domotique et éventuellement la catégorie cible implicite.
fn parse_command_and_category(norm: &str) -> Option<(HomeCommand, Option<DeviceCategory>)> {
    // Actions Tier 3 critiques (priorité maximale pour ne jamais sous-classifier)
    if norm.contains("deverrouill") || norm.contains("unlock") || norm.contains("ouvre la serrure")
    {
        return Some((HomeCommand::UnlockDoor, Some(DeviceCategory::DoorLock)));
    }
    if norm.contains("verrouill") || norm.contains("lock") || norm.contains("ferme a cle") {
        return Some((HomeCommand::LockDoor, Some(DeviceCategory::DoorLock)));
    }
    if norm.contains("desarm") || norm.contains("disarm") || norm.contains("coupe l alarme") {
        return Some((
            HomeCommand::SetAlarm {
                mode: "disarmed".into(),
                pin_hash: None,
            },
            Some(DeviceCategory::AlarmPanel),
        ));
    }
    if norm.contains("arm") && (norm.contains("alarme") || norm.contains("alarm")) {
        return Some((
            HomeCommand::SetAlarm {
                mode: if norm.contains("nuit") || norm.contains("night") {
                    "armed_home".into()
                } else {
                    "armed_away".into()
                },
                pin_hash: None,
            },
            Some(DeviceCategory::AlarmPanel),
        ));
    }
    if norm.contains("disjoncteur") || norm.contains("breaker") {
        let closed = norm.contains("ferme") || norm.contains("rearme") || norm.contains("on");
        return Some((
            HomeCommand::SetMainBreaker { closed },
            Some(DeviceCategory::MainBreaker),
        ));
    }
    if norm.contains("vanne") || norm.contains("eau") || norm.contains("water valve") {
        let open = norm.contains("ouvre") || norm.contains("open");
        return Some((
            HomeCommand::SetWaterValve { open },
            Some(DeviceCategory::WaterValve),
        ));
    }

    // Borne véhicule électrique (OCPP)
    if norm.contains("voiture")
        || norm.contains("vehicule")
        || norm.contains("borne")
        || norm.contains("wallbox")
        || norm.contains("ev ")
        || norm.ends_with(" ev")
    {
        let stop = norm.contains("arret")
            || norm.contains("stop")
            || norm.contains("coupe")
            || norm.contains("pause");
        let amps = extract_first_number(norm).map(|a| a.clamp(6.0, 32.0));
        return Some((
            HomeCommand::SetEvCharging {
                enabled: !stop,
                max_amps: if stop { Some(0.0) } else { amps },
            },
            Some(DeviceCategory::EvCharger),
        ));
    }

    // Thermostat / température
    if (norm.contains("thermostat")
        || norm.contains("chauffage")
        || norm.contains("temperature")
        || norm.contains("degre")
        || norm.contains("°c"))
        && let Some(temp) = extract_first_number(norm)
    {
        return Some((
            HomeCommand::SetTemperature {
                celsius: temp.clamp(5.0, 35.0),
            },
            Some(DeviceCategory::Thermostat),
        ));
    }

    // Volets / stores
    if norm.contains("volet")
        || norm.contains("store")
        || norm.contains("blind")
        || norm.contains("cover")
    {
        let pos = if let Some(n) = extract_first_number(norm) {
            n.round().clamp(0.0, 100.0) as u8
        } else if norm.contains("ferme") || norm.contains("close") || norm.contains("baisse") {
            0
        } else {
            100
        };
        return Some((
            HomeCommand::SetCoverPosition { position_pct: pos },
            Some(DeviceCategory::CoverBlind),
        ));
    }

    // Intensité (%)
    if (norm.contains('%')
        || norm.contains("intensite")
        || norm.contains("luminosite")
        || norm.contains("dim "))
        && let Some(pct) = extract_first_number(norm)
    {
        return Some((
            HomeCommand::SetLevel {
                level_pct: pct.round().clamp(0.0, 100.0) as u8,
            },
            Some(DeviceCategory::Light),
        ));
    }

    // Allumer / Éteindre / Basculer
    if norm.contains("allume")
        || norm.contains("turn on")
        || norm.contains("switch on")
        || norm.contains("active ")
    {
        return Some((
            HomeCommand::SetPower { on: true },
            infer_hint_category(norm),
        ));
    }
    if norm.contains("etein")
        || norm.contains("coupe")
        || norm.contains("turn off")
        || norm.contains("switch off")
        || norm.contains("desactive")
    {
        return Some((
            HomeCommand::SetPower { on: false },
            infer_hint_category(norm),
        ));
    }
    if norm.contains("bascule") || norm.contains("toggle") {
        return Some((HomeCommand::Toggle, infer_hint_category(norm)));
    }

    None
}

fn infer_hint_category(norm: &str) -> Option<DeviceCategory> {
    if norm.contains("lampe") || norm.contains("lumiere") || norm.contains("light") {
        Some(DeviceCategory::Light)
    } else if norm.contains("prise") || norm.contains("plug") || norm.contains("switch") {
        Some(DeviceCategory::Switch)
    } else if norm.contains("clim") || norm.contains("hvac") {
        Some(DeviceCategory::Hvac)
    } else {
        None
    }
}

/// Résout une phrase naturelle FR/EN contre les appareils et pièces **réellement** enregistrés.
pub fn resolve_home_intent(
    utterance: &str,
    devices: &[HomeDevice],
    rooms: &[RoomZone],
) -> ResolvedHomeIntent {
    let norm = normalize_fr(utterance);

    if norm.contains("energie")
        || norm.contains("energy")
        || norm.contains("solaire")
        || norm.contains("solar")
        || norm.contains("tempo")
        || norm.contains("consommation")
        || norm.contains("bilan")
    {
        return ResolvedHomeIntent::QueryEnergy;
    }

    if (norm.contains("statut") || norm.contains("status") || norm.contains("liste"))
        && (norm.contains("maison") || norm.contains("home") || norm.contains("appareil"))
    {
        return ResolvedHomeIntent::QueryStatus;
    }

    let Some((command, cat_hint)) = parse_command_and_category(&norm) else {
        return ResolvedHomeIntent::Unmatched {
            reason: format!(
                "Aucune intention domotique reconnue dans '{}'",
                utterance.trim()
            ),
        };
    };

    if devices.is_empty() {
        return ResolvedHomeIntent::Unmatched {
            reason: "Aucun appareil réel n'est enregistré dans le hub (zéro simulation).".into(),
        };
    }

    // Détection d'une pièce explicitement mentionnée
    let matched_room: Option<String> = rooms
        .iter()
        .find(|r| {
            let rid = normalize_fr(&r.id);
            let rname = normalize_fr(&r.name);
            (!rid.is_empty() && norm.contains(&rid)) || (!rname.is_empty() && norm.contains(&rname))
        })
        .map(|r| r.id.clone())
        .or_else(|| {
            devices.iter().map(|d| d.room.clone()).find(|room| {
                let r = normalize_fr(room);
                !r.is_empty() && norm.contains(&r)
            })
        });

    // Commande groupée sur une pièce ("éteins tout dans le salon")
    if (norm.contains("tout") || norm.contains("toutes") || norm.contains("all "))
        && let Some(room_id) = &matched_room
    {
        let targets: Vec<&HomeDevice> = devices
            .iter()
            .filter(|d| d.room.eq_ignore_ascii_case(room_id))
            .filter(|d| {
                cat_hint.is_none_or(|c| {
                    d.category == c
                        || (c == DeviceCategory::Light
                            && matches!(
                                d.category,
                                DeviceCategory::Light
                                    | DeviceCategory::Dimmer
                                    | DeviceCategory::ColorLight
                            ))
                })
            })
            .collect();
        if !targets.is_empty() {
            let max_risk = targets
                .iter()
                .map(|d| HomeRiskClassifier::classify(d.category, &command))
                .max()
                .unwrap_or(HomeRiskTier::Tier0ReadOnly);
            return ResolvedHomeIntent::RoomCommand {
                room_id: room_id.clone(),
                device_ids: targets.iter().map(|d| d.id.clone()).collect(),
                command,
                max_risk_tier: max_risk,
                requires_600ms_hold: max_risk.requires_physical_hold(),
                explanation: format!(
                    "Commande de pièce '{}' ciblant {} appareil(s) réel(s)",
                    room_id,
                    targets.len()
                ),
            };
        }
    }

    // Score déterministe de chaque appareil réel
    let mut best: Option<(&HomeDevice, f32)> = None;
    for dev in devices {
        let mut score = 0.0f32;
        let id_norm = normalize_fr(&dev.id);
        let name_norm = normalize_fr(&dev.name);
        let room_norm = normalize_fr(&dev.room);

        if norm.contains(&id_norm) {
            score += 5.0;
        }
        if norm.contains(&name_norm) {
            score += 4.0;
        } else {
            for word in name_norm.split_whitespace().filter(|w| w.len() >= 3) {
                if norm.contains(word) {
                    score += 1.2;
                }
            }
        }
        if let Some(ref r) = matched_room
            && dev.room.eq_ignore_ascii_case(r)
        {
            score += 2.0;
        } else if !room_norm.is_empty() && norm.contains(&room_norm) {
            score += 1.5;
        }
        if let Some(hint) = cat_hint {
            let cat_ok = dev.category == hint
                || (hint == DeviceCategory::Light
                    && matches!(
                        dev.category,
                        DeviceCategory::Light | DeviceCategory::Dimmer | DeviceCategory::ColorLight
                    ));
            if cat_ok {
                score += 2.5;
            }
        }

        if score >= 2.0 && best.is_none_or(|(_, s)| score > s) {
            best = Some((dev, score));
        }
    }

    if let Some((dev, score)) = best {
        let risk_tier = HomeRiskClassifier::classify(dev.category, &command);
        let confidence = (score / 7.0).clamp(0.5, 0.99);
        ResolvedHomeIntent::DeviceCommand {
            device_id: dev.id.clone(),
            device_name: dev.name.clone(),
            room: dev.room.clone(),
            command,
            risk_tier,
            requires_600ms_hold: risk_tier.requires_physical_hold(),
            confidence,
            explanation: format!(
                "Résolu vers '{}' [{}] en pièce '{}' (risque {:?})",
                dev.name, dev.id, dev.room, risk_tier
            ),
        }
    } else {
        ResolvedHomeIntent::Unmatched {
            reason: format!(
                "Aucun appareil réel enregistré ne correspond à '{}'",
                utterance.trim()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProtocolKind;

    fn sample_devices() -> (Vec<HomeDevice>, Vec<RoomZone>) {
        let rooms = vec![
            RoomZone::new("salon", "Salon", 0, [0.0, 0.0, 0.0]),
            RoomZone::new("entree", "Entrée", 0, [4.0, 0.0, 0.0]),
        ];
        let devices = vec![
            HomeDevice::new(
                "lampe-salon",
                "Lampe Salon",
                "salon",
                ProtocolKind::Matter,
                DeviceCategory::ColorLight,
                "192.168.1.20:5540",
            ),
            HomeDevice::new(
                "thermo-salon",
                "Thermostat Salon",
                "salon",
                ProtocolKind::KnxIp,
                DeviceCategory::Thermostat,
                "192.168.1.30:3671",
            ),
            HomeDevice::new(
                "serrure-entree",
                "Serrure Entrée",
                "entree",
                ProtocolKind::Mqtt,
                DeviceCategory::DoorLock,
                "mqtt://192.168.1.10:1883",
            ),
        ];
        (devices, rooms)
    }

    #[test]
    fn resolves_french_lighting_thermostat_tier3_lock_and_rejects_empty_hub() {
        let (devices, rooms) = sample_devices();

        match resolve_home_intent("Allume la lampe du salon", &devices, &rooms) {
            ResolvedHomeIntent::DeviceCommand {
                device_id,
                command,
                requires_600ms_hold,
                ..
            } => {
                assert_eq!(device_id, "lampe-salon");
                assert_eq!(command, HomeCommand::SetPower { on: true });
                assert!(!requires_600ms_hold);
            }
            other => panic!("Attendu DeviceCommand lampe-salon, reçu {other:?}"),
        }

        match resolve_home_intent(
            "Règle le thermostat du salon à 20.5 degrés",
            &devices,
            &rooms,
        ) {
            ResolvedHomeIntent::DeviceCommand {
                device_id, command, ..
            } => {
                assert_eq!(device_id, "thermo-salon");
                assert_eq!(command, HomeCommand::SetTemperature { celsius: 20.5 });
            }
            other => panic!("Attendu thermostat, reçu {other:?}"),
        }

        match resolve_home_intent("Déverrouille la serrure d'entrée", &devices, &rooms) {
            ResolvedHomeIntent::DeviceCommand {
                device_id,
                command,
                risk_tier,
                requires_600ms_hold,
                ..
            } => {
                assert_eq!(device_id, "serrure-entree");
                assert_eq!(command, HomeCommand::UnlockDoor);
                assert_eq!(risk_tier, HomeRiskTier::Tier3CriticalExecution);
                assert!(requires_600ms_hold);
            }
            other => panic!("Attendu serrure Tier 3, reçu {other:?}"),
        }

        // Zéro hallucination si aucun appareil enregistré
        assert!(matches!(
            resolve_home_intent("Allume la lampe du salon", &[], &[]),
            ResolvedHomeIntent::Unmatched { .. }
        ));
    }
}
