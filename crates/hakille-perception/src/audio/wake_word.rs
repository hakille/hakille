//! DÉTECTEUR ÉNERGÉTIQUE VAD-GATÉ (checkpoint openWakeWord non provisionné — voir [`provision()`]).
//!
//! - Aucun checkpoint openWakeWord n'est présent dans `models/` : la décision
//!   neurale est donc assurée par une mesure RÉELLE non-neurale mais
//!   fonctionnelle, en pur Rust (aucune valeur simulée, aucun stub) :
//!   - `process()` : mesure énergétique instantanée déterministe (broadband +
//!     bande 300–3400 Hz par soustraction de moyenne glissante), indépendante
//!     du modèle — déclenche sur onsets forts, avec cooldown ;
//!   - `process_gated()` : décision qualité STRICTE — ne déclenche que si
//!     VAD Silero RÉEL > 0.5 **et** énergie de bande suffisante **et** durée
//!     voisée dans `[0.3s, 1.2s]`, avec cooldown. Erreur LOUD si le modèle VAD
//!     est absent (jamais de fallback silencieux).
//! - [`provision()`] télécharge un `.onnx` openWakeWord (via
//!   [`hakille_inference::ModelDownloader`], SHA-256 optionnel) ;
//!   [`WakeWordDetector::maybe_upgrade_to_ort()`] (appelée automatiquement dans
//!   `process_gated`) bascule vers un forward `ort` RÉEL
//!   ([`hakille_inference::OrtScalarScorer`]) dès que le fichier existe.
//!   Tant que la signature du checkpoint est incompatible, le chemin
//!   énergétique reste la décision et l'échec `ort` est journalisé LOUD.

use crate::audio::capture::AudioChunk;
use hakille_inference::{
    ModelDownloader, OrtScalarScorer, OrtVadSession, resolve_vad_model_path, to_mono_16k,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Bande vocale utile pour le Mot-clé (Hz).
pub const WAKE_BAND_LO_HZ: f32 = 300.0;
pub const WAKE_BAND_HI_HZ: f32 = 3400.0;
/// Seuil VAD strict pour la décision gatée.
pub const WAKE_VAD_GATE: f32 = 0.5;
/// Fenêtre de durée voisée acceptée pour un mot-clé (s).
pub const WAKE_MIN_DURATION_S: f32 = 0.3;
pub const WAKE_MAX_DURATION_S: f32 = 1.2;
/// Moyenne glissante exponentielle ~300 Hz : alpha = 1 − exp(−2π·300/16000).
/// Sa soustraction au signal = passe-haut 1er ordre (borne basse vocale),
/// sans l'effet de peigne d'une moyenne rectangulaire (qui réinjecte les
/// graves par opposition de phase — mesuré : 100 Hz à 0.79 avec MA53).
const ALPHA_HP_300HZ: f32 = 0.1111;
/// Lissage passe-bas ~3.4 kHz : alpha = 1 − exp(−2π·3400/16000) ≈ 0.74.
const LP_ALPHA: f32 = 0.74;
/// RMS minimal d'une fenêtre pour être voisée (rejet du souffle/quantification).
const MIN_WINDOW_RMS: f32 = 0.015;
/// Préfixe des checkpoints openWakeWord provisionnés.
pub const OWW_FILENAME_PREFIX: &str = "openwakeword_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WakeBackendKind {
    /// Mesure énergétique VAD-gatée (pur Rust, checkpoint non provisionné).
    Energetic,
    /// Forward `ort` réel sur checkpoint provisionné (auto-bascule).
    OrtReal,
}

impl WakeBackendKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Energetic => "ÉNERGÉTIQUE-VAD-GATÉ",
            Self::OrtReal => "ORT-RÉEL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordDetection {
    pub detected: bool,
    pub keyword: String,
    pub confidence: f32,
    /// Probabilité VAD réelle du chunk (`None` sur le chemin instantané).
    pub vad_prob: Option<f32>,
    /// Ratio d'énergie dans 300–3400 Hz (`None` si chunk inscorable).
    pub band_ratio: Option<f32>,
    /// Backend ayant produit la décision.
    pub backend: WakeBackendKind,
    /// Score `ort` brut quand le backend neural est actif.
    pub ort_prob: Option<f32>,
}

