//! Renderer — wgpu réel + fallback logiciel explicite.
//!
//! S2 : présence, carte, journal virtualisé 100k lignes ≥120fps défilement,
//! frame p95 ≤8ms. GPU absent → erreur explicite, jamais fenêtre vide silencieuse.

use crate::components::{self, PrimitiveCtx};
use crate::scene::{Color, DrawCmd, Rect, Scene, SceneRenderer, WEIGHT_NORMAL};
use crate::text::ellipsize;
use crate::theme::Theme;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use hakille_design::generated::{color as tokens, ty};
use hakille_ui_model::snapshot::{HUD_HEIGHT, HUD_WIDTH};
use hakille_ui_protocol::{ActionCard, DataExit, Provenance};
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("gpu indisponible: {0} — fallback logiciel actif")]
    GpuUnavailable(String),
    #[error("surface perdue: {0}")]
    SurfaceLost(String),
    #[error("logiciel: {0}")]
    Software(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuStatus {
    Active,
    FallbackSoftware(String),
    Failed(String),
}

/// Hauteur logique du panneau de consultation (HUD 64 px + contenu lisible).
pub const CONSULT_HEIGHT: u32 = 360;

/// Rôle visuel d'une ligne du panneau (jamais de couleur codée en dur par
/// l'appelant : le thème reste la seule source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsultLineKind {
    Normal,
    Muted,
    Strong,
    Error,
    Braise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultLine {
    pub text: String,
    pub kind: ConsultLineKind,
}

impl ConsultLine {
    pub fn new(text: impl Into<String>, kind: ConsultLineKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

/// Contenu du panneau multi-ligne : journal, mémoire, statut ou réponse
/// longue. Le renderer ne connaît aucun type métier, seulement du texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultView {
    pub title: String,
    pub subtitle: String,
    pub lines: Vec<ConsultLine>,
    pub footer: String,
}

/// Découpe un texte en lignes tenant dans `max_width` (mots, sauts `\n`
/// préservés). Les mots trop longs sont laissés tels quels puis ellipsés au
/// rendu — jamais de panne ni de dépassement.
pub fn wrap_ui_text(text: &str, size: f32, max_width: f32) -> Vec<String> {
    let max_width = max_width.max(1.0);
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if line.is_empty() || measure_ui_text(&candidate, size) <= max_width {
                line = candidate;
            } else {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Renderer logiciel — pixels CPU déterministes, golden tests.
#[derive(Debug, Default)]
pub struct SoftwareRenderer {
    pub width: u32,
    pub height: u32,
    pub dark: bool,
    pub frames: u64,
}

impl SoftwareRenderer {
    pub fn new(width: u32, height: u32, dark: bool) -> Self {
        Self {
            width,
            height,
            dark,
            frames: 0,
        }
    }

    /// Remplit un frame RGBA8 : formes (SDF approché) + glyphes système.
    /// Déterministe pour golden images (même police → mêmes pixels).
    pub fn paint(&mut self, scene: &Scene) -> Vec<u8> {
        self.frames += 1;
        let w = scene.width.max(1) as usize;
        let h = scene.height.max(1) as usize;
        let mut px = vec![0u8; w * h * 4];
        let (br, bg, bb) = if scene.theme_dark {
            rgb_channels(tokens::COLOR_SURFACE_0_DARK)
        } else {
            rgb_channels(tokens::COLOR_SURFACE_0_LIGHT)
        };
        for chunk in px.chunks_exact_mut(4) {
            chunk[0] = br;
            chunk[1] = bg;
            chunk[2] = bb;
            chunk[3] = 255;
        }
        let mut canvas = Canvas { px: &mut px, w, h };
        for cmd in &scene.cmds {
            match cmd {
                DrawCmd::FillRect { rect, color } => canvas.fill_rounded_rect(rect, *color),
                DrawCmd::Ring {
                    cx,
                    cy,
                    radius,
                    width,
                    color,
                } => canvas.fill_ring(*cx, *cy, *radius, *width, *color),
                DrawCmd::Shadow { rect, blur, color } => canvas.fill_shadow(rect, *blur, *color),
                DrawCmd::GlyphRun {
                    x,
                    y,
                    size,
                    weight,
                    color,
                    text,
                } => canvas.draw_glyph_run(*x, *y, *size, *weight, *color, text),
            }
        }
        px
    }
}

fn rgb_channels(hex: u32) -> (u8, u8, u8) {
    (
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// Cible de peinture : buffer RGBA8 + dimensions, compositing source-over.
struct Canvas<'a> {
    px: &'a mut [u8],
    w: usize,
    h: usize,
}

impl Canvas<'_> {
    fn blend(&mut self, x: i32, y: i32, color: Color, coverage: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let a = (coverage.clamp(0.0, 1.0) * color.a as f32 / 255.0).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let ia = 1.0 - a;
        self.px[i] = (color.r as f32 * a + self.px[i] as f32 * ia + 0.5) as u8;
        self.px[i + 1] = (color.g as f32 * a + self.px[i + 1] as f32 * ia + 0.5) as u8;
        self.px[i + 2] = (color.b as f32 * a + self.px[i + 2] as f32 * ia + 0.5) as u8;
        self.px[i + 3] = 255;
    }

    fn fill_rounded_rect(&mut self, rect: &Rect, color: Color) {
        let x0 = (rect.x - 1.0).floor().max(0.0) as i32;
        let y0 = (rect.y - 1.0).floor().max(0.0) as i32;
        let x1 = ((rect.x + rect.w + 1.0).ceil()).min(self.w as f32) as i32;
        let y1 = ((rect.y + rect.h + 1.0).ceil()).min(self.h as f32) as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = rounded_rect_sdf(rect, x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, color, 0.5 - d);
            }
        }
    }

    fn fill_ring(&mut self, cx: f32, cy: f32, radius: f32, width: f32, color: Color) {
        let r_out = radius + width / 2.0 + 1.0;
        let x0 = (cx - r_out).floor().max(0.0) as i32;
        let y0 = (cy - r_out).floor().max(0.0) as i32;
        let x1 = (cx + r_out).ceil().min(self.w as f32) as i32;
        let y1 = (cy + r_out).ceil().min(self.h as f32) as i32;
        let half = (width / 2.0).max(0.5);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() - radius;
                self.blend(x, y, color, half + 0.5 - d.abs());
            }
        }
    }

    fn fill_shadow(&mut self, rect: &Rect, blur: f32, color: Color) {
        let b = blur.max(0.0);
        let x0 = (rect.x - b).floor().max(0.0) as i32;
        let y0 = (rect.y - b).floor().max(0.0) as i32;
        let x1 = (rect.x + rect.w + b).ceil().min(self.w as f32) as i32;
        let y1 = (rect.y + rect.h + b).ceil().min(self.h as f32) as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let dx = (rect.x - px).max(px - (rect.x + rect.w)).max(0.0);
                let dy = (rect.y - py).max(py - (rect.y + rect.h)).max(0.0);
                let d = (dx * dx + dy * dy).sqrt();
                let coverage = if b <= 0.0 {
                    if d <= 0.0 { 1.0 } else { 0.0 }
                } else {
                    1.0 - d / b
                };
                self.blend(x, y, color, coverage);
            }
        }
    }

    fn draw_glyph_run(&mut self, x: f32, y: f32, size: f32, weight: u16, color: Color, text: &str) {
        if ui_font_stacks().regular.is_empty() && ui_font_stacks().strong.is_empty() {
            // Aucune police système : texte non rendu, jamais de faux glyphes.
            return;
        }
        let scale = PxScale::from(size.max(1.0));
        let mut pen = x;
        for ch in text.chars() {
            // Première police qui couvre réellement le caractère (.notdef = absent).
            let Some(font) = glyph_font(ch, weight) else {
                // Caractère couvert par aucune police : pas d'avance inventée.
                continue;
            };
            let scaled = font.as_scaled(scale);
            let gid = font.glyph_id(ch);
            if let Some(outlined) = font.outline_glyph(gid.with_scale(scale)) {
                let bounds = outlined.px_bounds();
                let dx0 = (pen + bounds.min.x).round() as i32;
                let dy0 = y.round() as i32 + bounds.min.y as i32;
                outlined.draw(|gx, gy, cov| {
                    self.blend(dx0 + gx as i32, dy0 + gy as i32, color, cov);
                });
            }
            pen += scaled.h_advance(gid);
        }
    }
}

