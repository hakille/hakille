//! MOTEUR RÉEL — frontend log-mel + LFR + MVN pur Rust (zéro dépendance).
//!
//! - Pré-accentuation 0.97, fenêtres 25 ms / pas 10 ms @ 16 kHz, fenêtre
//!   de Hamming (comme `WavFrontend(window="hamming")` de FunASR).
//! - FFT radix-2 implémentée ici même (pas de crate FFT).
//! - 80 bancs mel triangulaires (HTK) de 125 Hz à 7600 Hz,
//!   `ln(max(énergie, 1e-10))` puis centrage moyen par utterance.
//! - LFR (`lfr_m = 7`, `lfr_n = 6`) : port fidèle de
//!   `funasr/frontends/wav_frontend.py::apply_lfr` (pré-padding ×3 avec la
//!   première trame, concat de 7 trames, post-padding avec la dernière).
//!   Sortie `[T_lfr, 560]` avec `T_lfr = ceil(T / 6)`.
//! - MVN utterance (moyenne + variance par dim sur l'énoncé) : approximation
//!   documentée du CMVN global `am.mvn` (fichier absent de `models/`).
//! - Sortie [`pcm_to_sensevoice_feats`] `[T_lfr, 560]`, au moins 1 trame.
//!
//! Deltas v1 vs FunASR exact (documentés, pas simulés) : pas de `dither`,
//! pas d'upscale ×32768 (inutile sous MVN utterance : gain constant absorbé
//! par la normalisation), échelle mel HTK (l'option exacte de
//! `funasr/utils/fbank.py` n'est pas versionnée ici), MVN utterance au lieu
//! du stats globales `am.mvn`.

use std::f32::consts::PI;

/// Fréquence d'échantillonnage attendue (Hz).
pub const SAMPLE_RATE_HZ: usize = 16_000;
/// Longueur de fenêtre : 25 ms @ 16 kHz = 400 échantillons.
pub const WIN_SAMPLES: usize = 400;
/// Pas inter-trames : 10 ms @ 16 kHz = 160 échantillons.
pub const HOP_SAMPLES: usize = 160;
/// Taille FFT (puissance de 2 >= fenêtre).
pub const N_FFT: usize = 512;
/// Nombre de bancs mel (standard SenseVoice).
pub const N_MELS: usize = 80;
/// Borne basse des bancs mel (Hz).
pub const MEL_FMIN_HZ: f32 = 125.0;
/// Borne haute des bancs mel (Hz).
pub const MEL_FMAX_HZ: f32 = 7600.0;
/// Coefficient de pré-accentuation (kaldi/funasr).
pub const PREEMPHASIS: f32 = 0.97;
/// Plancher d'énergie avant log (évite `ln(0)`).
pub const LOG_FLOOR: f32 = 1e-10;
/// Fenêtre LFR (7 trames concaténées : 80 × 7 = 560).
pub const LFR_M: usize = 7;
/// Pas LFR (1 trame empilée toutes les 6 trames mel).
pub const LFR_N: usize = 6;
/// Dimension empilée : 80 × 7 = 560 (exigée en `dim[2]` par le graphe ONNX).
pub const STACKED_DIM: usize = N_MELS * LFR_M;
/// Garde-fou variance MVN (silence -> zéros, jamais de division par zéro).
pub const MVN_EPS: f32 = 1e-10;
/// Upscale FunASR/kaldi : le frontend officiel multiplie le PCM [-1,1] par
/// 2^15 avant fbank (`upsacle_samples=True`).
pub const UPSCALE_I16: f32 = 32768.0;
/// Écart-type du dither kaldi (`dither=1.0`) exprimé en PCM [-1,1].
pub const DITHER_SIGMA: f32 = 1.0 / UPSCALE_I16;
/// Graine fixe du dither : bruit kaldi-compatible mais DÉTERMINISTE
/// (reproductibilité des tests ; le dither officiel est aléatoire).
pub const DITHER_SEED: u64 = 0x9E3779B97F4A7C15;

/// Nombre de trames mel pour `n_samples` (1 minimum, même à vide ;
/// équivalent `snip_edges=True` : que des fenêtres complètes).
pub fn frame_count(n_samples: usize) -> usize {
    if n_samples <= WIN_SAMPLES {
        1
    } else {
        1 + (n_samples - WIN_SAMPLES) / HOP_SAMPLES
    }
}

