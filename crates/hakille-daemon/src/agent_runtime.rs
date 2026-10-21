//! Runtime des agents WASM de référence, réellement matérialisés et exécutés.
//!
//! Le daemon écrit les binaires sous `<data_dir>/agents` (content-addressed),
//! vérifie leur hash SHA-256, les charge dans [`AgentSandbox`] et exécute un
//! auto-test borné (fuel, timeout, mémoire). Aucun import hôte n'est lié :
//! l'agent ne peut ni lire l'environnement, ni le réseau, ni le disque.
//!
//! Ce module ne prétend pas exécuter les cinq agents métier (`agent-mail`,
//! etc.) : seuls les deux agents de référence embarqués ont un binaire réel.
//! Les manifests métier restent des déclarations, ce que `:agents` affiche
//! explicitement.

use hakille_agents::{
    Agent, AgentError, AgentSandbox, ReferenceArtifact, RunLimits, materialize_reference_agents,
    sha256_hex,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Entrée JSON de l'auto-test au démarrage.
const SELF_TEST_INPUT: &str = r#"{"probe":true}"#;

/// Plafond d'entrée d'un agent (cohérent avec une page WASM confortable).
pub const MAX_AGENT_INPUT_BYTES: usize = 64 * 1024;

/// Limites appliquées à chaque exécution et à l'auto-test.
fn run_limits() -> RunLimits {
    RunLimits {
        fuel: 10_000_000,
        timeout: Duration::from_secs(2),
        max_memory_pages: 32,
    }
}

/// Agent de référence chargé depuis son binaire sur disque.
#[derive(Debug)]
pub struct LoadedReferenceAgent {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes_len: usize,
    agent: Agent,
}

/// Résultat d'une exécution réelle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    pub agent: String,
    pub output: String,
    pub duration_ms: u128,
    pub input_sha256: String,
    pub output_sha256: String,
}

/// Runtime prêt à exécuter les agents de référence.
#[derive(Debug)]
pub struct ReferenceAgentRuntime {
    pub dir: PathBuf,
    pub self_test_duration_ms: u128,
    loaded: Vec<LoadedReferenceAgent>,
}

impl ReferenceAgentRuntime {
    /// Matérialise, vérifie et auto-teste les agents de référence.
    ///
    /// Toute erreur (disque, hash, ABI, auto-test) est propagée : le daemon
    /// peut alors afficher un runtime indisponible au lieu de feindre un
    /// succès. L'appelant décide si cette indisponibilité est bloquante.
    pub fn provision(data_dir: &Path) -> Result<Self, AgentError> {
        let dir = data_dir.join("agents");
        let artifacts = materialize_reference_agents(&dir)?;
        let sandbox = AgentSandbox::new()?;
        let started = Instant::now();
        let mut loaded = Vec::new();

        for artifact in artifacts {
            let bytes = fs::read(&artifact.path).map_err(|e| AgentError::ArtifactIo {
                path: artifact.path.display().to_string(),
                message: e.to_string(),
            })?;
            let actual = sha256_hex(&bytes);
            if actual != artifact.sha256 {
                return Err(AgentError::ArtifactHashMismatch {
                    path: artifact.path.display().to_string(),
                    expected: artifact.sha256,
                    actual,
                });
            }
            let mut agent = sandbox.load(&artifact.name, &bytes)?;
            let output = agent.run(SELF_TEST_INPUT, run_limits())?;
            serde_json::from_str::<serde_json::Value>(&output).map_err(|e| {
                AgentError::InvalidOutput {
                    agent: artifact.name.clone(),
                    message: format!("auto-test non JSON : {e}"),
                }
            })?;
            loaded.push(loaded_agent(artifact, agent));
        }

        Ok(Self {
            dir,
            self_test_duration_ms: started.elapsed().as_millis(),
            loaded,
        })
    }

    /// Agents chargés (ordre de matérialisation stable).
    pub fn agents(&self) -> &[LoadedReferenceAgent] {
        &self.loaded
    }

