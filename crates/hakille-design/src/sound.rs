//! Sons système (spec §6) : trois signaux synthétisés par corde pincée
//! (Karplus-Strong), PCM mono f32 à 48 kHz. Aucun fichier audio embarqué.
//!
//! *éveil* (deux notes montantes, 180 ms), *fait* (une note douce, 220 ms),
//! *attention* (deux notes, 320 ms). Notes et durées viennent des tokens
//! (`tokens/motion.toml` → `generated::motion::MOTION_SOUND_*`) : rien en dur.
//! Le volume est fourni par l'appelant (bruit ambiant + Sélecteur de Canal).
//! Synthèse déterministe : mêmes entrées → mêmes échantillons, bit à bit.

use crate::generated::motion as sound_tokens;

/// Fréquence d'échantillonnage de synthèse (PCM mono f32).
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Atténuation par aller-retour de la ligne à retard (timbre « corde »).
const KS_DAMPING: f32 = 0.996;
/// Gain d'une corde pincée : marge sous 1,0, jamais d'écrêtage.
const PLUCK_GAIN: f32 = 0.8;
/// Rampe d'attaque en ms (départ exactement à zéro : pas de clic).
const ATTACK_MS: u32 = 2;
/// Fondu de sortie en ms (retour exactement à zéro : pas de clic).
const RELEASE_MS: u32 = 15;

/// Signal d'état synthétisable (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateSound {
    /// Deux notes montantes, 180 ms.
    Eveil,
    /// Une note douce, 220 ms.
    Fait,
    /// Deux notes, 320 ms.
    Attention,
}

impl StateSound {
    /// Les trois signaux, dans l'ordre de la spec.
    pub const ALL: [StateSound; 3] = [Self::Eveil, Self::Fait, Self::Attention];

    /// Paires (note MIDI, durée ms) issues des tokens.
    pub fn notes(self) -> &'static [(i64, i64)] {
        match self {
            Self::Eveil => &sound_tokens::MOTION_SOUND_EVEIL,
            Self::Fait => &sound_tokens::MOTION_SOUND_FAIT,
            Self::Attention => &sound_tokens::MOTION_SOUND_ATTENTION,
        }
    }

    /// Durée totale en ms (somme des notes, issue des tokens).
    pub fn duration_ms(self) -> u32 {
        self.notes().iter().map(|&(_, ms)| ms.max(0) as u32).sum()
    }

    /// Libellé français, tel qu'écrit dans la spec.
    pub fn label(self) -> &'static str {
        match self {
            Self::Eveil => "éveil",
            Self::Fait => "fait",
            Self::Attention => "attention",
        }
    }
}

