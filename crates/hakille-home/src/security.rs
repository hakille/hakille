//! Sécurité souveraine domotique : `HomeLanGuard`, classification des risques et Biscuit v5.
//!
//! Principes fail-closed :
//! 1. `HomeLanGuard` n'autorise QUE les réseaux locaux (RFC 1918, ULA `fc00::/7`, link-local
//!    hors métadonnées cloud `169.254.169.254`, multicast IoT explicite, loopback pour ponts
//!    locaux, et ports série locaux `/dev/tty*`/`COM*` sans traversée `..`).
//!    Tout hôte public / WAN est bloqué : la maison ne dépend jamais du cloud.
//! 2. `HomeRiskClassifier` élève automatiquement en `Tier3CriticalExecution` tout déverrouillage
//!    de serrure, ouverture de garage, désarmement d'alarme, coupure d'eau/disjoncteur ou flux caméra.
//! 3. `HomeBiscuitPolicy` atténue et vérifie des jetons Biscuit v5 épinglés avec faits
//!    `home_room`, `home_device`, `home_protocol`, `home_risk` et profil `Guest`/`Minor`.

use crate::model::{
    DeviceCategory, HomeCommand, HomeDevice, HomeError, HomeResult, HomeRiskTier, ProtocolKind,
};
use biscuit_auth::{AuthorizerLimits, Biscuit, KeyPair, PublicKey};
use chrono::{DateTime, Utc};
use hakille_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;

/// Profil résident d'autorisation domotique (constitutionnel : protection mineurs & invités).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HomeResidentRole {
    Owner,
    AdultResident,
    Guest,
    Minor,
}

impl HomeResidentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::AdultResident => "adult_resident",
            Self::Guest => "guest",
            Self::Minor => "minor",
        }
    }
}

/// Garde réseau local souverain pour la domotique.
///
/// Contrairement à `AntiSsrfValidator` (destiné au web public et qui bloque les IPs privées),
/// `HomeLanGuard` bloque **toutes les IPs publiques/WAN** ainsi que les endpoints de métadonnées
/// cloud (`169.254.169.254`, `fd00:ec2::254`, etc.) et les ports série invalides.
#[derive(Debug, Clone)]
pub struct HomeLanGuard {
    pub allow_loopback: bool,
    pub allowed_multicast_v4: Vec<Ipv4Addr>,
}

impl Default for HomeLanGuard {
    fn default() -> Self {
        Self {
            allow_loopback: true,
            allowed_multicast_v4: vec![
                Ipv4Addr::new(224, 0, 0, 251),     // mDNS
                Ipv4Addr::new(239, 255, 255, 250), // SSDP / ONVIF WS-Discovery
                Ipv4Addr::new(224, 0, 23, 12),     // KNXnet/IP Routing
                Ipv4Addr::new(224, 0, 1, 187),     // CoAP All-Nodes
            ],
        }
    }
}

