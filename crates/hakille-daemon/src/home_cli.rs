//! Intégration REPL & IPC du Hub Domotique Souverain (`hakille-home`) dans `hakille-daemon`.
//!
//! Contrat d'honnêteté strict :
//! - Aucun appareil fictif par défaut.
//! - Toute action domotique est auditée dans `AuditJournal` (Ed25519 chaîné).
//! - Les actions Tier 3 (serrure, garage, alarme, disjoncteur, vanne, caméra) exigent
//!   un maintien physique >= 600 ms ou passent par une `ActionCard` soumise au HUD.

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use hakille_audit::journal::AuditJournal;
use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp};
use hakille_home::{
    CommandReceipt, DeviceCategory, HomeBiscuitPolicy, HomeCommand, HomeDevice, HomeDryRunPreview,
    HomeResidentRole, HomeRiskTier, PhysicalHoldProof, ProtocolKind, ResolvedHomeIntent, RoomZone,
    SovereignHomeHub, TariffPlan, TempoDayColor,
};
use hakille_kernel::{
    orchestrator::ActionCardPreview,
    planner::{ExecutionPlan, PlannedStep},
};
use hakille_ui_protocol::{HomeDeviceDto, HomeEnergyDto, HomeProtocolStatusDto};

use crate::audit_action_with_id;

/// Construit le DTO d'appareil exposé au protocole UI / HUD.
pub fn dto_from_device(d: &HomeDevice) -> HomeDeviceDto {
    let mut summary_parts = Vec::new();
    if let Some(on) = d.state.on {
        summary_parts.push(if on {
            "allumé".to_string()
        } else {
            "éteint".to_string()
        });
    }
    if let Some(lvl) = d.state.level_pct {
        summary_parts.push(format!("{lvl}%"));
    }
    if let Some(t) = d.state.temperature_c {
        summary_parts.push(format!("{t:.1}°C"));
    }
    if let Some(target) = d.state.target_temperature_c {
        summary_parts.push(format!("consigne {target:.1}°C"));
    }
    if let Some(locked) = d.state.locked {
        summary_parts.push(if locked {
            "verrouillé".to_string()
        } else {
            "déverrouillé".to_string()
        });
    }
    if let Some(pos) = d.state.position_pct {
        summary_parts.push(format!("pos {pos}%"));
    }
    if let Some(mode) = &d.state.alarm_mode {
        summary_parts.push(format!("alarme {mode}"));
    }
    let power_w = d.state.energy.as_ref().map(|e| e.power_w).unwrap_or(0.0);
    if d.state.energy.is_some() {
        summary_parts.push(format!("{power_w:.0} W"));
    }
    if summary_parts.is_empty() {
        summary_parts.push(d.status.label());
    }

    HomeDeviceDto {
        id: d.id.clone(),
        name: d.name.clone(),
        room: d.room.clone(),
        protocol: d.protocol.as_str().to_string(),
        category: d.category.as_str().to_string(),
        endpoint: d.endpoint.clone(),
        status: d.status.label(),
        summary: summary_parts.join(" · "),
        power_w,
        pose_3d: [d.pose_3d.x_m, d.pose_3d.y_m, d.pose_3d.z_m],
        is_critical: d.category.is_security_critical(),
    }
}

/// Construit le snapshot complet (appareils + bilan énergétique + catalogue 19 protocoles).
pub async fn collect_home_snapshot(
    hub: &SovereignHomeHub,
) -> (
    Vec<HomeDeviceDto>,
    HomeEnergyDto,
    Vec<HomeProtocolStatusDto>,
) {
    let devices: Vec<HomeDeviceDto> = hub
        .list_devices()
        .await
        .iter()
        .map(dto_from_device)
        .collect();
    let balance = hub.energy_balance().await;
    let dialectic = hub.energy_dialectic().await;
    let energy = HomeEnergyDto {
        solar_w: balance.solar_generation_w,
        load_w: balance.total_load_w,
        ev_w: balance.ev_charging_w,
        hvac_w: balance.hvac_load_w,
        net_grid_w: balance.net_grid_w,
        battery_soc_pct: balance.battery_soc_pct,
        self_consumption_pct: (balance.self_consumption_ratio * 100.0).round(),
        measured_devices: balance.measured_device_count,
        verdict: dialectic.arbiter_verdict,
    };
    let protocols = hub
        .protocol_catalog()
        .into_iter()
        .map(|p| HomeProtocolStatusDto {
            protocol: p.protocol.as_str().to_string(),
            display_name: p.display_name,
            transport_mode: p.transport_mode,
            default_port: p.default_port,
            honesty_note: p.honesty_note,
        })
        .collect();
    (devices, energy, protocols)
}

