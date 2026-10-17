//! Moteur de texte — trait stable, POC Parley autorisé derrière.
//!
//! Exigences S4 : FR/EN/AR-RTL/JA-IME, aucun caractère coupé, fallback
//! polices (Inter + Noto chaînée), shaping et cache mesurés.
//! Implémentation interne sans dépendance expérimentale obligatoire :
//! mesure déterministe + détection RTL + curseur UTF-8 correct.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapedRun {
    pub text: String,
    pub width_px: f32,
    pub is_rtl: bool,
    pub glyph_count: usize,
}

pub trait TextEngine {
    fn shape(&self, text: &str, size_px: f32, max_width: Option<f32>) -> ShapedRun;
    fn line_height(&self, size_px: f32) -> f32;
    fn cursor_offset(&self, text: &str, byte_index: usize) -> f32;
}

/// Moteur simple déterministe — référence pour tests S4 + golden.
#[derive(Debug, Default, Clone)]
pub struct SimpleTextEngine {
    pub avg_advance_ratio: f32,
}

impl SimpleTextEngine {
    pub fn new() -> Self {
        Self { avg_advance_ratio: 0.6 }
    }

    fn detect_rtl(text: &str) -> bool {
        text.chars().any(|c| matches!(c,
            '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' |
            '\u{0590}'..='\u{05FF}' | '\u{FB50}'..='\u{FDFF}' |
            '\u{FE70}'..='\u{FEFF}'
        ))
    }

    fn is_cjk(c: char) -> bool {
        matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
    }
}

impl TextEngine for SimpleTextEngine {
    fn shape(&self, text: &str, size_px: f32, max_width: Option<f32>) -> ShapedRun {
        let mut w = 0.0f32;
        let mut glyphs = 0usize;
        for c in text.chars() {
            let adv = if Self::is_cjk(c) { 1.0 } else if c.is_ascii_uppercase() { 0.72 } else if c == ' ' { 0.35 } else { self.avg_advance_ratio };
            w += adv * size_px;
            glyphs += 1;
            if let Some(m) = max_width {
                if w > m {
                    break;
                }
            }
        }
        ShapedRun { text: text.to_string(), width_px: w, is_rtl: Self::detect_rtl(text), glyph_count: glyphs }
    }

    fn line_height(&self, size_px: f32) -> f32 {
        size_px * 1.5
    }

    fn cursor_offset(&self, text: &str, byte_index: usize) -> f32 {
        // Curseur correct sur frontières UTF-8 (ne coupe jamais un char).
        let clamped = text.len().min(byte_index);
        let mut safe = clamped;
        while safe > 0 && !text.is_char_boundary(safe) {
            safe -= 1;
        }
        let prefix = &text[..safe];
        self.shape(prefix, 16.0, None).width_px
    }
}

/// Chaîne de fallback polices — miroir `annora-design::typo::font_chain`.
pub fn font_chain() -> Vec<&'static str> {
    vec![
        "Inter Variable",
        "Noto Sans",
        "Noto Sans Arabic",
        "Noto Sans Devanagari",
        "Noto Sans CJK",
        "Noto Sans Adlam",
        "Noto Sans NKo",
        "JetBrains Mono",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_latin_arabic_cjk() {
        let e = SimpleTextEngine::new();
        let fr = e.shape("Bonjour Akora", 16.0, None);
        assert!(!fr.is_rtl);
        assert!(fr.width_px > 0.0);
        let ar = e.shape("مرحبا عكورا", 16.0, None);
        assert!(ar.is_rtl);
        let ja = e.shape("こんにちはアコラ", 16.0, None);
        assert_eq!(ja.glyph_count, 8);
    }

    #[test]
    fn cursor_never_splits_utf8() {
        let e = SimpleTextEngine::new();
        let t = "héllo 世界 مرحبا";
        for i in 0..=t.len() {
            let _ = e.cursor_offset(t, i);
        }
        // Milieu d'un emoji multi-octets → clampé, pas de panic.
        let em = "a😀b";
        let _ = e.cursor_offset(em, 2);
        let _ = e.cursor_offset(em, 3);
    }

    #[test]
    fn font_chain_covers_scripts() {
        let c = font_chain();
        assert!(c.contains(&"Noto Sans Arabic"));
        assert!(c.contains(&"Noto Sans CJK"));
    }
}