impl HomeLanGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Valide une adresse IP locale ou multicast IoT autorisée.
    pub fn validate_ip(&self, ip: IpAddr) -> HomeResult<()> {
        match ip {
            IpAddr::V4(v4) => self.validate_ipv4(v4),
            IpAddr::V6(v6) => self.validate_ipv6(v6),
        }
    }

    fn validate_ipv4(&self, ip: Ipv4Addr) -> HomeResult<()> {
        let ep = ip.to_string();
        // Bloquer explicitement l'endpoint de métadonnées cloud (AWS/GCP/Azure/OpenStack)
        if ip == Ipv4Addr::new(169, 254, 169, 254) {
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "IP de métadonnées cloud strictement interdite".to_string(),
            });
        }
        if ip.is_unspecified() {
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "Adresse 0.0.0.0 non routable refusée".to_string(),
            });
        }
        if ip.is_loopback() {
            if self.allow_loopback {
                return Ok(());
            }
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "Loopback désactivé par configuration".to_string(),
            });
        }
        if ip.is_private() || ip.is_link_local() {
            return Ok(());
        }
        if ip.is_multicast() && self.allowed_multicast_v4.contains(&ip) {
            return Ok(());
        }
        Err(HomeError::LanGuardBlocked {
            endpoint: ep,
            reason: "Adresse WAN/publique ou multicast non IoT refusée (zéro-cloud souverain)"
                .to_string(),
        })
    }

    fn validate_ipv6(&self, ip: Ipv6Addr) -> HomeResult<()> {
        let ep = ip.to_string();
        let segs = ip.segments();
        // Bloquer fd00:ec2::254 (métadonnées AWS IPv6)
        if segs == [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254] {
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "Endpoint métadonnées cloud IPv6 interdit".to_string(),
            });
        }
        if ip.is_unspecified() {
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "Adresse :: non spécifiée refusée".to_string(),
            });
        }
        if ip.is_loopback() {
            if self.allow_loopback {
                return Ok(());
            }
            return Err(HomeError::LanGuardBlocked {
                endpoint: ep,
                reason: "Loopback IPv6 désactivé".to_string(),
            });
        }
        // RFC 4193 ULA (fc00::/7) ou Link-Local (fe80::/10)
        let is_ula = (segs[0] & 0xfe00) == 0xfc00;
        let is_link_local = (segs[0] & 0xffc0) == 0xfe80;
        // Multicast link-local IoT (ff02::/16, ex: ff02::fb mDNS, ff02::1 Thread/CoAP)
        let is_iot_multicast = (segs[0] & 0xff0f) == 0xff02;
        if is_ula || is_link_local || is_iot_multicast {
            return Ok(());
        }
        Err(HomeError::LanGuardBlocked {
            endpoint: ep,
            reason: "Adresse IPv6 globale/WAN refusée (zéro-cloud souverain)".to_string(),
        })
    }

    /// Valide un chemin de port série local (Zigbee, Z-Wave, Modbus RTU).
    pub fn validate_serial_path(&self, raw_path: &str) -> HomeResult<()> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() || trimmed.contains("..") || trimmed.contains('\0') {
            return Err(HomeError::LanGuardBlocked {
                endpoint: raw_path.to_string(),
                reason: "Chemin série invalide ou traversée de répertoire détectée".to_string(),
            });
        }
        let allowed_unix = trimmed.starts_with("/dev/ttyUSB")
            || trimmed.starts_with("/dev/ttyACM")
            || trimmed.starts_with("/dev/serial/by-id/")
            || trimmed.starts_with("/dev/cu.");
        let allowed_windows = trimmed
            .strip_prefix("COM")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));
        if allowed_unix || allowed_windows {
            // Vérification additionnelle contre tout composant caché ou relatif
            let path = Path::new(trimmed);
            if path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(HomeError::LanGuardBlocked {
                    endpoint: raw_path.to_string(),
                    reason: "Composant '..' interdit dans le port série".to_string(),
                });
            }
            return Ok(());
        }
        Err(HomeError::LanGuardBlocked {
            endpoint: raw_path.to_string(),
            reason:
                "Port série hors périmètre (/dev/ttyUSB*, /dev/ttyACM*, /dev/serial/by-id/*, COM*)"
                    .to_string(),
        })
    }

    /// Valide un endpoint complet (URL `http://`/`ws://`/`mqtt://`/`coap://`, `host:port`,
    /// adresse IP brute, adresse BLE MAC `ble://AA:BB:CC:DD:EE:FF`, ou chemin série).
    pub fn validate_endpoint(&self, endpoint: &str) -> HomeResult<()> {
        let trimmed = endpoint.trim();
        if trimmed.is_empty() {
            return Err(HomeError::LanGuardBlocked {
                endpoint: endpoint.to_string(),
                reason: "Endpoint vide".to_string(),
            });
        }

        if trimmed.starts_with("/dev/") || trimmed.to_ascii_uppercase().starts_with("COM") {
            return self.validate_serial_path(trimmed);
        }

        if let Some(mac) = trimmed.strip_prefix("ble://") {
            let valid_mac = mac.len() == 17
                && mac.split(':').count() == 6
                && mac
                    .split(':')
                    .all(|b| b.len() == 2 && b.chars().all(|c| c.is_ascii_hexdigit()));
            if valid_mac {
                return Ok(());
            }
            return Err(HomeError::LanGuardBlocked {
                endpoint: endpoint.to_string(),
                reason: "Adresse MAC BLE invalide".to_string(),
            });
        }

        // Extraire la partie autorité d'une éventuelle URL
        let authority = if let Some((_scheme, rest)) = trimmed.split_once("://") {
            rest.split('/').next().unwrap_or("")
        } else {
            trimmed.split('/').next().unwrap_or("")
        };

        // Retirer userinfo éventuel (user@host)
        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = if host_port.starts_with('[') {
            // IPv6 entre crochets [fe80::1]:5540
            host_port
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
        } else if host_port.matches(':').count() == 1 {
            host_port.split(':').next().unwrap_or("")
        } else {
            host_port
        };

        if let Ok(ip) = host.parse::<IpAddr>() {
            return self.validate_ip(ip);
        }

        // Accepter uniquement les noms locaux mDNS (`.local`, `localhost`) sans DNS public
        let lower = host.to_ascii_lowercase();
        if lower == "localhost" || lower.ends_with(".local") || lower.ends_with(".home.arpa") {
            return Ok(());
        }

        Err(HomeError::LanGuardBlocked {
            endpoint: endpoint.to_string(),
            reason: "Nom d'hôte externe/WAN interdit : seuls IP LAN, .local, .home.arpa, BLE et série local sont autorisés"
                .to_string(),
        })
    }
}

