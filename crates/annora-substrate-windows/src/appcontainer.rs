use annora_core::{CapabilityScope, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppContainerProfile {
    pub package_name: String,
    pub allowed_capabilities: Vec<CapabilityScope>,
    pub isolated_folder: String,
}

pub struct AppContainerSandbox;

impl AppContainerSandbox {
    pub fn create_profile(
        name: &str,
        capabilities: Vec<CapabilityScope>,
        isolated_folder: &str,
    ) -> Result<AppContainerProfile> {
        Ok(AppContainerProfile {
            package_name: format!("Annora.AppContainer.{}", name),
            allowed_capabilities: capabilities,
            isolated_folder: isolated_folder.to_string(),
        })
    }
}
