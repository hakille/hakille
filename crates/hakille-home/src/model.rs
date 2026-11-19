//! Modèle de données canonique et contrat d'honnêteté pour la domotique souveraine Hakille.
//!
//! Chaque protocole, appareil, télémétrie énergétique, coordonnée spatiale 3D et reçu
//! d'exécution est représenté sans succès simulé : un transport absent ou un appareil
//! hors ligne est explicitement typé comme tel.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

/// Erreurs explicites du moteur domotique souverain (fail-closed, aucun succès simulé).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HomeError {
    #[error("Accès réseau refusé par HomeLanGuard ({endpoint}) : {reason}")]
    LanGuardBlocked { endpoint: String, reason: String },

    #[error("Politique Biscuit refusée : {0}")]
    CapabilityDenied(String),

    #[error("Confirmation physique requise (maintien >= 600 ms) pour la commande critique : {0}")]
    ConfirmationRequired(String),

    #[error("Appareil introuvable dans le registre local : {0}")]
    DeviceNotFound(String),

    #[error("Pièce introuvable dans le jumeau spatial : {0}")]
    RoomNotFound(String),

    #[error("Transport matériel indisponible pour {protocol} : {reason}")]
    TransportUnavailable { protocol: String, reason: String },

    #[error("Trame protocole invalide ({protocol}) : {details}")]
    InvalidFrame { protocol: String, details: String },

    #[error("Paramètre de commande invalide : {0}")]
    InvalidCommand(String),

    #[error("Erreur d'E/S réseau ou série ({context}) : {message}")]
    Io { context: String, message: String },

    #[error("Erreur de persistance redb : {0}")]
    Storage(String),
}

pub type HomeResult<T> = std::result::Result<T, HomeError>;

/// Niveau de risque d'une commande domotique, aligné sur `hakille-capability`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HomeRiskTier {
    /// Lecture seule : découverte, état, télémétrie, jumeau spatial.
    Tier0ReadOnly = 0,
    /// Mutation réversible non sensible : éclairage, thermostat borné, média.
    Tier1StateMutating = 1,
    /// Mutation système / scène globale / automatisation multi-appareils.
    Tier2SystemConfig = 2,
    /// Action critique physique ou vie privée : déverrouillage serrure, désarmement alarme,
    /// ouverture garage, coupure disjoncteur/vanne d'eau, flux caméra/micro.
    Tier3CriticalExecution = 3,
}

impl HomeRiskTier {
    /// Retourne `true` si le niveau de risque exige un maintien physique >= 600 ms.
    pub fn requires_physical_hold(&self) -> bool {
        matches!(self, Self::Tier3CriticalExecution)
    }
}

impl fmt::Display for HomeRiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tier0ReadOnly => write!(f, "Tier0 (Lecture seule)"),
            Self::Tier1StateMutating => write!(f, "Tier1 (État local)"),
            Self::Tier2SystemConfig => write!(f, "Tier2 (Scène/Config)"),
            Self::Tier3CriticalExecution => write!(f, "Tier3 (Critique — Validation 600 ms)"),
        }
    }
}

/// Tous les protocoles domotiques, bâtiment, énergie et multimédia supportés nativement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProtocolKind {
    Matter,
    ThreadCoap,
    MdnsDnssd,
    SsdpUpnp,
    Mqtt,
    Zigbee,
    Zwave,
    BleGatt,
    KnxIp,
    ModbusTcp,
    ModbusRtu,
    HomeKitHap,
    EspHome,
    HomeAssistant,
    PhilipsHue,
    ShellyRpc,
    OcppEv,
    SunSpecSolar,
    OnvifRtsp,
}