/// Classificateur de risque déterministe pour chaque paire (appareil, commande).
pub struct HomeRiskClassifier;

impl HomeRiskClassifier {
    pub fn classify(category: DeviceCategory, command: &HomeCommand) -> HomeRiskTier {
        match command {
            HomeCommand::ReadState => HomeRiskTier::Tier0ReadOnly,
            HomeCommand::UnlockDoor
            | HomeCommand::OpenGarage
            | HomeCommand::SetWaterValve { .. }
            | HomeCommand::SetMainBreaker { .. }
            | HomeCommand::StreamCamera { .. } => HomeRiskTier::Tier3CriticalExecution,
            HomeCommand::SetAlarm { mode, .. } => {
                if mode.eq_ignore_ascii_case("disarmed") || mode.eq_ignore_ascii_case("disarm") {
                    HomeRiskTier::Tier3CriticalExecution
                } else {
                    HomeRiskTier::Tier2SystemConfig
                }
            }
            HomeCommand::SetPower { on: false } | HomeCommand::Toggle
                if matches!(
                    category,
                    DeviceCategory::MainBreaker
                        | DeviceCategory::AlarmPanel
                        | DeviceCategory::SecurityCamera
                ) =>
            {
                HomeRiskTier::Tier3CriticalExecution
            }
            HomeCommand::SetPower { .. }
            | HomeCommand::Toggle
            | HomeCommand::SetLevel { .. }
            | HomeCommand::SetColorRgb { .. }
            | HomeCommand::SetColorTemp { .. }
            | HomeCommand::SetTemperature { .. }
            | HomeCommand::SetCoverPosition { .. }
            | HomeCommand::LockDoor
            | HomeCommand::CloseGarage => {
                if category.is_security_critical()
                    && !matches!(command, HomeCommand::LockDoor | HomeCommand::CloseGarage)
                {
                    HomeRiskTier::Tier3CriticalExecution
                } else {
                    HomeRiskTier::Tier1StateMutating
                }
            }
            HomeCommand::SetEvCharging { .. } => HomeRiskTier::Tier2SystemConfig,
            HomeCommand::RawProtocol { .. } => {
                if category.is_security_critical() {
                    HomeRiskTier::Tier3CriticalExecution
                } else {
                    HomeRiskTier::Tier2SystemConfig
                }
            }
        }
    }
}

