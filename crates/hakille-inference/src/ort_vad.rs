//! Session VAD réelle via ONNX Runtime (`ort` v2, binaires précompilés).
//!
//! - Modèle : `models/silero_vad.onnx` (Silero VAD v5, ~2.3 MB).
//! - Signature nominale v5 : `input[1,512]f32`, `state[2,1,128]f32` récurrent,
//!   `sr[1]int64` (=16000) -> `output[1,1]f32`, `stateN[2,1,128]f32`.
//! - La signature RÉELLE est lue au runtime via `session.inputs()` /
//!   `session.outputs()` ; si `state`/`sr` (ou `stateN`) sont absents, le
//!   forward dégrade gracieusement vers le seul `input`
//!   (fallback documenté, pas de panic).
//! - `score()` applique une sigmoïde si la sortie brute ressemble à des logits
//!   (hors `[0,1]`), conserve l'état récurrent entre chunks, et ne contient
//!   aucun appel paniquant sur le chemin audio : toute anomalie remonte en
//!   [`hakille_core::HakilleError::InferenceError`].
//! - `process_stream()` découpe un flux quelconque en fenêtres 512 avec carry
//!   de reste inter-appels : aucun échantillon n'est perdu.
//! - `is_real() == true` / `status() == Real` car chaque probabilité provient
//!   d'un forward `ort` réellement exécuté (voir `InferenceEngine`).

use hakille_core::{HakilleError, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fenêtre Silero : 512 échantillons @ 16 kHz (32 ms).
pub const VAD_CHUNK: usize = 512;
/// Dimension de l'état récurrent : 2*1*128.
pub const VAD_STATE_LEN: usize = 256;
/// Sample-rate attendu par Silero.
pub const VAD_SAMPLE_RATE: i64 = 16_000;
/// Nom de fichier canonique du modèle VAD.
pub const VAD_MODEL_FILENAME: &str = "silero_vad.onnx";

fn sigmoid(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let clamped = x.clamp(-30.0, 30.0);
    1.0 / (1.0 + (-clamped).exp())
}

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("OrtVadSession::{context}: {e}"))
}

/// Résout `silero_vad.onnx` sans jamais de fallback silencieux.
///
/// Ordre de recherche :
/// 1. `models_dir` explicite (paramètre d'appel) ;
/// 2. `$HAKILLE_MODELS_DIR` ;
/// 3. `./models/` (CWD) ;
/// 4. `../../models/` (CWD = crate, ex. `crates/hakille-inference`) ;
/// 5. `<CARGO_MANIFEST_DIR>/../../models/` (ancre robuste au CWD, vaut
///    `<workspace>/models/` pour `crates/hakille-inference`).
///
/// Retourne une erreur LOUD listant tous les candidats testés si absent.
pub fn resolve_vad_model_path(models_dir: Option<&Path>) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(VAD_MODEL_FILENAME));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(VAD_MODEL_FILENAME));
    }
    candidates.push(PathBuf::from("./models").join(VAD_MODEL_FILENAME));
    candidates.push(PathBuf::from("../../models").join(VAD_MODEL_FILENAME));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(VAD_MODEL_FILENAME),
    );
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    let tried: Vec<String> = candidates
        .iter()
        .map(|p| format!("'{}'", p.display()))
        .collect();
    Err(HakilleError::InferenceError(format!(
        "OrtVadSession::resolve: '{}' introuvable — candidats testés : {} ; \
         renseignez models_dir ou $HAKILLE_MODELS_DIR",
        VAD_MODEL_FILENAME,
        tried.join(", ")
    )))
}

/// Session VAD réelle (forward `ort`) avec état récurrent + carry inter-appels.
pub struct OrtVadSession {
    // Environnement PARTAGÉ par processus : ONNX Runtime n'en autorise
    // qu'un seul (`only one environment is allowed per process`). Le garder
    // par session faisait échouer toute 2e ouverture dans le même processus.
    _env: Arc<ort::environment::Environment>,
    session: Session,
    model_path: PathBuf,
    input_name: String,
    state_name: Option<String>,
    sr_name: Option<String>,
    output_name: String,
    state_output_name: Option<String>,
    state: Vec<f32>,
    sample_rate: i64,
    /// Reste d'échantillons (< 512) en attente du prochain `process_stream`.
    carry: Vec<f32>,
    /// Index du prochain chunk 512 qui sera scoré.
    next_chunk_idx: usize,
    /// Compteur de forwards `ort` réellement exécutés avec succès.
    forward_calls: u64,
}

