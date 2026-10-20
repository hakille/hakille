//! Scène 2D Hakille — trait stable, POC Vello autorisé derrière.
//!
//! Le métier ne dépend jamais de wgpu/Vello directement : tout passe par
//! `SceneRenderer`. Le backend réel est `WgpuRenderer`/`SoftwareRenderer`
//! dans `renderer.rs` ; ici le trait + la scène + un backend mémoire pour tests.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            radius: 0.0,
        }
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
}

/// Graisse normale — miroir de `hakille_design::generated::ty::TYPE_BODY_WEIGHT`.
pub const WEIGHT_NORMAL: u16 = 400;

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    FillRect {
        rect: Rect,
        color: Color,
    },
    Ring {
        cx: f32,
        cy: f32,
        radius: f32,
        width: f32,
        color: Color,
    },
    GlyphRun {
        x: f32,
        y: f32,
        size: f32,
        weight: u16,
        color: Color,
        text: String,
    },
    Shadow {
        rect: Rect,
        blur: f32,
        color: Color,
    },
}

/// Scène immuable — snapshot du frame, snapshotable pour golden tests.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub cmds: Vec<DrawCmd>,
    pub width: u32,
    pub height: u32,
    pub theme_dark: bool,
}

impl Scene {
    pub fn new(width: u32, height: u32, theme_dark: bool) -> Self {
        Self {
            cmds: Vec::new(),
            width,
            height,
            theme_dark,
        }
    }
    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.cmds.push(DrawCmd::FillRect { rect, color });
    }
    pub fn ring(&mut self, cx: f32, cy: f32, radius: f32, width: f32, color: Color) {
        self.cmds.push(DrawCmd::Ring {
            cx,
            cy,
            radius,
            width,
            color,
        });
    }
    pub fn text(&mut self, x: f32, y: f32, size: f32, color: Color, text: impl Into<String>) {
        self.text_weighted(x, y, size, WEIGHT_NORMAL, color, text);
    }
    pub fn text_weighted(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        weight: u16,
        color: Color,
        text: impl Into<String>,
    ) {
        self.cmds.push(DrawCmd::GlyphRun {
            x,
            y,
            size,
            weight,
            color,
            text: text.into(),
        });
    }
    pub fn shadow(&mut self, rect: Rect, blur: f32, color: Color) {
        self.cmds.push(DrawCmd::Shadow { rect, blur, color });
    }
    /// Hash déterministe pour snapshots golden (taille/DPI/thème fixés).
    pub fn fingerprint(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        self.theme_dark.hash(&mut h);
        format!("{:?}", self.cmds).hash(&mut h);
        h.finish()
    }
}

/// Frontière renderer — implémentée par wgpu, software, tests.
pub trait SceneRenderer {
    type Error: std::fmt::Debug;
    fn begin(&mut self, width: u32, height: u32, dark: bool);
    fn submit(&mut self, scene: &Scene) -> Result<Vec<u8>, Self::Error>;
    fn present_supported(&self) -> bool;
}

/// Renderer mémoire — référence pour golden tests + headless CI.
#[derive(Debug, Default)]
pub struct MemSceneRenderer {
    scene: Option<Scene>,
}

impl SceneRenderer for MemSceneRenderer {
    type Error = String;
    fn begin(&mut self, _w: u32, _h: u32, _dark: bool) {}
    fn submit(&mut self, scene: &Scene) -> Result<Vec<u8>, String> {
        self.scene = Some(scene.clone());
        // Faux pixels RGBA déterministes : fingerprint répété.
        let fp = scene.fingerprint();
        let px = (scene.width as usize) * (scene.height as usize) * 4;
        let mut out = vec![0u8; px.min(4096)];
        for (i, b) in out.iter_mut().enumerate() {
            *b = ((fp >> ((i % 8) * 8)) & 0xFF) as u8;
        }
        Ok(out)
    }
    fn present_supported(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_fingerprint_stable() {
        let mut s = Scene::new(720, 64, true);
        s.fill(
            Rect::new(0.0, 0.0, 720.0, 64.0).rounded(12.0),
            Color::from_hex(0x0B0C0F),
        );
        s.text(16.0, 40.0, 16.0, Color::from_hex(0xECEEF2), "Hakille");
        let a = s.fingerprint();
        let mut r = MemSceneRenderer::default();
        r.begin(720, 64, true);
        let px = r.submit(&s).unwrap();
        assert!(!px.is_empty());
        assert_eq!(s.fingerprint(), a);
    }
}