/// Validation Datalog Biscuit v5 spécifique à la domotique (épinglage racine obligatoire).
pub struct HomeBiscuitPolicy {
    pub root_public_key: PublicKey,
}

impl HomeBiscuitPolicy {
    pub fn new(root_public_key: PublicKey) -> Self {
        Self { root_public_key }
    }

    /// Émet un jeton racine pour les opérations domotiques, atténué avec les faits du foyer.
    pub fn issue_home_token(
        authority: &CapabilityAuthority,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        expiry: Timestamp,
        room: Option<&str>,
        protocol: Option<ProtocolKind>,
        role: HomeResidentRole,
    ) -> HomeResult<CapabilityToken> {
        let root = authority
            .create_root_token(scopes, actions, expiry)
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;

        Self::attenuate_home_token(root, room, protocol, role)
    }

    /// Atténue un jeton existant de façon monotone avec périmètre domotique.
    pub fn attenuate_home_token(
        token: CapabilityToken,
        room: Option<&str>,
        protocol: Option<ProtocolKind>,
        role: HomeResidentRole,
    ) -> HomeResult<CapabilityToken> {
        let mut block = biscuit_auth::builder::BlockBuilder::new();
        block
            .add_fact(format!("home_role(\"{}\")", role.as_str()).as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        if let Some(r) = room {
            let safe_r = sanitize_datalog_literal(r)?;
            block
                .add_fact(format!("home_room(\"{safe_r}\")").as_str())
                .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
            block
                .add_check(format!("check if requested_room($req), $req == \"{safe_r}\"").as_str())
                .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        }
        if let Some(p) = protocol {
            let pname = p.as_str();
            block
                .add_fact(format!("home_protocol(\"{pname}\")").as_str())
                .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
            block
                .add_check(format!("check if requested_protocol($p), $p == \"{pname}\"").as_str())
                .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        }
        if matches!(role, HomeResidentRole::Guest | HomeResidentRole::Minor) {
            // Constitution Règle 7 & principe de moindre privilège : jamais de Tier 3 pour invité/mineur.
            block
                .add_check("check if requested_risk($tier), $tier < 3")
                .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        }
        let biscuit: Biscuit = token
            .biscuit
            .append(block)
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        Ok(CapabilityToken { biscuit })
    }

    /// Vérifie un jeton contre la clé racine épinglée et le contexte de l'appareil/commande.
    pub fn verify_command(
        &self,
        token: &CapabilityToken,
        device: &HomeDevice,
        command: &HomeCommand,
        role: HomeResidentRole,
    ) -> HomeResult<HomeRiskTier> {
        let tier = HomeRiskClassifier::classify(device.category, command);
        if matches!(role, HomeResidentRole::Guest | HomeResidentRole::Minor)
            && tier == HomeRiskTier::Tier3CriticalExecution
        {
            return Err(HomeError::CapabilityDenied(format!(
                "Profil '{}' interdit pour la commande critique '{}' sur '{}'",
                role.as_str(),
                command.action_name(),
                device.name
            )));
        }

        // Re-sérialiser + re-parser avec épinglage explicite de la clé racine pour garantir
        // qu'aucun jeton auto-signé ne passe.
        let raw = token
            .to_vec()
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        let pinned = CapabilityToken::from_vec(raw, &self.root_public_key)
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;

        let required_scope = match command {
            HomeCommand::ReadState => CapabilityScope::Sensor,
            _ => CapabilityScope::Network,
        };
        let required_action = match command {
            HomeCommand::ReadState => CapabilityAction::Read,
            _ => CapabilityAction::Execute,
        };

        let mut authorizer = biscuit_auth::Authorizer::new();
        authorizer
            .add_token(&pinned.biscuit)
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;

        let now = Utc::now().timestamp();
        let scope_name = required_scope.datalog_name();
        let action_name = format!("{:?}", required_action).to_lowercase();
        let safe_room = sanitize_datalog_literal(&device.room)?;
        let safe_dev = sanitize_datalog_literal(&device.id)?;

        authorizer
            .add_fact(format!("time({now})").as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        authorizer
            .add_fact(format!("requested_room(\"{safe_room}\")").as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        authorizer
            .add_fact(format!("requested_device(\"{safe_dev}\")").as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        authorizer
            .add_fact(format!("requested_protocol(\"{}\")", device.protocol.as_str()).as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;
        authorizer
            .add_fact(format!("requested_risk({})", tier as i64).as_str())
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;

        authorizer
            .add_code(
                format!(
                    "allow if right(\"{scope_name}\", \"{action_name}\");\n\
                 allow if right(\"fullsystem\", \"admin\");"
                )
                .as_str(),
            )
            .map_err(|e| HomeError::CapabilityDenied(e.to_string()))?;

        authorizer
            .authorize_with_limits(AuthorizerLimits {
                max_facts: 256,
                max_iterations: 64,
                max_time: std::time::Duration::from_millis(25),
            })
            .map_err(|e| {
                HomeError::CapabilityDenied(format!("Autorisation Biscuit refusée : {e}"))
            })?;

        Ok(tier)
    }
}

/// Preuve de maintien physique (600 ms minimum) pour les actions domotiques Tier 3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalHoldProof {
    pub action_id: String,
    pub hold_duration_ms: u64,
    pub confirmed_at: DateTime<Utc>,
}

impl PhysicalHoldProof {
    pub const MIN_HOLD_MS: u64 = 600;
    pub const MAX_AGE_SECS: i64 = 30;

    pub fn validate(&self, expected_action_id: Option<&str>) -> HomeResult<()> {
        if self.hold_duration_ms < Self::MIN_HOLD_MS {
            return Err(HomeError::ConfirmationRequired(format!(
                "Maintien insuffisant : {} ms < {} ms requis",
                self.hold_duration_ms,
                Self::MIN_HOLD_MS
            )));
        }
        let age = Utc::now()
            .signed_duration_since(self.confirmed_at)
            .num_seconds();
        if !(-5..=Self::MAX_AGE_SECS).contains(&age) {
            return Err(HomeError::ConfirmationRequired(format!(
                "Preuve physique expirée (âge={age}s, max={}s)",
                Self::MAX_AGE_SECS
            )));
        }
        if let Some(expected) = expected_action_id
            && self.action_id != expected
        {
            return Err(HomeError::ConfirmationRequired(format!(
                "Incohérence action_id : '{}' != '{expected}'",
                self.action_id
            )));
        }
        Ok(())
    }
}

/// Génère une paire éphémère de test ou d'autorité locale.
pub fn new_ephemeral_authority() -> CapabilityAuthority {
    CapabilityAuthority::with_keypair(KeyPair::new())
}

fn sanitize_datalog_literal(raw: &str) -> HomeResult<String> {
    if raw.is_empty()
        || raw.len() > 128
        || raw.contains('"')
        || raw.contains('\\')
        || raw.contains('\n')
        || raw.contains('\r')
    {
        return Err(HomeError::CapabilityDenied(format!(
            "Littéral Datalog invalide ou potentiel vecteur d'injection : {raw}"
        )));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn lan_guard_allows_local_and_blocks_wan_and_metadata() {
        let guard = HomeLanGuard::new();
        assert!(guard.validate_endpoint("192.168.1.42:5540").is_ok());
        assert!(guard.validate_endpoint("http://10.0.0.15:8123/api").is_ok());
        assert!(guard.validate_endpoint("mqtt://172.16.0.5:1883").is_ok());
        assert!(guard.validate_endpoint("[fe80::1]:5683").is_ok());
        assert!(guard.validate_endpoint("[fd12:3456:789a::1]:5540").is_ok());
        assert!(guard.validate_endpoint("224.0.0.251:5353").is_ok());
        assert!(guard.validate_endpoint("shelly-salon.local").is_ok());
        assert!(guard.validate_endpoint("ble://A4:C1:38:9F:10:22").is_ok());
        assert!(guard.validate_endpoint("/dev/ttyUSB0").is_ok());
        assert!(guard.validate_endpoint("COM4").is_ok());

        // WAN, métadonnées cloud, traversée série : strictement refusés
        assert!(guard.validate_endpoint("169.254.169.254").is_err());
        assert!(
            guard
                .validate_endpoint("http://169.254.169.254/latest/meta-data")
                .is_err()
        );
        assert!(guard.validate_endpoint("8.8.8.8:443").is_err());
        assert!(
            guard
                .validate_endpoint("https://api.tuya-cloud.com")
                .is_err()
        );
        assert!(guard.validate_endpoint("/dev/ttyUSB0/../sda").is_err());
        assert!(guard.validate_endpoint("/etc/passwd").is_err());
    }

    #[test]
    fn biscuit_policy_enforces_room_and_blocks_guest_on_tier3() {
        let auth = new_ephemeral_authority();
        let policy = HomeBiscuitPolicy::new(auth.root_public_key());
        let expiry = Timestamp(Utc::now() + Duration::minutes(10));

        let salon_light = HomeDevice::new(
            "hue-salon-1",
            "Suspension Salon",
            "salon",
            ProtocolKind::PhilipsHue,
            DeviceCategory::ColorLight,
            "192.168.1.20:443",
        );
        let front_lock = HomeDevice::new(
            "matter-lock-1",
            "Serrure Entrée",
            "entree",
            ProtocolKind::Matter,
            DeviceCategory::DoorLock,
            "192.168.1.33:5540",
        );

        let guest_salon_token = HomeBiscuitPolicy::issue_home_token(
            &auth,
            vec![CapabilityScope::Network, CapabilityScope::Sensor],
            vec![CapabilityAction::Read, CapabilityAction::Execute],
            expiry.clone(),
            Some("salon"),
            None,
            HomeResidentRole::Guest,
        )
        .unwrap();

        // Invité peut allumer la lumière du salon
        let tier = policy
            .verify_command(
                &guest_salon_token,
                &salon_light,
                &HomeCommand::SetPower { on: true },
                HomeResidentRole::Guest,
            )
            .unwrap();
        assert_eq!(tier, HomeRiskTier::Tier1StateMutating);

        // Invité ne peut PAS déverrouiller la serrure (Tier 3 + pièce différente)
        let owner_token = HomeBiscuitPolicy::issue_home_token(
            &auth,
            vec![CapabilityScope::Network, CapabilityScope::Sensor],
            vec![CapabilityAction::Read, CapabilityAction::Execute],
            expiry,
            None,
            None,
            HomeResidentRole::Owner,
        )
        .unwrap();

        assert!(
            policy
                .verify_command(
                    &owner_token,
                    &front_lock,
                    &HomeCommand::UnlockDoor,
                    HomeResidentRole::Guest
                )
                .is_err()
        );
        assert_eq!(
            policy
                .verify_command(
                    &owner_token,
                    &front_lock,
                    &HomeCommand::UnlockDoor,
                    HomeResidentRole::Owner
                )
                .unwrap(),
            HomeRiskTier::Tier3CriticalExecution
        );
    }

    #[test]
    fn physical_hold_proof_requires_600ms() {
        let weak = PhysicalHoldProof {
            action_id: "unlock-1".into(),
            hold_duration_ms: 450,
            confirmed_at: Utc::now(),
        };
        assert!(weak.validate(Some("unlock-1")).is_err());

        let strong = PhysicalHoldProof {
            action_id: "unlock-1".into(),
            hold_duration_ms: 620,
            confirmed_at: Utc::now(),
        };
        assert!(strong.validate(Some("unlock-1")).is_ok());
    }
}
