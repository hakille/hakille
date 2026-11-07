//! Trait et contexte d'exécution pour les agents souverains (`HakilleAgent`) (§36).
//!
//! Contrat d'interface stable pour le développement d'extensions souveraines.

use crate::manifest::SdkManifest;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentError {
    #[error("Capability manquante ou non accordée : {0}")]
    MissingCapability(String),
    #[error("Erreur de traitement interne de l'agent : {0}")]
    Execution(String),
    #[error("Payload d'entrée invalide ou trop volumineux")]
    InvalidPayload,
}

/// Contexte sécurisé fourni à l'agent lors de ses cycles d'exécution.
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub agent_id: String,
    pub granted_capabilities: Vec<String>,
    logs: Arc<Mutex<Vec<(String, String)>>>,
}

impl AgentContext {
    pub fn new(agent_id: impl Into<String>, granted_capabilities: Vec<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            granted_capabilities,
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Vérifie si l'agent dispose légitimement d'une capability.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.granted_capabilities.iter().any(|c| c == capability)
    }

    /// Exige une capability sous peine d'interruption fail-closed.
    pub fn require_capability(&self, capability: &str) -> Result<(), AgentError> {
        if self.has_capability(capability) {
            Ok(())
        } else {
            Err(AgentError::MissingCapability(capability.to_string()))
        }
    }

    /// Enregistre une entrée de journal sécurisée.
    pub fn log(&self, level: impl Into<String>, message: impl Into<String>) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push((level.into(), message.into()));
        }
    }

    /// Récupère la liste des logs générés pendant l'exécution.
    pub fn get_logs(&self) -> Vec<(String, String)> {
        self.logs.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

/// Trait universel qu'un agent souverain doit implémenter.
pub trait HakilleAgent: Send + Sync {
    /// Retourne le manifeste de l'agent.
    fn manifest(&self) -> &SdkManifest;

    /// Initialisation de l'agent.
    fn on_init(&mut self, ctx: &AgentContext) -> Result<(), AgentError> {
        let _ = ctx;
        Ok(())
    }

    /// Traitement d'un message entrant synchrone.
    fn on_message(&mut self, ctx: &AgentContext, payload: &[u8]) -> Result<Vec<u8>, AgentError>;

    /// Arrêt propre de l'agent et nettoyage des ressources.
    fn on_shutdown(&mut self, ctx: &AgentContext) -> Result<(), AgentError> {
        let _ = ctx;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEchoAgent {
        manifest: SdkManifest,
    }

    impl HakilleAgent for MockEchoAgent {
        fn manifest(&self) -> &SdkManifest {
            &self.manifest
        }

        fn on_message(
            &mut self,
            ctx: &AgentContext,
            payload: &[u8],
        ) -> Result<Vec<u8>, AgentError> {
            ctx.require_capability("echo:process")?;
            ctx.log("info", "Echo processing payload");
            Ok(payload.to_vec())
        }
    }

    #[test]
    fn test_agent_lifecycle_and_capability_guards() {
        let manifest = SdkManifest::parse_and_validate(
            r#"
id = "echo-agent"
name = "Echo Agent"
version = "1.0.0"
description = "Echoes messages"
author = "Tester"
license = "MIT"
entrypoint = "echo.wasm"
capabilities = ["echo:process"]
"#,
        )
        .unwrap();

        let mut agent = MockEchoAgent { manifest };

        // 1. Context without granted capability must be refused
        let unprivileged_ctx = AgentContext::new("echo-agent", vec![]);
        assert_eq!(
            agent.on_message(&unprivileged_ctx, b"hello").unwrap_err(),
            AgentError::MissingCapability("echo:process".to_string())
        );

        // 2. Context with granted capability succeeds and logs
        let privileged_ctx = AgentContext::new("echo-agent", vec!["echo:process".to_string()]);
        let res = agent
            .on_message(&privileged_ctx, b"sovereign hello")
            .unwrap();
        assert_eq!(res, b"sovereign hello");

        let logs = privileged_ctx.get_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, "info");
        assert!(logs[0].1.contains("Echo processing"));
    }
}