    /// Exécute un agent sur une entrée JSON, avec limites et vérification de
    /// sortie. Aucun effet de bord hors de la mémoire WASM.
    pub fn run(&mut self, name: &str, input_json: &str) -> Result<AgentRun, AgentError> {
        if input_json.len() > MAX_AGENT_INPUT_BYTES {
            return Err(AgentError::InputTooLarge {
                agent: name.to_string(),
                input_len: input_json.len(),
                max_bytes: MAX_AGENT_INPUT_BYTES,
            });
        }
        serde_json::from_str::<serde_json::Value>(input_json).map_err(|e| {
            AgentError::InvalidInput {
                agent: name.to_string(),
                message: e.to_string(),
            }
        })?;

        let available = self
            .loaded
            .iter()
            .map(|agent| agent.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let loaded = self
            .loaded
            .iter_mut()
            .find(|agent| agent.name == name)
            .ok_or_else(|| AgentError::UnknownAgent {
                agent: name.to_string(),
                available,
            })?;
        let started = Instant::now();
        let output = loaded.agent.run(input_json, run_limits())?;
        let duration_ms = started.elapsed().as_millis();
        let output_sha256 = sha256_hex(output.as_bytes());
        Ok(AgentRun {
            agent: loaded.name.clone(),
            output,
            duration_ms,
            input_sha256: sha256_hex(input_json.as_bytes()),
            output_sha256,
        })
    }
}

fn loaded_agent(artifact: ReferenceArtifact, agent: Agent) -> LoadedReferenceAgent {
    LoadedReferenceAgent {
        name: artifact.name,
        path: artifact.path,
        sha256: artifact.sha256,
        bytes_len: artifact.bytes_len,
        agent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisions_and_runs_real_wasm_from_disk() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let mut runtime = ReferenceAgentRuntime::provision(dir.path()).expect("provisionnement");
        assert_eq!(runtime.agents().len(), 2);
        assert!(runtime.dir.ends_with("agents"));
        for agent in runtime.agents() {
            assert!(agent.path.exists(), "binaire réel sur disque");
            assert_eq!(
                sha256_hex(&fs::read(&agent.path).unwrap()),
                agent.sha256,
                "hash du binaire"
            );
        }

        let echo = runtime
            .run("echo-agent", r#"{"msg":"bonjour"}"#)
            .expect("exécution echo");
        assert_eq!(echo.output, r#"{"msg":"bonjour"}"#);
        assert_eq!(echo.agent, "echo-agent");
        assert_eq!(echo.input_sha256.len(), 64);
        assert_eq!(echo.output_sha256, echo.input_sha256);

        let count = runtime
            .run("count-agent", r#"{"msg":"bonjour"}"#)
            .expect("exécution count");
        let value: serde_json::Value = serde_json::from_str(&count.output).unwrap();
        assert_eq!(
            value.get("count").and_then(|v| v.as_u64()),
            Some(r#"{"msg":"bonjour"}"#.len() as u64)
        );
    }

    #[test]
    fn invalid_input_and_unknown_agent_are_refused() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let mut runtime = ReferenceAgentRuntime::provision(dir.path()).expect("provisionnement");

        let err = runtime.run("echo-agent", "pas du json").unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidInput { .. }),
            "attendu InvalidInput, vu {err:?}"
        );

        let err = runtime.run("absent", "{}").unwrap_err();
        assert!(
            matches!(err, AgentError::UnknownAgent { .. }),
            "attendu UnknownAgent, vu {err:?}"
        );
    }

    #[test]
    fn input_size_is_bounded_before_execution() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let mut runtime = ReferenceAgentRuntime::provision(dir.path()).expect("provisionnement");
        let oversized = format!(r#"{{"x":"{}"}}"#, "a".repeat(MAX_AGENT_INPUT_BYTES));
        let err = runtime.run("echo-agent", &oversized).unwrap_err();
        assert!(
            matches!(err, AgentError::InputTooLarge { .. }),
            "attendu InputTooLarge, vu {err:?}"
        );
    }
}