/// Environnement ONNX Runtime unique du processus (délégation vers le holder
/// crate-wide [`crate::ort_env::shared_ort_env`] : un seul `ort::init()` par
/// processus, partagé par `ort_vad`, `sensevoice_onnx`, `speaker_onnx` et
/// `embed_onnx`).
fn shared_env() -> Result<Arc<ort::environment::Environment>> {
    crate::ort_env::shared_ort_env()
}

impl OrtVadSession {
    /// Ouvre un fichier modèle et fige la signature réelle du graphe.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "OrtVadSession::open: modèle absent '{}'",
                path.display()
            )));
        }
        let env = shared_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("open(builder)", e))?
            .commit_from_file(path)
            .map_err(|e| ort_err("open(commit_from_file)", e))?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        if input_names.is_empty() {
            return Err(HakilleError::InferenceError(
                "OrtVadSession::open: aucun input dans le graphe".to_string(),
            ));
        }
        if output_names.is_empty() {
            return Err(HakilleError::InferenceError(
                "OrtVadSession::open: aucun output dans le graphe".to_string(),
            ));
        }
        let pick = |want: &str, names: &[String]| -> Option<String> {
            names.iter().find(|n| n.as_str() == want).cloned()
        };
        let input_name = match pick("input", &input_names) {
            Some(n) => n,
            None => match input_names.first() {
                Some(first) => first.clone(),
                None => "input".to_string(),
            },
        };
        let output_name = match pick("output", &output_names) {
            Some(n) => n,
            None => match output_names.first() {
                Some(first) => first.clone(),
                None => "output".to_string(),
            },
        };
        // Fallback documenté : sans `state`/`sr` le forward reste valide (input seul).
        let state_name = pick("state", &input_names);
        let sr_name = pick("sr", &input_names);
        let state_output_name = pick("stateN", &output_names);
        if state_name.is_none() || sr_name.is_none() {
            tracing::warn!(
                inputs = ?input_names,
                "OrtVadSession::open: signature sans state/sr — fallback input-seul"
            );
        }

        Ok(Self {
            _env: env,
            session,
            model_path: path.to_path_buf(),
            input_name,
            state_name,
            sr_name,
            output_name,
            state_output_name,
            state: vec![0.0f32; VAD_STATE_LEN],
            sample_rate: VAD_SAMPLE_RATE,
            carry: Vec::new(),
            next_chunk_idx: 0,
            forward_calls: 0,
        })
    }

    /// Ouvre le modèle résolu via [`resolve_vad_model_path`] (erreur LOUD si absent).
    pub fn open_default(models_dir: Option<&Path>) -> Result<Self> {
        let path = resolve_vad_model_path(models_dir)?;
        Self::open(path)
    }

    /// Chemin du modèle ouvert.
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Noms d'inputs réels du graphe (honnêteté mesurable).
    pub fn real_input_names(&self) -> Vec<String> {
        self.session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect()
    }

    /// Noms d'outputs réels du graphe.
    pub fn real_output_names(&self) -> Vec<String> {
        self.session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect()
    }

    /// `true` si le graphe expose l'état récurrent (`state` + `stateN`).
    pub fn has_recurrent_state(&self) -> bool {
        self.state_name.is_some() && self.state_output_name.is_some()
    }

    /// Nombre de forwards `ort` réellement exécutés avec succès.
    pub fn forward_calls(&self) -> u64 {
        self.forward_calls
    }

    /// Échantillons en attente (< 512) conservés entre deux `process_stream`.
    pub fn pending_carry_len(&self) -> usize {
        self.carry.len()
    }

    /// Index du prochain chunk 512 qui sera scoré.
    pub fn next_chunk_index(&self) -> usize {
        self.next_chunk_idx
    }

    /// Réinitialise l'état récurrent (zéros).
    pub fn reset(&mut self) {
        self.reset_state();
    }

    /// Réinitialise l'état récurrent + le carry inter-appels + l'index de chunk.
    ///
    /// Ne touche pas à `forward_calls` (compteur cumulé honnête).
    pub fn reset_state(&mut self) {
        self.state = vec![0.0f32; VAD_STATE_LEN];
        self.carry.clear();
        self.next_chunk_idx = 0;
    }

    /// Probabilité de parole pour exactement 512 échantillons mono @16kHz.
    ///
    /// - Sanitize : `NaN`/`Inf` -> `0.0`, clamp `[-1,1]`.
    /// - Sigmoïde appliquée uniquement si la sortie brute est hors `[0,1]`
    ///   (logits) ; sinon valeur clampée telle quelle.
    /// - L'état récurrent est mis à jour depuis `stateN` quand disponible.
    pub fn score(&mut self, chunk512: &[f32]) -> Result<f32> {
        if chunk512.len() != VAD_CHUNK {
            return Err(HakilleError::InferenceError(format!(
                "OrtVadSession::score: chunk512 attendu (512), reçu {}",
                chunk512.len()
            )));
        }
        let mut clean = Vec::with_capacity(VAD_CHUNK);
        for s in chunk512.iter() {
            let v = *s;
            clean.push(if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            });
        }
        let prob = self.forward(&clean)?;
        self.forward_calls += 1;
        Ok(prob)
    }

    /// Découpe un flux quelconque en fenêtres 512 @16kHz avec carry inter-appels.
    ///
    /// - Sanitize d'entrée (`NaN`/`Inf` -> `0.0`, clamp `[-1,1]`).
    /// - Les échantillons qui ne forment pas une fenêtre complète sont conservés
    ///   dans le carry (`pending_carry_len() < 512`) pour l'appel suivant :
    ///   **aucune perte d'échantillon**.
    /// - Entrée vide -> `Ok(vec![])` (pas d'erreur, carry inchangé).
    /// - Retourne `(chunk_idx, prob)` avec `chunk_idx` strictement croissant
    ///   depuis l'ouverture / le dernier `reset_state()`.
    pub fn process_stream(&mut self, pcm_16k: &[f32]) -> Result<Vec<(usize, f32)>> {
        for s in pcm_16k.iter() {
            let v = *s;
            self.carry.push(if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            });
        }
        let mut out = Vec::new();
        while self.carry.len() >= VAD_CHUNK {
            let window: Vec<f32> = self.carry.drain(..VAD_CHUNK).collect();
            if window.len() != VAD_CHUNK {
                return Err(HakilleError::InferenceError(format!(
                    "OrtVadSession::process_stream: fenêtre incohérente ({})",
                    window.len()
                )));
            }
            let prob = self.score(&window)?;
            out.push((self.next_chunk_idx, prob));
            self.next_chunk_idx += 1;
        }
        Ok(out)
    }

    /// Forward `ort` réel sur une fenêtre déjà sanitizée (512 échantillons).
    ///
    /// Met à jour l'état récurrent depuis `stateN` quand le graphe l'expose.
    /// Toute anomalie remonte en `InferenceError`, jamais de panic.
    fn forward(&mut self, clean512: &[f32]) -> Result<f32> {
        if clean512.len() != VAD_CHUNK {
            return Err(HakilleError::InferenceError(format!(
                "OrtVadSession::forward: 512 échantillons attendus, reçu {}",
                clean512.len()
            )));
        }
        let mut owned = Vec::with_capacity(VAD_CHUNK);
        for s in clean512.iter() {
            owned.push(*s);
        }
        let input_tensor = Tensor::<f32>::from_array(([1usize, VAD_CHUNK], owned))
            .map_err(|e| ort_err("score(input_tensor)", e))?;
        let state_tensor =
            Tensor::<f32>::from_array(([2usize, 1usize, 128usize], self.state.clone()))
                .map_err(|e| ort_err("score(state_tensor)", e))?;
        let sr_tensor = Tensor::<i64>::from_array(([1usize], vec![self.sample_rate]))
            .map_err(|e| ort_err("score(sr_tensor)", e))?;

        let has_state = self.state_name.is_some();
        let has_sr = self.sr_name.is_some();
        let input_key = self.input_name.clone();
        let state_key = self.state_name.clone();
        let sr_key = self.sr_name.clone();
        let output_key = self.output_name.clone();
        let state_out_key = self.state_output_name.clone();

        let outputs = if has_state && has_sr {
            let sk = match state_key {
                Some(k) => k,
                None => "state".to_string(),
            };
            let rk = match sr_key {
                Some(k) => k,
                None => "sr".to_string(),
            };
            self.session
                .run(ort::inputs![input_key => input_tensor, sk => state_tensor, rk => sr_tensor])
                .map_err(|e| ort_err("score(run3)", e))?
        } else if has_state {
            let sk = match state_key {
                Some(k) => k,
                None => "state".to_string(),
            };
            self.session
                .run(ort::inputs![input_key => input_tensor, sk => state_tensor])
                .map_err(|e| ort_err("score(run2-state)", e))?
        } else if has_sr {
            let rk = match sr_key {
                Some(k) => k,
                None => "sr".to_string(),
            };
            self.session
                .run(ort::inputs![input_key => input_tensor, rk => sr_tensor])
                .map_err(|e| ort_err("score(run2-sr)", e))?
        } else {
            self.session
                .run(ort::inputs![input_key => input_tensor])
                .map_err(|e| ort_err("score(run1)", e))?
        };

        let raw: f32 = outputs
            .get(&output_key)
            .and_then(|v| v.try_extract_tensor::<f32>().ok())
            .and_then(|(_, data)| data.first().copied())
            .ok_or_else(|| {
                HakilleError::InferenceError(format!(
                    "OrtVadSession::score: output '{output_key}' illisible"
                ))
            })?;
        if !raw.is_finite() {
            return Err(HakilleError::InferenceError(
                "OrtVadSession::score: sortie non-finie".to_string(),
            ));
        }
        // Si `stateN` présent, l'état récurrent est conservé entre chunks.
        if let Some(skey) = state_out_key
            && let Some((_, data)) = outputs
                .get(&skey)
                .and_then(|v| v.try_extract_tensor::<f32>().ok())
            && data.len() == VAD_STATE_LEN
        {
            let mut next = Vec::with_capacity(VAD_STATE_LEN);
            for v in data.iter() {
                let x = *v;
                next.push(if x.is_finite() { x } else { 0.0 });
            }
            self.state = next;
        }
        let prob = if (0.0..=1.0).contains(&raw) {
            raw.clamp(0.0, 1.0)
        } else {
            sigmoid(raw).clamp(0.0, 1.0)
        };
        Ok(prob)
    }
}

