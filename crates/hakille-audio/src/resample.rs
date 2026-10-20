//! Rééchantillonnage linéaire et mixage mono, sans dépendance externe.
//!
//! Les primitives publiques ([`resample_linear`], [`downmix_to_mono`]) sont pures
//! et testables ; la capture utilise [`LinearStreamResampler`], variante streaming
//! continue (interpolation entre blocs consécutifs).

/// Convertit un flux entrelacé en mono par moyenne des canaux.
///
/// Un éventuel reste de trame incomplète est ignoré. `channels == 0` renvoie vide.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let channel_count = channels as usize;
    if channel_count == 0 {
        return Vec::new();
    }
    if channel_count == 1 {
        return interleaved.to_vec();
    }
    let mut out = Vec::with_capacity(interleaved.len() / channel_count);
    for frame in interleaved.chunks_exact(channel_count) {
        let sum: f32 = frame.iter().copied().sum();
        out.push(sum / channel_count as f32);
    }
    out
}

/// Rééchantillonne `input` (mono) de `from_rate` vers `to_rate` par interpolation
/// linéaire. Le signal en fin de buffer est prolongé par le dernier échantillon.
///
/// Renvoie un buffer vide si l'entrée est vide ou si une fréquence est nulle.
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (input.len() as f64 * to_rate as f64 / from_rate as f64).round() as usize;
    let last = input.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let source = i as f64 * ratio;
        let index = source.floor() as usize;
        let frac = (source - index as f64) as f32;
        let a = input[index.min(last)];
        let b = input[(index + 1).min(last)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[inline]
fn lerp(a: f32, b: f32, frac: f32) -> f32 {
    a + (b - a) * frac
}

/// Rééchantillonneur linéaire streaming : conserve le dernier échantillon comme
/// point d'ancrage pour interpoler entre deux blocs cpal consécutifs.
pub(crate) struct LinearStreamResampler {
    step: f64,
    buffer: Vec<f32>,
    position: f64,
}

impl LinearStreamResampler {
    /// `from_rate` et `to_rate` doivent être non nuls (garanti par l'appelant).
    pub(crate) fn new(from_rate: u32, to_rate: u32) -> Self {
        let step = if from_rate == 0 || to_rate == 0 {
            1.0
        } else {
            from_rate as f64 / to_rate as f64
        };
        Self {
            step,
            buffer: Vec::new(),
            position: 0.0,
        }
    }

    /// Ajoute des échantillons source et pousse les échantillons rééchantillonnés
    /// prêts (ceux qui nécessitent un échantillon suivant déjà connu) dans `out`.
    pub(crate) fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() || self.step <= 0.0 || !self.step.is_finite() {
            return;
        }
        self.buffer.extend_from_slice(input);
        while self.position + 1.0 < self.buffer.len() as f64 {
            let index = self.position.floor() as usize;
            let frac = (self.position - index as f64) as f32;
            out.push(lerp(self.buffer[index], self.buffer[index + 1], frac));
            self.position += self.step;
        }
        let keep = (self.position.floor() as usize).min(self.buffer.len() - 1);
        if keep > 0 {
            self.buffer.drain(..keep);
            self.position -= keep as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_mono_is_passthrough() {
        let input = [0.1, -0.2, 0.3];
        assert_eq!(downmix_to_mono(&input, 1), vec![0.1, -0.2, 0.3]);
    }

    #[test]
    fn downmix_stereo_averages_exactly() {
        let input = [1.0, 0.0, 0.5, 0.5];
        assert_eq!(downmix_to_mono(&input, 2), vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_four_channels_averages_exactly() {
        let input = [0.25, 0.75, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        assert_eq!(downmix_to_mono(&input, 4), vec![0.0, 1.0]);
    }

    #[test]
    fn downmix_ignores_incomplete_frame_and_handles_edge_cases() {
        assert_eq!(downmix_to_mono(&[0.5, 0.5, 1.0], 2), vec![0.5]);
        assert_eq!(downmix_to_mono(&[], 2), Vec::<f32>::new());
        assert_eq!(downmix_to_mono(&[0.5, 0.5], 0), Vec::<f32>::new());
    }

    #[test]
    fn resample_identity_is_exact_copy() {
        let input = [0.0, 0.25, 1.0, -1.0];
        assert_eq!(resample_linear(&input, 16_000, 16_000), input.to_vec());
    }

    #[test]
    fn resample_downsample_4_to_2_is_exact() {
        let input = [0.0, 1.0, 2.0, 3.0];
        assert_eq!(resample_linear(&input, 4, 2), vec![0.0, 2.0]);
    }

    #[test]
    fn resample_upsample_2_to_4_interpolates_exactly() {
        let input = [0.0, 1.0];
        assert_eq!(resample_linear(&input, 2, 4), vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn resample_48k_to_16k_keeps_every_third_sample() {
        let input: Vec<f32> = (0..480).map(|i| i as f32).collect();
        let out = resample_linear(&input, 48_000, 16_000);
        assert_eq!(out.len(), 160);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[10], 30.0);
        assert_eq!(out[159], 477.0);
    }

    #[test]
    fn resample_single_sample_is_repeated() {
        let out = resample_linear(&[0.25], 1, 4);
        assert_eq!(out, vec![0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn resample_zero_rate_is_empty_not_panic() {
        assert!(resample_linear(&[0.1, 0.2], 0, 16_000).is_empty());
        assert!(resample_linear(&[0.1, 0.2], 16_000, 0).is_empty());
    }

    #[test]
    fn streaming_resampler_identity_keeps_lag_of_one_sample() {
        let mut resampler = LinearStreamResampler::new(16_000, 16_000);
        let mut out = Vec::new();
        resampler.push(&[1.0, 2.0, 3.0], &mut out);
        assert_eq!(out, vec![1.0, 2.0]);
        out.clear();
        resampler.push(&[4.0, 5.0], &mut out);
        assert_eq!(out, vec![3.0, 4.0]);
    }

    #[test]
    fn streaming_resampler_downsample_by_two_is_exact() {
        let mut resampler = LinearStreamResampler::new(2, 1);
        let mut out = Vec::new();
        resampler.push(&[0.0, 1.0, 2.0, 3.0], &mut out);
        assert_eq!(out, vec![0.0, 2.0]);
    }

    #[test]
    fn streaming_resampler_handles_extreme_downsample_without_panic() {
        let mut resampler = LinearStreamResampler::new(48_000, 1);
        let mut out = Vec::new();
        resampler.push(&[0.0, 1.0, 2.0, 3.0], &mut out);
        assert!(!out.is_empty());
        resampler.push(&[4.0, 5.0, 6.0], &mut out);
    }
}