/// Convertit une note MIDI en Hz.
pub fn midi_to_hz(midi: u8) -> f32 {
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Borne un volume dans `0..=1` :
/// * négatif → `0.0` (silence) ;
/// * `> 1.0` (ou `+∞`) → `1.0` (saturation : le gain reste ≤ 1, pas
///   d'écrêtage interne) ;
/// * `NaN` (ou `−∞`) → `0.0` (silence sûr, aucune panique).
pub fn sanitize_volume(volume: f32) -> f32 {
    if volume.is_nan() {
        0.0
    } else {
        volume.clamp(0.0, 1.0)
    }
}

/// Longueur en échantillons d'une durée ms (arrondi au plus proche).
fn samples_for_ms(ms: u32) -> usize {
    ((SAMPLE_RATE_HZ as u64 * ms as u64 + 500) / 1000) as usize
}

/// Applique une rampe d'attaque et un fondu de sortie (anti-clic) :
/// premier et dernier échantillon exactement à zéro.
fn apply_attack_release(buf: &mut [f32]) {
    if buf.is_empty() {
        return;
    }
    let sr = SAMPLE_RATE_HZ as usize;
    let attack = (sr * ATTACK_MS as usize / 1000).max(1).min(buf.len());
    let release = (sr * RELEASE_MS as usize / 1000).max(1).min(buf.len());
    for (i, sample) in buf.iter_mut().enumerate().take(attack) {
        *sample *= i as f32 / attack as f32;
    }
    for (i, sample) in buf.iter_mut().rev().enumerate().take(release) {
        *sample *= i as f32 / release as f32;
    }
}

/// Corde pincée Karplus-Strong : bruit initial filtré par une ligne à retard
/// de N = sr/freq échantillons, moyennée à chaque période. Retourne des
/// échantillons f32 en −1..1, sans clic aux extrémités, déterministe.
pub fn pluck(freq_hz: f32, duration_ms: u32, seed: u64) -> Vec<f32> {
    let sr = SAMPLE_RATE_HZ as u64;
    let total = samples_for_ms(duration_ms);
    let freq = if freq_hz.is_finite() && freq_hz > 0.0 {
        freq_hz
    } else {
        1.0
    };
    let n = ((sr as f32 / freq).round() as usize).max(2);
    // PRNG déterministe (xorshift64) : mêmes notes → mêmes échantillons.
    let mut s = seed.max(1);
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
    };
    let mut line: Vec<f32> = (0..n).map(|_| next()).collect();
    let mut out = Vec::with_capacity(total);
    let mut idx = 0usize;
    for _ in 0..total {
        let cur = line[idx];
        let nxt = line[(idx + 1) % n];
        line[idx] = KS_DAMPING * 0.5 * (cur + nxt);
        out.push(cur * PLUCK_GAIN);
        idx = (idx + 1) % n;
    }
    apply_attack_release(&mut out);
    out
}

/// Joue une séquence de (note MIDI, durée ms) bout à bout.
/// Chaque corde est rendue sans clic ; la concaténation l'est donc aussi.
pub fn play_notes(notes: &[(u8, u32)]) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, (midi, ms)) in notes.iter().enumerate() {
        out.extend(pluck(midi_to_hz(*midi), *ms, 0x9E37 + i as u64));
    }
    out
}

/// Rend un signal d'état en PCM mono f32 48 kHz.
///
/// * `volume` est borné par [`sanitize_volume`] : `0` → silence de même
///   durée (la longueur du buffer est conservée), `> 1` → saturation à `1`,
///   `NaN` → silence.
/// * Déterministe bit à bit : mêmes `kind`/`volume` → mêmes échantillons.
pub fn render_state_sound(kind: StateSound, volume: f32) -> Vec<f32> {
    let gain = sanitize_volume(volume);
    let notes: Vec<(u8, u32)> = kind
        .notes()
        .iter()
        .map(|&(midi, ms)| (midi.clamp(0, 127) as u8, ms.max(0) as u32))
        .collect();
    let mut out = play_notes(&notes);
    if gain == 0.0 {
        out.fill(0.0);
    } else {
        for sample in &mut out {
            *sample *= gain;
        }
    }
    out
}

/// Signal *éveil* : deux notes montantes, 180 ms (volume plein).
pub fn signal_eveil() -> Vec<f32> {
    render_state_sound(StateSound::Eveil, 1.0)
}

/// Signal *fait* : une note douce, 220 ms (volume plein).
pub fn signal_fait() -> Vec<f32> {
    render_state_sound(StateSound::Fait, 1.0)
}