impl crate::InferenceEngine for OrtVadSession {
    fn engine_id(&self) -> &str {
        "silero-vad-ort"
    }

    /// `true` : chaque probabilité provient d'un forward `ort` réellement exécuté.
    fn is_real(&self) -> bool {
        true
    }

    fn status(&self) -> crate::ModelStatus {
        crate::ModelStatus::Real
    }

    /// Exécute un VRAI forward `ort` (session éphémère ouverte sur le même
    /// modèle, état récurrent zéro-initialisé) sur 512 floats d'entrée.
    ///
    /// Entrée : clé `"input"` (ou à défaut la première du map), exactement
    /// 512 valeurs. Sortie : map `{"output": TensorBuffer[1,1](prob)}`.
    /// Erreur LOUD dans tous les autres cas — jamais de valeur simulée.
    fn run_real(
        &self,
        inputs: HashMap<String, crate::candle_runtime::TensorBuffer>,
    ) -> hakille_core::Result<HashMap<String, crate::candle_runtime::TensorBuffer>> {
        let buf = inputs
            .get("input")
            .or_else(|| inputs.values().next())
            .ok_or_else(|| {
                HakilleError::InferenceError(
                    "OrtVadSession::run_real: aucun tenseur d'entrée fourni".to_string(),
                )
            })?;
        if buf.data.len() != VAD_CHUNK {
            return Err(HakilleError::InferenceError(format!(
                "OrtVadSession::run_real: 512 valeurs attendues, reçu {}",
                buf.data.len()
            )));
        }
        // Session éphémère : forward réel, sans toucher à l'état du `self`.
        // (`Session::run` exige `&mut`, incompatible avec `&self` du trait.)
        let mut ephemeral = Self::open(&self.model_path)?;
        let prob = ephemeral.score(&buf.data)?;
        let mut out = HashMap::new();
        out.insert(
            "output".to_string(),
            crate::candle_runtime::TensorBuffer::new(vec![1, 1], vec![prob]),
        );
        Ok(out)
    }
}