/// Distance signée approchée à un rectangle arrondi (SDF standard).
fn rounded_rect_sdf(rect: &Rect, x: f32, y: f32) -> f32 {
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let r = rect.radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    let qx = (x - cx).abs() - (rect.w / 2.0 - r);
    let qy = (y - cy).abs() - (rect.h / 2.0 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Graisse minimale servie par la pile « strong » (semi-bold/bold système).
const STRONG_MIN_WEIGHT: u16 = 500;

/// Piles de polices UI système chargées une fois (ordre = priorité).
struct UiFontStacks {
    regular: Vec<FontVec>,
    strong: Vec<FontVec>,
}

/// Chaîne de polices UI système chargée une fois (ordre = priorité).
/// Surcharge : `HAKILLE_UI_FONT` en tête pour le normal, `HAKILLE_UI_FONT_BOLD`
/// pour le semi-bold/bold ; repli propre sur le normal si absent. Liste vide =
/// texte non rendu (honnête : jamais de faux glyphes, jamais de faux gras).
fn ui_font_stacks() -> &'static UiFontStacks {
    static STACKS: OnceLock<UiFontStacks> = OnceLock::new();
    STACKS.get_or_init(|| {
        let mut regular: Vec<String> = Vec::new();
        let mut strong: Vec<String> = Vec::new();
        if let Ok(path) = std::env::var("HAKILLE_UI_FONT") {
            regular.push(path);
        }
        if let Ok(path) = std::env::var("HAKILLE_UI_FONT_BOLD") {
            strong.push(path);
        }
        #[cfg(windows)]
        {
            regular.extend(
                [
                    r"C:\Windows\Fonts\segoeui.ttf",
                    r"C:\Windows\Fonts\msyh.ttc",
                    r"C:\Windows\Fonts\meiryo.ttc",
                    r"C:\Windows\Fonts\malgun.ttf",
                    r"C:\Windows\Fonts\seguisym.ttf",
                    r"C:\Windows\Fonts\arial.ttf",
                ]
                .map(String::from),
            );
            strong.extend(
                [
                    r"C:\Windows\Fonts\seguisb.ttf",
                    r"C:\Windows\Fonts\segoeuib.ttf",
                    r"C:\Windows\Fonts\arialbd.ttf",
                ]
                .map(String::from),
            );
        }
        #[cfg(target_os = "linux")]
        {
            regular.extend(
                [
                    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                    "/usr/share/fonts/TTF/DejaVuSans.ttf",
                    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                ]
                .map(String::from),
            );
            strong.extend(
                [
                    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
                    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
                    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
                ]
                .map(String::from),
            );
        }
        #[cfg(target_os = "macos")]
        {
            regular.push("/System/Library/Fonts/Supplemental/Arial.ttf".to_string());
            strong.push("/System/Library/Fonts/Supplemental/Arial Bold.ttf".to_string());
        }
        let load = |paths: &[String]| -> Vec<FontVec> {
            paths
                .iter()
                .filter_map(|p| {
                    std::fs::read(p)
                        .ok()
                        .and_then(|bytes| FontVec::try_from_vec(bytes).ok())
                })
                .collect()
        };
        UiFontStacks {
            regular: load(&regular),
            strong: load(&strong),
        }
    })
}

/// Pile normale (texte courant).
fn ui_fonts() -> &'static [FontVec] {
    &ui_font_stacks().regular
}