/// Trames empilées LFR pour `n_mel_frames` trames mel.
pub fn lfr_frame_count(n_mel_frames: usize) -> usize {
    n_mel_frames.div_ceil(LFR_N).max(1)
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

/// Matrice des filtres triangulaires `[N_MELS, N_FFT/2 + 1]` (échelle HTK).
pub fn mel_filterbank() -> Vec<Vec<f32>> {
    let n_bins = N_FFT / 2 + 1;
    let mel_lo = hz_to_mel(MEL_FMIN_HZ);
    let mel_hi = hz_to_mel(MEL_FMAX_HZ);
    let mut points = Vec::with_capacity(N_MELS + 2);
    for i in 0..N_MELS + 2 {
        let m = mel_lo + (mel_hi - mel_lo) * i as f32 / (N_MELS + 1) as f32;
        points.push(mel_to_hz(m));
    }
    let mut fb = vec![vec![0.0f32; n_bins]; N_MELS];
    for m in 0..N_MELS {
        let f_lo = points[m];
        let f_c = points[m + 1];
        let f_hi = points[m + 2];
        for k in 0..n_bins {
            let freq = k as f32 * SAMPLE_RATE_HZ as f32 / N_FFT as f32;
            let w = if freq < f_lo || freq > f_hi {
                0.0
            } else if freq <= f_c {
                if f_c > f_lo {
                    (freq - f_lo) / (f_c - f_lo)
                } else {
                    0.0
                }
            } else if f_hi > f_c {
                (f_hi - freq) / (f_hi - f_c)
            } else {
                0.0
            };
            fb[m][k] = w.max(0.0);
        }
    }
    fb
}

/// FFT directe radix-2 en place (`re`/`im` de longueur puissance de 2).
fn fft_in_place(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    debug_assert!(n.is_power_of_two() && n >= 2);
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (w_re, w_im) = (ang.cos(), ang.sin());
        let half = len / 2;
        let mut i = 0;
        while i < n {
            let (mut cur_re, mut cur_im) = (1.0f32, 0.0f32);
            for k in 0..half {
                let u_re = re[i + k];
                let u_im = im[i + k];
                let v_re = re[i + k + half] * cur_re - im[i + k + half] * cur_im;
                let v_im = re[i + k + half] * cur_im + im[i + k + half] * cur_re;
                re[i + k] = u_re + v_re;
                im[i + k] = u_im + v_im;
                re[i + k + half] = u_re - v_re;
                im[i + k + half] = u_im - v_im;
                let nxt_re = cur_re * w_re - cur_im * w_im;
                cur_im = cur_re * w_im + cur_im * w_re;
                cur_re = nxt_re;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Fenêtre de Hamming symétrique (`0.54 - 0.46·cos`, défaut FunASR/kaldi).
fn hamming_window() -> Vec<f32> {
    let mut w = Vec::with_capacity(WIN_SAMPLES);
    for n in 0..WIN_SAMPLES {
        w.push(0.54 - 0.46 * (2.0 * PI * n as f32 / (WIN_SAMPLES - 1) as f32).cos());
    }
    w
}

/// Convertit du PCM mono @ 16 kHz en log-mel `[n_frames, 80]` centré.
///
/// Sanitize : `NaN`/`Inf` -> `0.0`, clamp `[-1, 1]`. Ne panique jamais.
pub fn pcm_to_log_mel(pcm: &[f32]) -> Vec<Vec<f32>> {
    let mut log_mel = pcm_to_log_mel_raw(pcm);
    center_mel(&mut log_mel);
    log_mel
}

/// Centre la moyenne par utterance (par banc). Exposée pour tests.
pub fn center_mel(log_mel: &mut [Vec<f32>]) {
    let n_frames = log_mel.len();
    if n_frames == 0 {
        return;
    }
    if n_frames > 1 {
        for m in 0..N_MELS {
            let mut mean = 0.0f32;
            for f in 0..n_frames {
                mean += log_mel[f][m];
            }
            mean /= n_frames as f32;
            for row in log_mel.iter_mut() {
                if m < row.len() {
                    row[m] -= mean;
                }
            }
        }
    } else if !log_mel[0].is_empty() {
        let n = log_mel[0].len() as f32;
        let mean = log_mel[0].iter().sum::<f32>() / n;
        for v in log_mel[0].iter_mut() {
            *v -= mean;
        }
    }
}

/// Log-mel BRUT (sans normalisation) — étage réel du pipeline, exposé pour
/// valider l'axe fréquentiel (le centrage utterance annule les moyennes
/// par construction et ne peut pas servir à localiser un pic).
pub fn pcm_to_log_mel_raw(pcm: &[f32]) -> Vec<Vec<f32>> {
    let n_frames = frame_count(pcm.len());
    let window = hamming_window();
    let fb = mel_filterbank();
    let n_bins = N_FFT / 2 + 1;

    let mut clean = Vec::with_capacity(pcm.len().max(1));
    for &s in pcm.iter() {
        clean.push(if s.is_finite() {
            s.clamp(-1.0, 1.0)
        } else {
            0.0
        });
    }

    let mut log_mel = vec![vec![0.0f32; N_MELS]; n_frames];
    let mut re = vec![0.0f32; N_FFT];
    let mut im = vec![0.0f32; N_FFT];
    let mut prev = 0.0f32;

    for f in 0..n_frames {
        // Pré-accentuation + fenêtrage Hamming, zéro-padding jusqu'à N_FFT.
        re.fill(0.0);
        im.fill(0.0);
        for n in 0..WIN_SAMPLES {
            let idx = f * HOP_SAMPLES + n;
            let x = if idx < clean.len() { clean[idx] } else { 0.0 };
            let y = x - PREEMPHASIS * prev;
            prev = x;
            re[n] = y * window[n];
        }
        fft_in_place(&mut re, &mut im);
        // Spectre de puissance (normalisé par N_FFT) -> bancs mel -> log.
        for m in 0..N_MELS {
            let mut e = 0.0f32;
            for k in 0..n_bins {
                let p = (re[k] * re[k] + im[k] * im[k]) / N_FFT as f32;
                e += p * fb[m][k];
            }
            log_mel[f][m] = e.max(LOG_FLOOR).ln();
        }
    }

    log_mel
}

/// Empilement LFR `[T, D]` -> `[ceil(T/6), 7·D]`.
///
/// Port fidèle de `funasr/frontends/wav_frontend.py::apply_lfr` avec
/// `lfr_m = 7`, `lfr_n = 6` : pré-padding avec `(7-1)/2 = 3` copies de la
/// première trame, concat glissante de 7 trames au pas 6, post-padding avec
/// la dernière trame selon la formule officielle. Si l'entrée est trop
/// courte (< 7 trames après pré-padding, cas < 0,16 s d'audio), complète
/// avec la dernière trame (garde-fou documenté, `as_strided` serait UB).
pub fn apply_lfr(mel: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let m = LFR_M;
    let n = LFR_N;
    let dim = mel.first().map(|r| r.len()).unwrap_or(N_MELS).max(1);
    let t_lfr = mel.len().div_ceil(n).max(1);

    // Pré-padding : (m-1)/2 copies de la première trame (ou zéros à vide).
    let first: Vec<f32> = mel.first().cloned().unwrap_or_else(|| vec![0.0; dim]);
    let mut buf: Vec<Vec<f32>> = Vec::new();
    for _ in 0..(m - 1) / 2 {
        buf.push(first.clone());
    }
    for row in mel {
        let mut r = vec![0.0f32; dim];
        let take = row.len().min(dim);
        r[..take].copy_from_slice(&row[..take]);
        buf.push(r);
    }
    // Garde-fou : jamais moins de m trames avant la fenêtre glissante.
    while buf.len() < m {
        buf.push(buf.last().cloned().unwrap_or_else(|| vec![0.0; dim]));
    }

    // Post-padding (formule officielle, arithmétique entière signée).
    let t = buf.len() as isize;
    let mm = m as isize;
    let nn = n as isize;
    let t_lfr_i = t_lfr as isize;
    let last_idx = (t - mm) / nn + 1;
    let num_padding = mm - (t - last_idx * nn);
    if num_padding > 0 {
        let extra = (2 * mm - 2 * t + (t_lfr_i - 1 + last_idx) * nn) / 2 * (t_lfr_i - last_idx);
        for _ in 0..extra.max(0) {
            buf.push(buf.last().cloned().unwrap());
        }
    }

    // Concat glissante : sortie[i] = buf[i*6 .. i*6+7] aplati.
    let mut out = vec![vec![0.0f32; m * dim]; t_lfr];
    for i in 0..t_lfr {
        for j in 0..m {
            let src = &buf[i * n + j];
            let dst = &mut out[i][j * dim..(j + 1) * dim];
            dst.copy_from_slice(&src[..dim.min(src.len())]);
        }
    }
    out
}

/// Normalisation moyenne+variance par utterance (par dim, en place).
///
/// Approximation documentée du CMVN global `am.mvn` (absent de `models/`) :
/// `x = (x - mean) / sqrt(var + eps)`. Le silence exact reste à ~zéro.
pub fn utterance_mvn(feats: &mut [Vec<f32>]) {
    if feats.is_empty() {
        return;
    }
    let dim = feats[0].len();
    if dim == 0 {
        return;
    }
    let n = feats.len() as f32;
    for d in 0..dim {
        let mut mean = 0.0f32;
        for row in feats.iter() {
            mean += row[d];
        }
        mean /= n;
        let mut var = 0.0f32;
        for row in feats.iter() {
            let c = row[d] - mean;
            var += c * c;
        }
        var /= n;
        let inv = 1.0 / (var + MVN_EPS).sqrt();
        for row in feats.iter_mut() {
            row[d] = (row[d] - mean) * inv;
        }
    }
}

/// Pipeline complet : PCM @16kHz -> `[T_lfr, 560]` prêt pour le graphe
/// (`x` en `(batch, temps, 560)`).
///
/// Approximation v1 (MVN utterance) quand les stats globales `am.mvn`
/// sont absentes du modèle.
pub fn pcm_to_sensevoice_feats(pcm: &[f32]) -> Vec<Vec<f32>> {
    let mel = pcm_to_log_mel(pcm);
    let mut stacked = apply_lfr(&mel);
    // Sécurité dimensionnelle : exactement 560 par trame.
    for row in stacked.iter_mut() {
        row.resize(STACKED_DIM, 0.0);
    }
    utterance_mvn(&mut stacked);
    stacked
}

/// Générateur déterministe (splitmix64) — base du dither reproductible.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Ajoute un dither gaussien `N(0, sigma)` DÉTERMINISTE (Box-Muller sur
/// splitmix64, graine [`DITHER_SEED`]) — équivalent kaldi `dither=1.0`
/// après upscale, sans l'aléa inter-runs.
pub fn add_dither(waveform: &mut [f32], sigma: f32) {
    if !(sigma > 0.0) || waveform.is_empty() {
        return;
    }
    let mut state = DITHER_SEED;
    let mut i = 0;
    while i < waveform.len() {
        // Deux uniformes (0,1] -> paire gaussienne (Box-Muller).
        let u1 = (splitmix64(&mut state) as f64 / u64::MAX as f64).clamp(1e-12, 1.0);
        let u2 = splitmix64(&mut state) as f64 / u64::MAX as f64;
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        waveform[i] += (r * theta.cos()) as f32 * sigma;
        i += 1;
        if i < waveform.len() {
            waveform[i] += (r * theta.sin()) as f32 * sigma;
            i += 1;
        }
    }
}

/// Applique le CMVN global `am.mvn` : `x = (x + neg_mean) * inv_stddev`
/// (sémantique exacte de `apply_cmvn` FunASR). Dimensions hors 560 ->
/// repli silencieux sur les dims communes (jamais de panic).
pub fn apply_global_cmvn(feats: &mut [Vec<f32>], neg_mean: &[f32], inv_stddev: &[f32]) {
    for row in feats.iter_mut() {
        let n = row.len().min(neg_mean.len()).min(inv_stddev.len());
        for d in 0..n {
            row[d] = (row[d] + neg_mean[d]) * inv_stddev[d];
        }
    }
}

/// Pipeline fidèle : PCM @16kHz -> upscale ×32768 + dither -> log-mel ->
/// LFR -> CMVN global `am.mvn` (`[T_lfr, 560]`).
///
/// À utiliser quand les métadonnées du modèle fournissent `neg_mean` /
/// `inv_stddev` (même conditionnement qu'à l'entraînement).
pub fn pcm_to_sensevoice_feats_global(
    pcm: &[f32],
    neg_mean: &[f32],
    inv_stddev: &[f32],
) -> Vec<Vec<f32>> {
    // Upscale + dither (domaine int16, comme WavFrontend).
    let mut wave = Vec::with_capacity(pcm.len().max(1));
    for &s in pcm.iter() {
        wave.push(
            if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            } * UPSCALE_I16,
        );
    }
    add_dither(&mut wave, 1.0);
    // fbank sur le signal upscalé : on réutilise le chemin log-mel en
    // divisant l'upscale (le shift log constant est absorbé par le CMVN
    // global) — le dither, lui, reste (plancher réaliste).
    let pcm_scaled: Vec<f32> = wave.iter().map(|v| v / UPSCALE_I16).collect();
    let mel = pcm_to_log_mel(&pcm_scaled);
    let mut stacked = apply_lfr(&mel);
    for row in stacked.iter_mut() {
        row.resize(STACKED_DIM, 0.0);
        // Compense le shift log de l'upscale : ln(E·G²) = ln(E) + ln(G²).
        let shift = 2.0 * UPSCALE_I16.ln();
        for v in row.iter_mut() {
            *v += shift;
        }
    }
    apply_global_cmvn(&mut stacked, neg_mean, inv_stddev);
    stacked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_440(len: usize) -> Vec<f32> {
        (0..len)
            .map(|t| (t as f32 / SAMPLE_RATE_HZ as f32 * 440.0f32 * 2.0 * PI).sin() * 0.5)
            .collect()
    }

    #[test]
    fn test_mel_exact_dims() {
        let mel = pcm_to_log_mel(&vec![0.0f32; 16_000]);
        assert_eq!(mel.len(), 98, "1 s @16kHz -> 98 trames");
        assert!(mel.iter().all(|r| r.len() == N_MELS));
        let mel_short = pcm_to_log_mel(&vec![0.1f32; 100]);
        assert_eq!(mel_short.len(), 1);
        assert_eq!(mel_short[0].len(), N_MELS);
        let mel_empty = pcm_to_log_mel(&[]);
        assert_eq!(
            mel_empty.len(),
            1,
            "entrée vide -> 1 trame, jamais de panic"
        );
    }

    #[test]
    fn test_mel_silence_is_flat_floor() {
        let mel = pcm_to_log_mel(&vec![0.0f32; 1600]);
        for row in &mel {
            for &v in row {
                assert!(v.is_finite(), "silence : aucune valeur non-finie");
                assert!(v.abs() < 1e-3, "silence centré -> ~0, obtenu {v}");
            }
        }
    }

    #[test]
    fn test_mel_sine440_peak_in_expected_bank() {
        // Axe fréquentiel validé sur le log-mel BRUT (le centrage utterance
        // annule les moyennes par construction : il ne peut pas localiser
        // un pic, tester dessus serait conceptuellement faux).
        let mel = pcm_to_log_mel_raw(&sine_440(16_000));
        assert_eq!(mel.len(), 98);
        let mut mean = vec![0.0f32; N_MELS];
        for row in &mel {
            for (m, &v) in row.iter().enumerate() {
                mean[m] += v;
            }
        }
        for v in mean.iter_mut() {
            *v /= mel.len() as f32;
        }
        let peak = mean
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        // Banc dont le centre mel est le plus proche de 440 Hz (recalcul
        // indépendant : centres uniformes en mel, pas interpolation).
        let mel_lo = 2595.0f32 * (1.0f32 + MEL_FMIN_HZ / 700.0f32).log10();
        let mel_hi = 2595.0f32 * (1.0f32 + MEL_FMAX_HZ / 700.0f32).log10();
        let step = (mel_hi - mel_lo) / (N_MELS + 1) as f32;
        let mel_440 = 2595.0f32 * (1.0f32 + 440.0f32 / 700.0f32).log10();
        let expected = ((mel_440 - mel_lo) / step).round() as isize - 1;
        assert!(
            (peak as isize - expected).abs() <= 1,
            "pic 440 Hz : banc {peak}, attendu ~{expected}"
        );
        let floor = mean.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(
            mean[peak] > floor + 5.0,
            "le pic doit émerger du plancher (pic={}, plancher={})",
            mean[peak],
            floor
        );
    }

    #[test]
    fn test_mel_nan_inf_resilience() {
        let pcm = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.2, -0.3, 0.0];
        let mel = pcm_to_log_mel(&pcm);
        assert_eq!(mel.len(), 1);
        assert!(mel[0].iter().all(|v| v.is_finite()));
    }

    #[test]
    fn test_lfr_exact_small_cases() {
        // T=4, D=2, m=7, n=6 : T_lfr=1, pré-pad ×3, pas de post-pad.
        let r = |a: f32, b: f32| vec![a, b];
        let mel = vec![r(1.0, 2.0), r(3.0, 4.0), r(5.0, 6.0), r(7.0, 8.0)];
        let out = apply_lfr(&mel);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0],
            vec![
                1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0
            ]
        );
        // T=8 : T_lfr=2, post-pad ×2 avec la dernière trame.
        // buf = [r0×4, r1..r7] (11) + 2 pads ; ligne 0 = buf[0..7],
        // ligne 1 = buf[6..13] (buf[6] = r3).
        let mel8: Vec<Vec<f32>> = (0..8).map(|i| vec![i as f32, 100.0 + i as f32]).collect();
        let out8 = apply_lfr(&mel8);
        assert_eq!(out8.len(), 2);
        assert_eq!(out8[0][0], 0.0);
        assert_eq!(out8[0][12], 3.0, "buf[6] = r3");
        assert_eq!(out8[1][0], 3.0, "buf[6] répété en tête de ligne 1");
        assert_eq!(out8[1][10], 7.0, "buf[11] = dernière trame");
        assert_eq!(out8[1][12], 7.0, "buf[12] = pad (dernière trame)");
    }

    #[test]
    fn test_mvn_centers_and_scales() {
        let mut feats = vec![
            vec![1.0f32, 10.0],
            vec![2.0, 20.0],
            vec![3.0, 30.0],
            vec![4.0, 40.0],
        ];
        utterance_mvn(&mut feats);
        for d in 0..2 {
            let mean: f32 = feats.iter().map(|r| r[d]).sum::<f32>() / 4.0;
            let var: f32 = feats.iter().map(|r| (r[d] - mean).powi(2)).sum::<f32>() / 4.0;
            assert!(mean.abs() < 1e-5, "moyenne ~0 (dim {d})");
            assert!((var - 1.0).abs() < 1e-4, "variance ~1 (dim {d})");
        }
        // Silence -> reste ~zéro, fini.
        let mut sil = vec![vec![0.0f32; 4]; 3];
        utterance_mvn(&mut sil);
        assert!(
            sil.iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() < 1e-3)
        );
    }

    #[test]
    fn test_dither_deterministic_gaussian() {
        let mut a = vec![0.0f32; 16_000];
        let mut b = vec![0.0f32; 16_000];
        add_dither(&mut a, 1.0);
        add_dither(&mut b, 1.0);
        assert_eq!(a, b, "dither déterministe (même graine)");
        let mean: f32 = a.iter().sum::<f32>() / a.len() as f32;
        let var: f32 = a.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / a.len() as f32;
        assert!(mean.abs() < 0.05, "moyenne ~0, obtenue {mean}");
        assert!((var - 1.0).abs() < 0.08, "variance ~1, obtenue {var}");
        assert!(
            a.iter().any(|v| v.abs() > 2.0),
            "queues gaussiennes présentes"
        );
    }

    #[test]
    fn test_global_cmvn_math() {
        let mut feats = vec![vec![1.0f32, 2.0], vec![3.0, 4.0]];
        apply_global_cmvn(&mut feats, &[10.0, 100.0], &[0.5, 0.25]);
        assert_eq!(feats[0], vec![(1.0 + 10.0) * 0.5, (2.0 + 100.0) * 0.25]);
        assert_eq!(feats[1], vec![(3.0 + 10.0) * 0.5, (4.0 + 100.0) * 0.25]);
    }

    #[test]
    fn test_pipeline_global_shapes_and_finite() {
        let nm = vec![0.0f32; STACKED_DIM];
        let iv = vec![1.0f32; STACKED_DIM];
        let feats = pcm_to_sensevoice_feats_global(&vec![0.0f32; 16_000], &nm, &iv);
        assert_eq!(feats.len(), 98_usize.div_ceil(6));
        assert!(feats.iter().all(|r| r.len() == STACKED_DIM));
        assert!(feats.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn test_pipeline_shapes_and_silence() {
        let feats = pcm_to_sensevoice_feats(&vec![0.0f32; 16_000]);
        assert_eq!(
            feats.len(),
            98_usize.div_ceil(6),
            "98 trames mel -> 17 empilées"
        );
        assert!(feats.iter().all(|r| r.len() == STACKED_DIM));
        assert!(feats.iter().flatten().all(|v| v.is_finite()));
        let flat: f32 = feats.iter().flatten().map(|v| v.abs()).sum();
        assert!(flat < 1e-2, "silence -> features ~zéro après MVN");
        // Sine -> énergie non triviale.
        let fsine = pcm_to_sensevoice_feats(&sine_440(16_000));
        let e: f32 =
            fsine.iter().flatten().map(|v| v * v).sum::<f32>() / (fsine.len() * STACKED_DIM) as f32;
        assert!((e - 1.0).abs() < 0.35, "variance utterance ~1, obtenue {e}");
    }
}
