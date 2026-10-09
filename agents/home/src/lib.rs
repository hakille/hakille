use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    Light,
    Thermostat,
    Lock,
    Camera,
    Sensor,
    Switch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DeviceState {
    pub power: bool,
    pub brightness: Option<u8>,
    pub temperature_celsius: Option<f32>,
    pub target_temperature_celsius: Option<f32>,
    pub is_locked: Option<bool>,
    pub motion_detected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartDevice {
    pub id: String,
    pub name: String,
    pub room: String,
    pub device_type: DeviceType,
    pub state: DeviceState,
    pub requires_validation_chip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeCommand {
    pub device_id: String,
    pub action: String,
    pub params: serde_json::Value,
    pub user_confirmed: bool,
}

pub struct SmartHomeHub {
    devices: HashMap<String, SmartDevice>,
}

impl SmartHomeHub {
    pub fn new() -> Self {
        let mut hub = Self {
            devices: HashMap::new(),
        };

        // Initialize default mock home environment
        hub.register_device(SmartDevice {
            id: "light-living-01".to_string(),
            name: "Lumière Salon".to_string(),
            room: "Salon".to_string(),
            device_type: DeviceType::Light,
            state: DeviceState {
                power: false,
                brightness: Some(80),
                ..Default::default()
            },
            requires_validation_chip: false,
        });

        hub.register_device(SmartDevice {
            id: "thermostat-main".to_string(),
            name: "Thermostat Principal".to_string(),
            room: "Salon".to_string(),
            device_type: DeviceType::Thermostat,
            state: DeviceState {
                power: true,
                temperature_celsius: Some(20.5),
                target_temperature_celsius: Some(21.0),
                ..Default::default()
            },
            requires_validation_chip: false,
        });

        hub.register_device(SmartDevice {
            id: "lock-front-door".to_string(),
            name: "Serrure Entrée".to_string(),
            room: "Entrée".to_string(),
            device_type: DeviceType::Lock,
            state: DeviceState {
                power: true,
                is_locked: Some(true),
                ..Default::default()
            },
            requires_validation_chip: true, // Physical perimeter security requires explicit consent
        });

        hub.register_device(SmartDevice {
            id: "sensor-motion-hall".to_string(),
            name: "Détecteur de Présence Couloir".to_string(),
            room: "Couloir".to_string(),
            device_type: DeviceType::Sensor,
            state: DeviceState {
                power: true,
                motion_detected: Some(false),
                ..Default::default()
            },
            requires_validation_chip: false,
        });

        hub
    }

    pub fn register_device(&mut self, device: SmartDevice) {
        self.devices.insert(device.id.clone(), device);
    }

    pub fn discover_devices(&self) -> Vec<SmartDevice> {
        self.devices.values().cloned().collect()
    }

    pub fn get_device(&self, id: &str) -> Option<&SmartDevice> {
        self.devices.get(id)
    }

    pub fn get_devices_by_room(&self, room: &str) -> Vec<&SmartDevice> {
        self.devices
            .values()
            .filter(|d| d.room.eq_ignore_ascii_case(room))
            .collect()
    }

    pub fn execute_command(&mut self, cmd: &HomeCommand) -> Result<String, String> {
        let device = self
            .devices
            .get_mut(&cmd.device_id)
            .ok_or_else(|| format!("Appareil '{}' introuvable dans le réseau domotique.", cmd.device_id))?;

        // Strict Safety Policy: Physical perimeter security actions require explicit confirmation
        let is_security_critical = device.requires_validation_chip
            || cmd.action.eq_ignore_ascii_case("unlock")
            || cmd.action.eq_ignore_ascii_case("disarm");

        if is_security_critical && !cmd.user_confirmed {
            return Err(
                "Action critique de sécurité refusée : la validation explicite de l'utilisateur (Coral Chip #FF5722) est requise pour déverrouiller ou désarmer."
                    .to_string(),
            );
        }

        match cmd.action.to_lowercase().as_str() {
            "turn_on" => {
                device.state.power = true;
                Ok(format!("Appareil '{}' ({}) allumé avec succès.", device.name, device.id))
            }
            "turn_off" => {
                device.state.power = false;
                Ok(format!("Appareil '{}' ({}) éteint avec succès.", device.name, device.id))
            }
            "set_brightness" => {
                let brightness = cmd
                    .params
                    .get("brightness")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "Paramètre 'brightness' (0-100) manquant ou invalide.".to_string())?
                    as u8;
                let clamped = brightness.min(100);
                device.state.brightness = Some(clamped);
                device.state.power = clamped > 0;
                Ok(format!("Luminosité de '{}' ajustée à {}%.", device.name, clamped))
            }
            "set_temperature" => {
                let target = cmd
                    .params
                    .get("target_temperature")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| "Paramètre 'target_temperature' manquant ou invalide.".to_string())?
                    as f32;
                device.state.target_temperature_celsius = Some(target);
                Ok(format!("Consigne de température de '{}' réglée à {:.1}°C.", device.name, target))
            }
            "lock" => {
                device.state.is_locked = Some(true);
                Ok(format!("Serrure '{}' verrouillée avec succès.", device.name))
            }
            "unlock" => {
                device.state.is_locked = Some(false);
                Ok(format!("Serrure '{}' déverrouillée avec succès avec validation sécurisée.", device.name))
            }
            other => Err(format!("Commande domotique inconnue ou non supportée: '{}'.", other)),
        }
    }
}