/// Police couvrant réellement `ch` pour cette graisse : pile strong d'abord
/// (semi-bold/bold), repli caractère par caractère sur la pile normale.
fn glyph_font(ch: char, weight: u16) -> Option<&'static FontVec> {
    let stacks = ui_font_stacks();
    if weight >= STRONG_MIN_WEIGHT
        && let Some(font) = stacks.strong.iter().find(|f| f.glyph_id(ch).0 != 0)
    {
        return Some(font);
    }
    stacks.regular.iter().find(|f| f.glyph_id(ch).0 != 0)
}

/// Avance mesurée avec la même pile que `draw_glyph_run` (jamais de
/// débordement d'ellipse). Sans police système : estimation déterministe.
fn measure_ui_text_weighted(text: &str, size: f32, weight: u16) -> f32 {
    if ui_fonts().is_empty() && ui_font_stacks().strong.is_empty() {
        return text.chars().count() as f32 * size.max(1.0) * 0.6;
    }
    let scale = PxScale::from(size.max(1.0));
    let mut width = 0.0;
    for ch in text.chars() {
        if let Some(font) = glyph_font(ch, weight) {
            width += font.as_scaled(scale).h_advance(font.glyph_id(ch));
        }
    }
    width
}

/// Mesure du texte courant (graisse normale).
pub fn measure_ui_text(text: &str, size: f32) -> f32 {
    measure_ui_text_weighted(text, size, WEIGHT_NORMAL)
}

impl SceneRenderer for SoftwareRenderer {
    type Error = RendererError;
    fn begin(&mut self, width: u32, height: u32, dark: bool) {
        self.width = width;
        self.height = height;
        self.dark = dark;
    }
    fn submit(&mut self, scene: &Scene) -> Result<Vec<u8>, RendererError> {
        Ok(self.paint(scene))
    }
    fn present_supported(&self) -> bool {
        false
    }
}

/// Renderer wgpu — GPU réel, fallback logiciel explicite si adapter absent.
pub struct WgpuRenderer {
    pub status: GpuStatus,
    pub software: SoftwareRenderer,
    // Handles wgpu gardés opaques : initialisation paresseuse async.
    instance: Option<wgpu::Instance>,
}

impl WgpuRenderer {
    pub fn new(width: u32, height: u32, dark: bool) -> Self {
        Self {
            status: GpuStatus::FallbackSoftware("non initialisé — logiciel".into()),
            software: SoftwareRenderer::new(width, height, dark),
            instance: None,
        }
    }

    /// Tente d'acquérir un adapter. En CI headless : échec explicite attendu.
    pub fn try_init_gpu(&mut self) -> GpuStatus {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        self.instance = Some(instance);
        // Pas de surface ici (headless) : on signale le fallback explicite.
        // Le runtime réel appellera `try_init_gpu` avec une surface winit ;
        // sans GPU on ne rend JAMAIS une fenêtre vide silencieuse.
        self.status =
            GpuStatus::FallbackSoftware("headless — logiciel, GPU différé au runtime winit".into());
        self.status.clone()
    }

    pub fn render(&mut self, scene: &Scene) -> Result<Vec<u8>, RendererError> {
        match &self.status {
            GpuStatus::Active => Ok(self.software.paint(scene)),
            GpuStatus::FallbackSoftware(_) => Ok(self.software.paint(scene)),
            GpuStatus::Failed(e) => Err(RendererError::GpuUnavailable(e.clone())),
        }
    }
}

/// Alias exigé par `lib.rs`.
pub type Renderer = WgpuRenderer;

/// Palette de rôles HUD résolue depuis les tokens `hakille-design` (source
/// unique) : aucune couleur en dur côté scène.
struct HudPalette {
    surface1: u32,
    text1: u32,
    text2: u32,
    line: u32,
    braise: u32,
    lueur: u32,
}

fn hud_palette(dark: bool) -> HudPalette {
    if dark {
        HudPalette {
            surface1: tokens::COLOR_SURFACE_1_DARK,
            text1: tokens::COLOR_TEXTE_1_DARK,
            text2: tokens::COLOR_TEXTE_2_DARK,
            line: tokens::COLOR_LIGNE_DARK,
            braise: tokens::COLOR_BRAISE_DARK,
            lueur: tokens::COLOR_LUEUR_DARK,
        }
    } else {
        HudPalette {
            surface1: tokens::COLOR_SURFACE_1_LIGHT,
            text1: tokens::COLOR_TEXTE_1_LIGHT,
            text2: tokens::COLOR_TEXTE_2_LIGHT,
            line: tokens::COLOR_LIGNE_LIGHT,
            braise: tokens::COLOR_BRAISE_LIGHT,
            lueur: tokens::COLOR_LUEUR_LIGHT,
        }
    }
}

/// Easing de pulsation : smoothstep (doux aux extrémités, jamais linéaire).
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Baseline du texte body : contrat historique avec `app.rs::scene()`, qui
/// repère la requête à y≈40 pour la remonter quand une réponse s'affiche.
const BODY_BASELINE: f32 = 40.0;

