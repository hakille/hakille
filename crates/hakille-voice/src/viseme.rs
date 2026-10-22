//! Visèmes : formes de bouche dérivées de la source phonémique RÉELLE.
//!
//! HONNÊTETÉ : il s'agit d'une timeline de **visèmes phonémiques**, pas de
//! lip-sync image par image. La coarticulation (transition entre deux
//! phonèmes) n'est pas modélisée : la timeline est donc une approximation
//! documentée, jamais un « lip-sync parfait ».
//!
//! Source unique : la timeline phonémique du TTS formantique (`FormantStream`)
//! — le même contenu que ce qui est RÉELLEMENT synthétisé. Une timeline
//! divergeant de l'audio serait un mensonge, donc l'invariant
//! « somme des durées == nombre d'échantillons » est vérifié par test.

use serde::{Deserialize, Serialize};

/// Forme de bouche (visème).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viseme {
    /// Silence (pause / respiration).
    Silence,
    /// Bouche ouverte large (a).
    A,
    /// Bouche mi-ouverte, langue basse (e).
    E,
    /// Bouche étirée, langue haute avant (i).
    I,
    /// Bouche arrondie (o).
    O,
    /// Boucle serrée, langue arrière (u).
    U,
    /// Lèvres fermées (m, b, p).
    Mbp,
    /// Lèvres contacting les dents (f, v).
    Fv,
    /// Pointe de la langue sur les dents (l, th).
    L,
    /// Position neutre (consonnes non articulatoires).
    Rest,
    /// Phonème non cartographié.
    Unknown,
}

/// Visème associé à un symbole phonémique du TTS.
///
/// Les symboles proviennent de `phoneme_params` (`crates/hakille-voice/src/
/// formant.rs`) : la table ci-dessous les couvre tous.
pub fn viseme_for_phoneme(sym: &str) -> Viseme {
    match sym {
        // Voyelles : la forme suit l'aperture et la position linguale.
        "a" | "A" | "ae" | "an" => Viseme::A,
        "e" | "E" => Viseme::E,
        "i" | "I" | "in" => Viseme::I,
        "o" | "O" | "on" => Viseme::O,
        "u" | "U" | "un" | "oe" | "eu" => Viseme::U,
        "y" => Viseme::I,
        // Consonnes labiales : lèvres fermées.
        "m" | "b" | "p" => Viseme::Mbp,
        // Consonnes labio-dentales : lèvres sur les dents.
        "f" | "v" => Viseme::Fv,
        // Pointe linguale.
        "l" | "R" | "th" | "dh" => Viseme::L,
        // Pauses.
        s if s.starts_with("pau") => Viseme::Silence,
        // Tout le reste (s, z, t, k, n, j, w, …) : position neutre.
        s if !s.is_empty() => Viseme::Rest,
        _ => Viseme::Unknown,
    }
}

/// Tranche de timeline : un visème tenu sur `[start_sample, start+len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisemeFrame {
    pub viseme: Viseme,
    pub start_sample: usize,
    pub len_samples: usize,
}

impl VisemeFrame {
    /// Fin de la tranche (échantillon exclusive).
    pub fn end_sample(&self) -> usize {
        self.start_sample + self.len_samples
    }

    /// Durée en millisecondes (fréquence d'échantillonnage donnée).
    pub fn duration_ms(&self, sample_rate: u32) -> u64 {
        if sample_rate == 0 {
            return 0;
        }
        (self.len_samples as u64 * 1000) / sample_rate as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_covers_the_phoneme_inventory() {
        assert_eq!(viseme_for_phoneme("a"), Viseme::A);
        assert_eq!(viseme_for_phoneme("i"), Viseme::I);
        assert_eq!(viseme_for_phoneme("u"), Viseme::U);
        assert_eq!(viseme_for_phoneme("o"), Viseme::O);
        assert_eq!(viseme_for_phoneme("e"), Viseme::E);
        assert_eq!(viseme_for_phoneme("m"), Viseme::Mbp);
        assert_eq!(viseme_for_phoneme("f"), Viseme::Fv);
        assert_eq!(viseme_for_phoneme("l"), Viseme::L);
        assert_eq!(viseme_for_phoneme("pau"), Viseme::Silence);
        assert_eq!(viseme_for_phoneme("s"), Viseme::Rest);
        assert_eq!(viseme_for_phoneme(""), Viseme::Unknown);
    }

    #[test]
    fn frame_duration_is_real_time() {
        let f = VisemeFrame {
            viseme: Viseme::A,
            start_sample: 0,
            len_samples: 24_000,
        };
        assert_eq!(f.end_sample(), 24_000);
        assert_eq!(f.duration_ms(24_000), 1000);
        assert_eq!(f.duration_ms(0), 0);
    }
}