impl Default for SmartHomeHub {
    fn default() -> Self {
        Self::new()
    }
}

pub fn check_home_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Network, CapabilityAction::Read),
        (CapabilityScope::Network, CapabilityAction::Write),
        (CapabilityScope::Sensor, CapabilityAction::Read),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_discovery_and_room_filtering() {
        let hub = SmartHomeHub::new();
        let devices = hub.discover_devices();
        assert_eq!(devices.len(), 4);

        let salon_devices = hub.get_devices_by_room("Salon");
        assert_eq!(salon_devices.len(), 2);
    }

    #[test]
    fn test_light_and_thermostat_control() {
        let mut hub = SmartHomeHub::new();

        let light_cmd = HomeCommand {
            device_id: "light-living-01".to_string(),
            action: "set_brightness".to_string(),
            params: serde_json::json!({ "brightness": 65 }),
            user_confirmed: false,
        };
        let res = hub.execute_command(&light_cmd);
        assert!(res.is_ok());

        let dev = hub.get_device("light-living-01").unwrap();
        assert_eq!(dev.state.brightness, Some(65));
        assert!(dev.state.power);

        let thermo_cmd = HomeCommand {
            device_id: "thermostat-main".to_string(),
            action: "set_temperature".to_string(),
            params: serde_json::json!({ "target_temperature": 22.5 }),
            user_confirmed: false,
        };
        let res_thermo = hub.execute_command(&thermo_cmd);
        assert!(res_thermo.is_ok());

        let thermo = hub.get_device("thermostat-main").unwrap();
        assert_eq!(thermo.state.target_temperature_celsius, Some(22.5));
    }

    #[test]
    fn test_security_critical_door_unlock_requires_confirmation() {
        let mut hub = SmartHomeHub::new();

        // Attempting to unlock without confirmation fails
        let unconfirmed = HomeCommand {
            device_id: "lock-front-door".to_string(),
            action: "unlock".to_string(),
            params: serde_json::Value::Null,
            user_confirmed: false,
        };
        let res_err = hub.execute_command(&unconfirmed);
        assert!(res_err.is_err());
        assert!(res_err.unwrap_err().contains("Action critique de sécurité refusée"));

        let dev = hub.get_device("lock-front-door").unwrap();
        assert_eq!(dev.state.is_locked, Some(true));

        // Attempting to unlock with user confirmation succeeds
        let confirmed = HomeCommand {
            device_id: "lock-front-door".to_string(),
            action: "unlock".to_string(),
            params: serde_json::Value::Null,
            user_confirmed: true,
        };
        let res_ok = hub.execute_command(&confirmed);
        assert!(res_ok.is_ok());

        let dev_after = hub.get_device("lock-front-door").unwrap();
        assert_eq!(dev_after.state.is_locked, Some(false));
    }
}
