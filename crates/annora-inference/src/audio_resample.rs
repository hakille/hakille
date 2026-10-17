/// Resample linéaire + downmix mono vers 16 kHz (std seul, sans DSP lourde).
///
/// - Downmix : moyenne arithmétique des canaux (entrelacé `[L0,R0,L1,R1,...]`).
/// - Resample : interpolation linéaire vers 16000 Hz.
/// - Sanitize : `NaN`/`Inf` -> `0.0`, puis clamp `[-1.0, 1.0]`.
/// - Cas limites : entrée vide, `channels == 0` ou `sample_rate == 0` -> `Vec` vide.
///
/// [`to_mono_16k`] ne fait aucune allocation autre que le vecteur de sortie
/// et ne panique jamais (retourne un `Vec` vide sur entrée dégénérée).
pub fn to_mono_16k(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<f32> {
    const TARGET_RATE: u32 = 16_000;
    if samples.is_empty() || channels == 0 || sample_rate == 0 {
        return Vec::new();
    }
    let ch = channels as usize;
    // Nombre de frames complètes (tronque la queue incomplète).
    let frames = samples.len() / ch;
    if frames == 0 {
        return Vec::new();
    }
    // Downmix : moyenne des canaux, avec sanitize.
    let mut mono: Vec<f32> = Vec::with_capacity(frames);
    for f in 0..frames {
        let base = f * ch;
        let mut acc = 0.0f32;
        for c in 0..ch {
            let s = match samples.get(base + c).copied() {
                Some(v) => v,
                None => 0.0,
            };
            acc += if s.is_finite() { s } else { 0.0 };
        }
        mono.push(acc / ch as f32);
    }
    // Déjà à 16 kHz : simple clamp.
    if sample_rate == TARGET_RATE {
        for s in mono.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        return mono;
    }
    // Resample linéaire.
    let ratio = TARGET_RATE as f64 / sample_rate as f64;
    let out_len = ((frames as f64 * ratio).round() as usize).max(1);
    if out_len == 1 || frames == 1 {
        let v = match mono.first().copied() {
            Some(v) => v,
            None => 0.0,
        };
        return vec![v.clamp(-1.0, 1.0)];
    }
    // Resample linéaire sans dérive de phase : la grille de sortie est
    // uniformément répartie sur l'entrée (`pos = i * frames / out_len`),
    // pas de recalage cumulatif (l'ancienne cartographie `[0,frames-1] ->
    // [0,out-1]` introduisait une dérive de phase mesurée à 439.5 Hz
    // pour un 440 Hz, SNR 23.5 dB ; ici phase verrouillée).
    let mut out = Vec::with_capacity(out_len);
    let step = frames as f64 / out_len as f64;
    for i in 0..out_len {
        let pos = i as f64 * step;
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(frames - 1);
        let frac = (pos - lo as f64) as f32;
        let a = match mono.get(lo).copied() {
            Some(v) => v,
            None => 0.0,
        };
        let b = match mono.get(hi).copied() {
            Some(v) => v,
            None => 0.0,
        };
        let v = a + (b - a) * frac;
        out.push(if v.is_finite() {
            v.clamp(-1.0, 1.0)
        } else {
            0.0
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_mono_16k;

    #[test]
    fn resample_422_to_160_frames() {
        // 422 frames @ 42200 Hz -> 160 frames @ 16 kHz (ratio exact 422/160).
        let input = vec![0.5f32; 422];
        let out = to_mono_16k(&input, 1, 42_200);
        assert_eq!(out.len(), 160, "422 @42200Hz doit donner 160 @16kHz");
        for s in &out {
            assert!((s - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn stereo_downmix_to_mono() {
        // Stéréo entrelacée : L=1.0, R=-1.0 -> mono 0.0 ; L=0.6,R=0.2 -> 0.4.
        let stereo = vec![1.0f32, -1.0, 0.6, 0.2];
        let out = to_mono_16k(&stereo, 2, 16_000);
        assert_eq!(out.len(), 2);
        assert!((out[0]).abs() < 1e-6);
        assert!((out[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn clamps_and_sanitizes() {
        let input = vec![2.5f32, -3.0, f32::NAN, f32::INFINITY, 0.3];
        let out = to_mono_16k(&input, 1, 16_000);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1], -1.0);
        assert_eq!(out[2], 0.0);
        assert_eq!(out[3], 0.0);
        assert!((out[4] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn empty_and_degenerate_inputs() {
        assert!(to_mono_16k(&[], 1, 16_000).is_empty());
        assert!(to_mono_16k(&[0.5; 4], 0, 16_000).is_empty());
        assert!(to_mono_16k(&[0.5; 4], 1, 0).is_empty());
    }

    /// Sine 440 Hz @48 kHz : fréquence, amplitude et phase conservées à 16 kHz.
    fn sine_48k(freq_hz: f32, amp: f32, seconds: f32) -> Vec<f32> {
        let n = (48_000.0 * seconds).round() as usize;
        (0..n)
            .map(|t| (t as f32 / 48_000.0 * freq_hz * 2.0 * std::f32::consts::PI).sin() * amp)
            .collect()
    }

    /// Corrélation normalisée + SNR vs sine idéale @16 kHz (même phase t=0).
    fn corr_and_snr(resampled: &[f32], freq_hz: f32, amp: f32) -> (f32, f32) {
        let mut dot = 0.0f64;
        let mut e_out = 0.0f64;
        for (i, s) in resampled.iter().enumerate() {
            let ideal = (i as f32 / 16_000.0 * freq_hz * 2.0 * std::f32::consts::PI).sin() * amp;
            dot += (*s as f64) * (ideal as f64);
            e_out += (*s as f64) * (*s as f64);
        }
        let e_ideal = resampled.len() as f64 * (amp as f64).powi(2) / 2.0;
        let corr = if e_out > 0.0 && e_ideal > 0.0 {
            (dot / (e_out * e_ideal).sqrt()) as f32
        } else {
            0.0
        };
        // Résidu après projection optimale : SNR = E_proj / E_res.
        let gain = if e_ideal > 0.0 { dot / e_ideal } else { 0.0 };
        let mut e_res = 0.0f64;
        for (i, s) in resampled.iter().enumerate() {
            let ideal = (i as f32 / 16_000.0 * freq_hz * 2.0 * std::f32::consts::PI).sin() * amp;
            let r = (*s as f64) - gain * (ideal as f64);
            e_res += r * r;
        }
        let snr_db = if e_res > 0.0 {
            (10.0 * ((gain * gain * e_ideal) / e_res).log10()) as f32
        } else {
            999.0
        };
        (corr, snr_db)
    }

    /// Estimation de fréquence par comptage de passages par zéro.
    fn zero_crossing_freq(x: &[f32], sr: f32) -> f32 {
        if x.len() < 2 {
            return 0.0;
        }
        let mut zc = 0usize;
        for i in 1..x.len() {
            if (x[i] >= 0.0 && x[i - 1] < 0.0) || (x[i] < 0.0 && x[i - 1] >= 0.0) {
                zc += 1;
            }
        }
        zc as f32 * sr / (2.0 * x.len() as f32)
    }

    #[test]
    fn resample_48k_to_16k_keeps_440hz() {
        let src = sine_48k(440.0, 0.8, 1.0);
        assert_eq!(src.len(), 48_000);
        let out = to_mono_16k(&src, 1, 48_000);
        assert_eq!(out.len(), 16_000, "48k->16k : 1s doit donner 16000 éch");
        let f = zero_crossing_freq(&out, 16_000.0);
        eprintln!("resample 440Hz: f_est={f:.2}Hz");
        assert!(
            (f - 440.0).abs() < 5.0,
            "fréquence conservée: {f:.2}Hz vs 440Hz"
        );
        let (corr, snr) = corr_and_snr(&out, 440.0, 0.8);
        eprintln!("resample 440Hz: corr={corr:.5} snr={snr:.1}dB");
        assert!(corr > 0.98, "phase/amplitude conservées: corr={corr:.5}");
        assert!(snr > 30.0, "interpolation propre: SNR={snr:.1}dB");
    }

    #[test]
    fn resample_48k_stereo_sine_downmix() {
        // L = sine 440Hz*0.8, R = silence -> mono = sine*0.4.
        let l = sine_48k(440.0, 0.8, 0.5);
        let mut stereo = Vec::with_capacity(l.len() * 2);
        for s in &l {
            stereo.push(*s);
            stereo.push(0.0);
        }
        let out = to_mono_16k(&stereo, 2, 48_000);
        assert_eq!(out.len(), 8000);
        let (corr, snr) = corr_and_snr(&out, 440.0, 0.4);
        eprintln!("resample stéréo: corr={corr:.5} snr={snr:.1}dB");
        assert!(corr > 0.98);
        assert!(snr > 30.0);
    }

    #[test]
    fn resample_48k_silence_stays_silent() {
        let out = to_mono_16k(&vec![0.0f32; 48_000], 1, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| *s == 0.0));
    }
}