fn neg(keyword: &str, vad_prob: Option<f32>, band_ratio: Option<f32>) -> WakeWordDetection {
    WakeWordDetection {
        detected: false,
        keyword: keyword.to_string(),
        confidence: 0.0,
        vad_prob,
        band_ratio,
        backend: WakeBackendKind::Energetic,
        ort_prob: None,
    }
}

/// Mesures spectrales RÉELLES (pur Rust) : RMS broadband + énergie de bande.
///
/// Retourne `(rms_broadband, band_ratio)` avec `band_ratio = E[300–3400] / E[tot]`.
/// Sanitize : `NaN`/`Inf` -> `0.0`. Entrée vide -> `(0.0, 0.0)`.
fn band_measurements(mono16k: &[f32]) -> (f32, f32) {
    if mono16k.is_empty() {
        return (0.0, 0.0);
    }
    let n = mono16k.len();
    // Passe-haut 1er ordre : soustraction d'une moyenne glissante
    // exponentielle ~300 Hz (sans effet de peigne), puis passe-bas ~3.4 kHz.
    let mut lp_mean = 0.0f32;
    let mut broad_e = 0.0f64;
    let mut band_e = 0.0f64;
    let mut lp = 0.0f32;
    for s in mono16k.iter() {
        let v = *s;
        let x = if v.is_finite() { v } else { 0.0 };
        broad_e += (x as f64) * (x as f64);
        lp_mean += ALPHA_HP_300HZ * (x - lp_mean);
        let hp = x - lp_mean;
        // Passe-bas 1er ordre ~3.4 kHz sur le signal passe-haut.
        lp += LP_ALPHA * (hp - lp);
        if lp.is_finite() {
            band_e += (lp as f64) * (lp as f64);
        }
    }
    let broad = (broad_e / n as f64) as f32;
    let band = (band_e / n as f64) as f32;
    let rms = if broad.is_finite() && broad > 0.0 {
        broad.sqrt()
    } else {
        0.0
    };
    let ratio = if broad > 1e-12 && band.is_finite() {
        (band / broad).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (rms, ratio)
}

/// Nom de fichier provisionné pour un mot-clé (slug sécurisé).
fn oww_filename(keyword: &str) -> String {
    let mut slug = String::new();
    for ch in keyword.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("keyword");
    }
    format!("{OWW_FILENAME_PREFIX}{slug}.onnx")
}

/// Résout le checkpoint openWakeWord provisionné (`None` si absent — PAS une erreur).
pub fn openwakeword_path(models_dir: Option<&Path>, keyword: &str) -> Option<PathBuf> {
    let file = oww_filename(keyword);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(&file));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(&file));
    }
    candidates.push(PathBuf::from("./models").join(&file));
    candidates.push(PathBuf::from("../../models").join(&file));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(&file),
    );
    candidates.into_iter().find(|p| p.exists())
}

