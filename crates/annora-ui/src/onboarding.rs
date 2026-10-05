use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitationDeclaration {
    pub title: String,
    pub description: String,
    pub non_negotiable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingManifest {
    pub guaranteed_limits: Vec<LimitationDeclaration>,
    pub granted_capabilities: Vec<String>,
    pub acknowledged: bool,
}

impl Default for OnboardingManifest {
    fn default() -> Self {
        Self {
            guaranteed_limits: vec![
                LimitationDeclaration {
                    title: "Raw Sensor Data Isolation".to_string(),
                    description: "Microphone and camera data are processed in local volatile memory and NEVER leave the device unencrypted.".to_string(),
                    non_negotiable: true,
                },
                LimitationDeclaration {
                    title: "Untrusted Content Barrier".to_string(),
                    description: "External content (emails, web pages, third-party messages) can NEVER become executable instructions.".to_string(),
                    non_negotiable: true,
                },
                LimitationDeclaration {
                    title: "No Silent Actions in Sensitive Domains".to_string(),
                    description: "Financial transactions, legal agreements, and medical actions strictly require explicit user confirmation.".to_string(),
                    non_negotiable: true,
                },
                LimitationDeclaration {
                    title: "Bounded Initiative Limits".to_string(),
                    description: "Annora will never perform actions outside of specifically delegated capability tokens.".to_string(),
                    non_negotiable: true,
                },
            ],
            granted_capabilities: vec![
                "Continuous conversational awareness".to_string(),
                "Local knowledge graph memory".to_string(),
                "Task planning & System 2 deep reasoning".to_string(),
                "Sandboxed GUI action suggestions".to_string(),
            ],
            acknowledged: false,
        }
    }
}

pub struct OnboardingManager {
    manifest: OnboardingManifest,
}

impl OnboardingManager {
    pub fn new() -> Self {
        Self {
            manifest: OnboardingManifest::default(),
        }
    }

    pub fn manifest(&self) -> &OnboardingManifest {
        &self.manifest
    }

    pub fn acknowledge_limits(&mut self) {
        self.manifest.acknowledged = true;
    }

    pub fn is_ready(&self) -> bool {
        self.manifest.acknowledged
    }
}

impl Default for OnboardingManager {
    fn default() -> Self {
        Self::new()
    }
}
