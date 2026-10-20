//! Navigation, composants, mise en page adaptative (spec §8, §26).
//!
//! Quatre destinations, pas une de plus. Pas de modale bloquante (sauf
//! ré-auth). Pas de toasts : tout va au Journal ou à la Présence.

use serde::{Deserialize, Serialize};

/// Les quatre destinations — et seulement quatre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    Journal,
    Memoire,
    Pouvoirs,
    Reglages,
}

/// Classe de largeur : navigation + journal s'adaptent en direct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    Compact, // < 600 : barre inférieure, détail en feuille
    Medium,  // 600–839 : rail latéral, détail en volet
    Wide,    // ≥ 840 : rail + liste et détail côte à côte
}

impl WidthClass {
    pub fn of(width_px: u32) -> Self {
        if width_px < 600 {
            Self::Compact
        } else if width_px < 840 {
            Self::Medium
        } else {
            Self::Wide
        }
    }
}

/// Contenu centré, 720 px max ; ligne de commande 640 pt (compacte :
/// pleine largeur − 32 pt).
pub const CONTENT_MAX_PX: u32 = 720;
pub const COMMAND_BAR_PT: u32 = 640;

/// Focus clavier : anneau 2 px en texte/1, décalé de 2 px. Jamais en braise.
pub const FOCUS_RING_PX: u32 = 2;
pub const FOCUS_RING_OFFSET_PX: u32 = 2;

/// Bouton : un seul primaire par vue, libellé = verbe. 44 pt (36 px pointeur).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Tertiary,
}

pub const BUTTON_HEIGHT_PT: u32 = 44;
pub const BUTTON_HEIGHT_POINTER_PX: u32 = 36;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_width_classes_and_constants() {
        assert_eq!(WidthClass::of(599), WidthClass::Compact);
        assert_eq!(WidthClass::of(600), WidthClass::Medium);
        assert_eq!(WidthClass::of(839), WidthClass::Medium);
        assert_eq!(WidthClass::of(840), WidthClass::Wide);
        assert_eq!(CONTENT_MAX_PX, 720);
        assert_eq!(COMMAND_BAR_PT, 640);
        assert_eq!(FOCUS_RING_PX, 2);
    }
}