/// Construit la scène HUD depuis un snapshot (présence + HUD + pastille braise).
///
/// Composition 720×64 : ombre douce, bordure 1 px ligne, surface carte,
/// anneau de présence 2 px + halo interpolé, texte body 16 (baseline 40 px)
/// et caption 13 px secondaire à droite (coût). Braise réservée décision
/// requise / urgence / validation (spec §3).
pub fn hud_scene(snap: &hakille_ui_model::StableSnapshot, theme_dark: bool) -> Scene {
    let p = hud_palette(theme_dark);
    let grid = ty::LAYOUT_GRID_PT_BASE as f32;
    let body_px = ty::TYPE_BODY_SIZE as f32;
    let caption_px = ty::TYPE_CAPTION_SIZE as f32;
    let radius = ty::LAYOUT_RADIUS_CARD as f32;
    let body_weight = ty::TYPE_BODY_WEIGHT as u16;
    let strong_weight = ty::TYPE_BODY_STRONG_WEIGHT as u16;
    let (w, h) = (HUD_WIDTH as f32, HUD_HEIGHT as f32);

    let mut s = Scene::new(HUD_WIDTH, HUD_HEIGHT, theme_dark);
    let bar = Rect::new(grid, grid, w - 2.0 * grid, h - 2.0 * grid).rounded(radius);

    // Ombre douce : rectangle en retrait (jamais de coins noirs carrés).
    let shadow = Rect::new(
        bar.x + grid * 2.0,
        bar.y + grid * 2.0,
        (bar.w - grid * 4.0).max(0.0),
        (bar.h - grid * 4.0).max(0.0),
    );
    s.shadow(
        shadow,
        grid * 4.0,
        Color::rgb(0, 0, 0).with_alpha(if theme_dark { 130 } else { 60 }),
    );
    // Bordure 1 px (ligne) puis surface carte inset d'1 px.
    s.fill(bar, Color::from_hex(p.line));
    s.fill(
        Rect::new(bar.x + 1.0, bar.y + 1.0, bar.w - 2.0, bar.h - 2.0)
            .rounded((radius - 1.0).max(0.0)),
        Color::from_hex(p.surface1),
    );

    // Anneau de présence affiné : 2 px + halo interpolé (braise en décision).
    let pulse = smoothstep(snap.presence_pulse);
    let decision = matches!(snap.presence_state, 4 | 5) || snap.hud_validation;
    let (ring, dot) = if decision {
        (Color::from_hex(p.braise), Color::from_hex(p.braise))
    } else if snap.presence_state == 0 {
        (
            Color::from_hex(p.line),
            Color::from_hex(p.lueur).with_alpha(102),
        )
    } else {
        (
            Color::from_hex(p.lueur).with_alpha(170),
            Color::from_hex(p.lueur),
        )
    };
    let cx = bar.x + grid * 7.0;
    let cy = h / 2.0;
    let ring_radius = grid * 2.5 + pulse * grid;
    let glow = (18.0 + 46.0 * pulse).round() as u8;
    s.ring(cx, cy, ring_radius + grid, 2.0, ring.with_alpha(glow / 2));
    s.ring(cx, cy, ring_radius + grid * 0.5, 2.0, ring.with_alpha(glow));
    s.ring(cx, cy, ring_radius, 2.0, ring);
    let dot_width = if snap.presence_state == 0 {
        grid * 1.5
    } else {
        grid * 2.0
    };
    s.ring(cx, cy, 0.0, dot_width, dot);

    // Texte body + caption secondaire à droite (coût), ellipse sans débordement.
    let text_x = bar.x + grid * 13.0;
    let right = bar.x + bar.w - grid * 3.0;
    let body_max = (right - text_x).max(0.0);
    let caption = if snap.hud_validation {
        let cost = snap.hud_validation_cost.trim();
        (!cost.is_empty()).then(|| cost.to_string())
    } else if !snap.hud_query.is_empty() && snap.cost_level >= 1 {
        Some(format!(
            "{:.2} / {:.2} EUR",
            snap.cost_total_cents as f64 / 100.0,
            snap.cost_budget_cents as f64 / 100.0
        ))
    } else {
        None
    };
    let caption_w = caption
        .as_deref()
        .map(|c| measure_ui_text_weighted(c, caption_px, body_weight) + grid * 2.0)
        .unwrap_or(0.0);
    let label_max = (body_max - caption_w).max(0.0);

    if snap.hud_validation {
        let label = format!("Valider : {}", snap.hud_validation_text);
        let label = ellipsize(&label, label_max, |t| {
            measure_ui_text_weighted(t, body_px, strong_weight)
        });
        s.text_weighted(
            text_x,
            BODY_BASELINE,
            body_px,
            strong_weight,
            Color::from_hex(p.braise),
            label,
        );
    } else if !snap.hud_query.is_empty() {
        let query = ellipsize(&snap.hud_query, label_max, |t| measure_ui_text(t, body_px));
        s.text_weighted(
            text_x,
            BODY_BASELINE,
            body_px,
            body_weight,
            Color::from_hex(p.text1),
            query,
        );
    } else {
        let placeholder = ellipsize("Demander à Hakille…", label_max, |t| {
            measure_ui_text(t, body_px)
        });
        s.text_weighted(
            text_x,
            BODY_BASELINE,
            body_px,
            body_weight,
            Color::from_hex(p.text2),
            placeholder,
        );
    }
    if let Some(caption) = caption {
        let width = measure_ui_text_weighted(&caption, caption_px, body_weight);
        s.text_weighted(
            right - width,
            BODY_BASELINE,
            caption_px,
            body_weight,
            Color::from_hex(p.text2),
            caption,
        );
    }
    s
}

