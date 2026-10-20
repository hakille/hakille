//! Environnement ONNX Runtime unique du processus (crate-wide).
//!
//! ONNX Runtime n'autorise qu'UN seul environnement par processus :
//! tout second `ort::init().build()` échoue avec
//! `only one environment is allowed per process`.
//!
//! Tous les moteurs (`ort_vad`, `sensevoice_onnx`, `speaker_onnx`,
//! `embed_onnx`) DOIVENT passer par [`shared_ort_env`] — jamais par un
//! `ort::init()` local. Les anciens singletons par-module sont conservés
//! comme simples délégations pour compatibilité, mais partagent tous ce
//! même holder.

use hakille_core::{HakilleError, Result};
use std::sync::{Arc, Mutex};

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("OrtEnv::{context}: {e}"))
}

/// Environnement ONNX Runtime partagé par tout le processus.
///
/// Le premier appel construit l'environnement, les suivants clonent le
/// même `Arc`. Erreur LOUD si initialisation impossible — jamais de
/// repli silencieux.
pub fn shared_ort_env() -> Result<Arc<ort::environment::Environment>> {
    static ENV: Mutex<Option<Arc<ort::environment::Environment>>> = Mutex::new(None);
    let mut guard = ENV
        .lock()
        .map_err(|e| ort_err("open(environment-lock)", format!("mutex empoisonné : {e}")))?;
    if let Some(env) = guard.as_ref() {
        return Ok(env.clone());
    }
    let env = Arc::new(
        ort::init()
            .build()
            .map_err(|e| ort_err("open(environment)", e))?,
    );
    *guard = Some(env.clone());
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_env_singleton() {
        let a = shared_ort_env().expect("env shared construisible");
        let b = shared_ort_env().expect("second appel clone le même env");
        assert!(
            Arc::ptr_eq(&a, &b),
            "les deux appels doivent partager le même Arc"
        );
    }
}
