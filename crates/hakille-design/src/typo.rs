//! Typographie, écritures, i18n (spec §4, §12).
//!
//! Inter Variable (UI) + JetBrains Mono (code) sous OFL, chaîne Noto
//! (arabe, devanāgarī, CJK, Adlam, N'Ko). Zéro tofu : `font_chain()`
//! couvre chaque langue livrée ; la CI pseudo-localise pour attraper les
//! chaînes en dur et les coupures (budget d'expansion 40 %).

/// Chaîne de repli (miroir de tokens/type.toml).
pub const FALLBACK_CHAIN: [&str; 6] = [
    "Noto Sans",
    "Noto Sans Arabic",
    "Noto Sans Devanagari",
    "Noto Sans CJK",
    "Noto Sans Adlam",
    "Noto Sans NKo",
];

/// Style de texte : taille/interligne px + graisse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub size_px: u16,
    pub line_px: u16,
    pub weight: u16,
}

pub const DISPLAY: TextStyle = TextStyle {
    size_px: 40,
    line_px: 44,
    weight: 600,
};
pub const TITLE: TextStyle = TextStyle {
    size_px: 28,
    line_px: 34,
    weight: 600,
};
pub const HEADING: TextStyle = TextStyle {
    size_px: 20,
    line_px: 26,
    weight: 600,
};
pub const BODY: TextStyle = TextStyle {
    size_px: 16,
    line_px: 24,
    weight: 400,
};
pub const BODY_STRONG: TextStyle = TextStyle {
    size_px: 16,
    line_px: 24,
    weight: 500,
};
pub const CAPTION: TextStyle = TextStyle {
    size_px: 13,
    line_px: 18,
    weight: 400,
};
pub const MONO: TextStyle = TextStyle {
    size_px: 13,
    line_px: 20,
    weight: 400,
};

/// Lignes de 68 caractères max ; chiffres tabulaires pour coûts/horodatages.
pub const MAX_LINE_CHARS: usize = 68;
/// Budget d'expansion du texte traduit : 40 %.
pub const EXPANSION_BUDGET: f32 = 0.40;
/// Échelle utilisateur 80 % → 200 %.
pub const TEXT_SCALE_MIN: f32 = 0.80;
pub const TEXT_SCALE_MAX: f32 = 2.00;

/// Langues livrées → écritures requises → polices de la chaîne.
pub fn font_chain(lang: &str) -> Vec<&'static str> {
    let mut chain = vec!["Inter Variable"];
    match lang {
        "ar" => chain.push("Noto Sans Arabic"),
        "hi" | "mr" | "ne" => chain.push("Noto Sans Devanagari"),
        "zh" | "ja" | "ko" => chain.push("Noto Sans CJK"),
        "ff" => chain.push("Noto Sans Adlam"),
        "nqo" | "man" => chain.push("Noto Sans NKo"),
        _ => chain.push("Noto Sans"),
    }
    chain.push("JetBrains Mono");
    chain
}

/// La langue exige-t-elle un miroir droite-à-gauche complet ?
pub fn is_rtl(lang: &str) -> bool {
    matches!(lang, "ar" | "he" | "fa" | "ur")
}

/// Les icônes directionnelles se retournent ; la forme d'onde, jamais.
pub fn mirror_icon(icon_kind: &str, rtl: bool) -> bool {
    rtl && matches!(icon_kind, "directional")
}

/// Pseudo-localisation CI : accents + ~40 % d'expansion + crochets.
/// Attrape chaînes codées en dur (non passées par Fluent) et coupures.
pub fn pseudo_localize(s: &str) -> String {
    let accented: String = s
        .chars()
        .map(|c| match c {
            'a' => 'à',
            'e' => 'ë',
            'i' => 'ï',
            'o' => 'ö',
            'u' => 'ü',
            'A' => 'À',
            'E' => 'Ë',
            'n' => 'ñ',
            c => c,
        })
        .collect();
    let pad_len = (s.chars().count() as f32 * EXPANSION_BUDGET).round() as usize;
    let pad: String = "～".repeat(pad_len.max(1));
    format!("[{accented} {pad}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_tofu_chains() {
        for lang in ["fr", "ar", "hi", "zh", "ff", "nqo", "wo", "bm"] {
            let chain = font_chain(lang);
            assert!(chain.contains(&"Inter Variable"), "{lang}");
            assert!(!chain.is_empty());
        }
        assert!(font_chain("ff").contains(&"Noto Sans Adlam"));
        assert!(font_chain("nqo").contains(&"Noto Sans NKo"));
        assert!(font_chain("ar").contains(&"Noto Sans Arabic"));
    }

    #[test]
    fn test_rtl_mirror_rules() {
        assert!(is_rtl("ar"));
        assert!(!is_rtl("fr"));
        assert!(mirror_icon("directional", true));
        assert!(!mirror_icon("directional", false));
        assert!(!mirror_icon("waveform", true)); // la forme d'onde ne se retourne pas
    }

    #[test]
    fn test_pseudo_localization_expands() {
        let src = "Envoyer ce mail";
        let pseudo = pseudo_localize(src);
        assert!(pseudo.starts_with('[') && pseudo.ends_with(']'));
        assert!(pseudo.chars().count() >= (src.chars().count() as f32 * 1.3) as usize);
    }
}