impl ProtocolKind {
    /// Liste canonique ordonnée de tous les protocoles supportés.
    pub fn all() -> &'static [ProtocolKind] {
        &[
            Self::Matter,
            Self::ThreadCoap,
            Self::MdnsDnssd,
            Self::SsdpUpnp,
            Self::Mqtt,
            Self::Zigbee,
            Self::Zwave,
            Self::BleGatt,
            Self::KnxIp,
            Self::ModbusTcp,
            Self::ModbusRtu,
            Self::HomeKitHap,
            Self::EspHome,
            Self::HomeAssistant,
            Self::PhilipsHue,
            Self::ShellyRpc,
            Self::OcppEv,
            Self::SunSpecSolar,
            Self::OnvifRtsp,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Matter => "matter",
            Self::ThreadCoap => "thread-coap",
            Self::MdnsDnssd => "mdns-dnssd",
            Self::SsdpUpnp => "ssdp-upnp",
            Self::Mqtt => "mqtt",
            Self::Zigbee => "zigbee",
            Self::Zwave => "zwave",
            Self::BleGatt => "ble-gatt",
            Self::KnxIp => "knx-ip",
            Self::ModbusTcp => "modbus-tcp",
            Self::ModbusRtu => "modbus-rtu",
            Self::HomeKitHap => "homekit-hap",
            Self::EspHome => "esphome",
            Self::HomeAssistant => "home-assistant",
            Self::PhilipsHue => "philips-hue",
            Self::ShellyRpc => "shelly-rpc",
            Self::OcppEv => "ocpp-ev",
            Self::SunSpecSolar => "sunspec-solar",
            Self::OnvifRtsp => "onvif-rtsp",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Matter => "Matter 1.3 (UDP/TLV/PASE-CASE)",
            Self::ThreadCoap => "Thread / CoAP RFC 7252",
            Self::MdnsDnssd => "mDNS / DNS-SD RFC 6762-6763",
            Self::SsdpUpnp => "SSDP / UPnP LAN",
            Self::Mqtt => "MQTT v3.1.1 & v5.0 (TCP/TLS)",
            Self::Zigbee => "Zigbee (ZCL / ASH-EZSP / ZNP / Z2M)",
            Self::Zwave => "Z-Wave SerialAPI (LRC)",
            Self::BleGatt => "BLE GATT / BTHome v2 / Ruuvi / MiBeacon",
            Self::KnxIp => "KNXnet/IP (cEMI / DPT)",
            Self::ModbusTcp => "Modbus TCP (MBAP)",
            Self::ModbusRtu => "Modbus RTU (RS-485 CRC-16)",
            Self::HomeKitHap => "Apple HomeKit HAP (ChaCha20Poly1305)",
            Self::EspHome => "ESPHome Native API (Protobuf)",
            Self::HomeAssistant => "Home Assistant Local WS/REST",
            Self::PhilipsHue => "Philips Hue CLIP v2",
            Self::ShellyRpc => "Shelly Gen2/Gen3 JSON-RPC",
            Self::OcppEv => "OCPP 1.6J / 2.0.1 (Borne VE)",
            Self::SunSpecSolar => "SunSpec Solaire & Batterie (Modbus)",
            Self::OnvifRtsp => "ONVIF WS-Discovery / RTSP",
        }
    }

    /// Port réseau standard si applicable.
    pub fn default_port(&self) -> Option<u16> {
        match self {
            Self::Matter => Some(5540),
            Self::ThreadCoap => Some(5683),
            Self::MdnsDnssd => Some(5353),
            Self::SsdpUpnp => Some(1900),
            Self::Mqtt => Some(1883),
            Self::KnxIp => Some(3671),
            Self::ModbusTcp | Self::SunSpecSolar => Some(502),
            Self::EspHome => Some(6053),
            Self::HomeAssistant => Some(8123),
            Self::PhilipsHue => Some(443),
            Self::ShellyRpc => Some(80),
            Self::OcppEv => Some(8887),
            Self::OnvifRtsp => Some(3702),
            Self::HomeKitHap => Some(51826),
            Self::Zigbee | Self::Zwave | Self::BleGatt | Self::ModbusRtu => None,
        }
    }

    pub fn from_str_loose(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "matter" => Some(Self::Matter),
            "thread" | "coap" | "thread-coap" | "thread_coap" => Some(Self::ThreadCoap),
            "mdns" | "dnssd" | "mdns-dnssd" | "mdns_dnssd" => Some(Self::MdnsDnssd),
            "ssdp" | "upnp" | "ssdp-upnp" | "ssdp_upnp" => Some(Self::SsdpUpnp),
            "mqtt" | "mqtts" => Some(Self::Mqtt),
            "zigbee" | "z2m" | "ezsp" | "znp" => Some(Self::Zigbee),
            "zwave" | "z-wave" => Some(Self::Zwave),
            "ble" | "bthome" | "ble-gatt" | "ble_gatt" | "bluetooth" => Some(Self::BleGatt),
            "knx" | "knx-ip" | "knx_ip" | "knxnet" => Some(Self::KnxIp),
            "modbus" | "modbus-tcp" | "modbus_tcp" => Some(Self::ModbusTcp),
            "modbus-rtu" | "modbus_rtu" | "rs485" => Some(Self::ModbusRtu),
            "hap" | "homekit" | "homekit-hap" | "homekit_hap" => Some(Self::HomeKitHap),
            "esphome" => Some(Self::EspHome),
            "ha" | "homeassistant" | "home-assistant" | "home_assistant" => {
                Some(Self::HomeAssistant)
            }
            "hue" | "philips-hue" | "philips_hue" => Some(Self::PhilipsHue),
            "shelly" | "shelly-rpc" | "shelly_rpc" => Some(Self::ShellyRpc),
            "ocpp" | "ev" | "ocpp-ev" | "ocpp_ev" => Some(Self::OcppEv),
            "sunspec" | "solar" | "sunspec-solar" | "sunspec_solar" => Some(Self::SunSpecSolar),
            "onvif" | "rtsp" | "onvif-rtsp" | "onvif_rtsp" => Some(Self::OnvifRtsp),
            _ => None,
        }
    }
}