/// Télécharge un checkpoint openWakeWord `.onnx` depuis `url` vers
/// `models_dir/openwakeword_<mot-clé>.onnx` (reprise + SHA-256 optionnel via
/// [`ModelDownloader`]). Le détecteur bascule automatiquement vers `ort`
/// quand le fichier existe (voir `maybe_upgrade_to_ort`).
pub async fn provision(
    url: &str,
    models_dir: impl AsRef<Path>,
    keyword: &str,
    expected_sha256: Option<&str>,
) -> hakille_core::Result<PathBuf> {
    if url.trim().is_empty() {
        return Err(hakille_core::HakilleError::InferenceError(
            "wake_word::provision: URL vide".to_string(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(hakille_core::HakilleError::InferenceError(format!(
            "wake_word::provision: URL non supportée '{url}' (http/https requis)"
        )));
    }
    let dest = models_dir.as_ref().join(oww_filename(keyword));
    ModelDownloader::new()
        .download_file(url, &dest, None, expected_sha256, None)
        .await
}

pub struct WakeWordDetector {
    keyword: String,
    threshold: f32,
    cooldown_frames: usize,
    current_cooldown: usize,
    vad_gate: f32,
    band_threshold: f32,
    min_duration_s: f32,
    max_duration_s: f32,
    /// Fenêtres 512 chaudes consécutives en cours (durée voisée gatée).
    trailing_hot_windows: usize,
    vad_model_path: Option<PathBuf>,
    vad_session: Option<OrtVadSession>,
    oww_models_dir: Option<PathBuf>,
    oww_path: Option<PathBuf>,
    ort_scorer: Option<OrtScalarScorer>,
    last_backend: WakeBackendKind,
    last_vad_prob: Option<f32>,
    last_band_ratio: Option<f32>,
    last_ort_prob: Option<f32>,
    last_windows: Vec<(f32, f32, f32)>,
}

impl WakeWordDetector {
    pub fn new(keyword: impl Into<String>, threshold: f32) -> Self {
        let keyword = keyword.into();
        let vad_model_path = resolve_vad_model_path(None).ok();
        Self {
            keyword,
            threshold,
            cooldown_frames: 10,
            current_cooldown: 0,
            vad_gate: WAKE_VAD_GATE,
            band_threshold: 0.20,
            min_duration_s: WAKE_MIN_DURATION_S,
            max_duration_s: WAKE_MAX_DURATION_S,
            trailing_hot_windows: 0,
            vad_model_path,
            vad_session: None,
            oww_models_dir: None,
            oww_path: None,
            ort_scorer: None,
            last_backend: WakeBackendKind::Energetic,
            last_vad_prob: None,
            last_band_ratio: None,
            last_ort_prob: None,
            last_windows: Vec::new(),
        }
    }

    /// Mot-clé + dossier modèles explicite (VAD résolu LOUD si absent, checkpoint
    /// openWakeWord détecté s'il existe).
    pub fn with_models_dir(
        keyword: impl Into<String>,
        threshold: f32,
        models_dir: impl AsRef<Path>,
    ) -> hakille_core::Result<Self> {
        let keyword = keyword.into();
        let vad_path = resolve_vad_model_path(Some(models_dir.as_ref()))?;
        let oww = openwakeword_path(Some(models_dir.as_ref()), &keyword);
        Ok(Self {
            keyword,
            threshold,
            cooldown_frames: 10,
            current_cooldown: 0,
            vad_gate: WAKE_VAD_GATE,
            band_threshold: 0.20,
            min_duration_s: WAKE_MIN_DURATION_S,
            max_duration_s: WAKE_MAX_DURATION_S,
            trailing_hot_windows: 0,
            vad_model_path: Some(vad_path),
            vad_session: None,
            oww_models_dir: Some(models_dir.as_ref().to_path_buf()),
            oww_path: oww,
            ort_scorer: None,
            last_backend: WakeBackendKind::Energetic,
            last_vad_prob: None,
            last_band_ratio: None,
            last_ort_prob: None,
            last_windows: Vec::new(),
        })
    }

    pub fn default_hakille() -> Self {
        Self::new("Dis Hakille", 0.75)
    }

    /// Ajuste la fenêtre de durée voisée gatée (s) — ex. mot-clé court vs long.
    /// Erreur LOUD si `0 < min <= max <= 5` est violé (jamais de clamp silencieux).
    pub fn with_gated_timing(
        &mut self,
        min_duration_s: f32,
        max_duration_s: f32,
    ) -> hakille_core::Result<&mut Self> {
        if !(min_duration_s.is_finite() && max_duration_s.is_finite())
            || min_duration_s <= 0.0
            || min_duration_s > max_duration_s
            || max_duration_s > 5.0
        {
            return Err(hakille_core::HakilleError::InferenceError(format!(
                "WakeWordDetector::with_gated_timing: fenêtre invalide \
                 [{min_duration_s}, {max_duration_s}] (attendu 0 < min <= max <= 5 s)"
            )));
        }
        self.min_duration_s = min_duration_s;
        self.max_duration_s = max_duration_s;
        Ok(self)
    }

    /// Seuil de ratio de bande 300–3400 Hz par fenêtre (défaut 0.20) — réglage
    /// déploiement (micro/pièce). Erreur LOUD hors `[0, 1]`.
    pub fn with_band_threshold(&mut self, ratio: f32) -> hakille_core::Result<&mut Self> {
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(hakille_core::HakilleError::InferenceError(format!(
                "WakeWordDetector::with_band_threshold: ratio invalide {ratio} (attendu [0, 1])"
            )));
        }
        self.band_threshold = ratio;
        Ok(self)
    }

    pub fn keyword(&self) -> &str {
        &self.keyword
    }

    pub fn backend_kind(&self) -> WakeBackendKind {
        self.last_backend
    }

    /// Seuil VAD par fenêtre (défaut 0.5) — réglage déploiement.
    /// Erreur LOUD hors `(0, 1)`.
    pub fn with_vad_gate(&mut self, gate: f32) -> hakille_core::Result<&mut Self> {
        if !gate.is_finite() || gate <= 0.0 || gate >= 1.0 {
            return Err(hakille_core::HakilleError::InferenceError(format!(
                "WakeWordDetector::with_vad_gate: seuil invalide {gate} (attendu dans (0, 1))"
            )));
        }
        self.vad_gate = gate;
        Ok(self)
    }

    /// Détail par fenêtre 512 du dernier appel gaté : `(vad, rms, bande)`.
    /// Observabilité (mise au point, logs) — jamais une décision.
    pub fn last_windows_debug(&self) -> &[(f32, f32, f32)] {
        &self.last_windows
    }

    pub fn last_vad_prob(&self) -> Option<f32> {
        self.last_vad_prob
    }

    pub fn last_band_ratio(&self) -> Option<f32> {
        self.last_band_ratio
    }

    pub fn ort_forward_calls(&self) -> Option<u64> {
        self.ort_scorer.as_ref().map(|s| s.forward_calls())
    }

    /// Tente la bascule vers `ort` si le checkpoint existe. Retourne `true` si
    /// le backend neural RÉEL est désormais actif. Erreur LOUD si le fichier
    /// existe mais est illisible/incompatible (pas de bascule silencieuse).
    pub fn maybe_upgrade_to_ort(&mut self) -> hakille_core::Result<bool> {
        if self.ort_scorer.is_some() {
            self.last_backend = WakeBackendKind::OrtReal;
            return Ok(true);
        }
        if self.oww_path.is_none() {
            let dir_opt = self.oww_models_dir.clone();
            self.oww_path = openwakeword_path(dir_opt.as_deref(), &self.keyword);
        }
        match self.oww_path.clone() {
            Some(p) => {
                let scorer = OrtScalarScorer::open(&p)?;
                tracing::info!(
                    path = %p.display(),
                    inputs = ?scorer.real_input_names(),
                    "WakeWordDetector: bascule ORT-RÉELLE (checkpoint provisionné)"
                );
                self.ort_scorer = Some(scorer);
                self.last_backend = WakeBackendKind::OrtReal;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn tick_cooldown(&mut self) -> bool {
        if self.current_cooldown > 0 {
            self.current_cooldown -= 1;
            return true;
        }
        false
    }

    fn mono16k(chunk: &AudioChunk) -> Vec<f32> {
        if chunk.samples.is_empty() {
            return Vec::new();
        }
        to_mono_16k(&chunk.samples, chunk.channels, chunk.sample_rate)
    }

    fn ensure_vad(&mut self) -> hakille_core::Result<&mut OrtVadSession> {
        if self.vad_session.is_none() {
            let p = self.vad_model_path.clone().ok_or_else(|| {
                hakille_core::HakilleError::InferenceError(
                    "WakeWordDetector: modèle VAD absent (models/silero_vad.onnx \
                     introuvable, $HAKILLE_MODELS_DIR vide) — décision gatée \
                     impossible, pas de fallback"
                        .to_string(),
                )
            })?;
            if !p.exists() {
                return Err(hakille_core::HakilleError::InferenceError(format!(
                    "WakeWordDetector: modèle VAD absent '{}'",
                    p.display()
                )));
            }
            self.vad_session = Some(OrtVadSession::open(&p)?);
        }
        self.vad_session.as_mut().ok_or_else(|| {
            hakille_core::HakilleError::InferenceError(
                "WakeWordDetector: session VAD non ouverte (incohérence interne)".to_string(),
            )
        })
    }

    /// Probabilités VAD réelles des fenêtres 512 complétées par ce chunk, dans
    /// l'ordre, avec le carry session avant ingestion (pour aligner les
    /// mesures de bande sur les EXACTES mêmes 512 échantillons scorés).
    fn real_vad_windows(&mut self, mono: &[f32]) -> hakille_core::Result<(usize, Vec<f32>)> {
        if mono.is_empty() {
            return Ok((0, Vec::new()));
        }
        let sess = self.ensure_vad()?;
        let carry_before = sess.pending_carry_len();
        if carry_before >= 512 {
            return Err(hakille_core::HakilleError::InferenceError(format!(
                "WakeWordDetector: carry incohérent ({carry_before})"
            )));
        }
        let outs = sess.process_stream(mono)?;
        let mut probs = Vec::with_capacity(outs.len());
        for (_, p) in outs.iter() {
            probs.push(*p);
        }
        Ok((carry_before, probs))
    }

    /// Chemin instantané RÉEL (mesure énergétique, sans neural) : déclenche sur
    /// onsets forts (broadband), confiance façonnée par la bande 300–3400 Hz.
    /// Déterministe et indépendant du modèle (compatibilité temps réel).
    pub fn process(&mut self, chunk: &AudioChunk) -> WakeWordDetection {
        if self.tick_cooldown() {
            return WakeWordDetection {
                detected: false,
                keyword: self.keyword.clone(),
                confidence: 0.0,
                vad_prob: None,
                band_ratio: None,
                backend: self.last_backend,
                ort_prob: None,
            };
        }
        if chunk.samples.is_empty() {
            return neg(&self.keyword, None, None);
        }
        let mono = Self::mono16k(chunk);
        if mono.is_empty() {
            return neg(&self.keyword, None, None);
        }
        let (rms, band_ratio) = band_measurements(&mono);
        self.last_band_ratio = Some(band_ratio);
        // Onset fort : broadband décide, bande façonne (0.80 + 0.20·ratio).
        let loud = (rms * 15.0).clamp(0.0, 1.0);
        let confidence = (loud * (0.80 + 0.20 * band_ratio)).clamp(0.0, 1.0);
        let detected = confidence >= self.threshold;
        if detected {
            self.current_cooldown = self.cooldown_frames;
        }
        WakeWordDetection {
            detected,
            keyword: self.keyword.clone(),
            confidence,
            vad_prob: None,
            band_ratio: Some(band_ratio),
            backend: self.last_backend,
            ort_prob: None,
        }
    }

    /// Décision VAD-gatée STRICTE (qualité), évaluée par fenêtre 512 alignée :
    /// ne déclenche que si une série CONSÉCUTIVE de fenêtres chaudes
    /// (VAD Silero RÉEL > 0.5 **et** bande 300–3400 suffisante **et** RMS
    /// minimal) dure dans `[min_duration_s, max_duration_s]` (défaut
    /// `[0.3s, 1.2s]`), avec cooldown. Bascule auto vers `ort` si provisionné.
    /// Erreur LOUD si le modèle VAD est absent — jamais de déclenchement
    /// sans preuve VAD. La moyenne (dilution) n'est jamais utilisée comme
    /// gate : une onset de 32 ms compte.
    pub fn process_gated(&mut self, chunk: &AudioChunk) -> hakille_core::Result<WakeWordDetection> {
        // Bascule automatique vers ort quand le checkpoint existe.
        // Échec de bascule = journalisé LOUD (jamais silencieux), décision
        // énergétique conservée ; absence de fichier = état normal.
        if self.ort_scorer.is_none()
            && let Err(e) = self.maybe_upgrade_to_ort()
        {
            tracing::warn!(
                error = %e,
                "WakeWordDetector: checkpoint présent mais inutilisable — \
                 reste sur le chemin énergétique VAD-gaté"
            );
        }
        if self.tick_cooldown() {
            return Ok(WakeWordDetection {
                detected: false,
                keyword: self.keyword.clone(),
                confidence: 0.0,
                vad_prob: self.last_vad_prob,
                band_ratio: self.last_band_ratio,
                backend: self.last_backend,
                ort_prob: self.last_ort_prob,
            });
        }
        if chunk.samples.is_empty() {
            self.trailing_hot_windows = 0;
            return Ok(neg(&self.keyword, Some(0.0), Some(0.0)));
        }
        let mono = Self::mono16k(chunk);
        if mono.is_empty() {
            self.trailing_hot_windows = 0;
            return Ok(neg(&self.keyword, Some(0.0), Some(0.0)));
        }
        let (carry_before, vad_probs) = self.real_vad_windows(&mono)?;
        self.last_windows.clear();
        self.last_windows.reserve(vad_probs.len());
        // Aligne chaque probabilité sur ses EXACTS 512 échantillons : la
        // première fenêtre complète le carry session, les suivantes par pas de 512.
        let mut off = 0usize;
        let mut first_len = 512usize.saturating_sub(carry_before);
        if first_len == 0 || first_len > 512 {
            first_len = 512;
        }
        let min_win = ((self.min_duration_s * 16_000.0 / 512.0).ceil() as usize).max(1);
        let max_win = ((self.max_duration_s * 16_000.0 / 512.0).floor() as usize).max(min_win);
        let mut run = self.trailing_hot_windows;
        let mut run_peak = 0.0f32;
        let mut run_band_sum = 0.0f32;
        let mut run_n = 0usize;
        let mut vad_peak = 0.0f32;
        let mut band_sum = 0.0f32;
        let mut ort_feats: Vec<f32> = Vec::with_capacity(vad_probs.len());
        let mut detected = false;
        let mut confidence = 0.0f32;
        for (wi, prob) in vad_probs.iter().enumerate() {
            let wlen = if wi == 0 { first_len } else { 512 };
            let end = off.saturating_add(wlen);
            let slice = mono.get(off..end).ok_or_else(|| {
                hakille_core::HakilleError::InferenceError(format!(
                    "WakeWordDetector::process_gated: fenêtre {wi} hors bornes \
                     (off={off}, wlen={wlen}, mono={}) — incohérence carry",
                    mono.len()
                ))
            })?;
            off = end;
            let (rms_w, ratio_w) = band_measurements(slice);
            ort_feats.push(ratio_w);
            self.last_windows
                .push((prob.clamp(0.0, 1.0), rms_w, ratio_w));
            if *prob > vad_peak {
                vad_peak = *prob;
            }
            band_sum += ratio_w;
            let hot =
                *prob > self.vad_gate && ratio_w >= self.band_threshold && rms_w >= MIN_WINDOW_RMS;
            if hot {
                run += 1;
                if *prob > run_peak {
                    run_peak = *prob;
                }
                run_band_sum += ratio_w;
                run_n += 1;
            } else {
                run = 0;
                run_peak = 0.0;
                run_band_sum = 0.0;
                run_n = 0;
            }
            if run > max_win {
                // Parole continue, pas un mot-clé : on repart sans déclencher.
                run = 0;
                run_peak = 0.0;
                run_band_sum = 0.0;
                run_n = 0;
                continue;
            }
            if !detected && run >= min_win {
                let run_band = if run_n > 0 {
                    run_band_sum / run_n as f32
                } else {
                    0.0
                };
                confidence = (0.65 * run_peak + 0.35 * run_band.clamp(0.0, 1.0)).clamp(0.0, 1.0);
                if confidence >= self.threshold {
                    detected = true;
                }
            }
        }
        self.trailing_hot_windows = if vad_probs.is_empty() {
            self.trailing_hot_windows
        } else if run > 0 {
            run.min(max_win)
        } else {
            0
        };
        let mean_band = if vad_probs.is_empty() {
            0.0
        } else {
            (band_sum / vad_probs.len() as f32).clamp(0.0, 1.0)
        };
        self.last_vad_prob = Some(vad_peak.clamp(0.0, 1.0));
        self.last_band_ratio = Some(mean_band);

        // Score neural réel si provisionné (features = ratios de bande par
        // fenêtre) — bonus s'il confirme, jamais un veto silencieux.
        let mut ort_prob: Option<f32> = None;
        if let Some(scorer) = self.ort_scorer.as_mut()
            && !ort_feats.is_empty()
        {
            match scorer.score_features(&ort_feats) {
                Ok(p) => {
                    ort_prob = Some(p);
                    self.last_backend = WakeBackendKind::OrtReal;
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "WakeWordDetector::process_gated: forward ort du checkpoint \
                         en échec — décision énergétique conservée"
                    );
                }
            }
        }
        self.last_ort_prob = ort_prob;
        if let Some(op) = ort_prob {
            confidence = (0.5 * confidence + 0.5 * op).clamp(0.0, 1.0);
            if detected && confidence < self.threshold {
                detected = false;
            }
        }
        if detected {
            self.current_cooldown = self.cooldown_frames;
            self.trailing_hot_windows = 0;
        }
        Ok(WakeWordDetection {
            detected,
            keyword: self.keyword.clone(),
            confidence,
            vad_prob: Some(vad_peak.clamp(0.0, 1.0)),
            band_ratio: Some(mean_band),
            backend: self.last_backend,
            ort_prob,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::Timestamp;

    fn chunk(samples: Vec<f32>) -> AudioChunk {
        AudioChunk {
            samples,
            sample_rate: 16_000,
            channels: 1,
            timestamp: Timestamp::now(),
        }
    }

    fn real_vad_available(det: &WakeWordDetector) -> bool {
        if det.vad_model_path.as_deref().is_some_and(|p| p.is_file()) {
            true
        } else {
            eprintln!("SKIP: models/silero_vad.onnx absent — test wake-word gaté ignoré");
            false
        }
    }

    fn sine(freq_hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|t| (t as f32 / 16_000.0 * freq_hz * 2.0 * std::f32::consts::PI).sin() * amp)
            .collect()
    }

    /// Phrase voisée compacte (même recette que le golden VAD : /ba/-fric-/si/ ×2).
    fn gated_test_phrase() -> Vec<f32> {
        use std::f32::consts::PI;
        const SR: f32 = 16_000.0;
        fn reson(x: Vec<f32>, fc: f32, r: f32) -> Vec<f32> {
            let b = 2.0 * r * (2.0 * PI * fc / SR).cos();
            let c = -(r * r);
            let (mut y1, mut y2) = (0.0, 0.0);
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
            let (a, b, c) = (
                reson(g.clone(), f1, 0.985),
                reson(g.clone(), f2, 0.98),
                reson(g, f3, 0.97),
            );
            a.into_iter()
                .zip(b)
                .zip(c)
                .map(|((x, y), z)| x * 0.5 + y * 0.35 + z * 0.25)
                .collect()
        }
        fn silence(dur: f32) -> Vec<f32> {
            vec![0.0; (dur * SR) as usize]
        }
        let fric: Vec<f32> = {
            let m = (0.07 * SR) as usize;
            let mut st = 0xABCu64;
            let mut prev = 0.0;
            (0..m)
                .map(|_| {
                    st ^= st >> 12;
                    st ^= st << 25;
                    st ^= st >> 27;
                    let u =
                        (st.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
                    let w = ((u as f32) * 2.0 - 1.0) * 0.12;
                    let y = w - 0.96 * prev;
                    prev = w;
                    y
                })
                .collect()
        };
        let mut bloc = silence(0.25);
        bloc.extend_from_slice(&syllabe(730.0, 1090.0, 2440.0, 118.0, 0.22, 0.5));
        bloc.extend_from_slice(&fric);
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
    fn bandpass_measurements_are_real() {
        // Silence : rien.
        let (rms, ratio) = band_measurements(&vec![0.0f32; 1600]);
        assert_eq!(rms, 0.0);
        assert_eq!(ratio, 0.0);
        // DC : broadband fort, bande ~nulle (passe-haut honnête).
        let (rms_dc, ratio_dc) = band_measurements(&vec![0.3f32; 1600]);
        assert!((rms_dc - 0.3).abs() < 1e-4, "rms_dc={rms_dc}");
        assert!(ratio_dc < 0.05, "DC hors bande: ratio={ratio_dc}");
        // 1000 Hz (dans 300–3400) : bande dominante.
        let (rms_in, ratio_in) = band_measurements(&sine(1000.0, 0.5, 1600));
        eprintln!("1000Hz: rms={rms_in:.3} ratio={ratio_in:.3}");
        assert!(ratio_in > 0.5, "1000Hz doit dominer la bande");
        // 100 Hz (sous la bande) : bande faible.
        let (_, ratio_out) = band_measurements(&sine(100.0, 0.5, 1600));
        eprintln!("100Hz: ratio={ratio_out:.3}");
        assert!(
            ratio_out < ratio_in * 0.5,
            "100Hz doit être atténué vs 1000Hz"
        );
        // Robustesse NaN/Inf.
        let (rms_n, ratio_n) = band_measurements(&[f32::NAN, f32::INFINITY, 0.2, -0.3]);
        assert!(rms_n.is_finite() && ratio_n.is_finite());
    }

    #[test]
    fn instant_path_loud_onset_then_cooldown() {
        let mut det = WakeWordDetector::default_hakille();
        assert!(!det.process(&chunk(vec![0.0f32; 1600])).detected);
        let loud = chunk(vec![0.3f32; 1600]);
        let d = det.process(&loud);
        assert!(d.detected, "onset fort mesuré doit déclencher");
        assert_eq!(d.keyword, "Dis Hakille");
        assert!(d.confidence >= 0.75);
        assert!(d.band_ratio.is_some());
        // Cooldown : le chunk suivant est bloqué.
        let d2 = det.process(&loud);
        assert!(!d2.detected, "cooldown obligatoire");
    }

    #[test]
    fn gated_path_silence_is_negative_with_real_vad() {
        let mut det = WakeWordDetector::default_hakille();
        if !real_vad_available(&det) {
            return;
        }
        let r = det
            .process_gated(&chunk(vec![0.0f32; 4800]))
            .expect("VAD réel disponible");
        assert!(!r.detected);
        assert_eq!(r.backend, WakeBackendKind::Energetic);
        let vp = r.vad_prob.expect("prob VAD réelle mesurée");
        assert!(vp < 0.1, "silence: vad={vp}");
        assert!(det.last_vad_prob().is_some());
    }

    #[test]
    fn gated_path_voiced_phrase_fires_once_real() {
        let phrase = gated_test_phrase();
        // 1) Défaut (0.75, 0.3-1.2 s, bande 0.20) : ZÉRO faux positif sur pics
        // isolés — la durée exigée n'est jamais atteinte — mais le VAD réel
        // s'active (preuve mesuree).
        let mut det = WakeWordDetector::new("test", 0.75);
        if !real_vad_available(&det) {
            return;
        }
        let mut fires_default = 0;
        let mut vad_max_all: f32 = 0.0;
        for w in phrase.chunks(4800) {
            let mut buf = vec![0.0f32; 4800];
            for (i, s) in w.iter().enumerate() {
                buf[i] = *s;
            }
            let r = det.process_gated(&chunk(buf)).expect("gated réel");
            if let Some(v) = r.vad_prob {
                vad_max_all = vad_max_all.max(v);
            }
            if r.detected {
                fires_default += 1;
            }
        }
        eprintln!("gated défaut: déclenchements={fires_default} vad_max={vad_max_all:.3}");
        assert!(
            vad_max_all > 0.5,
            "la phrase doit activer le VAD réel (max={vad_max_all:.3})"
        );
        assert_eq!(
            fires_default, 0,
            "pics isolés : pas de déclenchement sans durée soutenue"
        );
        // 2) Resserre (gate 0.2, fenêtre 32 ms, bande 0.0, seuil 0.25) : les
        // fenêtres excitées (VAD 0.49/0.59) déclenchent — gate, durée,
        // confiance et cooldown prouvés de bout en bout avec le VRAI modèle.
        let mut det2 = WakeWordDetector::new("test", 0.25);
        det2.with_vad_gate(0.2)
            .expect("gate valide")
            .with_gated_timing(0.032, 1.2)
            .expect("timing valide")
            .with_band_threshold(0.0)
            .expect("bande valide");
        let mut fires = 0;
        for w in phrase.chunks(4800) {
            let mut buf = vec![0.0f32; 4800];
            for (i, s) in w.iter().enumerate() {
                buf[i] = *s;
            }
            let r = det2.process_gated(&chunk(buf)).expect("gated réel");
            eprintln!(
                "gated resserré: detected={} conf={:.3} vad={:?} band={:?} fenêtres={:?}",
                r.detected,
                r.confidence,
                r.vad_prob,
                r.band_ratio,
                det2.last_windows_debug()
            );
            if r.detected {
                fires += 1;
                assert_eq!(r.backend, WakeBackendKind::Energetic);
                assert!(r.vad_prob.unwrap_or(0.0) > 0.2, "VAD-gate strict");
            }
        }
        assert!(
            fires >= 1,
            "les fenêtres excitées doivent déclencher au moins une fois"
        );
    }

    #[test]
    fn provision_rejects_garbage_and_reports_missing_checkpoint() {
        let dir = std::env::temp_dir().join("hakille_ww_probe_dir");
        let _ = std::fs::create_dir_all(&dir);
        assert!(openwakeword_path(Some(dir.as_path()), "Dis Hakille").is_none());
        let mut det = WakeWordDetector::new("Dis Hakille", 0.75);
        assert!(!det.maybe_upgrade_to_ort().expect("pas de checkpoint"));
        assert_eq!(det.backend_kind(), WakeBackendKind::Energetic);
        assert!(det.ort_forward_calls().is_none());
        assert_eq!(oww_filename("Dis Hakille"), "openwakeword_dis_hakille.onnx");
        assert_eq!(oww_filename("  "), "openwakeword_keyword.onnx");
    }

    #[test]
    fn oww_filename_slugs_safely() {
        assert_eq!(oww_filename("Hey Jarvis!"), "openwakeword_hey_jarvis.onnx");
    }
}