/// Signal *attention* : deux notes, 320 ms (volume plein).
pub fn signal_attention() -> Vec<f32> {
    render_state_sound(StateSound::Attention, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE_MS: f64 = 0.05;

    fn assert_duration(kind: StateSound, expected_ms: u32) {
        assert_eq!(kind.duration_ms(), expected_ms, "{kind:?}");
        let out = render_state_sound(kind, 1.0);
        let got_ms = out.len() as f64 * 1000.0 / SAMPLE_RATE_HZ as f64;
        let err = (got_ms - expected_ms as f64).abs() / expected_ms as f64;
        assert!(err <= TOLERANCE_MS, "{kind:?} : {got_ms} ms");
        assert_eq!(out.len(), samples_for_ms(expected_ms), "{kind:?}");
    }

    #[test]
    fn test_state_sounds_durations() {
        assert_duration(StateSound::Eveil, 180);
        assert_duration(StateSound::Fait, 220);
        assert_duration(StateSound::Attention, 320);
    }

    #[test]
    fn test_state_sounds_energy_and_bounds() {
        for kind in StateSound::ALL {
            let out = render_state_sound(kind, 1.0);
            let energy: f64 = out.iter().map(|x| (*x as f64) * (*x as f64)).sum();
            assert!(energy > 1e-3, "{kind:?} : énergie {energy}");
            assert!(out.iter().all(|x| x.abs() <= 1.0), "{kind:?} : > 1.0");
            let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.05, "{kind:?} : attaque inaudible ({peak})");
        }
    }

    #[test]
    fn test_volume_zero_is_silence_but_keeps_duration() {
        for kind in StateSound::ALL {
            let out = render_state_sound(kind, 0.0);
            assert_eq!(out.len(), samples_for_ms(kind.duration_ms()), "{kind:?}");
            assert!(out.iter().all(|&x| x == 0.0), "{kind:?} : pas silence");
        }
    }

    #[test]
    fn test_render_deterministic_bitwise() {
        for kind in StateSound::ALL {
            for volume in [0.0f32, 0.35, 1.0] {
                let a = render_state_sound(kind, volume);
                let b = render_state_sound(kind, volume);
                assert_eq!(a.len(), b.len());
                for (x, y) in a.iter().zip(&b) {
                    assert_eq!(x.to_bits(), y.to_bits(), "{kind:?} @ {volume}");
                }
            }
        }
    }

    #[test]
    fn test_no_click_at_edges() {
        for kind in StateSound::ALL {
            let out = render_state_sound(kind, 1.0);
            assert_eq!(out.first().copied(), Some(0.0), "{kind:?} : premier");
            assert_eq!(out.last().copied(), Some(0.0), "{kind:?} : dernier");
            let head = out.iter().take(8).fold(0.0f32, |m, x| m.max(x.abs()));
            let tail = out.iter().rev().take(8).fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(head < 0.1, "{kind:?} : clic d'attaque ({head})");
            assert!(tail < 0.1, "{kind:?} : clic de sortie ({tail})");
        }
    }

    #[test]
    fn test_volume_clamped_saturated_and_scaled() {
        let full = render_state_sound(StateSound::Fait, 1.0);
        let loud = render_state_sound(StateSound::Fait, 4.0);
        assert_eq!(full, loud, "volume > 1 doit saturer à 1");
        let quiet = render_state_sound(StateSound::Fait, 0.5);
        for (a, b) in quiet.iter().zip(&full) {
            assert!((a * 2.0 - b).abs() < 1e-6, "échelle linéaire");
        }
        assert!(
            render_state_sound(StateSound::Fait, -3.0)
                .iter()
                .all(|&x| x == 0.0)
        );
        assert!(
            render_state_sound(StateSound::Fait, f32::NAN)
                .iter()
                .all(|&x| x == 0.0)
        );
        assert_eq!(sanitize_volume(f32::INFINITY), 1.0);
        assert_eq!(sanitize_volume(-1.0), 0.0);
        assert_eq!(sanitize_volume(0.25), 0.25);
    }

    #[test]
    fn test_midi_and_token_sequences() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1e-3);
        assert!((midi_to_hz(81) - 880.0).abs() < 1e-3);
        assert_eq!(StateSound::Eveil.notes(), &[(69, 90), (76, 90)]);
        assert_eq!(StateSound::Fait.notes(), &[(72, 220)]);
        assert_eq!(StateSound::Attention.notes(), &[(67, 160), (67, 160)]);
        assert_eq!(StateSound::Eveil.label(), "éveil");
        assert_eq!(StateSound::Fait.label(), "fait");
        assert_eq!(StateSound::Attention.label(), "attention");
        // Le signal natif est le rendu à volume plein.
        assert_eq!(signal_fait(), render_state_sound(StateSound::Fait, 1.0));
        assert_eq!(signal_eveil(), render_state_sound(StateSound::Eveil, 1.0));
        assert_eq!(
            signal_attention(),
            render_state_sound(StateSound::Attention, 1.0)
        );
    }
}