impl fmt::Display for ProtocolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Statut descriptif d'un pilote de protocole (pour le HUD et `:home-protocols`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolCapabilityInfo {
    pub protocol: ProtocolKind,
    pub display_name: String,
    pub wire_codec_ready: bool,
    pub transport_mode: String,
    pub default_port: Option<u16>,
    pub honesty_note: String,
}

/// Catégorie fonctionnelle d'un appareil domestique ou énergétique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DeviceCategory {
    Light,
    Switch,
    Dimmer,
    ColorLight,
    Thermostat,
    Hvac,
    DoorLock,
    GarageDoor,
    AlarmPanel,
    SecurityCamera,
    CoverBlind,
    WaterValve,
    MainBreaker,
    Sensor,
    OccupancySensor,
    EnergyMeter,
    SolarInverter,
    BatteryStorage,
    EvCharger,
    MediaPlayer,
    Speaker,
    BridgeCoordinator,
}

impl DeviceCategory {
    /// Indique si la catégorie affecte la sécurité physique ou la vie privée.
    pub fn is_security_critical(&self) -> bool {
        matches!(
            self,
            Self::DoorLock
                | Self::GarageDoor
                | Self::AlarmPanel
                | Self::SecurityCamera
                | Self::WaterValve
                | Self::MainBreaker
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Switch => "switch",
            Self::Dimmer => "dimmer",
            Self::ColorLight => "color_light",
            Self::Thermostat => "thermostat",
            Self::Hvac => "hvac",
            Self::DoorLock => "door_lock",
            Self::GarageDoor => "garage_door",
            Self::AlarmPanel => "alarm_panel",
            Self::SecurityCamera => "security_camera",
            Self::CoverBlind => "cover_blind",
            Self::WaterValve => "water_valve",
            Self::MainBreaker => "main_breaker",
            Self::Sensor => "sensor",
            Self::OccupancySensor => "occupancy_sensor",
            Self::EnergyMeter => "energy_meter",
            Self::SolarInverter => "solar_inverter",
            Self::BatteryStorage => "battery_storage",
            Self::EvCharger => "ev_charger",
            Self::MediaPlayer => "media_player",
            Self::Speaker => "speaker",
            Self::BridgeCoordinator => "bridge_coordinator",
        }
    }

