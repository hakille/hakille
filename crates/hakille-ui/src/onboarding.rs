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
                    description: "Hakille will never perform actions outside of specifically delegated capability tokens.".to_string(),
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

    /// Injecte le manifeste plateforme réel (§3ter) dans l'écran d'onboarding :
    /// Tier + OS hôte + capacités accordées/refusées. Sans appel, l'écran reste
    /// générique — avec, l'utilisateur voit ses limites RÉELLES avant tout.
    pub fn inject_platform(
        &mut self,
        tier_name: &str,
        os_name: &str,
        os_version: &str,
        granted: &[String],
        denied: &[String],
    ) {
        let desc = format!(
            "Hôte détecté : {} {} — palier {:?}. Accordées : {}. Refusées : {}.",
            os_name,
            os_version,
            tier_name,
            if granted.is_empty() {
                "aucune".to_string()
            } else {
                granted.join(", ")
            },
            if denied.is_empty() {
                "aucune".to_string()
            } else {
                denied.join(", ")
            },
        );
        if let Some(slot) = self
            .manifest
            .guaranteed_limits
            .iter_mut()
            .find(|l| l.title == "Plateforme hôte détectée")
        {
            slot.description = desc;
        } else {
            self.manifest.guaranteed_limits.insert(
                0,
                LimitationDeclaration {
                    title: "Plateforme hôte détectée".to_string(),
                    description: desc,
                    non_negotiable: false,
                },
            );
        }
        for cap in granted {
            if !self.manifest.granted_capabilities.iter().any(|c| c == cap) {
                self.manifest.granted_capabilities.push(cap.clone());
            }
        }
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
