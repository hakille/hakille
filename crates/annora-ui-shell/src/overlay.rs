//! Overlay géométrie par OS. Pur Rust, testé sans fenêtre.
//!
//! Le rendu Slint consomme ces specs ; l'OS fournit le chrome natif.

use serde::{Deserialize, Serialize};

/// Spec öğe commune HUD overlay (miroir de AcrylicOverlayWindowSpec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPlacement {
    pub width: u32,
    pub height: u32,
    pub top_offset: i32,
    pub backend: String,
    pub topmost: bool,
    pub visible: bool,
}

impl OverlayPlacement {
    /// HUD overlay — décision géométrie tranchée (ADR-007, spec §8) :
    /// `720×64 r12` conservé, `640×56 r16` rejeté.
    ///
    /// Rationale : 720 px accommode les libellés FR longs + pastilles coût
    /// sans troncature (640 px tronquait « revalidation requise »), 64 px
    /// garde une cible tactile 48 px + marges, r12 matche `HUD_RADIUS` /
    /// Slint `border-radius: 12px` et le backend DWM/wlr/NSPanel déjà validé.
    /// `is_futurist_geometry` verrouille ce choix ; tout écart spec doit
    /// passer par ADR, pas par TODO silencieux.
    pub const HUD_WIDTH: u32 = 720;
    pub const HUD_HEIGHT: u32 = 64;
    pub const HUD_RADIUS: u32 = 12;
    pub const HUD_TOP_OFFSET: i32 = 120;

    pub fn hud(height: u32, visible: bool, backend: &str) -> Self {
        Self {
            width: Self::HUD_WIDTH,
            height,
            top_offset: Self::HUD_TOP_OFFSET,
            backend: backend.to_string(),
            topmost: true,
            visible,
        }
    }

    pub fn is_futurist_geometry(&self) -> bool {
        self.width == Self::HUD_WIDTH && self.top_offset == Self::HUD_TOP_OFFSET
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hud_geometry_is_futurist() {
        // Choix tranché : 720×64 r12 (pas 640×56 r16). Verrouille largeur,
        // hauteur canonique, offset et radius.
        let p = OverlayPlacement::hud(OverlayPlacement::HUD_HEIGHT, true, "DWM");
        assert!(p.is_futurist_geometry());
        assert!(p.topmost);
        assert_eq!(p.width, 720);
        assert_eq!(p.height, 64);
        assert_eq!(p.top_offset, 120);
        assert_eq!(OverlayPlacement::HUD_WIDTH, 720);
        assert_eq!(OverlayPlacement::HUD_RADIUS, 12);
        // Le spec 640×56 r16 est explicitement rejeté (pas de régression silencieuse).
        let stale = OverlayPlacement {
            width: 640,
            height: 56,
            top_offset: 120,
            backend: "DWM".to_string(),
            topmost: true,
            visible: true,
        };
        assert!(!stale.is_futurist_geometry());
    }
}