    pub fn from_str_loose(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "light" | "lamp" | "lumiere" => Some(Self::Light),
            "switch" | "plug" | "prise" | "relay" => Some(Self::Switch),
            "dimmer" | "variateur" => Some(Self::Dimmer),
            "color_light" | "color-light" | "rgb" => Some(Self::ColorLight),
            "thermostat" | "heating" | "chauffage" => Some(Self::Thermostat),
            "hvac" | "ac" | "clim" | "heat_pump" | "pac" => Some(Self::Hvac),
            "door_lock" | "door-lock" | "lock" | "serrure" => Some(Self::DoorLock),
            "garage_door" | "garage-door" | "garage" | "portail" => Some(Self::GarageDoor),
            "alarm_panel" | "alarm-panel" | "alarm" | "alarme" => Some(Self::AlarmPanel),
            "security_camera" | "security-camera" | "camera" | "cam" => Some(Self::SecurityCamera),
            "cover_blind" | "cover-blind" | "cover" | "blind" | "volet" => Some(Self::CoverBlind),
            "water_valve" | "water-valve" | "valve" | "vanne" => Some(Self::WaterValve),
            "main_breaker" | "main-breaker" | "breaker" | "disjoncteur" => Some(Self::MainBreaker),
            "sensor" | "capteur" => Some(Self::Sensor),
            "occupancy_sensor" | "occupancy-sensor" | "occupancy" | "presence" => {
                Some(Self::OccupancySensor)
            }
            "energy_meter" | "energy-meter" | "meter" | "compteur" => Some(Self::EnergyMeter),
            "solar_inverter" | "solar-inverter" | "solar" | "onduleur" => Some(Self::SolarInverter),
            "battery_storage" | "battery-storage" | "battery" | "batterie" => {
                Some(Self::BatteryStorage)
            }
            "ev_charger" | "ev-charger" | "ev" | "borne" | "wallbox" => Some(Self::EvCharger),
            "media_player" | "media-player" | "media" | "tv" => Some(Self::MediaPlayer),
            "speaker" | "enceinte" => Some(Self::Speaker),
            "bridge_coordinator" | "bridge-coordinator" | "bridge" | "hub" => {
                Some(Self::BridgeCoordinator)
            }
            _ => None,
        }
    }
}

/// État de connexion réel d'un appareil (zéro simulation silencieuse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Unpaired,
    Discovered,
    PairedOnline,
    Offline(String),
    UnsupportedTransport(String),
    Error(String),
}

impl ConnectionStatus {
    pub fn is_online(&self) -> bool {
        matches!(self, Self::PairedOnline)
    }

    pub fn label(&self) -> String {
        match self {
            Self::Unpaired => "non-appairé".to_string(),
            Self::Discovered => "découvert-lan".to_string(),
            Self::PairedOnline => "en-ligne".to_string(),
            Self::Offline(r) => format!("hors-ligne ({r})"),
            Self::UnsupportedTransport(r) => format!("transport-indisponible ({r})"),
            Self::Error(r) => format!("erreur ({r})"),
        }
    }
}

/// Position spatiale 3D d'un appareil dans le jumeau numérique de la maison (en mètres).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpatialPose3D {
    pub x_m: f32,
    pub y_m: f32,
    pub z_m: f32,
    pub orientation_deg: f32,
    pub hit_radius_m: f32,
}

impl Default for SpatialPose3D {
    fn default() -> Self {
        Self {
            x_m: 0.0,
            y_m: 1.5,
            z_m: 0.0,
            orientation_deg: 0.0,
            hit_radius_m: 0.45,
        }
    }
}

impl SpatialPose3D {
    pub fn new(x_m: f32, y_m: f32, z_m: f32) -> Self {
        Self {
            x_m,
            y_m,
            z_m,
            orientation_deg: 0.0,
            hit_radius_m: 0.45,
        }
    }

    pub fn with_radius(mut self, radius_m: f32) -> Self {
        self.hit_radius_m = radius_m.clamp(0.1, 3.0);
        self
    }
}

