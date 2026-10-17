//! Constantes générées depuis `tokens/` par `cargo xtask tokens`.
//! NE JAMAIS ÉDITER À LA MAIN — voir tokens/*.toml (source unique).

pub mod color {
    pub const COLOR_BRAISE_DARK: u32 = 0xFF7A2F;
    pub const COLOR_BRAISE_LIGHT: u32 = 0xC2410C;
    pub const COLOR_LIGNE_DARK: u32 = 0x262A33;
    pub const COLOR_LIGNE_LIGHT: u32 = 0xDAD6CE;
    pub const COLOR_LUEUR_DARK: u32 = 0xF4EBDD;
    pub const COLOR_LUEUR_LIGHT: u32 = 0x14161B;
    pub const COLOR_SUR_BRAISE_DARK: u32 = 0x0B0C0F;
    pub const COLOR_SUR_BRAISE_LIGHT: u32 = 0xFFFFFF;
    pub const COLOR_SURFACE_0_DARK: u32 = 0x0B0C0F;
    pub const COLOR_SURFACE_0_LIGHT: u32 = 0xF6F4F0;
    pub const COLOR_SURFACE_1_DARK: u32 = 0x121419;
    pub const COLOR_SURFACE_1_LIGHT: u32 = 0xFFFFFF;
    pub const COLOR_SURFACE_2_DARK: u32 = 0x1A1D24;
    pub const COLOR_SURFACE_2_LIGHT: u32 = 0xEDEAE4;
    pub const COLOR_TEXTE_1_DARK: u32 = 0xECEEF2;
    pub const COLOR_TEXTE_1_LIGHT: u32 = 0x14161B;
    pub const COLOR_TEXTE_2_DARK: u32 = 0xA3A9B6;
    pub const COLOR_TEXTE_2_LIGHT: u32 = 0x4A5060;
    pub const COLOR_TEXTE_3_DARK: u32 = 0x7A8190;
    pub const COLOR_TEXTE_3_LIGHT: u32 = 0x667085;
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
}
pub mod ty {
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
    pub const LAYOUT_GRID_PT_BASE: i64 = 4;
    pub const LAYOUT_GRID_PT_SPACINGS: [i64; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
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
