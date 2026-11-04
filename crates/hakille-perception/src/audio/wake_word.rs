//! API historique de mesure acoustique, sans reconnaissance de mot-clé.
//!
//! L'énergie et le VAD indiquent une activité sonore ou vocale, jamais le mot
//! « Hakille ». Sans frontend et checkpoint compatibles, `detected` reste faux
//! et `confidence` vaut zéro. L'activation utilisable est effectuée sur la
//! transcription ASR par le contrôleur `WakeActivation`.
//!
//! Un fichier openWakeWord seul n'est pas exploitable ici : les 64 ratios de
//! bande de l'ancien chemin ne constituent pas son frontend mel/embeddings.
//! Le chargement via `OrtScalarScorer` a donc été retiré ; aucun état `OrtReal`
//! ne peut être annoncé par cette implémentation.

use crate::audio::capture::AudioChunk;
use hakille_inference::{ModelDownloader, OrtVadSession, resolve_vad_model_path, to_mono_16k};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const WAKE_BAND_LO_HZ: f32 = 300.0;
pub const WAKE_BAND_HI_HZ: f32 = 3400.0;
pub const WAKE_VAD_GATE: f32 = 0.5;
pub const WAKE_MIN_DURATION_S: f32 = 0.3;
pub const WAKE_MAX_DURATION_S: f32 = 1.2;
const ALPHA_HP_300HZ: f32 = 0.1111;
const LP_ALPHA: f32 = 0.74;
pub const OWW_FILENAME_PREFIX: &str = "openwakeword_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WakeBackendKind {
    /// Mesures acoustiques uniquement ; aucune reconnaissance de mot-clé.
    Energetic,
    /// Variante historique conservée pour compatibilité, jamais produite ici.
    OrtReal,
}
impl WakeBackendKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Energetic => "MESURES-ACOUSTIQUES — MOT-CLÉ INDISPONIBLE",
            Self::OrtReal => "ORT-RÉEL (VARIANTE HISTORIQUE NON RACCORDÉE)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordDetection {
    pub detected: bool,
    /// Mot-clé demandé, pas un mot reconnu lorsque `detected` vaut faux.
    pub keyword: String,
    pub confidence: f32,
    pub vad_prob: Option<f32>,
    pub band_ratio: Option<f32>,
    pub backend: WakeBackendKind,
    pub ort_prob: Option<f32>,
}
fn neg(keyword: &str, vad_prob: Option<f32>, band_ratio: Option<f32>) -> WakeWordDetection {
    WakeWordDetection {
        detected: false,
        keyword: keyword.to_owned(),
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
/// [`ModelDownloader`]). Le téléchargement ne raccorde pas de moteur de reconnaissance :
/// le frontend openWakeWord et un modèle entraîné pour le mot-clé restent requis.
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

/// Compatibilité de l'ancien détecteur. Les mesures exposées ne doivent
/// jamais servir de preuve de reconnaissance du mot-clé.
pub struct WakeWordDetector {
    keyword: String,
    vad_model_path: Option<PathBuf>,
    vad_session: Option<OrtVadSession>,
    oww_models_dir: Option<PathBuf>,
    oww_path: Option<PathBuf>,
    last_vad_prob: Option<f32>,
    last_band_ratio: Option<f32>,
    last_windows: Vec<(f32, f32, f32)>,
    diagnostic_carry: Vec<f32>,
}
impl WakeWordDetector {
    /// `threshold` est conservé pour compatibilité ; il ne peut activer un mot.
    pub fn new(keyword: impl Into<String>, _threshold: f32) -> Self {
        Self {
            keyword: keyword.into(),
            vad_model_path: resolve_vad_model_path(None).ok(),
            vad_session: None,
            oww_models_dir: None,
            oww_path: None,
            last_vad_prob: None,
            last_band_ratio: None,
            last_windows: Vec::new(),
            diagnostic_carry: Vec::new(),
        }
    }
    pub fn with_models_dir(
        keyword: impl Into<String>,
        threshold: f32,
        models_dir: impl AsRef<Path>,
    ) -> hakille_core::Result<Self> {
        let mut detector = Self::new(keyword, threshold);
        detector.vad_model_path = Some(resolve_vad_model_path(Some(models_dir.as_ref()))?);
        detector.oww_models_dir = Some(models_dir.as_ref().to_path_buf());
        detector.oww_path = openwakeword_path(Some(models_dir.as_ref()), &detector.keyword);
        Ok(detector)
    }
    pub fn default_hakille() -> Self {
        Self::new("Dis Hakille", 0.75)
    }
    pub fn keyword(&self) -> &str {
        &self.keyword
    }
    pub fn backend_kind(&self) -> WakeBackendKind {
        WakeBackendKind::Energetic
    }
    pub fn recognition_available(&self) -> bool {
        false
    }
    pub fn unavailable_reason(&self) -> &'static str {
        "Reconnaissance acoustique du mot-clé indisponible : frontend openWakeWord et checkpoint Hakille compatibles non raccordés ; utilisez l'activation après transcription ASR"
    }
    /// Réglage historique validé pour compatibilité ; aucune décision de mot-clé.
    pub fn with_gated_timing(&mut self, min: f32, max: f32) -> hakille_core::Result<&mut Self> {
        if !min.is_finite() || !max.is_finite() || min <= 0.0 || min > max || max > 5.0 {
            return Err(hakille_core::HakilleError::InferenceError(
                "Fenêtre historique invalide : 0 < min <= max <= 5 s requis".into(),
            ));
        }
        Ok(self)
    }
    /// Réglage historique validé ; aucune décision de mot-clé.
    pub fn with_band_threshold(&mut self, ratio: f32) -> hakille_core::Result<&mut Self> {
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(hakille_core::HakilleError::InferenceError(
                "Ratio de bande hors [0,1]".into(),
            ));
        }
        Ok(self)
    }
    /// Réglage historique validé ; aucune décision de mot-clé.
    pub fn with_vad_gate(&mut self, gate: f32) -> hakille_core::Result<&mut Self> {
        if !gate.is_finite() || gate <= 0.0 || gate >= 1.0 {
            return Err(hakille_core::HakilleError::InferenceError(
                "Seuil VAD hors ]0,1[".into(),
            ));
        }
        Ok(self)
    }
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
        None
    }

    /// Aucun modèle acoustique n'est compatible avec ce chemin historique.
    /// Absence : `Ok(false)` ; présence : erreur explicite, jamais un repli.
    pub fn maybe_upgrade_to_ort(&mut self) -> hakille_core::Result<bool> {
        if self.oww_path.is_none() {
            self.oww_path = openwakeword_path(self.oww_models_dir.as_deref(), &self.keyword);
        }
        if self.oww_path.is_some() {
            return Err(hakille_core::HakilleError::InferenceError(
                "Checkpoint openWakeWord refusé : frontend mel/embeddings et contrat d'entrée non implémentés ; les ratios de bande ne sont pas des features openWakeWord compatibles".into()));
        }
        Ok(false)
    }

    fn mono16k(chunk: &AudioChunk) -> Vec<f32> {
        to_mono_16k(&chunk.samples, chunk.channels, chunk.sample_rate)
    }
    fn ensure_vad(&mut self) -> hakille_core::Result<&mut OrtVadSession> {
        if self.vad_session.is_none() {
            let path = self.vad_model_path.as_ref().ok_or_else(|| {
                hakille_core::HakilleError::InferenceError(
                    "Modèle VAD absent : mesures acoustiques indisponibles".into(),
                )
            })?;
            self.vad_session = Some(OrtVadSession::open(path)?);
        }
        Ok(self
            .vad_session
            .as_mut()
            .expect("session initialisée ci-dessus"))
    }

    /// Mesure acoustique seule : `detected=false`, `confidence=0`, toujours.
    pub fn measure_activity(&mut self, chunk: &AudioChunk) -> WakeWordDetection {
        let (_, ratio) = band_measurements(&Self::mono16k(chunk));
        self.last_vad_prob = None;
        self.last_band_ratio = Some(ratio);
        self.last_windows.clear();
        neg(&self.keyword, None, Some(ratio))
    }

    #[deprecated(
        note = "L'énergie ne reconnaît aucun mot-clé ; utiliser WakeActivation après ASR ou measure_activity pour les seules mesures"
    )]
    pub fn process(&mut self, chunk: &AudioChunk) -> WakeWordDetection {
        self.measure_activity(chunk)
    }

    /// VAD réel et mesures de bande uniquement. Même une forte probabilité
    /// de parole ne prouve pas « Hakille » : aucune détection n'est produite.
    /// Un checkpoint présent sans frontend compatible provoque une erreur.
    pub fn process_gated(&mut self, chunk: &AudioChunk) -> hakille_core::Result<WakeWordDetection> {
        self.last_vad_prob = None;
        self.last_band_ratio = None;
        self.last_windows.clear();
        self.maybe_upgrade_to_ort()?;
        let mono = Self::mono16k(chunk);
        let probabilities = self.ensure_vad()?.process_stream(&mono)?;
        // Le carry est conservé pour mesurer les mêmes 512 échantillons que
        // le VAD, même si les chunks entrants sont fragmentés arbitrairement.
        self.diagnostic_carry.extend_from_slice(&mono);
        for ((_, probability), window) in probabilities
            .iter()
            .zip(self.diagnostic_carry.chunks_exact(512))
        {
            let (rms, ratio) = band_measurements(window);
            self.last_windows.push((*probability, rms, ratio));
        }
        let consumed = probabilities.len() * 512;
        if consumed > self.diagnostic_carry.len() {
            return Err(hakille_core::HakilleError::InferenceError(
                "Carry de diagnostic VAD incohérent".into(),
            ));
        }
        self.diagnostic_carry.drain(..consumed);
        if !self.last_windows.is_empty() {
            self.last_vad_prob = Some(self.last_windows.iter().map(|w| w.0).fold(0.0f32, f32::max));
            self.last_band_ratio = Some(
                self.last_windows.iter().map(|w| w.2).sum::<f32>() / self.last_windows.len() as f32,
            );
        }
        Ok(neg(&self.keyword, self.last_vad_prob, self.last_band_ratio))
    }
}