/// Télémétrie énergétique instantanée d'un nœud (compteur, onduleur, batterie, VE, prise).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EnergyTelemetry {
    /// Puissance active en watts (>0 consommation, <0 injection pour bidirectionnel).
    pub power_w: f64,
    /// Production solaire instantanée en watts (si onduleur).
    pub solar_generation_w: Option<f64>,
    /// État de charge batterie en % [0..100].
    pub battery_soc_pct: Option<f32>,
    /// Énergie cumulée importée/consommée en kWh.
    pub energy_kwh: Option<f64>,
    /// Tension secteur en volts.
    pub voltage_v: Option<f32>,
    /// Intensité en ampères.
    pub current_a: Option<f32>,
    /// Facteur de puissance [0..1].
    pub power_factor: Option<f32>,
}

/// État physique d'un appareil domestique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DeviceState {
    pub on: Option<bool>,
    /// Pourcentage [0..100] (luminosité, vitesse, ouverture).
    pub level_pct: Option<u8>,
    /// Couleur RGB [0..255].
    pub color_rgb: Option<[u8; 3]>,
    /// Température de couleur en Kelvin (ex: 2700..6500).
    pub color_temp_kelvin: Option<u16>,
    /// Température mesurée en °C.
    pub temperature_c: Option<f32>,
    /// Consigne de température en °C.
    pub target_temperature_c: Option<f32>,
    /// Humidité relative en %.
    pub humidity_pct: Option<f32>,
    /// Pression atmosphérique en hPa.
    pub pressure_hpa: Option<f32>,
    /// CO2 en ppm.
    pub co2_ppm: Option<u16>,
    /// État de verrouillage (true = verrouillé).
    pub locked: Option<bool>,
    /// Position de volet/portail [0 = fermé, 100 = ouvert].
    pub position_pct: Option<u8>,
    /// Présence / mouvement détecté.
    pub occupancy: Option<bool>,
    /// Ouverture porte/fenêtre (true = ouvert).
    pub contact_open: Option<bool>,
    /// Batterie interne capteur [%].
    pub battery_pct: Option<u8>,
    /// État alarme (disarmed, armed_home, armed_away, triggered).
    pub alarm_mode: Option<String>,
    /// Télémétrie énergétique associée.
    pub energy: Option<EnergyTelemetry>,
    /// Attributs bruts spécifiques au protocole.
    pub raw_attributes: BTreeMap<String, serde_json::Value>,
    /// Horodatage RFC3339 de dernière mesure réelle.
    pub updated_at: Option<String>,
}

/// Zone ou pièce physique de la maison dans le jumeau 3D.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomZone {
    pub id: String,
    pub name: String,
    pub floor: i32,
    pub center_m: [f32; 3],
    pub dimensions_m: [f32; 3],
    pub privacy_zone: bool,
}

impl RoomZone {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        floor: i32,
        center_m: [f32; 3],
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            floor,
            center_m,
            dimensions_m: [4.5, 2.8, 4.0],
            privacy_zone: false,
        }
    }
}

/// Représentation unifiée d'un appareil dans le registre souverain Hakille Home.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeDevice {
    pub id: String,
    pub name: String,
    pub room: String,
    pub protocol: ProtocolKind,
    pub category: DeviceCategory,
    /// Adresse locale (ex: `192.168.1.42:5540`, `mqtt://192.168.1.10:1883`, `/dev/ttyUSB0`).
    pub endpoint: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub status: ConnectionStatus,
    pub pose_3d: SpatialPose3D,
    pub state: DeviceState,
    pub paired_at: Option<String>,
    pub tags: Vec<String>,
}

impl HomeDevice {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        room: impl Into<String>,
        protocol: ProtocolKind,
        category: DeviceCategory,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            room: room.into(),
            protocol,
            category,
            endpoint: endpoint.into(),
            manufacturer: None,
            model: None,
            firmware_version: None,
            status: ConnectionStatus::Discovered,
            pose_3d: SpatialPose3D::default(),
            state: DeviceState::default(),
            paired_at: None,
            tags: Vec::new(),
        }
    }
}