/// Scorer générique RÉEL pour petits modèles scalaires `.onnx` (ex. futur
/// checkpoint openWakeWord) : forward `ort` 1-D `[1, N] -> scalaire`.
///
/// Le premier input du graphe reçoit `[1, N]f32` (features sanitizées), la
/// première valeur du premier output est lue comme score brut (sigmoïde
/// conditionnelle comme [`OrtVadSession`]). Si la signature du graphe est
/// incompatible (rank/shape), `ort` échoue et l'erreur remonte LOUD —
///
/// l'appelant reste sur son chemin déterministe (jamais de valeur inventée).
pub struct OrtScalarScorer {
    _env: Arc<ort::environment::Environment>,
    session: Session,
    model_path: PathBuf,
    input_name: String,
    output_name: String,
    forward_calls: u64,
}

impl OrtScalarScorer {
    /// Ouvre un modèle scalaire (erreur LOUD si fichier absent / graphe vide).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "OrtScalarScorer::open: modèle absent '{}'",
                path.display()
            )));
        }
        let env = shared_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("scalar(builder)", e))?
            .commit_from_file(path)
            .map_err(|e| ort_err("scalar(commit_from_file)", e))?;
        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let input_name = input_names.first().cloned().ok_or_else(|| {
            HakilleError::InferenceError(
                "OrtScalarScorer::open: aucun input dans le graphe".to_string(),
            )
        })?;
        let output_name = output_names.first().cloned().ok_or_else(|| {
            HakilleError::InferenceError(
                "OrtScalarScorer::open: aucun output dans le graphe".to_string(),
            )
        })?;
        Ok(Self {
            _env: env,
            session,
            model_path: path.to_path_buf(),
            input_name,
            output_name,
            forward_calls: 0,
        })
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn forward_calls(&self) -> u64 {
        self.forward_calls
    }

    pub fn real_input_names(&self) -> Vec<String> {
        self.session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect()
    }

    pub fn real_output_names(&self) -> Vec<String> {
        self.session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect()
    }

    /// Forward réel : `features` sanitizées -> scalaire `[0,1]`.
    pub fn score_features(&mut self, features: &[f32]) -> Result<f32> {
        if features.is_empty() {
            return Err(HakilleError::InferenceError(
                "OrtScalarScorer::score_features: features vides".to_string(),
            ));
        }
        if features.len() > 1_000_000 {
            return Err(HakilleError::InferenceError(format!(
                "OrtScalarScorer::score_features: features démesurées ({})",
                features.len()
            )));
        }
        let mut clean = Vec::with_capacity(features.len());
        for s in features.iter() {
            let v = *s;
            clean.push(if v.is_finite() {
                v.clamp(-10.0, 10.0)
            } else {
                0.0
            });
        }
        let n = clean.len();
        let tensor = Tensor::<f32>::from_array(([1usize, n], clean))
            .map_err(|e| ort_err("scalar(input_tensor)", e))?;
        let input_key = self.input_name.clone();
        let output_key = self.output_name.clone();
        let outputs = self
            .session
            .run(ort::inputs![input_key => tensor])
            .map_err(|e| ort_err("scalar(run)", e))?;
        let raw: f32 = outputs
            .get(&output_key)
            .and_then(|v| v.try_extract_tensor::<f32>().ok())
            .and_then(|(_, data)| data.first().copied())
            .ok_or_else(|| {
                HakilleError::InferenceError(format!(
                    "OrtScalarScorer::score_features: output '{output_key}' illisible"
                ))
            })?;
        if !raw.is_finite() {
            return Err(HakilleError::InferenceError(
                "OrtScalarScorer::score_features: sortie non-finie".to_string(),
            ));
        }
        self.forward_calls += 1;
        Ok(if (0.0..=1.0).contains(&raw) {
            raw.clamp(0.0, 1.0)
        } else {
            sigmoid(raw).clamp(0.0, 1.0)
        })
    }
}