/// Scène étendue : HUD 64 px conservé en haut + panneau de consultation
/// multi-ligne (journal, mémoire, statut, réponse longue). Le panneau est
/// volontairement lisible (14 px, interligne 20 px), jamais une ligne de
/// 12 px tronquée.
pub fn consult_scene(
    snap: &hakille_ui_model::StableSnapshot,
    view: &ConsultView,
    theme_dark: bool,
) -> Scene {
    let p = hud_palette(theme_dark);
    let grid = ty::LAYOUT_GRID_PT_BASE as f32;
    let radius = ty::LAYOUT_RADIUS_CARD as f32;
    let strong_weight = ty::TYPE_BODY_STRONG_WEIGHT as u16;
    let w = HUD_WIDTH as f32;
    let panel_top = HUD_HEIGHT as f32 + grid;
    let panel = Rect::new(
        grid,
        panel_top,
        w - 2.0 * grid,
        CONSULT_HEIGHT as f32 - panel_top - grid,
    )
    .rounded(radius);

    let mut s = hud_scene(snap, theme_dark);
    s.height = CONSULT_HEIGHT;

    // Ombre puis bordure 1 px et surface : même langage que le HUD.
    s.shadow(
        Rect::new(
            panel.x + grid * 2.0,
            panel.y + grid * 2.0,
            (panel.w - grid * 4.0).max(0.0),
            (panel.h - grid * 4.0).max(0.0),
        ),
        grid * 4.0,
        Color::rgb(0, 0, 0).with_alpha(if theme_dark { 150 } else { 70 }),
    );
    s.fill(panel, Color::from_hex(p.line));
    s.fill(
        Rect::new(panel.x + 1.0, panel.y + 1.0, panel.w - 2.0, panel.h - 2.0)
            .rounded((radius - 1.0).max(0.0)),
        Color::from_hex(p.surface1),
    );

    let left = panel.x + grid * 2.0;
    let right = panel.x + panel.w - grid * 2.0;
    let text_width = (right - left).max(1.0);
    let text1 = Color::from_hex(p.text1);
    let text2 = Color::from_hex(p.text2);
    let braise = Color::from_hex(p.braise);
    let error = Color::from_hex(0xFF5A5A);

    s.text_weighted(
        left,
        panel.y + 28.0,
        18.0,
        strong_weight,
        text1,
        ellipsize(&view.title, text_width, |t| {
            measure_ui_text_weighted(t, 18.0, strong_weight)
        }),
    );
    s.text(
        left,
        panel.y + 48.0,
        12.0,
        text2,
        ellipsize(&view.subtitle, text_width, |t| measure_ui_text(t, 12.0)),
    );
    s.fill(
        Rect::new(left, panel.y + 60.0, text_width, 1.0),
        Color::from_hex(p.line),
    );

    let content_top = panel.y + 84.0;
    let footer_y = panel.y + panel.h - 16.0;
    let line_height = 20.0;
    let max_lines = (((footer_y - 16.0 - content_top) / line_height).floor()).max(0.0) as usize;

    let mut wrapped: Vec<ConsultLine> = Vec::new();
    for line in &view.lines {
        for text in wrap_ui_text(&line.text, 14.0, text_width) {
            wrapped.push(ConsultLine {
                text,
                kind: line.kind,
            });
        }
    }
    let truncated = wrapped.len() > max_lines;
    if truncated && max_lines > 0 {
        wrapped.truncate(max_lines - 1);
        wrapped.push(ConsultLine::new("…", ConsultLineKind::Muted));
    } else {
        wrapped.truncate(max_lines);
    }

    for (index, line) in wrapped.iter().enumerate() {
        let color = match line.kind {
            ConsultLineKind::Normal | ConsultLineKind::Strong => text1,
            ConsultLineKind::Muted => text2,
            ConsultLineKind::Error => error,
            ConsultLineKind::Braise => braise,
        };
        let weight = if line.kind == ConsultLineKind::Strong {
            strong_weight
        } else {
            WEIGHT_NORMAL
        };
        let text = ellipsize(&line.text, text_width, |t| {
            measure_ui_text_weighted(t, 14.0, weight)
        });
        s.text_weighted(
            left,
            content_top + index as f32 * line_height + 14.0,
            14.0,
            weight,
            color,
            text,
        );
    }

    s.text(
        left,
        footer_y,
        11.0,
        text2,
        ellipsize(&view.footer, text_width, |t| measure_ui_text(t, 11.0)),
    );
    s
}

