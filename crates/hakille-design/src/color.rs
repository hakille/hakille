//! Couleur (spec §3) : sombre par défaut, clair complet.
//!
//! La braise n'apparaît QUE sur bouton de validation, progression d'appui
//! long, anneau « décision requise ». `braise_allowed()` + le test
//! `test_braise_only_in_allowed` verrouillent la règle en CI.
//!
//! Contraste : la grille WCAG 2.1 AA (texte normal 4,5:1, grands textes et
//! éléments d'interface 3:1) est vérifiée en CI sur les couples
//! surface/texte/lueur/braise, dans les deux thèmes. La braise est un accent
//! graphique (jamais du texte sur surface) : seuil non-text 3:1 ; le texte
//! posé sur braise passe le seuil texte via `sur-braise`.

/// Usage autorisés de la braise (miroir de `allowed_in` dans tokens/).
pub const BRAISE_ALLOWED_IN: [&str; 3] = ["validation", "hold-progress", "decision-ring"];

/// La braise est-elle autorisée dans ce contexte ?
pub fn braise_allowed(context: &str) -> bool {
    BRAISE_ALLOWED_IN.contains(&context)
}

/// Décode `#RRGGBB` en (r, g, b) 0..255.
pub fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8))
}

fn channel_luminance(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Luminance relative WCAG d'un hex.
pub fn luminance(hex: &str) -> Option<f64> {
    let (r, g, b) = hex_to_rgb(hex)?;
    let f = |c: u8| channel_luminance(c as f64 / 255.0);
    Some(0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b))
}

/// Ratio de contraste WCAG entre deux hex.
pub fn contrast_ratio(a: &str, b: &str) -> Option<f64> {
    let (l1, l2) = (luminance(a)?, luminance(b)?);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    Some((hi + 0.05) / (lo + 0.05))
}

