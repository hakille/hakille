//! Constantes générées depuis `tokens/` par `cargo xtask tokens`.
//! NE JAMAIS ÉDITER À LA MAIN — voir tokens/*.toml (source unique).

pub mod color {
    pub const COLOR_AVERTISSEMENT_DARK: u32 = 0xFBBF24;
    pub const COLOR_AVERTISSEMENT_LIGHT: u32 = 0xD97706;
    pub const COLOR_BRAISE_DARK: u32 = 0xFF7A2F;
    pub const COLOR_BRAISE_LIGHT: u32 = 0xC2410C;
    pub const COLOR_BRAISE_HALO_DARK: u32 = 0xFF7A2F47;
    pub const COLOR_BRAISE_HALO_LIGHT: u32 = 0xC2410C33;
    pub const COLOR_ERREUR_DARK: u32 = 0xFF5A5A;
    pub const COLOR_ERREUR_LIGHT: u32 = 0xDC2626;
    pub const COLOR_LIGNE_DARK: u32 = 0x262A33;
    pub const COLOR_LIGNE_LIGHT: u32 = 0xDAD6CE;
    pub const COLOR_LUEUR_DARK: u32 = 0xF4EBDD;
    pub const COLOR_LUEUR_LIGHT: u32 = 0x14161B;
    pub const COLOR_SUCCES_DARK: u32 = 0x34D399;
    pub const COLOR_SUCCES_LIGHT: u32 = 0x059669;
    pub const COLOR_SUR_BRAISE_DARK: u32 = 0x0B0C0F;
    pub const COLOR_SUR_BRAISE_LIGHT: u32 = 0xFFFFFF;
    pub const COLOR_SURFACE_0_DARK: u32 = 0x0B0C0F;
    pub const COLOR_SURFACE_0_LIGHT: u32 = 0xF6F4F0;
    pub const COLOR_SURFACE_1_DARK: u32 = 0x121419;
    pub const COLOR_SURFACE_1_LIGHT: u32 = 0xFFFFFF;
    pub const COLOR_SURFACE_1_ACTIVE_DARK: u32 = 0x0E1014;
    pub const COLOR_SURFACE_1_ACTIVE_LIGHT: u32 = 0xE4DFD6;
    pub const COLOR_SURFACE_1_HOVER_DARK: u32 = 0x181B22;
    pub const COLOR_SURFACE_1_HOVER_LIGHT: u32 = 0xF0ECE4;
    pub const COLOR_SURFACE_2_DARK: u32 = 0x1A1D24;
    pub const COLOR_SURFACE_2_LIGHT: u32 = 0xEDEAE4;
    pub const COLOR_SURFACE_DISABLED_DARK: u32 = 0x14161A;
    pub const COLOR_SURFACE_DISABLED_LIGHT: u32 = 0xECE8E0;
    pub const COLOR_TEXTE_1_DARK: u32 = 0xECEEF2;
    pub const COLOR_TEXTE_1_LIGHT: u32 = 0x14161B;
    pub const COLOR_TEXTE_2_DARK: u32 = 0xA3A9B6;
    pub const COLOR_TEXTE_2_LIGHT: u32 = 0x4A5060;
    pub const COLOR_TEXTE_3_DARK: u32 = 0x82899A;
    pub const COLOR_TEXTE_3_LIGHT: u32 = 0x5C6679;
    pub const COLOR_TEXTE_DISABLED_DARK: u32 = 0x4E5463;
    pub const COLOR_TEXTE_DISABLED_LIGHT: u32 = 0xA0A5B2;
    pub const COLOR_VERRE_FOND_DARK: u32 = 0x121419D1;
    pub const COLOR_VERRE_FOND_LIGHT: u32 = 0xFFFFFFE0;
    pub const COLOR_VERRE_REFLET_DARK: u32 = 0xECEEF21F;
    pub const COLOR_VERRE_REFLET_LIGHT: u32 = 0xFFFFFF99;
}
pub mod motion {
    pub const MOTION_DURATION_MS_T120: i64 = 120;
    pub const MOTION_DURATION_MS_T200: i64 = 200;
    pub const MOTION_DURATION_MS_T320: i64 = 320;
    pub const MOTION_DURATION_MS_T600: i64 = 600;
    pub const MOTION_EASE_EASE_IN: &str = "cubic-bezier(0.2, 0, 0, 1)";
    pub const MOTION_EASE_EASE_OUT: &str = "cubic-bezier(0.3, 0, 0.8, 0.15)";
    pub const MOTION_HAPTIC_MS_TICK: i64 = 10;
    pub const MOTION_SOUND_ATTENTION: [(i64, i64); 2] = [(67, 160), (67, 160)];
    pub const MOTION_SOUND_EVEIL: [(i64, i64); 2] = [(69, 90), (76, 90)];
    pub const MOTION_SOUND_FAIT: [(i64, i64); 1] = [(72, 220)];
    pub const MOTION_SPRING_CARD_DAMPING: i64 = 32;
    pub const MOTION_SPRING_CARD_MASS: i64 = 1;
    pub const MOTION_SPRING_CARD_STIFFNESS: i64 = 380;
    pub const PRESENCE_WIRE_COUNT_COMPACT: i64 = 7;
    pub const PRESENCE_WIRE_COUNT_FULL: i64 = 21;
    pub const PRESENCE_WIRE_DECAY_MS: i64 = 80;
    pub const PRESENCE_WIRE_GAP_PX: f64 = 3.0;
    pub const PRESENCE_WIRE_MAX_HEIGHT_PX: f64 = 24.0;
    pub const PRESENCE_WIRE_WIDTH_PX: f64 = 2.0;
}
pub mod ty {
    pub const BLUR_GLOW: i64 = 16;
    pub const BLUR_HUD_ACRYLIC: i64 = 30;
    pub const BLUR_PANEL: i64 = 24;
    pub const FONT_FALLBACK_CHAIN: [&str; 6] = [
        "Noto Sans",
        "Noto Sans Arabic",
        "Noto Sans Devanagari",
        "Noto Sans CJK",
        "Noto Sans Adlam",
        "Noto Sans NKo",
    ];
    pub const FONT_INTERFACE_FAMILY: &str = "Inter Variable";
    pub const FONT_MONO_FAMILY: &str = "JetBrains Mono";
    pub const LAYOUT_CARD_HEIGHT: i64 = 360;
    pub const LAYOUT_CARD_WIDTH: i64 = 720;
    pub const LAYOUT_GRID_PT_BASE: i64 = 4;
    pub const LAYOUT_GRID_PT_SPACINGS: [i64; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
    pub const LAYOUT_HUD_HEIGHT: i64 = 64;
    pub const LAYOUT_HUD_RADIUS: i64 = 12;
    pub const LAYOUT_HUD_TOP_OFFSET: i64 = 120;
    pub const LAYOUT_HUD_WIDTH: i64 = 720;
    pub const LAYOUT_RADIUS_CARD: i64 = 16;
    pub const LAYOUT_RADIUS_CONTROL: i64 = 8;
    pub const LAYOUT_TOUCH_PT_ANDROID: i64 = 48;
    pub const LAYOUT_TOUCH_PT_IOS: i64 = 44;
    pub const LAYOUT_TOUCH_PT_POINTER_MIN_PX: i64 = 32;
    pub const TYPE_BODY_LINE: i64 = 24;
    pub const TYPE_BODY_SIZE: i64 = 16;
    pub const TYPE_BODY_WEIGHT: i64 = 400;
    pub const TYPE_BODY_STRONG_LINE: i64 = 24;
    pub const TYPE_BODY_STRONG_SIZE: i64 = 16;
    pub const TYPE_BODY_STRONG_WEIGHT: i64 = 500;
    pub const TYPE_CAPTION_LINE: i64 = 18;
    pub const TYPE_CAPTION_SIZE: i64 = 13;
    pub const TYPE_CAPTION_WEIGHT: i64 = 400;
    pub const TYPE_DISPLAY_LINE: i64 = 44;
    pub const TYPE_DISPLAY_SIZE: i64 = 40;
    pub const TYPE_DISPLAY_WEIGHT: i64 = 600;
    pub const TYPE_HEADING_LINE: i64 = 26;
    pub const TYPE_HEADING_SIZE: i64 = 20;
    pub const TYPE_HEADING_WEIGHT: i64 = 600;
    pub const TYPE_MONO_LINE: i64 = 20;
    pub const TYPE_MONO_SIZE: i64 = 13;
    pub const TYPE_MONO_WEIGHT: i64 = 400;
    pub const TYPE_TITLE_LINE: i64 = 34;
    pub const TYPE_TITLE_SIZE: i64 = 28;
    pub const TYPE_TITLE_WEIGHT: i64 = 600;
}
