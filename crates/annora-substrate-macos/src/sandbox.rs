use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacosSandboxProfile {
    pub profile_string: String,
    pub allow_file_read: bool,
    pub allow_file_write: bool,
    pub allow_network_outbound: bool,
}

impl Default for MacosSandboxProfile {
    fn default() -> Self {
        Self {
            profile_string: "(version 1)(deny default)(allow file-read* (subpath \"/Users/Shared/Annora\"))".to_string(),
            allow_file_read: true,
            allow_file_write: false,
            allow_network_outbound: false,
        }
    }
}