/// Convertit une couleur UI `0xRRGGBB` en `#RRGGBB` (réciproque de
/// [`hex_to_rgb`]).
pub fn u32_to_hex(color: u32) -> String {
    format!("#{:06X}", color & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::color as tok;

    /// WCAG 2.1 AA, texte normal (< 18 pt / < 14 pt gras).
    const AA_TEXT: f64 = 4.5;
    /// WCAG 2.1 AA, grand texte et éléments d'interface (1.4.11).
    const AA_NON_TEXT: f64 = 3.0;

    fn ratio(a: u32, b: u32) -> f64 {
        contrast_ratio(&u32_to_hex(a), &u32_to_hex(b))
            .unwrap_or_else(|| panic!("couleur invalide : {a:#08X} / {b:#08X}"))
    }

    #[test]
    fn test_generated_matches_tokens_source() {
        // Garde-fou : les constantes générées reflètent tokens/color.toml.
        assert_eq!(tok::COLOR_SURFACE_0_DARK, 0x0B0C0F);
        assert_eq!(tok::COLOR_SURFACE_0_LIGHT, 0xF6F4F0);
        assert_eq!(tok::COLOR_BRAISE_DARK, 0xFF7A2F);
        assert_eq!(tok::COLOR_BRAISE_LIGHT, 0xC2410C);
        assert_eq!(tok::COLOR_LUEUR_DARK, 0xF4EBDD);
        assert_eq!(tok::COLOR_TEXTE_3_DARK, 0x82899A);
        assert_eq!(tok::COLOR_TEXTE_3_LIGHT, 0x5C6679);
    }

    #[test]
    fn test_u32_to_hex_roundtrip() {
        assert_eq!(u32_to_hex(0x0B0C0F), "#0B0C0F");
        assert_eq!(u32_to_hex(0xFF7A2F), "#FF7A2F");
        assert_eq!(u32_to_hex(0xFFFF_FFFF), "#FFFFFF");
        for hex in ["#0B0C0F", "#F6F4F0", "#FF7A2F", "#C2410C", "#EDEAE4"] {
            let (r, g, b) = hex_to_rgb(hex).unwrap();
            let packed = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            assert_eq!(u32_to_hex(packed), hex.to_uppercase());
        }
    }

    #[test]
    fn test_braise_only_in_allowed() {
        assert!(braise_allowed("validation"));
        assert!(braise_allowed("hold-progress"));
        assert!(braise_allowed("decision-ring"));
        assert!(!braise_allowed("decoration"));
        assert!(!braise_allowed("background"));
        assert!(!braise_allowed(""));
    }

    #[test]
    fn test_aa_text_grid_dark() {
        // Thème sombre : texte 1/2/3 et lueur ≥ 4,5:1 sur les 3 surfaces.
        let surfaces = [
            ("surface/0", tok::COLOR_SURFACE_0_DARK),
            ("surface/1", tok::COLOR_SURFACE_1_DARK),
            ("surface/2", tok::COLOR_SURFACE_2_DARK),
        ];
        let foregrounds = [
            ("texte/1", tok::COLOR_TEXTE_1_DARK),
            ("texte/2", tok::COLOR_TEXTE_2_DARK),
            ("texte/3", tok::COLOR_TEXTE_3_DARK),
            ("lueur", tok::COLOR_LUEUR_DARK),
        ];
        for (sname, surface) in surfaces {
            for (fname, fg) in foregrounds {
                let r = ratio(fg, surface);
                assert!(r >= AA_TEXT, "sombre {fname}/{sname} : {r:.2} < {AA_TEXT}");
            }
            // Braise : accent graphique (anneau, bouton), seuil non-text 3:1.
            let r = ratio(tok::COLOR_BRAISE_DARK, surface);
            assert!(
                r >= AA_NON_TEXT,
                "sombre braise/{sname} : {r:.2} < {AA_NON_TEXT}"
            );
        }
        // Garanties renforcées (spec §3) : braise ≥ 4,5:1 sur le fond
        // d'application, texte sur braise ≥ 4,5:1.
        assert!(ratio(tok::COLOR_BRAISE_DARK, tok::COLOR_SURFACE_0_DARK) >= AA_TEXT);
        assert!(ratio(tok::COLOR_SUR_BRAISE_DARK, tok::COLOR_BRAISE_DARK) >= AA_TEXT);
    }

    #[test]
    fn test_aa_text_grid_light() {
        let surfaces = [
            ("surface/0", tok::COLOR_SURFACE_0_LIGHT),
            ("surface/1", tok::COLOR_SURFACE_1_LIGHT),
            ("surface/2", tok::COLOR_SURFACE_2_LIGHT),
        ];
        let foregrounds = [
            ("texte/1", tok::COLOR_TEXTE_1_LIGHT),
            ("texte/2", tok::COLOR_TEXTE_2_LIGHT),
            ("texte/3", tok::COLOR_TEXTE_3_LIGHT),
            ("lueur", tok::COLOR_LUEUR_LIGHT),
        ];
        for (sname, surface) in surfaces {
            for (fname, fg) in foregrounds {
                let r = ratio(fg, surface);
                assert!(r >= AA_TEXT, "clair {fname}/{sname} : {r:.2} < {AA_TEXT}");
            }
            let r = ratio(tok::COLOR_BRAISE_LIGHT, surface);
            assert!(
                r >= AA_NON_TEXT,
                "clair braise/{sname} : {r:.2} < {AA_NON_TEXT}"
            );
        }
        assert!(ratio(tok::COLOR_BRAISE_LIGHT, tok::COLOR_SURFACE_0_LIGHT) >= AA_TEXT);
        assert!(ratio(tok::COLOR_SUR_BRAISE_LIGHT, tok::COLOR_BRAISE_LIGHT) >= AA_TEXT);
    }

    #[test]
    fn test_light_theme_complete_in_tokens() {
        // Thème clair complet : chaque section de tokens/color.toml définit
        // ses deux variantes `dark` et `light` (source unique).
        use std::collections::BTreeMap;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tokens/color.toml");
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} : {e}", path.display()));
        let mut current = "";
        let mut seen: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.trim();
                seen.entry(current).or_default();
                continue;
            }
            let Some((key, _)) = line.split_once('=') else {
                continue;
            };
            let entry = seen
                .get_mut(current)
                .unwrap_or_else(|| panic!("clé hors section dans color.toml : {line}"));
            match key.trim() {
                "dark" => entry.0 = true,
                "light" => entry.1 = true,
                "allowed_in" => {}
                other => panic!("clé inattendue `{other}` dans [{current}]"),
            }
        }
        for required in [
            "color.surface.0",
            "color.surface.1",
            "color.surface.2",
            "color.ligne",
            "color.texte.1",
            "color.texte.2",
            "color.texte.3",
            "color.lueur",
            "color.braise",
            "color.sur-braise",
        ] {
            assert!(
                seen.contains_key(required),
                "section manquante : {required}"
            );
        }
        for (section, (dark, light)) in &seen {
            assert!(
                section.starts_with("color."),
                "section inattendue : {section}"
            );
            assert!(
                *dark && *light,
                "thème incomplet pour [{section}] : dark={dark} light={light}"
            );
        }
    }
}
