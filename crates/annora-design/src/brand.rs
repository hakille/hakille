//! Identité Akora (spec §2) : la Lueur sous l'arc de 21 fils.

/// Nom produit, toujours en minuscules.
pub const BRAND_NAME: &str = "akora";
/// Signature texte/2 taille Caption.
pub const BRAND_SIGNATURE: &str = "akora · par Hakille";
/// Cordes de la kora → 21 fils de la forme d'onde / de la Présence Parole.
pub const WIRES_FULL: usize = 21;
/// Repli petites tailles (< 24 px).
pub const WIRES_SMALL: usize = 7;
/// Seuil de repli en px.
pub const WIRES_SMALL_BELOW_PX: u32 = 24;
/// Zone de protection : 2 × le diamètre de la Lueur.
pub const CLEARSPACE_DIAMETERS: u32 = 2;
/// Tailles minimales : signe seul / logotype.
pub const MIN_SIZE_SIGN_PX: u32 = 16;
pub const MIN_SIZE_LOGOTYPE_PX: u32 = 64;
/// Le « o » est un cercle parfait ; logotype semi-bold, approche −1 %.
pub const LOGOTYPE_TRACKING_PCT: i8 = -1;

/// Nombre de fils selon la taille d'affichage (spec §7).
pub fn wire_count(viewport_px: u32) -> usize {
    if viewport_px < WIRES_SMALL_BELOW_PX {
        WIRES_SMALL
    } else {
        WIRES_FULL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brand_identity() {
        assert_eq!(BRAND_NAME, "akora");
        assert!(BRAND_SIGNATURE.contains("Hakille"));
        assert_eq!(wire_count(16), 7);
        assert_eq!(wire_count(24), 21);
        assert_eq!(wire_count(200), 21);
    }
}
