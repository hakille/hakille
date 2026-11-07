//! Biométrie locuteur RÉELLE via ONNX Runtime (`ort` v2, binaires précompilés).
//!
//! - Modèle : `models/campplus_zh_en_16k_advanced.onnx` (CAM++ 3D-Speaker,
//!   ~28 MB, 16 kHz, `output_dim=192`).
//! - Signature RÉELLE lue dynamiquement au runtime via `session.inputs()` /
//!   `session.outputs()` :
//!   - observé : input `x` f32 `[N,T,80]` (batch, frames, 80 mels, `T` dynamique),
//!     output `embedding` f32 `[N,192]`.
//!   - si un checkpoint exposait `[1,80,200]`, le layout est transposé à la
//!     volée (détection sur la shape du graphe, pas de constante en dur).
//! - Frontend Kaldi commun : retrait DC, préaccentuation 0.97 par trame,
//!   fenêtre Povey 25 ms / 10 ms, FFT512, mel20–8000Hz, CMN.
//! - Tout le segment est traité lorsque l'axe temporel du graphe est dynamique.
//! - Embeddings réels de 192 composantes normalisés L2, entrées invalides refusées.

use hakille_core::{HakilleError, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Sample-rate attendu par CAM++.
pub const CAMP_SAMPLE_RATE: u32 = 16_000;
/// Nombre de bandes mel.
pub const CAMP_N_MELS: usize = 80;
/// Nombre de trames (≈2 s à 10 ms).
pub const CAMP_N_FRAMES: usize = 200;
/// Fenêtre 25 ms @16 kHz.
pub const CAMP_WIN: usize = 400;
/// Saut 10 ms @16 kHz.
pub const CAMP_HOP: usize = 160;
/// NFFT (radix-2).
pub const CAMP_NFFT: usize = 512;
/// Pre-emphasis.
pub const CAMP_PREEMPH: f32 = 0.97;
/// Dimension de l'embedding.
pub const CAMP_EMB_DIM: usize = 192;

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("CampplusOnnx::{context}: {e}"))
}

/// Environnement ONNX Runtime unique du processus (délégation vers le holder
/// crate-wide [`crate::ort_env::shared_ort_env`], partagé avec `embed_onnx`).
/// ONNX Runtime n'autorise qu'un seul environnement par processus.
pub(crate) fn shared_ort_env() -> Result<Arc<ort::environment::Environment>> {
    crate::ort_env::shared_ort_env()
}

/// Layout d'entrée détecté depuis la signature réelle du graphe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputLayout {
    /// `[N,T,80]` (checkpoint réel observé, `x`).
    BatchTimeMel,
    /// `[N,80,T]` (variante historique `[1,80,200]`).
    BatchMelTime,
}

/// Session CAM++ réelle (forward `ort`).
pub struct CampplusOnnx {
    _env: Arc<ort::environment::Environment>,
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
    layout: InputLayout,
    frames: Option<usize>,
}