/// Commandes canoniques unifiées (indépendantes du protocole physique sous-jacent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HomeCommand {
    ReadState,
    SetPower {
        on: bool,
    },
    Toggle,
    SetLevel {
        level_pct: u8,
    },
    SetColorRgb {
        r: u8,
        g: u8,
        b: u8,
    },
    SetColorTemp {
        kelvin: u16,
    },
    SetTemperature {
        celsius: f32,
    },
    SetCoverPosition {
        position_pct: u8,
    },
    LockDoor,
    UnlockDoor,
    OpenGarage,
    CloseGarage,
    SetAlarm {
        mode: String,
        pin_hash: Option<String>,
    },
    SetWaterValve {
        open: bool,
    },
    SetMainBreaker {
        closed: bool,
    },
    SetEvCharging {
        enabled: bool,
        max_amps: Option<f32>,
    },
    StreamCamera {
        enabled: bool,
    },
    RawProtocol {
        topic_or_path: String,
        payload: serde_json::Value,
    },
}

impl HomeCommand {
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::ReadState => "read_state",
            Self::SetPower { .. } => "set_power",
            Self::Toggle => "toggle",
            Self::SetLevel { .. } => "set_level",
            Self::SetColorRgb { .. } => "set_color_rgb",
            Self::SetColorTemp { .. } => "set_color_temp",
            Self::SetTemperature { .. } => "set_temperature",
            Self::SetCoverPosition { .. } => "set_cover_position",
            Self::LockDoor => "lock_door",
            Self::UnlockDoor => "unlock_door",
            Self::OpenGarage => "open_garage",
            Self::CloseGarage => "close_garage",
            Self::SetAlarm { .. } => "set_alarm",
            Self::SetWaterValve { .. } => "set_water_valve",
            Self::SetMainBreaker { .. } => "set_main_breaker",
            Self::SetEvCharging { .. } => "set_ev_charging",
            Self::StreamCamera { .. } => "stream_camera",
            Self::RawProtocol { .. } => "raw_protocol",
        }
    }

    /// Parseur canonique unifié (partagé entre REPL, IPC et HUD) d'une action domotique.
    ///
    /// Accepte des alias ergonomiques (`on`, `off`, `toggle`, `read`, `level`, `brightness`,
    /// `temp`, `rgb`, `color_temp`, `cover`, `lock`, `unlock`, `open_garage`, `close_garage`,
    /// `alarm`, `water_valve`, `main_breaker`, `ev_charging`, `stream_camera`, `raw_protocol`)
    /// et un JSON optionnel de paramètres.
    pub fn parse_action(action: &str, params_json: Option<&str>) -> HomeResult<Self> {
        let act = action.trim().to_ascii_lowercase().replace('-', "_");
        let params: serde_json::Value = match params_json.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => serde_json::from_str(raw).map_err(|e| {
                HomeError::InvalidCommand(format!("JSON de paramètres invalide : {e}"))
            })?,
            None => serde_json::json!({}),
        };

        let read_bool = |keys: &[&str]| -> Option<bool> {
            for k in keys {
                if let Some(v) = params.get(*k).and_then(|v| v.as_bool()) {
                    return Some(v);
                }
            }
            None
        };
        let read_u64 = |keys: &[&str]| -> Option<u64> {
            for k in keys {
                if let Some(v) = params.get(*k).and_then(|v| v.as_u64()) {
                    return Some(v);
                }
            }
            None
        };
        let read_f64 = |keys: &[&str]| -> Option<f64> {
            for k in keys {
                if let Some(v) = params.get(*k).and_then(|v| v.as_f64()) {
                    return Some(v);
                }
            }
            None
        };

        match act.as_str() {
            "read" | "read_state" | "status" | "state" => Ok(Self::ReadState),
            "on" | "power_on" | "turn_on" => Ok(Self::SetPower { on: true }),
            "off" | "power_off" | "turn_off" => Ok(Self::SetPower { on: false }),
            "set_power" | "power" => {
                let on = read_bool(&["on", "value", "enabled"]).ok_or_else(|| {
                    HomeError::InvalidCommand("set_power exige {\"on\": true|false}".to_string())
                })?;
                Ok(Self::SetPower { on })
            }
            "toggle" => Ok(Self::Toggle),
            "set_level" | "level" | "brightness" | "dim" => {
                let pct =
                    read_u64(&["level_pct", "level", "brightness", "value"]).ok_or_else(|| {
                        HomeError::InvalidCommand(
                            "set_level exige {\"level_pct\": 0..100}".to_string(),
                        )
                    })?;
                if pct > 100 {
                    return Err(HomeError::InvalidCommand(
                        "level_pct doit être compris entre 0 et 100".to_string(),
                    ));
                }
                Ok(Self::SetLevel {
                    level_pct: pct as u8,
                })
            }
            "set_color_rgb" | "rgb" | "color" => {
                let r = read_u64(&["r"]).and_then(|v| u8::try_from(v).ok());
                let g = read_u64(&["g"]).and_then(|v| u8::try_from(v).ok());
                let b = read_u64(&["b"]).and_then(|v| u8::try_from(v).ok());
                match (r, g, b) {
                    (Some(r), Some(g), Some(b)) => Ok(Self::SetColorRgb { r, g, b }),
                    _ => Err(HomeError::InvalidCommand(
                        "set_color_rgb exige {\"r\": 0..255, \"g\": 0..255, \"b\": 0..255}"
                            .to_string(),
                    )),
                }
            }
            "set_color_temp" | "color_temp" | "kelvin" => {
                let kelvin = read_u64(&["kelvin", "value"]).ok_or_else(|| {
                    HomeError::InvalidCommand(
                        "set_color_temp exige {\"kelvin\": 1500..9000}".to_string(),
                    )
                })?;
                if !(1500..=9000).contains(&kelvin) {
                    return Err(HomeError::InvalidCommand(
                        "kelvin doit être compris entre 1500 et 9000 K".to_string(),
                    ));
                }
                Ok(Self::SetColorTemp {
                    kelvin: kelvin as u16,
                })
            }
            "set_temperature" | "temp" | "temperature" => {
                let celsius =
                    read_f64(&["celsius", "temperature", "target", "value"]).ok_or_else(|| {
                        HomeError::InvalidCommand(
                            "set_temperature exige {\"celsius\": 5.0..35.0}".to_string(),
                        )
                    })? as f32;
                if !(5.0..=35.0).contains(&celsius) {
                    return Err(HomeError::InvalidCommand(
                        "température consigne hors borne [5°C..35°C]".to_string(),
                    ));
                }
                Ok(Self::SetTemperature { celsius })
            }
            "set_cover_position" | "cover" | "blind" => {
                let pos = read_u64(&["position_pct", "position", "value"]).ok_or_else(|| {
                    HomeError::InvalidCommand(
                        "set_cover_position exige {\"position_pct\": 0..100}".to_string(),
                    )
                })?;
                if pos > 100 {
                    return Err(HomeError::InvalidCommand(
                        "position_pct doit être compris entre 0 et 100".to_string(),
                    ));
                }
                Ok(Self::SetCoverPosition {
                    position_pct: pos as u8,
                })
            }
            "lock" | "lock_door" => Ok(Self::LockDoor),
            "unlock" | "unlock_door" => Ok(Self::UnlockDoor),
            "open_garage" => Ok(Self::OpenGarage),
            "close_garage" => Ok(Self::CloseGarage),
            "set_alarm" | "alarm" => {
                let mode = params
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        HomeError::InvalidCommand(
                            "set_alarm exige {\"mode\": \"armed_away|armed_home|disarmed\"}"
                                .to_string(),
                        )
                    })?
                    .to_string();
                let pin_hash = params
                    .get("pin_hash")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned);
                Ok(Self::SetAlarm { mode, pin_hash })
            }
            "disarm" | "disarm_alarm" => Ok(Self::SetAlarm {
                mode: "disarmed".to_string(),
                pin_hash: None,
            }),
            "set_water_valve" | "water_valve" | "valve" => {
                let open = read_bool(&["open", "on", "value"]).ok_or_else(|| {
                    HomeError::InvalidCommand(
                        "set_water_valve exige {\"open\": true|false}".to_string(),
                    )
                })?;
                Ok(Self::SetWaterValve { open })
            }
            "set_main_breaker" | "main_breaker" | "breaker" => {
                let closed = read_bool(&["closed", "on", "value"]).ok_or_else(|| {
                    HomeError::InvalidCommand(
                        "set_main_breaker exige {\"closed\": true|false}".to_string(),
                    )
                })?;
                Ok(Self::SetMainBreaker { closed })
            }
            "set_ev_charging" | "ev_charging" | "ev" => {
                let enabled = read_bool(&["enabled", "on", "value"]).ok_or_else(|| {
                    HomeError::InvalidCommand(
                        "set_ev_charging exige {\"enabled\": true|false, \"max_amps\": 16}"
                            .to_string(),
                    )
                })?;
                let max_amps = read_f64(&["max_amps", "amps"]).map(|v| (v as f32).clamp(0.0, 80.0));
                Ok(Self::SetEvCharging { enabled, max_amps })
            }
            "stream_camera" | "camera" => {
                let enabled = read_bool(&["enabled", "on"]).unwrap_or(true);
                Ok(Self::StreamCamera { enabled })
            }
            "raw_protocol" | "raw" => {
                let topic_or_path = params
                    .get("topic_or_path")
                    .or_else(|| params.get("topic"))
                    .or_else(|| params.get("path"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        HomeError::InvalidCommand(
                            "raw_protocol exige {\"topic_or_path\": \"...\", \"payload\": ...}"
                                .to_string(),
                        )
                    })?
                    .to_string();
                let payload = params
                    .get("payload")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                Ok(Self::RawProtocol {
                    topic_or_path,
                    payload,
                })
            }
            _ => Err(HomeError::InvalidCommand(format!(
                "Action domotique inconnue '{action}'"
            ))),
        }
    }
}

