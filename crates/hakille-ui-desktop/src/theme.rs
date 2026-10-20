//! Thème Hakille — tokens comme source unique, 5 états par composant.
//!
//! Clair/sombre réel, contraste élevé, densité confortable/compacte,
//! texte 80–200 %. Chaque composant doit exister en :
//! normal, focus clavier, chargement, erreur, mouvement-réduit/contraste-élevé.

use hakille_design::generated::color as tokens;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    Comfortable,
    Compact,
}

/// Échelle de texte 0.8–2.0 (200 % dès la v1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TextScale(pub f32);

impl TextScale {
    pub fn new(v: f32) -> Self {
        Self(v.clamp(0.8, 2.0))
    }
    pub fn scaled(&self, base: f32) -> f32 {
        base * self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub mode: ThemeMode,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub density: Density,
    pub text_scale: TextScale,
    // Couleurs résolues depuis tokens.
    pub bg: u32,
    pub surface1: u32,
    pub text1: u32,
    pub text2: u32,
    pub line: u32,
    pub braise: u32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            high_contrast: false,
            reduced_motion: false,
            density: Density::Comfortable,
            text_scale: TextScale::new(1.0),
            bg: tokens::COLOR_SURFACE_0_DARK,
            surface1: tokens::COLOR_SURFACE_1_DARK,
            text1: tokens::COLOR_TEXTE_1_DARK,
            text2: tokens::COLOR_TEXTE_2_DARK,
            line: tokens::COLOR_LIGNE_DARK,
            braise: tokens::COLOR_BRAISE_DARK,
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            high_contrast: false,
            reduced_motion: false,
            density: Density::Comfortable,
            text_scale: TextScale::new(1.0),
            bg: tokens::COLOR_SURFACE_0_LIGHT,
            surface1: tokens::COLOR_SURFACE_1_LIGHT,
            text1: tokens::COLOR_TEXTE_1_LIGHT,
            text2: tokens::COLOR_TEXTE_2_LIGHT,
            line: tokens::COLOR_LIGNE_LIGHT,
            braise: tokens::COLOR_BRAISE_LIGHT,
        }
    }

    pub fn with_high_contrast(mut self, v: bool) -> Self {
        self.high_contrast = v;
        if v {
            self.text1 = match self.mode {
                ThemeMode::Dark => 0xFFFFFF,
                ThemeMode::Light => 0x000000,
            };
            self.line = self.text1;
        }
        self
    }

    pub fn with_reduced_motion(mut self, v: bool) -> Self {
        self.reduced_motion = v;
        self
    }

    pub fn with_text_scale(mut self, v: f32) -> Self {
        self.text_scale = TextScale::new(v);
        self
    }

    /// Braise strictement réservée à validation/hold/decision (spec §3).
    pub fn braise_allowed(&self, usage: &str) -> bool {
        hakille_design::color::braise_allowed(usage)
    }

    pub fn button_height(&self) -> f32 {
        match self.density {
            Density::Comfortable => 44.0,
            Density::Compact => 32.0,
        }
    }

    /// Ratio de contraste WCAG simplifié (luminance relative).
    pub fn contrast_ratio(&self, fg: u32, bg: u32) -> f32 {
        fn lum(h: u32) -> f32 {
            let r = (((h >> 16) & 0xFF) as f32 / 255.0).powf(2.2);
            let g = (((h >> 8) & 0xFF) as f32 / 255.0).powf(2.2);
            let b = ((h & 0xFF) as f32 / 255.0).powf(2.2);
            0.2126 * r + 0.7152 * g + 0.0722 * b
        }
        let (l1, l2) = (lum(fg).max(lum(bg)), lum(fg).min(lum(bg)));
        (l1 + 0.05) / (l2 + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_light_tokens() {
        let d = Theme::dark();
        assert_eq!(d.bg, 0x0B0C0F);
        assert_eq!(d.braise, 0xFF7A2F);
        let l = Theme::light();
        assert_eq!(l.bg, 0xF6F4F0);
        assert_eq!(l.braise, 0xC2410C);
        assert!(d.braise_allowed("validation"));
        assert!(!d.braise_allowed("decoration"));
    }

    #[test]
    fn contrast_aa() {
        let d = Theme::dark();
        assert!(d.contrast_ratio(d.text1, d.bg) >= 4.5);
        let hc = Theme::dark().with_high_contrast(true);
        assert!(hc.contrast_ratio(hc.text1, hc.bg) >= 7.0);
    }

    #[test]
    fn text_scale_clamped_80_200() {
        assert_eq!(Theme::dark().with_text_scale(5.0).text_scale.0, 2.0);
        assert_eq!(Theme::dark().with_text_scale(0.1).text_scale.0, 0.8);
    }
}
