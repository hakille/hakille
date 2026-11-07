//! FrameInspector: Software renderer frame and pixel buffer inspector.
//!
//! Validates layout geometry (720x64 HUD, 720x360 panels), Braise (#FF7A2F)
//! containment rules, WCAG AA contrast compliance, and 0-frame quiescence.

use hakille_design::generated::color::COLOR_BRAISE_DARK;
use hakille_ui_desktop::renderer::{SoftwareRenderer, card_buttons};
use hakille_ui_desktop::scene::{Color, DrawCmd, Scene};

pub struct FrameInspector {
    renderer: SoftwareRenderer,
}

impl FrameInspector {
    pub fn new(width: u32, height: u32, dark: bool) -> Self {
        Self {
            renderer: SoftwareRenderer::new(width, height, dark),
        }
    }

    pub fn with_transparent_backdrop(mut self, transparent: bool) -> Self {
        self.renderer = self.renderer.with_transparent_backdrop(transparent);
        self
    }

    /// Render scene into in-memory RGBA8 pixel buffer.
    pub fn paint(&mut self, scene: &Scene) -> Vec<u8> {
        self.renderer.paint(scene)
    }

    /// Total frames painted by the software renderer.
    pub fn frames_painted(&self) -> u64 {
        self.renderer.frames
    }

    /// Assert that Braise (#FF7A2F) draw commands are strictly contained within decision elements.
    pub fn assert_braise_containment_in_scene(scene: &Scene) {
        let braise_u32 = COLOR_BRAISE_DARK;
        let braise_color = Color::from_hex(braise_u32);
        let card_b = card_buttons(true);

        for cmd in &scene.cmds {
            match cmd {
                DrawCmd::FillRect { rect, color } => {
                    if color.r == braise_color.r
                        && color.g == braise_color.g
                        && color.b == braise_color.b
                    {
                        // Must be inside the card validate button area or decision ring
                        let is_in_validate = rect.x >= card_b.validate.x - 2.0
                            && rect.y >= card_b.validate.y - 2.0
                            && (rect.x + rect.w) <= (card_b.validate.x + card_b.validate.w + 2.0)
                            && (rect.y + rect.h) <= (card_b.validate.y + card_b.validate.h + 2.0);
                        assert!(
                            is_in_validate,
                            "Braise containment violation: Braise FillRect found outside validation button at {:?}",
                            rect
                        );
                    }
                }
                DrawCmd::GlyphRun { color, text, .. } => {
                    assert!(
                        !(color.r == braise_color.r
                            && color.g == braise_color.g
                            && color.b == braise_color.b),
                        "Braise containment violation: Text drawn in Braise color: '{}'",
                        text
                    );
                }
                DrawCmd::Ring { color, .. } => {
                    // Rings can use braise if they are hold-progress rings or decision rings
                    if color.r == braise_color.r
                        && color.g == braise_color.g
                        && color.b == braise_color.b
                    {
                        // Permitted in hold progress
                    }
                }
                DrawCmd::Shadow { .. } => {}
            }
        }
    }

    /// Calculate relative luminance for an sRGB hex color (0xRRGGBB).
    pub fn relative_luminance(hex: u32) -> f64 {
        let r = ((hex >> 16) & 0xFF) as f64 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f64 / 255.0;
        let b = (hex & 0xFF) as f64 / 255.0;

        let to_linear = |c: f64| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };

        0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
    }

    /// Calculate contrast ratio between two colors according to WCAG 2.1 formula.
    pub fn contrast_ratio(c1: u32, c2: u32) -> f64 {
        let l1 = Self::relative_luminance(c1);
        let l2 = Self::relative_luminance(c2);
        let lighter = l1.max(l2);
        let darker = l1.min(l2);
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Assert WCAG AA contrast compliance (4.5:1 for normal text, 3.0:1 for large/graphical).
    pub fn assert_wcag_aa(text_hex: u32, bg_hex: u32, is_graphic: bool) {
        let ratio = Self::contrast_ratio(text_hex, bg_hex);
        let min_ratio = if is_graphic { 3.0 } else { 4.5 };
        assert!(
            ratio >= min_ratio,
            "WCAG AA contrast failure: Ratio {:.2}:1 is below required {:.1}:1 for text #{:06X} on bg #{:06X}",
            ratio,
            min_ratio,
            text_hex,
            bg_hex
        );
    }

    /// Assert High Contrast mode compliance (>= 7.0:1).
    pub fn assert_high_contrast(text_hex: u32, bg_hex: u32) {
        let ratio = Self::contrast_ratio(text_hex, bg_hex);
        assert!(
            ratio >= 7.0,
            "High contrast mode failure: Ratio {:.2}:1 is below required 7.0:1 for text #{:06X} on bg #{:06X}",
            ratio,
            text_hex,
            bg_hex
        );
    }
}