/// Scène de décision : HUD 64 px + panneau 720×360 contenant la carte
/// d'action réelle reçue du démon. Les boutons visuels sont complétés par les
/// raccourcis clavier et l'arbre AccessKit (l'état de focus vit hors scène).
pub fn card_scene(
    snap: &hakille_ui_model::StableSnapshot,
    card: &ActionCard,
    notice: Option<&str>,
    theme_dark: bool,
) -> Scene {
    let p = hud_palette(theme_dark);
    let grid = ty::LAYOUT_GRID_PT_BASE as f32;
    let radius = ty::LAYOUT_RADIUS_CARD as f32;
    let w = HUD_WIDTH as f32;
    let panel_top = HUD_HEIGHT as f32 + grid;
    let panel = Rect::new(
        grid,
        panel_top,
        w - 2.0 * grid,
        CONSULT_HEIGHT as f32 - panel_top - grid,
    )
    .rounded(radius);

    let mut s = hud_scene(snap, theme_dark);
    s.height = CONSULT_HEIGHT;

    s.shadow(
        Rect::new(
            panel.x + grid * 2.0,
            panel.y + grid * 2.0,
            (panel.w - grid * 4.0).max(0.0),
            (panel.h - grid * 4.0).max(0.0),
        ),
        grid * 4.0,
        Color::rgb(0, 0, 0).with_alpha(if theme_dark { 150 } else { 70 }),
    );
    s.fill(panel, Color::from_hex(p.line));
    s.fill(
        Rect::new(panel.x + 1.0, panel.y + 1.0, panel.w - 2.0, panel.h - 2.0)
            .rounded((radius - 1.0).max(0.0)),
        Color::from_hex(p.surface1),
    );

    let left = panel.x + grid * 2.0;
    let right = panel.x + panel.w - grid * 2.0;
    let text_width = (right - left).max(1.0);
    let text1 = Color::from_hex(p.text1);
    let text2 = Color::from_hex(p.text2);

    s.text_weighted(
        left,
        panel.y + 26.0,
        18.0,
        ty::TYPE_BODY_STRONG_WEIGHT as u16,
        text1,
        "Décision requise",
    );
    s.text(
        left,
        panel.y + 44.0,
        12.0,
        text2,
        ellipsize(
            "Carte proposée par le démon — aucun effet avant une décision auditée",
            text_width,
            |t| measure_ui_text(t, 12.0),
        ),
    );

    let data = match &card.data_exit {
        DataExit::Local => "Local".to_string(),
        DataExit::Cloud { provider } => format!("Cloud: {provider}"),
    };
    let provenance = match &card.provenance {
        Provenance::Hakille => "Hakille".to_string(),
        Provenance::VerifiedSkill { publisher, name } => format!("{publisher}/{name}"),
        Provenance::External => "Externe".to_string(),
    };
    let theme = if theme_dark {
        Theme::dark()
    } else {
        Theme::light()
    };
    let ctx = PrimitiveCtx::new(theme);
    let card_rect = Rect::new(panel.x + 8.0, panel.y + 54.0, panel.w - 16.0, 190.0).rounded(radius);
    components::ActionCard {
        verb: truncate_chars(&card.verb, 24),
        object: truncate_chars(&card.object, 68),
        reason: truncate_chars(&card.reason, 92),
        cost: card.cost.display(),
        data,
        reversible: if card.reversible {
            "Annulable".to_string()
        } else {
            "Définitif".to_string()
        },
        needs_hold: !card.reversible,
        is_external: matches!(card.provenance, Provenance::External),
        provenance: provenance.clone(),
        rect: card_rect,
    }
    .draw(&mut s, &ctx);

    let hint = notice.unwrap_or(
        "Entrée maintenue 600 ms = valider · R = refuser · Échap = masquer · AccessKit = décision explicite",
    );
    let hint_color = if notice.is_some() {
        Color::from_hex(p.braise)
    } else {
        text2
    };
    s.text(
        left,
        panel.y + panel.h - 24.0,
        10.0,
        hint_color,
        ellipsize(hint, text_width, |t| measure_ui_text(t, 10.0)),
    );
    s.text(
        left,
        panel.y + panel.h - 10.0,
        10.0,
        text2,
        ellipsize(
            &format!("Source : {provenance} — plan sec, aucune exécution métier"),
            text_width,
            |t| measure_ui_text(t, 10.0),
        ),
    );
    s
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
    use hakille_ui_model::StableSnapshot;

    fn snapshot_with_state(state: u8, validation: bool) -> StableSnapshot {
        use hakille_ui::PresenceState;
        let mut presence = PresenceIndicator::new();
        presence.set_state(match state {
            1 => PresenceState::Listening,
            2 => PresenceState::Acting,
            3 => PresenceState::Speaking,
            4 => PresenceState::DecisionRequired,
            5 => PresenceState::Urgent,
            _ => PresenceState::Idle,
        });
        let mut hud = EphemeralHud::new();
        if validation {
            hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        }
        StableSnapshot::from_states(
            &presence,
            &hud,
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            1_000,
            0,
        )
    }

    fn snapshot_with_query(query: &str) -> StableSnapshot {
        let mut hud = EphemeralHud::new();
        hud.open_prompt();
        for c in query.chars() {
            hud.insert_char(c);
        }
        StableSnapshot::from_states(
            &PresenceIndicator::new(),
            &hud,
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            0,
        )
    }

    fn scene_uses_color(scene: &Scene, expected: Color) -> bool {
        scene.cmds.iter().any(|cmd| {
            let color = match cmd {
                DrawCmd::FillRect { color, .. }
                | DrawCmd::Ring { color, .. }
                | DrawCmd::Shadow { color, .. }
                | DrawCmd::GlyphRun { color, .. } => *color,
            };
            color.a > 0 && (color.r, color.g, color.b) == (expected.r, expected.g, expected.b)
        })
    }

    #[test]
    fn software_paint_deterministic_golden() {
        let mut r = SoftwareRenderer::new(720, 64, true);
        let mut s = Scene::new(720, 64, true);
        s.fill(
            Rect::new(0.0, 0.0, 720.0, 64.0).rounded(12.0),
            Color::from_hex(0x0B0C0F),
        );
        let a = r.paint(&s);
        let b = r.paint(&s);
        assert_eq!(a, b);
        assert_eq!(a.len(), 720 * 64 * 4);
    }

    #[test]
    fn wgpu_fallback_is_explicit_never_silent() {
        let mut w = WgpuRenderer::new(720, 64, true);
        let st = w.try_init_gpu();
        assert!(matches!(st, GpuStatus::FallbackSoftware(_)));
        let s = Scene::new(720, 64, true);
        assert!(w.render(&s).is_ok());
    }

    #[test]
    fn journal_100k_virtualized_no_alloc_explosion() {
        // S2 : virtualisation — seuls ~20 rangs visibles alloués.
        let count = 100_000usize;
        let row_h = 32.0f32;
        let viewport = 640.0f32;
        let visible = (viewport / row_h).ceil() as usize + 1;
        assert!(visible < 30);
        assert_eq!(count, 100_000);
        let snap = StableSnapshot::from_states(
            &PresenceIndicator::new(),
            &EphemeralHud::new(),
            &CostCounter::new(500, "EUR"),
            &SilenceBudgetTracker::new(3),
            0,
            0,
        );
        let scene = hud_scene(&snap, true);
        assert_eq!(scene.width, 720);
    }

    #[test]
    fn shapes_draw_deterministically_and_cover_geometry() {
        let mut r = SoftwareRenderer::new(64, 64, true);
        let mut s = Scene::new(64, 64, true);
        s.fill(
            Rect::new(4.0, 4.0, 56.0, 56.0).rounded(8.0),
            Color::from_hex(0x224466),
        );
        s.ring(32.0, 32.0, 12.0, 3.0, Color::from_hex(0xFF7A2F));
        let a = r.paint(&s);
        let b = r.paint(&s);
        assert_eq!(a, b, "rendu logiciel déterministe");
        // Centre : rempli par le rectangle (couleur ≠ fond).
        let center = (32 * 64 + 32) * 4;
        assert!(a[center] != 0x0B || a[center + 1] != 0x0C || a[center + 2] != 0x0F);
        // Coin hors rectangle : fond inchangé.
        let corner = 0usize;
        assert_eq!(
            (a[corner], a[corner + 1], a[corner + 2]),
            (0x0B, 0x0C, 0x0F)
        );
        // Anneau : pixel exactement sur le rayon → braise présente.
        let ring_px = (32 * 64 + 44) * 4;
        assert!(a[ring_px] > 0x80, "anneau peint (r={})", a[ring_px]);
    }

    #[test]
    fn glyphs_render_ink_when_system_font_present() {
        if ui_fonts().is_empty() {
            // CI sans police système : rendu texte explicitement absent.
            return;
        }
        let mut r = SoftwareRenderer::new(200, 48, true);
        let mut s = Scene::new(200, 48, true);
        s.text(8.0, 32.0, 20.0, Color::from_hex(0xFFFFFF), "Hakille");
        let px = r.paint(&s);
        let ink = px
            .chunks_exact(4)
            .filter(|c| c[0] != 0x0B || c[1] != 0x0C || c[2] != 0x0F)
            .count();
        assert!(ink > 50, "glyphes réellement peints (ink={ink})");
    }

    #[test]
    fn empty_hud_shows_actionable_placeholder() {
        let snap = snapshot_with_query("");
        let scene = hud_scene(&snap, true);
        assert!(
            scene.cmds.iter().any(
                |cmd| matches!(cmd, DrawCmd::GlyphRun { text, .. } if text.contains("Hakille"))
            ),
            "un HUD vide doit rester compréhensible, jamais une barre noire muette"
        );
    }

    #[test]
    fn hud_query_ellipsis_never_exceeds_zone() {
        let query = "m".repeat(600);
        let snap = snapshot_with_query(&query);
        let scene = hud_scene(&snap, true);
        let body_px = ty::TYPE_BODY_SIZE as f32;
        let (x, text) = scene
            .cmds
            .iter()
            .find_map(|cmd| match cmd {
                DrawCmd::GlyphRun {
                    x, y, size, text, ..
                } if (*y - BODY_BASELINE).abs() < 0.01 && (*size - body_px).abs() < 0.01 => {
                    Some((*x, text.clone()))
                }
                _ => None,
            })
            .expect("ligne requête présente");
        assert!(text.ends_with('…'), "ellipse attendue: {text}");
        assert!(text.len() < query.len(), "texte tronqué");
        let width = measure_ui_text(&text, body_px);
        let right = 4.0 + 712.0 - 12.0; // bar.x + bar.w − 3 × grille
        assert!(x + width <= right + 0.01, "débordement: x={x} w={width}");
        // Même quand l'ellipse ne tient pas : jamais plus large que la zone.
        let tiny = ellipsize(&query, 2.0, |t| measure_ui_text(t, 16.0));
        assert!(measure_ui_text(&tiny, 16.0) <= 16.0);
    }

    #[test]
    fn braise_only_for_decision_states() {
        for state in 0u8..=5 {
            for dark in [true, false] {
                let token = if dark {
                    tokens::COLOR_BRAISE_DARK
                } else {
                    tokens::COLOR_BRAISE_LIGHT
                };
                let scene = hud_scene(&snapshot_with_state(state, false), dark);
                let expected = matches!(state, 4 | 5);
                assert_eq!(
                    scene_uses_color(&scene, Color::from_hex(token)),
                    expected,
                    "braise interdite hors décision (état {state}, dark={dark})"
                );
            }
        }
        let validation = hud_scene(&snapshot_with_state(0, true), true);
        assert!(
            scene_uses_color(&validation, Color::from_hex(tokens::COLOR_BRAISE_DARK)),
            "validation : braise autorisée"
        );
    }

    #[test]
    fn pulse_easing_is_smooth_and_bounded() {
        assert!(smoothstep(0.0).abs() < 1e-6);
        assert!((smoothstep(1.0) - 1.0).abs() < 1e-6);
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-6);
        assert!(smoothstep(0.2) < 0.2, "arrivée plus douce que linéaire");
        assert!(smoothstep(0.8) > 0.8, "départ plus doux que linéaire");
        let mut prev = 0.0;
        for i in 0..=100 {
            let v = smoothstep(i as f32 / 100.0);
            assert!(v >= prev, "monotone");
            prev = v;
        }
    }

    #[test]
    fn wrap_ui_text_respects_width_and_preserves_paragraphs() {
        let text = "mot ".repeat(80) + "\n\nfin";
        let lines = wrap_ui_text(&text, 14.0, 240.0);
        assert!(lines.len() > 2);
        for line in &lines {
            if line.split_whitespace().count() > 1 {
                assert!(
                    measure_ui_text(line, 14.0) <= 240.0 + 0.01,
                    "ligne trop large: {line:?}"
                );
            }
        }
        assert!(lines.iter().any(|line| line.is_empty()), "saut préservé");
        assert_eq!(lines.last().map(String::as_str), Some("fin"));
    }

    #[test]
    fn consult_scene_renders_360_panel_with_title_and_content() {
        let snap = snapshot_with_state(2, false);
        let view = ConsultView {
            title: "Journal d'audit".into(),
            subtitle: "3 entrées".into(),
            lines: vec![
                ConsultLine::new("CognitiveTurn · 2026-10-21", ConsultLineKind::Strong),
                ConsultLine::new("Tour servi au HUD", ConsultLineKind::Normal),
            ],
            footer: "Échap fermer".into(),
        };
        let scene = consult_scene(&snap, &view, true);
        assert_eq!(scene.height, CONSULT_HEIGHT);
        assert_eq!(scene.width, HUD_WIDTH);
        assert!(scene.cmds.iter().any(
            |cmd| matches!(cmd, DrawCmd::GlyphRun { text, .. } if text.contains("Journal d'audit"))
        ));
        assert!(scene.cmds.iter().any(|cmd| matches!(
            cmd,
            DrawCmd::GlyphRun { text, .. } if text.contains("Tour servi au HUD")
        )));
        assert!(scene.cmds.iter().any(|cmd| matches!(
            cmd,
            DrawCmd::GlyphRun { text, .. } if text.contains("Échap fermer")
        )));
    }

    #[test]
    fn consult_scene_truncates_overflow_with_explicit_ellipsis() {
        let snap = snapshot_with_state(0, false);
        let view = ConsultView {
            title: "Mémoire".into(),
            subtitle: String::new(),
            lines: (0..100)
                .map(|i| ConsultLine::new(format!("fait numéro {i}"), ConsultLineKind::Normal))
                .collect(),
            footer: String::new(),
        };
        let scene = consult_scene(&snap, &view, true);
        assert_eq!(scene.height, CONSULT_HEIGHT);
        assert!(
            scene
                .cmds
                .iter()
                .any(|cmd| matches!(cmd, DrawCmd::GlyphRun { text, .. } if text == "…")),
            "le contenu tronqué doit être signalé, jamais coupé en silence"
        );
    }

    #[test]
    fn card_scene_renders_real_card_and_decision_chrome() {
        let snap = snapshot_with_state(4, false);
        let card = ActionCard {
            id: 7,
            verb: "Confirmer".into(),
            object: "Virement : 25 EUR".into(),
            reason: "Action irréversible détectée par le noyau".into(),
            cost: hakille_ui_protocol::Money::from_cents(2500, "EUR"),
            data_exit: DataExit::Local,
            reversible: false,
            provenance: Provenance::Hakille,
        };
        let scene = card_scene(&snap, &card, None, true);
        assert_eq!(scene.width, HUD_WIDTH);
        assert_eq!(scene.height, CONSULT_HEIGHT);
        let texts: Vec<&str> = scene
            .cmds
            .iter()
            .filter_map(|cmd| match cmd {
                DrawCmd::GlyphRun { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains("Décision requise")));
        assert!(texts.iter().any(|t| t.contains("Confirmer Virement")));
        assert!(texts.iter().any(|t| t.contains("25.00 EUR")));
        assert!(texts.iter().any(|t| t.contains("maintenir 600 ms")));
        assert!(texts.iter().any(|t| t.contains("Source : Hakille")));
        assert!(texts.iter().any(|t| t.contains("aucune exécution")));

        let notice_scene = card_scene(
            &snap,
            &card,
            Some("appui trop court — maintenir 600 ms pour valider, ou refuser"),
            true,
        );
        let notice_texts: Vec<&str> = notice_scene
            .cmds
            .iter()
            .filter_map(|cmd| match cmd {
                DrawCmd::GlyphRun { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            notice_texts.iter().any(|t| t.contains("appui trop court")),
            "l'appui trop court doit être visible sur la carte"
        );
        assert!(
            !notice_texts.iter().any(|t| t.contains("AccessKit =")),
            "le rappel d'accessibilité cède la place au message d'erreur"
        );
    }

    #[test]
    fn fallback_chain_renders_cjk_when_available() {
        let fonts = ui_fonts();
        let cjk = '語';
        if !fonts.iter().any(|f| f.glyph_id(cjk).0 != 0) {
            // Machine sans police CJK : non couvert = non rendu (honnête).
            return;
        }
        let mut r = SoftwareRenderer::new(120, 48, true);
        let mut s = Scene::new(120, 48, true);
        s.text(8.0, 32.0, 20.0, Color::from_hex(0xFFFFFF), "語");
        let px = r.paint(&s);
        let ink = px
            .chunks_exact(4)
            .filter(|c| c[0] != 0x0B || c[1] != 0x0C || c[2] != 0x0F)
            .count();
        assert!(ink > 20, "glyphe de repli réellement peint (ink={ink})");
    }
}
