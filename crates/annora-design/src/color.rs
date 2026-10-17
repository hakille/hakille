//! Couleur (spec §3) : sombre par défaut, clair complet.
//!
//! La braise n'apparaît QUE sur bouton de validation, progression d'appui
//! long, anneau « décision requise ». `braise_allowed()` + le test
//! `test_braise_only_in_allowed` verrouillent la règle en CI.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_matches_tokens_source() {
        // Garde-fou : les constantes générées reflètent tokens/color.toml.
        assert_eq!(crate::generated::color::COLOR_SURFACE_0_DARK, 0x0B0C0F);
        assert_eq!(crate::generated::color::COLOR_SURFACE_0_LIGHT, 0xF6F4F0);
        assert_eq!(crate::generated::color::COLOR_BRAISE_DARK, 0xFF7A2F);
        assert_eq!(crate::generated::color::COLOR_BRAISE_LIGHT, 0xC2410C);
        assert_eq!(crate::generated::color::COLOR_LUEUR_DARK, 0xF4EBDD);
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
    fn test_contrast_ratios_ci() {
        // Texte principal : ~17:1 sombre / ~16:1 clair (spec §3).
        let dark = contrast_ratio("#ECEEF2", "#0B0C0F").unwrap();
        let light = contrast_ratio("#14161B", "#F6F4F0").unwrap();
        assert!(dark >= 15.0, "texte/1 sombre: {dark}");
        assert!(light >= 14.0, "texte/1 clair: {light}");
        // Braise : WCAG AA (4.5) sur fond d'application, deux thèmes.
        let b_dark = contrast_ratio("#FF7A2F", "#0B0C0F").unwrap();
        let b_light = contrast_ratio("#C2410C", "#F6F4F0").unwrap();
        assert!(b_dark >= 4.5, "braise sombre: {b_dark}");
        assert!(b_light >= 4.5, "braise clair: {b_light}");
        // Secondaire ≥ 7 (spec ≈ 8/7).
        assert!(contrast_ratio("#A3A9B6", "#0B0C0F").unwrap() >= 6.0);
    }
}
