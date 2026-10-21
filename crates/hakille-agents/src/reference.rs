//! Artefacts WASM de référence : source WAT embarquée, compilation déterministe
//! et écriture content-addressed sur disque.
//!
//! Les agents de référence (`echo-agent`, `count-agent`) ne sont pas des
//! binaires opaques versionnés dans le dépôt : leur source WAT vit dans
//! [`crate::wasm_runner::reference_agents`], elle est compilée par le crate
//! `wat`, puis le binaire obtenu est écrit sous
//! `<dir>/<nom>-<sha256>.wasm`. Le nom contient le hash complet : un artefact
//! altéré est détecté avant tout chargement et n'est jamais exécuté.
//!
//! Cette étape ne charge ni n'exécute le module ; l'appelant reste responsable
//! de créer un [`crate::AgentSandbox`] et d'appliquer ses limites.

use crate::wasm_runner::{AgentError, reference_agents};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Artefact WASM matérialisé, avec son hash de contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceArtifact {
    /// Nom logique de l'agent (`echo-agent`, `count-agent`).
    pub name: String,
    /// Chemin réel du binaire sur disque.
    pub path: PathBuf,
    /// Hash SHA-256 (hexadécimal minuscule) des octets du fichier.
    pub sha256: String,
    /// Taille du binaire en octets.
    pub bytes_len: usize,
}

/// Hash SHA-256 hexadécimal minuscule d'un buffer.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compile les agents de référence WAT en WASM binaire.
///
/// Retourne une erreur explicite si une source embarquée ne compile pas : il
/// s'agit d'un défaut de build, jamais d'un succès silencieux.
pub fn reference_agents_wasm() -> Result<Vec<(&'static str, Vec<u8>)>, AgentError> {
    reference_agents()
        .into_iter()
        .map(|(name, wat)| {
            let wasm = wat::parse_str(wat).map_err(|e| AgentError::InvalidModule {
                agent: name.to_string(),
                message: format!("compilation WAT embarquée impossible : {e}"),
            })?;
            Ok((name, wasm))
        })
        .collect()
}

/// Matérialise les agents de référence sous `dir`.
///
/// Chaque fichier est nommé `<agent>-<sha256>.wasm` et n'est jamais réécrit
/// silencieusement : si un fichier existant ne correspond pas au hash attendu,
/// l'erreur [`AgentError::ArtifactHashMismatch`] est retournée (fail-closed).
pub fn materialize_reference_agents(dir: &Path) -> Result<Vec<ReferenceArtifact>, AgentError> {
    fs::create_dir_all(dir).map_err(|e| AgentError::ArtifactIo {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;

    let mut artifacts = Vec::new();
    for (name, wasm) in reference_agents_wasm()? {
        let sha256 = sha256_hex(&wasm);
        let path = dir.join(format!("{name}-{sha256}.wasm"));
        if path.exists() {
            let existing = fs::read(&path).map_err(|e| AgentError::ArtifactIo {
                path: path.display().to_string(),
                message: e.to_string(),
            })?;
            let actual = sha256_hex(&existing);
            if actual != sha256 {
                return Err(AgentError::ArtifactHashMismatch {
                    path: path.display().to_string(),
                    expected: sha256,
                    actual,
                });
            }
        } else {
            write_atomic(&path, &wasm)?;
        }

        // Relecture systématique : détecte un fichier tronqué ou altéré entre
        // l'écriture et le chargement, y compris en cas de course entre deux
        // processus qui matérialisent le même artefact.
        let written = fs::read(&path).map_err(|e| AgentError::ArtifactIo {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let actual = sha256_hex(&written);
        if actual != sha256 {
            return Err(AgentError::ArtifactHashMismatch {
                path: path.display().to_string(),
                expected: sha256,
                actual,
            });
        }

        artifacts.push(ReferenceArtifact {
            name: name.to_string(),
            path,
            sha256,
            bytes_len: written.len(),
        });
    }
    Ok(artifacts)
}

/// Écriture atomique best-effort : fichier temporaire dans le même répertoire
/// puis `rename`. Un `rename` qui échoue parce qu'un autre processus a déjà
/// créé le fichier content-addressed est accepté ; la relecture et le hash
/// restent la source de vérité.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AgentError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("agent.wasm");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    fs::write(&tmp, bytes).map_err(|e| AgentError::ArtifactIo {
        path: tmp.display().to_string(),
        message: e.to_string(),
    })?;

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            if path.exists() {
                Ok(())
            } else {
                Err(AgentError::ArtifactIo {
                    path: path.display().to_string(),
                    message: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentSandbox, RunLimits};

    #[test]
    fn materializes_content_addressed_artifacts_idempotently() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let first = materialize_reference_agents(dir.path()).expect("matérialisation");
        assert_eq!(first.len(), 2, "deux agents de référence");

        for artifact in &first {
            assert!(artifact.path.exists());
            assert_eq!(artifact.bytes_len, fs::read(&artifact.path).unwrap().len());
            let file_name = artifact.path.file_name().unwrap().to_string_lossy();
            assert!(
                file_name.starts_with(&format!("{}-", artifact.name)),
                "nom content-addressed: {file_name}"
            );
            assert!(
                file_name.contains(&artifact.sha256),
                "le nom porte le hash complet"
            );
            let bytes = fs::read(&artifact.path).unwrap();
            assert_eq!(sha256_hex(&bytes), artifact.sha256);
        }

        let second = materialize_reference_agents(dir.path()).expect("idempotence");
        assert_eq!(
            first.iter().map(|a| &a.path).collect::<Vec<_>>(),
            second.iter().map(|a| &a.path).collect::<Vec<_>>(),
            "aucun nouveau fichier pour un contenu identique"
        );
    }

    #[test]
    fn materialized_agents_load_and_run() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let artifacts = materialize_reference_agents(dir.path()).expect("matérialisation");
        let sandbox = AgentSandbox::new().expect("bac à sable");
        let input = r#"{"msg":"bonjour","n":1}"#;

        for artifact in &artifacts {
            let bytes = fs::read(&artifact.path).unwrap();
            let mut agent = sandbox
                .load(&artifact.name, &bytes)
                .expect("chargement du binaire matérialisé");
            let output = agent.run(input, RunLimits::default()).expect("exécution");
            serde_json::from_str::<serde_json::Value>(&output).expect("sortie JSON");
            if artifact.name == "echo-agent" {
                assert_eq!(output, input);
            } else {
                assert_eq!(output, format!(r#"{{"count":{}}}"#, input.len()));
            }
        }
    }

    #[test]
    fn corrupted_artifact_is_refused_before_execution() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let artifacts = materialize_reference_agents(dir.path()).expect("matérialisation");
        let victim = &artifacts[0];
        fs::write(&victim.path, b"not wasm at all").expect("corruption volontaire");

        let err = materialize_reference_agents(dir.path()).expect_err("hash mismatch attendu");
        assert!(
            matches!(err, AgentError::ArtifactHashMismatch { .. }),
            "attendu ArtifactHashMismatch, vu {err:?}"
        );
    }
}