/// Extrait `hold_confirmed_ms` d'un payload JSON optionnel.
pub fn extract_hold_ms(params_json: &str) -> Option<u64> {
    let trimmed = params_json.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|v| v.get("hold_confirmed_ms").and_then(|m| m.as_u64()))
}

/// Construit une `ActionCardPreview` + `ExecutionPlan` quand une commande Tier 3 est demandée
/// sans maintien physique préalable de 600 ms.
pub fn build_tier3_action_card(
    preview: &HomeDryRunPreview,
    command: &HomeCommand,
) -> (ActionCardPreview, ExecutionPlan) {
    let action_name = command.action_name();
    let mut plan = ExecutionPlan::new(
        format!("home-tier3-{}-{}", preview.device_id, action_name),
        &preview.summary,
    );
    plan.estimated_risk = hakille_kernel::planner::RiskLevel::High;
    plan.requires_human_confirmation = true;
    plan.steps.push(PlannedStep {
        step_id: 1,
        description: format!(
            "Action domotique critique '{}' sur '{}' [{}] ({})",
            action_name,
            preview.device_name,
            preview.protocol.as_str(),
            preview.device_id
        ),
        required_scope: CapabilityScope::Network,
        required_action: CapabilityAction::Execute,
        is_irreversible: true,
        fallback_strategy: Some("Maintenir Entrée 600 ms pour confirmer ; R pour refuser".into()),
    });

    let card_preview = ActionCardPreview {
        verb: format!("Valider 600 ms ({action_name})"),
        object: format!("{} [{}]", preview.device_name, preview.protocol.as_str()),
        reason: "Action physique critique Tier 3 — maintien >= 600 ms requis (zéro exécution auto)"
            .to_string(),
        cost_cents: 0,
        currency: "EUR".to_string(),
        reversible: false,
        data_exit_local: true,
        provenance_label: "Hakille Home — réseau local souverain".to_string(),
    };
    (card_preview, plan)
}

/// Exécute une commande domotique autorisée avec jeton Biscuit épinglé et trace d'audit Ed25519.
pub async fn execute_home_command_with_audit(
    hub: &SovereignHomeHub,
    authority: &CapabilityAuthority,
    audit_journal: &AuditJournal,
    device_id: &str,
    command: &HomeCommand,
    hold_confirmed_ms: Option<u64>,
) -> Result<CommandReceipt, String> {
    let dev = hub
        .get_device(device_id)
        .await
        .ok_or_else(|| format!("Appareil '{device_id}' introuvable dans le registre"))?;

    let expiry = Timestamp(Utc::now() + ChronoDuration::minutes(2));
    let token = HomeBiscuitPolicy::issue_home_token(
        authority,
        vec![CapabilityScope::Network, CapabilityScope::Sensor],
        vec![CapabilityAction::Read, CapabilityAction::Execute],
        expiry,
        Some(&dev.room),
        Some(dev.protocol),
        HomeResidentRole::Owner,
    )
    .map_err(|e| format!("Émission jeton Biscuit refusée : {e}"))?;

    let policy = HomeBiscuitPolicy::new(authority.root_public_key());
    let hold_proof = hold_confirmed_ms.map(|ms| PhysicalHoldProof {
        action_id: format!("{device_id}:{}", command.action_name()),
        hold_duration_ms: ms,
        confirmed_at: Utc::now(),
    });

    // Audit préalable (fail-closed : pas d'action réseau sans journal)
    let _pre_audit = audit_action_with_id(
        audit_journal,
        "HomeCommandRequested",
        format!(
            "Commande domotique '{}' sur '{}' [{} @ {}]",
            command.action_name(),
            dev.name,
            dev.protocol.as_str(),
            dev.endpoint
        ),
        serde_json::json!({
            "device_id": dev.id,
            "protocol": dev.protocol.as_str(),
            "action": command.action_name(),
            "hold_ms": hold_confirmed_ms,
        }),
        !dev.category.is_security_critical(),
    )
    .map_err(|e| format!("Audit indisponible — exécution refusée : {e}"))?;

    match hub
        .execute_authorized(
            &policy,
            &token,
            HomeResidentRole::Owner,
            device_id,
            command,
            hold_proof.as_ref(),
        )
        .await
    {
        Ok(receipt) => {
            let _ = audit_action_with_id(
                audit_journal,
                "HomeCommandExecuted",
                format!(
                    "Exécution domotique réussie '{}' sur '{}' (sha256={})",
                    receipt.action, receipt.device_id, receipt.frame_sha256
                ),
                serde_json::json!({
                    "receipt_id": receipt.receipt_id,
                    "device_id": receipt.device_id,
                    "protocol": receipt.protocol.as_str(),
                    "risk_tier": format!("{:?}", receipt.risk_tier),
                    "executed_on_wire": receipt.executed_on_wire,
                    "frame_sha256": receipt.frame_sha256,
                }),
                receipt.risk_tier != HomeRiskTier::Tier3CriticalExecution,
            );
            Ok(receipt)
        }
        Err(err) => {
            let _ = audit_action_with_id(
                audit_journal,
                "HomeCommandFailed",
                format!(
                    "Commande domotique refusée/échouée '{}' sur '{}' : {err}",
                    command.action_name(),
                    device_id
                ),
                serde_json::json!({
                    "device_id": device_id,
                    "action": command.action_name(),
                    "error": err.to_string(),
                }),
                true,
            );
            Err(err.to_string())
        }
    }
}