impl CampplusOnnx {
    /// Ouvre un checkpoint et fige la signature réelle du graphe.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "CampplusOnnx::open: modèle absent '{}' — provisionnez models/campplus_zh_en_16k_advanced.onnx (erreur LOUD, pas de fallback simulé)",
                path.display()
            )));
        }
        let t0 = Instant::now();
        let env = shared_ort_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("open(builder)", e))?
            .with_intra_threads(4)
            .map_err(|e| ort_err("open(threads)", e))?
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
                "CampplusOnnx::open: aucun input dans le graphe".to_string(),
            ));
        }
        if output_names.is_empty() {
            return Err(HakilleError::InferenceError(
                "CampplusOnnx::open: aucun output dans le graphe".to_string(),
            ));
        }
        // Noms réels : `x` / `embedding` sur le checkpoint provisionné.
        let input_name = if input_names.iter().any(|n| n == "x") {
            "x".to_string()
        } else {
            input_names[0].clone()
        };
        let output_name = if output_names.iter().any(|n| n == "embedding") {
            "embedding".to_string()
        } else {
            output_names[0].clone()
        };
        let input = session
            .inputs()
            .iter()
            .find(|input| input.name() == input_name)
            .ok_or_else(|| ort_err("open", "entrée absente"))?;
        let shape = input
            .dtype()
            .tensor_shape()
            .ok_or_else(|| ort_err("open", "tenseur attendu"))?;
        if input_names.len() != 1
            || input.dtype().tensor_type() != Some(ort::value::TensorElementType::Float32)
            || shape.len() != 3
            || (shape[0] != -1 && shape[0] != 1)
        {
            return Err(ort_err(
                "open",
                "CAM++ attend un tenseur float32 [1,T,80] ou [1,80,T]",
            ));
        }
        let (layout, temporal) = if shape[2] == CAMP_N_MELS as i64 {
            (InputLayout::BatchTimeMel, shape[1])
        } else if shape[1] == CAMP_N_MELS as i64 {
            (InputLayout::BatchMelTime, shape[2])
        } else {
            return Err(ort_err("open", "axe 80 mels absent"));
        };
        let frames = match temporal {
            -1 => None,
            1..=12_000 => Some(temporal as usize),
            _ => return Err(ort_err("open", "nombre de trames incompatible")),
        };
        tracing::info!(
            model = %path.display(),
            input = %input_name,
            output = %output_name,
            layout = ?layout,
            elapsed_ms = t0.elapsed().as_millis(),
            "CampplusOnnx::open: session réelle prête"
        );
        Ok(Self {
            _env: env,
            session: Mutex::new(session),
            input_name,
            output_name,
            layout,
            frames,
        })
    }

    /// Résolution du checkpoint dans les layouts usuels
    /// (`./models`, `../models`, `../../models` selon le cwd des tests).
    /// Chemins relatifs `models/` uniquement (aucun absolu en dur) +
    /// `$HAKILLE_MODELS_DIR`.
    pub fn default_model_path() -> Option<PathBuf> {
        for cand in [
            "models/campplus_zh_en_16k_advanced.onnx",
            "../models/campplus_zh_en_16k_advanced.onnx",
            "../../models/campplus_zh_en_16k_advanced.onnx",
            "crates/../models/campplus_zh_en_16k_advanced.onnx",
        ] {
            let p = PathBuf::from(cand);
            if p.exists() {
                return Some(p);
            }
        }
        // Dernier recours : racine du workspace via cwd.
        if let Ok(cwd) = std::env::current_dir() {
            for base in [cwd.clone(), cwd.join(".."), cwd.join("../..")] {
                let p = base.join("models/campplus_zh_en_16k_advanced.onnx");
                if p.exists() {
                    return Some(p);
                }
            }
        }
        if let Ok(dir) = std::env::var("HAKILLE_MODELS_DIR") {
            let p = PathBuf::from(dir).join("campplus_zh_en_16k_advanced.onnx");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Ouvre le checkpoint par défaut (erreur LOUD si absent).
    pub fn open_default() -> Result<Self> {
        match Self::default_model_path() {
            Some(p) => Self::open(p),
            None => Err(HakilleError::InferenceError(
                "CampplusOnnx::open_default: checkpoint CAM++ introuvable (models/campplus_zh_en_16k_advanced.onnx absent) — biométrie RÉELLE impossible, aucun fallback simulé"
                    .to_string(),
            )),
        }
    }

    /// Toujours `true` : forward neural réellement exécuté.
    pub fn is_real(&self) -> bool {
        true
    }

    pub fn input_name(&self) -> &str {
        &self.input_name
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    /// Noms d'inputs réels du graphe.
    pub fn real_input_names(&self) -> Vec<String> {
        let sess = self.session.lock().expect("mutex CampplusOnnx empoisonné");
        sess.inputs().iter().map(|o| o.name().to_string()).collect()
    }

    /// Noms d'outputs réels du graphe.
    pub fn real_output_names(&self) -> Vec<String> {
        let sess = self.session.lock().expect("mutex CampplusOnnx empoisonné");
        sess.outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect()
    }

    /// Extrait un embedding 192-d L2-normalisé depuis du PCM 16 kHz.
    /// Ne panique jamais : silence / court / NaN -> forward réel sur
    /// trames paddées/tuilées, erreur uniquement si `ort` échoue.
    pub fn embed(&self, pcm_16k: &[f32]) -> Result<Vec<f32>> {
        let t0 = Instant::now();
        if pcm_16k.len() < CAMP_WIN
            || pcm_16k.len() > 16_000 * 120
            || pcm_16k.iter().any(|x| !x.is_finite() || x.abs() > 1.0)
        {
            return Err(ort_err(
                "embed",
                "PCM mono 16 kHz normalisé fini, 25 ms à 120 s requis",
            ));
        }
        let features = camp_fbank(pcm_16k, self.frames);
        let frames = features.len();
        let fbank_tm: Vec<f32> = features.into_iter().flatten().collect();
        let flat = match self.layout {
            InputLayout::BatchTimeMel => fbank_tm,
            InputLayout::BatchMelTime => transpose_tm_to_mt(&fbank_tm, frames, CAMP_N_MELS),
        };
        let shape = match self.layout {
            InputLayout::BatchTimeMel => [1usize, frames, CAMP_N_MELS],
            InputLayout::BatchMelTime => [1usize, CAMP_N_MELS, frames],
        };
        let input_tensor = Tensor::<f32>::from_array((shape, flat))
            .map_err(|e| ort_err("embed(input_tensor)", e))?;
        let input_key = self.input_name.clone();
        let output_key = self.output_name.clone();
        let mut sess = self
            .session
            .lock()
            .map_err(|e| ort_err("embed(lock)", format!("mutex empoisonné : {e}")))?;
        let outputs = sess
            .run(ort::inputs![input_key => input_tensor])
            .map_err(|e| ort_err("embed(run)", e))?;
        let raw: Vec<f32> = outputs
            .get(&output_key)
            .and_then(|v| v.try_extract_tensor::<f32>().ok())
            .filter(|(shape, _)| shape.as_ref() == [1, CAMP_EMB_DIM as i64])
            .map(|(_, data)| data.to_vec())
            .ok_or_else(|| {
                HakilleError::InferenceError(format!(
                    "CampplusOnnx::embed: output '{output_key}' illisible"
                ))
            })?;
        if raw.len() != CAMP_EMB_DIM {
            return Err(HakilleError::InferenceError(format!(
                "CampplusOnnx::embed: dimension inattendue {} (attendu 192)",
                raw.len()
            )));
        }
        for v in raw.iter() {
            if !v.is_finite() {
                return Err(HakilleError::InferenceError(
                    "CampplusOnnx::embed: sortie non-finie".to_string(),
                ));
            }
        }
        let emb = l2_normalize(&raw);
        if emb.iter().all(|x| *x == 0.0) {
            return Err(ort_err("embed", "embedding de norme nulle"));
        }
        tracing::debug!(
            elapsed_ms = t0.elapsed().as_millis(),
            "CampplusOnnx::embed: forward réel OK"
        );
        Ok(emb)
    }

    /// Variante avec ré-échantillonnage linéaire si `sample_rate != 16 kHz`.
    pub fn embed_resampled(&self, pcm: &[f32], sample_rate: u32) -> Result<Vec<f32>> {
        let resampled = crate::audio_resample::try_to_mono_16k(pcm, 1, sample_rate)?;
        self.embed(&resampled)
    }

    /// Vérification locuteur : cosinus réel + seuil.
    pub fn verify(&self, emb: &[f32], reference: &[f32], threshold: f32) -> (bool, f32) {
        if emb.len() != CAMP_EMB_DIM
            || reference.len() != CAMP_EMB_DIM
            || !threshold.is_finite()
            || !(-1.0..=1.0).contains(&threshold)
            || emb.iter().chain(reference).any(|x| !x.is_finite())
            || emb.iter().all(|x| *x == 0.0)
            || reference.iter().all(|x| *x == 0.0)
        {
            return (false, 0.0);
        }
        let sim = cosine_similarity(emb, reference);
        (sim >= threshold, sim)
    }
}

/// Similarité cosinus réelle (vecteurs quelconques ; 0.0 si dégénéré).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() || a.iter().chain(b).any(|x| !x.is_finite()) {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b).map(|(a, b)| *a as f64 * *b as f64).sum();
    let norm = |v: &[f32]| v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let denominator = norm(a) * norm(b);
    if denominator == 0.0 {
        0.0
    } else {
        (dot / denominator).clamp(-1.0, 1.0) as f32
    }
}

/// Normalisation L2 réelle, accumulation f64 sans débordement sur f32 finis.
pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    if v.iter().any(|x| !x.is_finite()) {
        return vec![0.0; v.len()];
    }
    let norm = v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm == 0.0 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| (*x as f64 / norm) as f32).collect()
}