/// Enregistrement d'une découverte réseau locale réelle (mDNS, SSDP, ONVIF, KNX, BLE…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryRecord {
    pub protocol: ProtocolKind,
    pub service_name: String,
    pub endpoint: String,
    pub inferred_category: DeviceCategory,
    pub txt_metadata: BTreeMap<String, String>,
    pub discovered_at: String,
}

/// Reçu cryptographique d'exécution réelle sur un transport ou trame générée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub receipt_id: String,
    pub device_id: String,
    pub protocol: ProtocolKind,
    pub action: String,
    pub risk_tier: HomeRiskTier,
    pub executed_on_wire: bool,
    pub frame_sha256: String,
    pub summary: String,
    pub timestamp: String,
}

impl CommandReceipt {
    pub fn compute_frame_hash(frame_bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(frame_bytes);
        hex::encode(hasher.finalize())
    }
}

/// Événements temps réel émis par le hub domotique vers le daemon / HUD / jumeau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HomeEvent {
    DeviceDiscovered(DiscoveryRecord),
    DeviceRegistered(Box<HomeDevice>),
    StateUpdated {
        device_id: String,
        protocol: ProtocolKind,
        state: DeviceState,
    },
    CommandExecuted(CommandReceipt),
    SceneTriggered {
        scene_id: String,
        commands_dispatched: usize,
    },
    SecurityAlert {
        device_id: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_category_and_command_parse_canonically() {
        assert_eq!(
            DeviceCategory::from_str_loose("serrure"),
            Some(DeviceCategory::DoorLock)
        );
        assert_eq!(
            DeviceCategory::from_str_loose("ev_charger"),
            Some(DeviceCategory::EvCharger)
        );
        assert_eq!(DeviceCategory::from_str_loose("inconnue"), None);

        assert_eq!(
            HomeCommand::parse_action("on", None).unwrap(),
            HomeCommand::SetPower { on: true }
        );
        assert_eq!(
            HomeCommand::parse_action("unlock", None).unwrap(),
            HomeCommand::UnlockDoor
        );
        assert_eq!(
            HomeCommand::parse_action("set_temperature", Some("{\"celsius\": 21.5}")).unwrap(),
            HomeCommand::SetTemperature { celsius: 21.5 }
        );
        assert!(HomeCommand::parse_action("set_level", Some("{\"level_pct\": 140}")).is_err());
        assert!(HomeCommand::parse_action("unknown_cmd", None).is_err());
    }
}