/// Traite les commandes REPL `:home*`. Retourne `true` si la commande a été reconnue.
pub async fn handle_home_repl_command(
    input: &str,
    hub: &SovereignHomeHub,
    authority: &CapabilityAuthority,
    audit_journal: &AuditJournal,
    knowledge_graph: &mut hakille_memory::KnowledgeGraph,
) -> bool {
    match input {
        ":home" => {
            let devices = hub.list_devices().await;
            let rooms = hub.list_rooms().await;
            let receipts = hub.list_receipts().await;
            println!(
                "Hakille Sovereign Home Hub ({} pièce(s), {} appareil(s), {} reçu(s) crypto) :",
                rooms.len(),
                devices.len(),
                receipts.len()
            );
            if devices.is_empty() {
                println!(
                    "  Aucun appareil appairé (zéro simulation). Utilisez :home-discover ou :home-pair."
                );
            } else {
                for d in &devices {
                    let dto = dto_from_device(d);
                    println!(
                        "  • [{}] {} ({}) | {} | {} | {} | pos=({:.1},{:.1},{:.1})",
                        d.id,
                        d.name,
                        d.room,
                        d.protocol.as_str(),
                        d.endpoint,
                        dto.summary,
                        d.pose_3d.x_m,
                        d.pose_3d.y_m,
                        d.pose_3d.z_m
                    );
                }
            }
            true
        }
        ":home-protocols" => {
            let catalog = hub.protocol_catalog();
            println!(
                "Constellation Domotique Hakille — {} protocole(s) LAN supportés :",
                catalog.len()
            );
            for p in &catalog {
                let port = p
                    .default_port
                    .map(|pt| pt.to_string())
                    .unwrap_or_else(|| "série/BLE".to_string());
                println!(
                    "  • {:<16} | {:<38} | {:<18} | port {:<9} | {}",
                    p.protocol.as_str(),
                    p.display_name,
                    p.transport_mode,
                    port,
                    p.honesty_note
                );
            }
            true
        }
        s if s == ":home-discover" || s.starts_with(":home-discover ") => {
            let arg = s.strip_prefix(":home-discover").unwrap_or("").trim();
            let filter = if arg.is_empty() {
                None
            } else {
                match ProtocolKind::from_str_loose(arg) {
                    Some(p) => Some(p),
                    None => {
                        eprintln!("✗ Protocole inconnu '{arg}'. Voir :home-protocols.");
                        return true;
                    }
                }
            };
            println!(
                "Découverte LAN active (mDNS UDP 5353 / SSDP 1900 / ONVIF WS-Discovery 3702, timeout 700 ms)..."
            );
            match hub.discover_lan(filter, Duration::from_millis(700)).await {
                Ok(found) if found.is_empty() => {
                    println!(
                        "  0 appareil détecté sur le réseau local dans la fenêtre de 700 ms (aucun faux appareil inventé)."
                    );
                }
                Ok(found) => {
                    println!("  {} service(s) LAN détecté(s) :", found.len());
                    for rec in found {
                        println!(
                            "    • [{}] {} @ {} (catégorie inférée: {})",
                            rec.protocol.as_str(),
                            rec.service_name,
                            rec.endpoint,
                            rec.inferred_category.as_str()
                        );
                    }
                }
                Err(e) => eprintln!("✗ Découverte LAN échouée : {e}"),
            }
            true
        }
        s if s == ":home-room" || s.starts_with(":home-room ") => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() < 3 {
                println!("Usage : :home-room <id> <nom> [étage] [x y z]");
                return true;
            }
            let id = parts[1];
            let name = parts[2];
            let floor = parts
                .get(3)
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0);
            let x = parts
                .get(4)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            let y = parts
                .get(5)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.4);
            let z = parts
                .get(6)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            let zone = RoomZone::new(id, name, floor, [x, y, z]);
            match hub.upsert_room(zone).await {
                Ok(()) => {
                    println!("✓ Pièce '{name}' [{id}] enregistrée dans home.redb (étage {floor}).")
                }
                Err(e) => eprintln!("✗ Enregistrement pièce impossible : {e}"),
            }
            true
        }
        s if s == ":home-pair" || s.starts_with(":home-pair ") => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() < 7 {
                println!(
                    "Usage : :home-pair <id> <nom> <pièce> <protocole> <catégorie> <endpoint_lan>"
                );
                println!(
                    "  Exemple : :home-pair prise-salon \"PriseSalon\" salon mqtt switch mqtt://192.168.1.40:1883"
                );
                return true;
            }
            let id = parts[1];
            let name = parts[2].trim_matches('"');
            let room = parts[3];
            let Some(proto) = ProtocolKind::from_str_loose(parts[4]) else {
                eprintln!("✗ Protocole invalide '{}'. Voir :home-protocols.", parts[4]);
                return true;
            };
            let Some(cat) = DeviceCategory::from_str_loose(parts[5]) else {
                eprintln!(
                    "✗ Catégorie invalide '{}' (ex: light, switch, thermostat, door_lock, garage, ev_charger, solar_inverter).",
                    parts[5]
                );
                return true;
            };
            let endpoint = parts[6];
            let mut dev = HomeDevice::new(id, name, room, proto, cat, endpoint);
            dev.paired_at = Some(Utc::now().to_rfc3339());
            match hub.register_device(dev).await {
                Ok(()) => {
                    let _ = audit_action_with_id(
                        audit_journal,
                        "HomeDevicePaired",
                        format!(
                            "Appareil '{name}' [{id}] appairé ({}/{endpoint})",
                            proto.as_str()
                        ),
                        serde_json::json!({
                            "device_id": id,
                            "protocol": proto.as_str(),
                            "category": cat.as_str(),
                            "endpoint": endpoint,
                        }),
                        true,
                    );
                    println!(
                        "✓ Appareil '{name}' [{id}] validé par HomeLanGuard et persisté dans home.redb."
                    );
                }
                Err(e) => eprintln!("✗ Appairage refusé : {e}"),
            }
            true
        }
        s if s == ":home-cmd" || s.starts_with(":home-cmd ") => {
            let mut hold_ms: Option<u64> = None;
            let cleaned = if let Some(stripped) = s.strip_suffix("--hold-600ms") {
                hold_ms = Some(600);
                stripped.trim()
            } else {
                s.trim()
            };
            let mut iter = cleaned.splitn(4, char::is_whitespace);
            let _cmd = iter.next();
            let device_id = iter.next().unwrap_or("").trim();
            let action = iter.next().unwrap_or("").trim();
            let params_json = iter.next().map(str::trim).filter(|p| !p.is_empty());

            if device_id.is_empty() || action.is_empty() {
                println!("Usage : :home-cmd <device_id> <action> [json_params] [--hold-600ms]");
                return true;
            }
            if hold_ms.is_none()
                && let Some(raw_json) = params_json
            {
                hold_ms = extract_hold_ms(raw_json);
            }

            let command = match HomeCommand::parse_action(action, params_json) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("✗ {e}");
                    return true;
                }
            };
            if let Ok(preview) = hub.preview_command(device_id, &command).await {
                println!("Aperçu sec : {}", preview.summary);
                if preview.requires_600ms_hold && hold_ms.unwrap_or(0) < 600 {
                    eprintln!(
                        "✗ Confirmation physique requise (>= 600 ms) pour '{}'. Relancez avec --hold-600ms ou validez dans le HUD.",
                        command.action_name()
                    );
                    return true;
                }
            }
            match execute_home_command_with_audit(
                hub,
                authority,
                audit_journal,
                device_id,
                &command,
                hold_ms,
            )
            .await
            {
                Ok(receipt) => {
                    println!(
                        "✓ Reçu [{}] ({:?}) — wire={} — sha256={}",
                        receipt.receipt_id,
                        receipt.risk_tier,
                        receipt.executed_on_wire,
                        receipt.frame_sha256
                    );
                    println!("  {}", receipt.summary);
                }
                Err(e) => eprintln!("✗ Échec exécution domotique : {e}"),
            }
            true
        }
        ":home-energy" => {
            let bal = hub.energy_balance().await;
            let report = hub.energy_dialectic().await;
            println!(
                "Homéostasie Énergétique ({} sonde(s) réelle(s)) :",
                bal.measured_device_count
            );
            println!("  • Production solaire : {:.1} W", bal.solar_generation_w);
            println!("  • Charge foyer       : {:.1} W", bal.total_load_w);
            println!("  • Recharge VE (OCPP) : {:.1} W", bal.ev_charging_w);
            println!("  • Chauffage/HVAC     : {:.1} W", bal.hvac_load_w);
            println!(
                "  • Bilan réseau       : {:.1} W (+import / -surplus)",
                bal.net_grid_w
            );
            if let Some(soc) = bal.battery_soc_pct {
                println!("  • Batterie SoC       : {:.1}%", soc);
            }
            println!(
                "  • Autoconsommation   : {:.1}%",
                bal.self_consumption_ratio * 100.0
            );
            println!("  [Catalyseur] {}", report.catalyst_thesis);
            println!("  [Avocat Diable] {}", report.devils_advocate_critique);
            println!(
                "  [Verdict] {} (marge {:.1}%)",
                report.arbiter_verdict, report.safety_margin_pct
            );
            true
        }
        ":home-twin" => {
            let inserted = hub.project_to_memory(knowledge_graph).await;
            let palace = hub.export_palace_3d().await;
            println!(
                "Jumeau Numérique 3D : {} triplet(s) projeté(s) vers KnowledgeGraph ; PalaceLayout : {} nœud(s), {} arête(s).",
                inserted,
                palace.nodes.len(),
                palace.edges.len()
            );
            true
        }
        ":home-scenes" => {
            let scenes = hub.list_scenes().await;
            println!("Scènes domotiques persistées ({}) :", scenes.len());
            for sc in scenes {
                println!(
                    "  • [{}] {} ({} étape(s)) — {}",
                    sc.id,
                    sc.name,
                    sc.steps.len(),
                    sc.description
                );
            }
            true
        }
        s if s == ":home-ask" || s.starts_with(":home-ask ") => {
            let raw_arg = s.strip_prefix(":home-ask").unwrap_or("").trim();
            let (phrase, exec_flag) = if let Some(stripped) = raw_arg.strip_suffix("--exec") {
                (stripped.trim(), true)
            } else {
                (raw_arg, false)
            };
            if phrase.is_empty() {
                println!(
                    "Usage : :home-ask <phrase FR/EN> [--exec] (ex: :home-ask Allume la lampe du salon)"
                );
                return true;
            }
            match hub.resolve_natural_intent(phrase).await {
                ResolvedHomeIntent::DeviceCommand {
                    device_id,
                    device_name,
                    room,
                    command,
                    risk_tier,
                    requires_600ms_hold,
                    confidence,
                    explanation,
                } => {
                    println!(
                        "Intention résolue hors-ligne (confiance {:.0}%) -> [{device_id}] {device_name} ({room}) | action={} | {risk_tier}",
                        confidence * 100.0,
                        command.action_name()
                    );
                    println!("  Explication : {explanation}");
                    if requires_600ms_hold {
                        println!(
                            "  ⚠ Action physique critique Tier 3 : maintien >= 600 ms obligatoire (exécutez :home-cmd {device_id} {} --hold-600ms ou validez dans le HUD).",
                            command.action_name()
                        );
                    } else if exec_flag {
                        match execute_home_command_with_audit(
                            hub,
                            authority,
                            audit_journal,
                            &device_id,
                            &command,
                            None,
                        )
                        .await
                        {
                            Ok(receipt) => println!(
                                "✓ Reçu [{}] — wire={} — sha256={} — {}",
                                receipt.receipt_id,
                                receipt.executed_on_wire,
                                receipt.frame_sha256,
                                receipt.summary
                            ),
                            Err(e) => eprintln!("✗ Échec exécution : {e}"),
                        }
                    } else {
                        println!("  (Aperçu sec — ajoutez --exec pour exécuter sur le réseau LAN)");
                    }
                }
                ResolvedHomeIntent::RoomCommand {
                    room_id,
                    device_ids,
                    command,
                    max_risk_tier,
                    requires_600ms_hold,
                    explanation,
                } => {
                    println!(
                        "Intention pièce '{room_id}' ({} appareil(s) : {:?}) | action={} | {max_risk_tier}",
                        device_ids.len(),
                        device_ids,
                        command.action_name()
                    );
                    println!("  Explication : {explanation}");
                    if requires_600ms_hold {
                        println!(
                            "  ⚠ Au moins un appareil cible exige une confirmation physique >= 600 ms (exécution groupée bloquée)."
                        );
                    } else if exec_flag {
                        for id in &device_ids {
                            match execute_home_command_with_audit(
                                hub,
                                authority,
                                audit_journal,
                                id,
                                &command,
                                None,
                            )
                            .await
                            {
                                Ok(r) => println!(
                                    "  ✓ [{id}] wire={} sha256={}",
                                    r.executed_on_wire, r.frame_sha256
                                ),
                                Err(e) => eprintln!("  ✗ [{id}] {e}"),
                            }
                        }
                    } else {
                        println!("  (Aperçu sec — ajoutez --exec pour exécuter sur le réseau LAN)");
                    }
                }
                ResolvedHomeIntent::QueryEnergy => {
                    let bal = hub.energy_balance().await;
                    println!(
                        "Bilan énergétique instantané ({} sonde(s)) : solaire {:.1} W | charge {:.1} W | réseau {:.1} W",
                        bal.measured_device_count,
                        bal.solar_generation_w,
                        bal.total_load_w,
                        bal.net_grid_w
                    );
                }
                ResolvedHomeIntent::QueryStatus => {
                    let devices = hub.list_devices().await;
                    let rooms = hub.list_rooms().await;
                    println!(
                        "Statut maison : {} pièce(s), {} appareil(s) LAN enregistrés (zéro simulation).",
                        rooms.len(),
                        devices.len()
                    );
                }
                ResolvedHomeIntent::Unmatched { reason } => {
                    println!("✗ Aucune cible réelle résolue (zéro hallucination) : {reason}");
                }
            }
            true
        }
        ":home-rules" => {
            let rules = hub.list_automation_rules().await;
            println!(
                "Automatisations domotiques persistées ({}) — invariant Tier 3 : toujours escaladé :",
                rules.len()
            );
            if rules.is_empty() {
                println!("  Aucune règle enregistrée dans home.redb.");
            } else {
                for r in &rules {
                    println!(
                        "  • [{}] {} | actif={} | cooldown={} s | {} action(s)",
                        r.id,
                        r.name,
                        r.enabled,
                        r.cooldown_secs,
                        r.actions.len()
                    );
                }
            }
            true
        }
        ":home-rules-eval" => {
            let report = hub.evaluate_automations().await;
            println!(
                "Évaluation des automatisations ({} règle(s) évaluée(s), {} déclenchée(s), {} en cooldown) :",
                report.evaluated_rules,
                report.triggered_rule_ids.len(),
                report.skipped_cooldown.len()
            );
            println!(
                "  • Actions automatiques Tier 0-2 planifiées : {}",
                report.auto_dispatches.len()
            );
            for d in &report.auto_dispatches {
                println!(
                    "    -> [{}] règle '{}' : {} ({:?})",
                    d.device_id,
                    d.rule_id,
                    d.command.action_name(),
                    d.risk_tier
                );
            }
            println!(
                "  • Escalades Tier 3 obligatoires (maintien 600 ms requis) : {}",
                report.escalations.len()
            );
            for esc in &report.escalations {
                println!(
                    "    ⚠ [{}] règle '{}' : {} — {}",
                    esc.device_id,
                    esc.rule_id,
                    esc.command.action_name(),
                    esc.reason
                );
            }
            true
        }
        s if s == ":home-tariffs" || s.starts_with(":home-tariffs ") => {
            let arg = s
                .strip_prefix(":home-tariffs")
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            match arg.as_str() {
                "" => {}
                "bleu" | "blue" => {
                    hub.set_tariff_plan(TariffPlan::Tempo {
                        day_color: TempoDayColor::Bleu,
                    })
                    .await;
                }
                "blanc" | "white" => {
                    hub.set_tariff_plan(TariffPlan::Tempo {
                        day_color: TempoDayColor::Blanc,
                    })
                    .await;
                }
                "rouge" | "red" => {
                    hub.set_tariff_plan(TariffPlan::Tempo {
                        day_color: TempoDayColor::Rouge,
                    })
                    .await;
                }
                "hphc" => {
                    hub.set_tariff_plan(TariffPlan::PeakOffPeak {
                        peak_eur_per_kwh: 0.2700,
                        off_peak_eur_per_kwh: 0.2068,
                        off_peak_start_hour: 22,
                        off_peak_end_hour: 6,
                    })
                    .await;
                }
                other => {
                    eprintln!("✗ Tarif inconnu '{other}'. Utilisez : bleu | blanc | rouge | hphc.");
                    return true;
                }
            }
            let daily = hub.daily_energy_report().await;
            println!("Grille tarifaire active : {}", daily.tariff_label);
            println!(
                "  • Prix heure courante : {:.4} €/kWh",
                daily.current_hour_price_eur_kwh
            );
            println!(
                "  • Échantillons réels  : {} (import {:.3} kWh, export {:.3} kWh, solaire {:.3} kWh)",
                daily.sample_count,
                daily.imported_kwh,
                daily.exported_kwh,
                daily.solar_generated_kwh
            );
            println!(
                "  • Coût import estimé  : {:.3} €",
                daily.estimated_import_cost_eur
            );
            println!("  • Conseil             : {}", daily.recommendation);
            true
        }
        ":home-anomalies" => {
            let anomalies = hub.detect_anomalies().await;
            if anomalies.is_empty() {
                println!("✓ Aucune anomalie détectée sur les sondes réelles du foyer.");
            } else {
                println!("⚠ {} anomalie(s) détectée(s) :", anomalies.len());
                for a in &anomalies {
                    println!(
                        "  • [{:?}] {} ({}) : {}",
                        a.severity, a.device_id, a.code, a.message
                    );
                }
            }
            true
        }
        ":home-mesh-sync" => {
            let delta = hub.export_mesh_delta("hakille-local-hub", 1).await;
            println!(
                "Delta Jumeau Maillé P2P (hakille-mesh AEAD ChaCha20-Poly1305 + Ed25519, Air-Gap/Link-Local uniquement) :"
            );
            println!("  • Nœud origine   : {}", delta.origin_node_id);
            println!("  • Séquence       : {}", delta.sequence);
            println!("  • Pièces         : {}", delta.rooms.len());
            println!("  • Appareils LAN  : {}", delta.devices.len());
            println!("  • Scènes         : {}", delta.scenes.len());
            println!("  • Automatisations: {}", delta.automations.len());
            println!("  • Horodatage UTC : {}", delta.generated_at);
            true
        }
        _ => false,
    }
}