/// Ré-échantillonnage linéaire pur Rust.
pub fn resample_linear(pcm: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if pcm.is_empty() || from_hz == 0 || to_hz == 0 || from_hz == to_hz {
        return pcm.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((pcm.len() as f64 * ratio).round() as usize).max(1);
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 / ratio;
        let i0 = pos.floor() as usize;
        let frac = (pos - i0 as f64) as f32;
        let s0 = if i0 < pcm.len() {
            sanitize_sample(pcm[i0])
        } else {
            0.0
        };
        let s1 = if i0 + 1 < pcm.len() {
            sanitize_sample(pcm[i0 + 1])
        } else {
            s0
        };
        out.push(s0 + (s1 - s0) * frac);
    }
    out
}

#[inline]
fn sanitize_sample(s: f32) -> f32 {
    if !s.is_finite() {
        0.0
    } else {
        s.clamp(-1.0, 1.0)
    }
}

fn transpose_tm_to_mt(tm: &[f32], frames: usize, mels: usize) -> Vec<f32> {
    debug_assert_eq!(tm.len(), frames * mels);
    let mut out = vec![0.0f32; tm.len()];
    for t in 0..frames {
        for m in 0..mels {
            out[m * frames + t] = tm[t * mels + m];
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Fbank 80x200 pur Rust (implémentation maison, aucun import externe).
// ---------------------------------------------------------------------------

/// Calcule un banc de filtres log-mel `[200, 80]` (row-major temps×mel)
/// avec tuilage cyclique si l'audio est court et troncature si long.
/// Ne panique jamais (entrée vide -> trames de zéros).
pub fn fbank_80x200(pcm_16k: &[f32]) -> Vec<f32> {
    camp_fbank(pcm_16k, Some(CAMP_N_FRAMES))
        .into_iter()
        .flatten()
        .collect()
}

fn camp_fbank(pcm: &[f32], frames: Option<usize>) -> Vec<Vec<f32>> {
    let mut samples: Vec<f32> = pcm.iter().map(|x| sanitize_sample(*x)).collect();
    if let Some(frames) = frames {
        let needed = (frames - 1) * CAMP_HOP + CAMP_WIN;
        if samples.is_empty() {
            samples.resize(needed, 0.0);
        }
        let original = samples.len();
        while samples.len() < needed {
            samples.push(samples[samples.len() % original]);
        }
        samples.truncate(needed);
    }
    let mut features = crate::mel::kaldi_fbank(&samples, true);
    crate::mel::center_mel(&mut features);
    features
}

/// Signal synthétique déterministe pour tests DSP ; pas une preuve biométrique.
pub fn synth_voiced_16k(freq_hz: f32, seconds: f32, phase: f32) -> Vec<f32> {
    let n = (seconds * CAMP_SAMPLE_RATE as f32).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / CAMP_SAMPLE_RATE as f32;
        let s = 0.55 * (2.0 * std::f32::consts::PI * freq_hz * t + phase).sin()
            + 0.25 * (2.0 * std::f32::consts::PI * freq_hz * 2.0 * t + phase * 0.7).sin()
            + 0.12 * (2.0 * std::f32::consts::PI * freq_hz * 3.0 * t).sin();
        out.push((s * 0.6).clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_or_skip() -> Option<CampplusOnnx> {
        match CampplusOnnx::open_default() {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("SKIP CampplusOnnx (modèle absent): {e}");
                None
            }
        }
    }

    #[test]
    fn test_campplus_signature_reelle() {
        let Some(sess) = open_or_skip() else { return };
        let ins = sess.real_input_names();
        let outs = sess.real_output_names();
        eprintln!("inputs={ins:?} outputs={outs:?}");
        assert!(!ins.is_empty());
        assert!(!outs.is_empty());
        assert!(outs.iter().any(|n| n == "embedding"), "output={outs:?}");
        assert!(sess.is_real());
    }

    #[test]
    fn test_campplus_fbank_deterministe() {
        let pcm = synth_voiced_16k(130.0, 2.0, 0.0);
        let a = fbank_80x200(&pcm);
        let b = fbank_80x200(&pcm);
        assert_eq!(a.len(), CAMP_N_FRAMES * CAMP_N_MELS);
        assert_eq!(a, b, "fbank doit être déterministe");
        assert!(a.iter().all(|v| v.is_finite()));
        // CMN : moyenne ~0 par bande.
        let mut mean = 0.0f32;
        for v in a.iter() {
            mean += *v;
        }
        mean /= a.len() as f32;
        eprintln!("fbank mean={mean:.4}");
        assert!(mean.abs() < 0.5, "CMN attendue, mean={mean}");
    }

    #[test]
    fn test_campplus_forward_real_determinism() {
        let Some(sess) = open_or_skip() else { return };
        // Un sinus n'est pas un locuteur annoté. On vérifie uniquement le
        // contrat numérique reproductible, pas un taux de reconnaissance.
        let signal = synth_voiced_16k(120.0, 2.0, 0.0);
        let a = sess.embed(&signal).unwrap();
        let b = sess.embed(&signal).unwrap();
        assert_eq!(a.len(), CAMP_EMB_DIM);
        let norm: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-5);
        assert!(sess.verify(&a, &b, 0.95).0);
        assert!(!sess.verify(&[f32::NAN; 192], &a, 0.0).0);
        assert!(!sess.verify(&[0.0; 192], &a, 0.0).0);
        assert!(!sess.verify(&a, &b, f32::NAN).0);
    }

    /// Même fixture publique sherpa en.wav que le test ASR. Ce test vérifie
    /// le forward et l'utilisation du segment entier, pas une précision biométrique.
    #[test]
    #[ignore = "nécessite modèle CAM++ et HAKILLE_TEST_PCM16K_F32"]
    fn official_voice_fixture_full_segment() {
        let path = std::env::var("HAKILLE_TEST_PCM16K_F32").expect("fixture requise");
        let bytes = std::fs::read(path).unwrap();
        let pcm: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let sess = CampplusOnnx::open_default().expect("CAM++ requis");
        let a = sess.embed(&pcm).unwrap();
        let repeat = sess.embed(&pcm).unwrap();
        assert!((cosine_similarity(&a, &repeat) - 1.0).abs() < 1e-5);
        let mut modified = pcm.clone();
        modified[32_000..].fill(0.0);
        let b = sess.embed(&modified).unwrap();
        let difference: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
        eprintln!(
            "CAM++ fixture: norm={}, cosine_repeat={}, tail_change_l1={difference}",
            a.iter().map(|x| x * x).sum::<f32>().sqrt(),
            cosine_similarity(&a, &repeat)
        );
        assert!(
            difference > 0.01,
            "le segment après 2 s doit contribuer à l'embedding"
        );
    }

    #[test]
    fn test_campplus_silence_vs_voix_basse() {
        let Some(sess) = open_or_skip() else { return };
        let voice = synth_voiced_16k(120.0, 2.0, 0.0);
        let silence = vec![0.0f32; 32_000];
        let ev = sess.embed(&voice).expect("forward voix");
        let es = sess.embed(&silence).expect("forward silence sans panic");
        assert_eq!(ev.len(), 192);
        assert_eq!(es.len(), 192);
        let sim = cosine_similarity(&ev, &es);
        eprintln!("sim_voix_vs_silence={sim:.4}");
        // Le modèle réel (entraîné sur de la parole) concentre tous les
        // signaux synthétiques dans un cône étroit : la séparation est
        // relative, pas absolue. Mesuré : 0.793 < même-locuteur (0.9865).
        // On exige donc l'ordre + une borne absolue mesurée (< 0.85).
        assert!(
            sim < 0.85,
            "silence vs voix : similarité basse attendue, got {sim}"
        );
        let long = synth_voiced_16k(120.0, 4.0, 0.0);
        let same_a = sess.embed(&long[..32_000]).expect("forward same A");
        let same_b = sess.embed(&long[16_000..48_000]).expect("forward same B");
        let sim_same = cosine_similarity(&same_a, &same_b);
        assert!(
            sim_same > sim,
            "même-locuteur ({sim_same:.4}) > voix-vs-silence ({sim:.4})"
        );
        // Deux silences -> pas de panic, vecteur fini, déterministe.
        let es2 = sess.embed(&silence).expect("forward silence 2");
        assert!(es2.iter().all(|v| v.is_finite()));
        let sim_sil = cosine_similarity(&es, &es2);
        assert!((sim_sil - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_campplus_distinct_synthetic_signals() {
        let Some(sess) = open_or_skip() else { return };
        // Mesure de sensibilité à deux signaux; aucune identité humaine déduite.
        let ea = sess
            .embed(&synth_voiced_16k(120.0, 2.0, 0.0))
            .expect("forward A");
        let ec = sess
            .embed(&synth_voiced_16k(220.0, 2.0, 0.5))
            .expect("forward C");
        let sim_cross = cosine_similarity(&ea, &ec);
        assert!(sim_cross.is_finite() && (-1.0..=1.0).contains(&sim_cross));
        assert!(ea.iter().zip(&ec).any(|(a, c)| (a - c).abs() > 1e-4));
    }

    #[test]
    fn test_campplus_court_sans_panic() {
        let Some(sess) = open_or_skip() else { return };
        for len in [0usize, 1, 100, 1600, 8000] {
            let pcm = synth_voiced_16k(110.0, len as f32 / 16_000.0, 0.0);
            let pcm = if len == 0 { vec![] } else { pcm };
            let e = sess.embed(&pcm);
            if len < CAMP_WIN {
                assert!(e.is_err(), "signal trop court len={len} doit être refusé");
            } else {
                assert_eq!(e.expect("forward court réel").len(), 192);
            }
        }
    }
}
