//! Sons système (spec §6) : trois signaux synthétisés par corde pincée
//! (Karplus-Strong). Aucun fichier audio embarqué.
//!
//! *éveil* (deux notes montantes, 180 ms), *fait* (une note douce, 220 ms),
//! *attention* (deux notes, 320 ms). Le volume suit bruit ambiant + canal.

/// Fréquence d'échantillonnage de synthèse.
pub const SAMPLE_RATE_HZ: u32 = 22050;

/// Convertit une note MIDI en Hz.
pub fn midi_to_hz(midi: u8) -> f32 {
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Corde pincée Karplus-Strong : bruit initial filtré par une ligne à retard
/// de N = sr/freq échantillons, moyennée à chaque période. Retourne des
/// échantillons f32 en −1..1 avec décroissance naturelle.
pub fn pluck(freq_hz: f32, duration_ms: u32, seed: u64) -> Vec<f32> {
    let sr = SAMPLE_RATE_HZ as usize;
    let n = ((sr as f32 / freq_hz).round() as usize).max(2);
    let total = (sr as f32 * duration_ms as f32 / 1000.0).round() as usize;
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
    let mut idx = 0;
    for _ in 0..total {
        let cur = line[idx];
        let nxt = line[(idx + 1) % n];
        let v = 0.996 * 0.5 * (cur + nxt);
        line[idx] = v;
        out.push(cur * 0.8);
        idx = (idx + 1) % n;
    }
    out
}

/// Joue une séquence de (note MIDI, durée ms) bout à bout.
pub fn play_notes(notes: &[(u8, u32)]) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, (midi, ms)) in notes.iter().enumerate() {
        out.extend(pluck(midi_to_hz(*midi), *ms, 0x9E37 + i as u64));
    }
    out
}

/// Signal *éveil* : deux notes montantes, 180 ms.
pub fn signal_eveil() -> Vec<f32> {
    play_notes(&[(69, 90), (76, 90)])
}

/// Signal *fait* : une note douce, 220 ms.
pub fn signal_fait() -> Vec<f32> {
    play_notes(&[(72, 220)])
}

/// Signal *attention* : deux notes, 320 ms.
pub fn signal_attention() -> Vec<f32> {
    play_notes(&[(67, 160), (67, 160)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signals_lengths_and_decay() {
        let e = signal_eveil();
        let f = signal_fait();
        let a = signal_attention();
        // 180/220/320 ms à 22050 Hz (±2 : arrondi par note).
        let close = |got: usize, ms: u32| {
            let expected = 22050 * ms as usize / 1000;
            assert!(
                got.abs_diff(expected) <= 2,
                "longueur signal : got {got}, expected {expected}"
            );
        };
        close(e.len(), 180);
        close(f.len(), 220);
        close(a.len(), 320);
        for sig in [&e, &f, &a] {
            // Non-silencieux au début, éteint à la fin (décroissance).
            let head: f32 = sig.iter().take(100).map(|x| x.abs()).sum();
            let tail: f32 = sig.iter().rev().take(100).map(|x| x.abs()).sum();
            assert!(head > 1.0, "attaque audible");
            assert!(tail < head * 0.5, "décroissance naturelle");
            assert!(sig.iter().all(|x| x.abs() <= 1.0));
        }
        // Déterministe.
        assert_eq!(signal_fait(), signal_fait());
    }
}