#[cfg(test)]
mod real_model_tests {
    use super::*;
    use std::time::Instant;

    fn open_real_session() -> OrtVadSession {
        let path = resolve_vad_model_path(None)
            .expect("modèle silero_vad.onnx requis pour les golden tests");
        eprintln!("golden: modèle utilisé: {}", path.display());
        let sess = OrtVadSession::open(&path).expect("ouverture OrtVadSession (golden)");
        let inputs = sess.real_input_names();
        let outputs = sess.real_output_names();
        eprintln!("golden: inputs={inputs:?} outputs={outputs:?}");
        assert!(
            sess.has_recurrent_state(),
            "le graphe silero doit exposer state/stateN"
        );
        sess
    }

    fn sine_16k(freq_hz: f32, amp: f32, n: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(n);
        for t in 0..n {
            let s = (t as f32 / 16_000.0 * freq_hz * 2.0 * std::f32::consts::PI).sin() * amp;
            v.push(s);
        }
        v
    }

    /// Bruit blanc déterministe (xorshift64*, seed fixe) — pas de `rand`.
    fn white_noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
        let mut st = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            st ^= st >> 12;
            st ^= st << 25;
            st ^= st >> 27;
            let u = (st.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
            v.push(((u as f32) * 2.0 - 1.0) * amp);
        }
        v
    }

    fn percentile(sorted_ms: &mut [f64], p: f64) -> f64 {
        if sorted_ms.is_empty() {
            return 0.0;
        }
        sorted_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = (p / 100.0 * (sorted_ms.len() - 1) as f64).round() as usize;
        sorted_ms[rank.min(sorted_ms.len() - 1)]
    }

    #[test]
    fn golden_silence_low_probability() {
        let mut sess = open_real_session();
        let silence = vec![0.0f32; 16_000];
        let probs = sess.process_stream(&silence).expect("forward silence");
        assert_eq!(
            probs.len(),
            31,
            "16000 échantillons -> 31 fenêtres + 128 carry"
        );
        assert_eq!(sess.pending_carry_len(), 16_000 - 31 * 512);
        let max_p: f32 = probs.iter().map(|(_, p)| *p).fold(0.0, f32::max);
        let mean_p: f32 = probs.iter().map(|(_, p)| *p).sum::<f32>() / probs.len() as f32;
        eprintln!("golden silence: max={max_p:.4} mean={mean_p:.4}");
        for (idx, p) in &probs {
            assert!(*p < 0.3, "silence chunk {idx}: prob={p:.4} doit être < 0.3");
        }
    }

    /// Phrase voisée synthétique déterministe (aucune dépendance, aucun asset
    /// binaire) : syllabes formantées /ba/-/si/ (F1/F2/F3 par résonateurs
    /// 2 pôles, F0 118/132 Hz, transients 15 ms) + fricative + pauses,
    /// bloc répété 2× (le contexte séquentiel fait grimper le score :
    /// 1re occurrence ~0.49, 2e ~0.59), normalisée à 0.5 peak.
    ///
    /// Fait empirique documenté : le Silero VAD v5 REJETTE les signaux
    /// stationnaires (sine pure, DC, harmoniques fixes : prob ~0.0005) et ne
    /// décolle que sur structure syllabique avec onsets/offsets (voir
    /// `golden_pure_tones_rejected` + `golden_voiced_phrase_takes_off`).
    fn synthetic_voiced_phrase() -> Vec<f32> {
        const SR: f32 = 16_000.0;
        fn reson(x: Vec<f32>, fc: f32, r: f32) -> Vec<f32> {
            use std::f32::consts::PI;
            let b = 2.0 * r * (2.0 * PI * fc / 16_000.0).cos();
            let c = -(r * r);
            let mut y1 = 0.0;
            let mut y2 = 0.0;
            x.into_iter()
                .map(|xn| {
                    let yn = xn + b * y1 + c * y2;
                    y2 = y1;
                    y1 = yn;
                    yn
                })
                .collect()
        }
        fn syllabe(f1: f32, f2: f32, f3: f32, f0: f32, dur: f32, amp: f32) -> Vec<f32> {
            use std::f32::consts::PI;
            let m = (dur * SR) as usize;
            let edge = (0.015 * SR).max(8.0);
            let g: Vec<f32> = (0..m)
                .map(|t| {
                    let tt = t as f32 / SR;
                    let env = ((t as f32 / edge).min(1.0)) * (((m - t) as f32 / edge).min(1.0));
                    let mut s = 0.0;
                    for k in 1..=16 {
                        s += (tt * f0 * k as f32 * 2.0 * PI).sin() * amp / k as f32;
                    }
                    s * env
                })
                .collect();
            let a = reson(g.clone(), f1, 0.985);
            let b = reson(g.clone(), f2, 0.98);
            let c = reson(g, f3, 0.97);
            a.into_iter()
                .zip(b)
                .zip(c)
                .map(|((x, y), z)| x * 0.5 + y * 0.35 + z * 0.25)
                .collect()
        }
        fn fricative(dur: f32, amp: f32, seed: u64) -> Vec<f32> {
            let m = (dur * SR) as usize;
            let mut st = seed;
            let mut prev = 0.0;
            (0..m)
                .map(|_| {
                    st ^= st >> 12;
                    st ^= st << 25;
                    st ^= st >> 27;
                    let u =
                        (st.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
                    let w = ((u as f32) * 2.0 - 1.0) * amp;
                    let y = w - 0.96 * prev;
                    prev = w;
                    y
                })
                .collect()
        }
        fn silence(dur: f32) -> Vec<f32> {
            vec![0.0; (dur * SR) as usize]
        }
        let mut bloc = silence(0.25);
        bloc.extend_from_slice(&syllabe(730.0, 1090.0, 2440.0, 118.0, 0.22, 0.5));
        bloc.extend_from_slice(&fricative(0.07, 0.12, 0xABC));
        bloc.extend_from_slice(&silence(0.10));
        bloc.extend_from_slice(&syllabe(270.0, 2290.0, 3010.0, 132.0, 0.18, 0.45));
        bloc.extend_from_slice(&silence(0.10));
        let mut phrase = bloc.clone();
        phrase.extend_from_slice(&bloc[(0.25 * SR) as usize..]);
        phrase.extend_from_slice(&silence(0.30));
        let peak = phrase
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max)
            .max(1e-6);
        for s in phrase.iter_mut() {
            *s = (*s / peak * 0.5).clamp(-1.0, 1.0);
        }
        phrase
    }

    #[test]
    fn golden_pure_tones_rejected_selectivity() {
        // Sélectivité prouvée : signaux stationnaires non-parole rejetés.
        let mut sess = open_real_session();
        for (name, sig) in [
            ("sine150Hzx0.4", sine_16k(150.0, 0.4, 16_000)),
            ("dc0.3", vec![0.3f32; 16_000]),
        ] {
            sess.reset_state();
            let probs = sess.process_stream(&sig).expect("forward tonal");
            let max_p: f32 = probs.iter().map(|(_, p)| *p).fold(0.0, f32::max);
            eprintln!("golden sélectivité {name}: max={max_p:.4}");
            assert!(
                max_p < 0.05,
                "{name} doit être rejeté (non-parole): max={max_p:.4}"
            );
        }
    }

    #[test]
    fn golden_voiced_phrase_takes_off() {
        let mut sess = open_real_session();
        // Référence silence sur session fraîche pour contraste relatif.
        let silence = vec![0.0f32; 16_000];
        let s_probs = sess.process_stream(&silence).expect("forward silence ref");
        let s_max: f32 = s_probs.iter().map(|(_, p)| *p).fold(0.0, f32::max);
        sess.reset_state();
        let phrase = synthetic_voiced_phrase();
        let probs = sess.process_stream(&phrase).expect("forward phrase");
        assert!(!probs.is_empty());
        let max_p: f32 = probs.iter().map(|(_, p)| *p).fold(0.0, f32::max);
        let mean_p: f32 = probs.iter().map(|(_, p)| *p).sum::<f32>() / probs.len() as f32;
        let hot = probs.iter().filter(|(_, p)| *p > 0.5).count();
        eprintln!(
            "golden phrase-voisée: max={max_p:.4} mean={mean_p:.4} \
             fenêtres>0.5:{hot} (silence max={s_max:.4})"
        );
        assert!(
            max_p > 0.5,
            "la phrase voisée doit décoller (>0.5): max={max_p:.4}"
        );
        assert!(
            max_p > s_max + 0.2,
            "contraste parole vs silence insuffisant: {max_p:.4} vs {s_max:.4}"
        );
        assert!(
            sess.forward_calls() >= probs.len() as u64 + s_probs.len() as u64,
            "compteur de forwards réels incohérent"
        );
    }

    #[test]
    fn golden_low_white_noise_stays_low() {
        let mut sess = open_real_session();
        let noise = white_noise(16_000, 0.02, 0x1234_5678);
        let probs = sess.process_stream(&noise).expect("forward bruit");
        assert_eq!(probs.len(), 31);
        let max_p: f32 = probs.iter().map(|(_, p)| *p).fold(0.0, f32::max);
        let mean_p: f32 = probs.iter().map(|(_, p)| *p).sum::<f32>() / probs.len() as f32;
        eprintln!("golden bruit-faible: max={max_p:.4} mean={mean_p:.4}");
        assert!(
            mean_p < 0.4,
            "bruit blanc faible: prob moyenne={mean_p:.4} doit rester basse"
        );
    }

    #[test]
    fn golden_carry_never_loses_samples() {
        let mut sess = open_real_session();
        let a = sine_16k(150.0, 0.4, 700);
        let b = sine_16k(150.0, 0.4, 500);
        let ra = sess.process_stream(&a).expect("stream A");
        assert_eq!(ra.len(), 1, "700 éch -> 1 fenêtre + 188 carry");
        assert_eq!(sess.pending_carry_len(), 188);
        assert_eq!(ra[0].0, 0);
        let rb = sess.process_stream(&b).expect("stream B");
        // 188 + 500 = 688 -> 1 fenêtre + 176 carry.
        assert_eq!(rb.len(), 1);
        assert_eq!(rb[0].0, 1, "index de chunk strictement croissant");
        assert_eq!(sess.pending_carry_len(), 176);
        // Équivalence avec un appel unique : mêmes probabilités.
        let mut ref_sess = open_real_session();
        let mut joined = a.clone();
        joined.extend_from_slice(&b);
        let r_all = ref_sess
            .process_stream(&joined[..(joined.len() - 176)])
            .expect("stream joint");
        assert_eq!(r_all.len(), 2);
        assert!(
            (ra[0].1 - r_all[0].1).abs() < 1e-6 && (rb[0].1 - r_all[1].1).abs() < 1e-6,
            "le carry ne doit altérer aucune probabilité"
        );
        // reset_state vide tout.
        sess.reset_state();
        assert_eq!(sess.pending_carry_len(), 0);
        assert_eq!(sess.next_chunk_index(), 0);
    }

    #[test]
    fn golden_latency_p50_p95_under_50ms() {
        let mut sess = open_real_session();
        let chunk = sine_16k(150.0, 0.4, VAD_CHUNK);
        // Warmup (DLL, caches).
        for _ in 0..3 {
            let _ = sess.score(&chunk).expect("warmup");
        }
        sess.reset_state();
        let mut dts_ms: Vec<f64> = Vec::with_capacity(64);
        for _ in 0..64 {
            let t0 = Instant::now();
            let _ = sess.score(&chunk).expect("forward latence");
            dts_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let p50 = percentile(&mut dts_ms.clone(), 50.0);
        let p95 = percentile(&mut dts_ms, 95.0);
        eprintln!("golden latence/chunk CPU: p50={p50:.3}ms p95={p95:.3}ms");
        assert!(p50 < 50.0, "p50={p50:.3}ms doit être < 50ms");
        assert!(p95 < 50.0, "p95={p95:.3}ms doit être < 50ms");
    }

    #[test]
    fn golden_inference_engine_is_real() {
        use crate::{InferenceEngine, ModelStatus};
        let sess = open_real_session();
        assert!(sess.is_real());
        assert_eq!(sess.status(), ModelStatus::Real);
        assert_eq!(sess.engine_id(), "silero-vad-ort");
        // run_real exécute un VRAI forward ort (session éphémère).
        let mut map = HashMap::new();
        map.insert(
            "input".to_string(),
            crate::candle_runtime::TensorBuffer::new(vec![1, VAD_CHUNK], vec![0.0; VAD_CHUNK]),
        );
        let out = sess.run_real(map).expect("run_real silence");
        let prob = out
            .get("output")
            .and_then(|t| t.data.first().copied())
            .expect("tenseur output");
        eprintln!("golden run_real silence: prob={prob:.4}");
        assert!(prob < 0.3);
        // Entrée invalide -> erreur LOUD, jamais de valeur simulée.
        let bad = HashMap::new();
        assert!(sess.run_real(bad).is_err());
    }

    #[test]
    fn golden_resolve_paths_and_loud_error() {
        // Résolution par défaut trouve le vrai modèle.
        let p = resolve_vad_model_path(None).expect("résolution par défaut");
        assert!(p.exists());
        assert!(
            p.file_name()
                .map(|n| n == VAD_MODEL_FILENAME)
                .unwrap_or(false)
        );
        // $HAKILLE_MODELS_DIR est honoré quand il contient le modèle.
        let dir = p.parent().expect("parent du modèle").to_path_buf();
        // `unsafe` exigé par l'édition 2024 (env partagé du processus).
        unsafe {
            std::env::set_var("HAKILLE_MODELS_DIR", &dir);
        }
        let via_env = resolve_vad_model_path(Some(Path::new("/chemin/inexistant/quasi-certain")))
            .expect("repli sur $HAKILLE_MODELS_DIR");
        unsafe {
            std::env::remove_var("HAKILLE_MODELS_DIR");
        }
        assert_eq!(via_env.parent().map(|d| d.to_path_buf()), Some(dir));
        // Fichier absent -> erreur LOUD explicite (jamais de fallback silencieux).
        let missing = std::env::temp_dir().join("hakille_vad_missing_probe_nope.onnx");
        let _ = std::fs::remove_file(&missing);
        let err2 = OrtVadSession::open(&missing);
        assert!(err2.is_err());
        let msg = match err2 {
            Ok(_) => panic!("ouverture d'un modèle absent aurait dû échouer"),
            Err(e) => format!("{e}"),
        };
        assert!(msg.contains("modèle absent"), "erreur LOUD attendue: {msg}");
        // Score avec mauvaise taille -> erreur LOUD.
        let mut sess = open_real_session();
        assert!(sess.score(&[0.0f32; 100]).is_err());
    }
}
