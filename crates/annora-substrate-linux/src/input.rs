use annora_substrate::InputAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UinputDeviceConfig {
    pub device_name: String,
    pub supports_mouse: bool,
    pub supports_keyboard: bool,
}

impl Default for UinputDeviceConfig {
    fn default() -> Self {
        Self {
            device_name: "Annora Virtual Uinput Device".to_string(),
            supports_mouse: true,
            supports_keyboard: true,
        }
    }
}

pub struct LinuxInputInjector {
    config: UinputDeviceConfig,
}

impl LinuxInputInjector {
    pub fn new(config: UinputDeviceConfig) -> Self {
        Self { config }
    }

    pub fn inject(&self, _action: &InputAction) -> Result<(), String> {
        // In real Linux environment, writes to /dev/uinput ioctl
        Ok(())
    }

    pub fn config(&self) -> &UinputDeviceConfig {
        &self.config
    }
}

impl Default for LinuxInputInjector {
    fn default() -> Self {
        Self::new(UinputDeviceConfig::default())
    }
}