#[cfg(test)]
#[allow(deprecated)]
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
    fn loud_energy_never_recognizes_hakille_even_with_zero_threshold() {
        for threshold in [0.0, 0.25, 0.75, f32::NAN] {
            let mut detector = WakeWordDetector::new("Hakille", threshold);
            assert!(!detector.recognition_available());
            for signal in [
                vec![0.0; 1600],
                vec![0.3; 1600],
                sine(1000.0, 0.9, 16000),
                vec![f32::INFINITY; 1600],
            ] {
                for _ in 0..12 {
                    let result = detector.process(&chunk(signal.clone()));
                    assert!(!result.detected);
                    assert_eq!(result.confidence, 0.0);
                    assert_eq!(result.ort_prob, None);
                }
            }
        }
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
    fn real_vad_speech_is_never_keyword_evidence() {
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
        // Même avec des paramètres historiquement permissifs, une activité
        // VAD réelle ne devient jamais une preuve de reconnaissance du mot.
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
            assert_eq!(r.confidence, 0.0);
            assert_eq!(r.ort_prob, None);
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
        assert_eq!(
            fires, 0,
            "Même un VAD/gate permissif ne reconnaît aucun mot-clé"
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
    fn checkpoint_presence_never_implies_compatible_keyword_frontend() {
        let path = std::env::temp_dir().join(format!("hakille-wake-{}.onnx", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"checkpoint de test non chargeable").unwrap();
        let mut detector = WakeWordDetector::default_hakille();
        detector.oww_path = Some(path.clone());
        let error = detector.maybe_upgrade_to_ort().unwrap_err();
        assert!(error.to_string().contains("frontend"));
        assert!(
            detector
                .process_gated(&chunk(sine(1000.0, 0.9, 16000)))
                .is_err()
        );
        assert_eq!(detector.backend_kind(), WakeBackendKind::Energetic);
        assert!(!detector.recognition_available());
        assert_eq!(detector.ort_forward_calls(), None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_vad_is_explicit_and_no_activity_becomes_keyword() {
        let mut detector = WakeWordDetector::default_hakille();
        detector.vad_model_path = None;
        assert!(detector.process_gated(&chunk(vec![0.3; 1600])).is_err());
        assert!(!detector.measure_activity(&chunk(vec![0.3; 1600])).detected);
    }

    #[test]
    fn gated_diagnostics_preserve_aligned_windows_across_fragmentation() {
        let mut whole = WakeWordDetector::default_hakille();
        if !real_vad_available(&whole) {
            return;
        }
        let mut fragmented = WakeWordDetector::default_hakille();
        let signal = sine(1000.0, 0.3, 2048);
        whole.process_gated(&chunk(signal.clone())).unwrap();
        let expected = whole.last_windows_debug().to_vec();
        let mut actual = Vec::new();
        for samples in signal.chunks(321) {
            let detection = fragmented.process_gated(&chunk(samples.to_vec())).unwrap();
            assert!(!detection.detected);
            actual.extend_from_slice(fragmented.last_windows_debug());
        }
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(&expected) {
            assert!((a.0 - b.0).abs() < 1e-6);
            assert!((a.1 - b.1).abs() < 1e-6);
            assert!((a.2 - b.2).abs() < 1e-6);
        }
    }

    #[test]
    fn oww_filename_slugs_safely() {
        assert_eq!(oww_filename("Hey Jarvis!"), "openwakeword_hey_jarvis.onnx");
    }
}
