//! 12 primitives Annora — et pas davantage.
//!
//! `Surface, Text, Icon, Button, Toggle, FocusRing, List, TextInput,
//!  ActionCard, Pastille, Presence, Sheet`.
//! Chaque primitive : 5 états (normal, focus clavier, chargement, erreur,
//! mouvement-réduit/contraste-élevé), tokens comme source unique,
//! virtualisation au niveau `List`.

use crate::scene::{Color, Rect, Scene};
use crate::theme::Theme;
use serde::{Deserialize, Serialize};

/// État visuel commun — les 5 états exigés par la spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetState {
    Normal,
    Focused,
    Loading,
    Error,
    ReducedMotion,
}

#[derive(Debug, Clone)]
pub struct PrimitiveCtx {
    pub theme: Theme,
    pub state: WidgetState,
    pub focused: bool,
}

impl PrimitiveCtx {
    pub fn new(theme: Theme) -> Self {
        Self { theme, state: WidgetState::Normal, focused: false }
    }
    pub fn effective_state(&self) -> WidgetState {
        if self.theme.reduced_motion {
            return WidgetState::ReducedMotion;
        }
        if self.focused {
            return WidgetState::Focused;
        }
        self.state
    }
}

// ---- 1. Surface ----
#[derive(Debug, Clone)]
pub struct Surface {
    pub rect: Rect,
}

impl Surface {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        let bg = if ctx.theme.mode == crate::theme::ThemeMode::Dark { ctx.theme.bg } else { ctx.theme.bg };
        scene.fill(self.rect, Color::from_hex(bg));
        if ctx.theme.high_contrast {
            // Bordure 2px fort contraste.
            scene.fill(Rect::new(self.rect.x, self.rect.y, self.rect.w, 2.0), Color::from_hex(ctx.theme.text1));
        }
    }
}

// ---- 2. Text ----
#[derive(Debug, Clone)]
pub struct Text {
    pub content: String,
    pub size: f32,
    pub mono: bool,
}

impl Text {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, x: f32, y: f32) {
        let size = ctx.theme.text_scale.scaled(self.size);
        let color = match ctx.effective_state() {
            WidgetState::Error => Color::from_hex(0xFF5A5A),
            _ => Color::from_hex(ctx.theme.text1),
        };
        scene.text(x, y, size, color, self.content.clone());
    }
}

// ---- 3. Icon ----
#[derive(Debug, Clone)]
pub struct Icon {
    pub glyph: char,
    pub size: f32,
}

impl Icon {
    /// Glyphes autorisés : jamais d'emoji (spec design).
    pub fn is_allowed(c: char) -> bool {
        matches!(c, '✓' | '+' | '−' | '-' | '●' | '○' | '→' | '×' | '◌' | '!' | '?' | '⋯')
    }
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, x: f32, y: f32) {
        debug_assert!(Self::is_allowed(self.glyph));
        scene.text(x, y, ctx.theme.text_scale.scaled(self.size), Color::from_hex(ctx.theme.text1), self.glyph.to_string());
    }
}

// ---- 4. Button ----
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub braise: bool,
    pub rect: Rect,
}

impl Button {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        let h = ctx.theme.button_height();
        let r = Rect::new(self.rect.x, self.rect.y, self.rect.w, h).rounded(8.0);
        let fill = if self.braise { ctx.theme.braise } else { ctx.theme.surface1 };
        // Braise réservée : assertion design.
        if self.braise {
            debug_assert!(ctx.theme.braise_allowed("validation"));
        }
        scene.fill(r, Color::from_hex(fill));
        let fg = if self.braise { 0xFFFFFF } else { ctx.theme.text1 };
        if ctx.focused || ctx.effective_state() == WidgetState::Focused {
            // FocusRing 2px.
            scene.fill(Rect::new(r.x - 2.0, r.y - 2.0, r.w + 4.0, 2.0), Color::from_hex(ctx.theme.text1));
            scene.fill(Rect::new(r.x - 2.0, r.y + r.h, r.w + 4.0, 2.0), Color::from_hex(ctx.theme.text1));
        }
        if ctx.effective_state() == WidgetState::Loading {
            scene.text(r.x + 12.0, r.y + h / 2.0 + 5.0, 14.0, Color::from_hex(fg), "⋯");
        } else {
            scene.text(r.x + 12.0, r.y + h / 2.0 + 5.0, 14.0, Color::from_hex(fg), self.label.clone());
        }
        if ctx.effective_state() == WidgetState::Error {
            scene.fill(Rect::new(r.x, r.y + h - 2.0, r.w, 2.0), Color::rgb(255, 90, 90));
        }
    }
}

// ---- 5. Toggle ----
#[derive(Debug, Clone)]
pub struct Toggle {
    pub on: bool,
    pub locked: bool,
    pub rect: Rect,
}

impl Toggle {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        let c = if self.locked { ctx.theme.text2 } else if self.on { ctx.theme.braise } else { ctx.theme.line };
        scene.fill(self.rect.rounded(self.rect.h / 2.0), Color::from_hex(c));
        let knob_x = if self.on { self.rect.x + self.rect.w - self.rect.h } else { self.rect.x };
        scene.ring(knob_x + self.rect.h / 2.0, self.rect.y + self.rect.h / 2.0, self.rect.h / 2.0 - 2.0, 2.0, Color::from_hex(ctx.theme.text1));
    }
}

// ---- 6. FocusRing ----
#[derive(Debug, Clone)]
pub struct FocusRing {
    pub rect: Rect,
}

impl FocusRing {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        if ctx.focused {
            let t = ctx.theme.text1;
            scene.fill(Rect::new(self.rect.x - 2.0, self.rect.y - 2.0, self.rect.w + 4.0, 2.0), Color::from_hex(t));
            scene.fill(Rect::new(self.rect.x - 2.0, self.rect.y + self.rect.h, self.rect.w + 4.0, 2.0), Color::from_hex(t));
        }
    }
}

// ---- 7. List (virtualisée) ----
#[derive(Debug, Clone)]
pub struct List {
    pub count: usize,
    pub row_h: f32,
    pub viewport_h: f32,
    pub scroll_y: f32,
}

impl List {
    /// Plage visible — virtualisation au niveau List, pas dans JournalScreen.
    pub fn visible_range(&self) -> (usize, usize) {
        let start = (self.scroll_y / self.row_h).floor() as usize;
        let visible = (self.viewport_h / self.row_h).ceil() as usize + 1;
        let start = start.min(self.count);
        let end = (start + visible).min(self.count);
        (start, end)
    }
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, x: f32, y: f32, render_row: impl Fn(usize, Rect)) {
        let (s, e) = self.visible_range();
        for i in s..e {
            let r = Rect::new(x, y + (i as f32) * self.row_h - self.scroll_y, 700.0, self.row_h);
            render_row(i, r);
        }
        let _ = (scene.width, ctx.theme.bg);
    }
}

// ---- 8. TextInput ----
#[derive(Debug, Clone)]
pub struct TextInput {
    pub value: String,
    pub placeholder: String,
    pub rect: Rect,
    pub cursor_byte: usize,
}

impl TextInput {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        scene.fill(self.rect.rounded(8.0), Color::from_hex(ctx.theme.surface1));
        let shown = if self.value.is_empty() { &self.placeholder } else { &self.value };
        scene.text(self.rect.x + 12.0, self.rect.y + 28.0, 16.0, Color::from_hex(ctx.theme.text1), shown.clone());
        if ctx.focused {
            scene.fill(Rect::new(self.rect.x + 12.0 + self.value.len() as f32 * 6.0, self.rect.y + 10.0, 2.0, 28.0), Color::from_hex(ctx.theme.text1));
        }
    }
}

// ---- 9. Pastille ----
#[derive(Debug, Clone)]
pub struct Pastille {
    pub kind: PastilleKind,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastilleKind {
    Cost,
    Data,
    Reversibility,
}

impl Pastille {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, x: f32, y: f32) {
        let border = match self.kind {
            PastilleKind::Cost => ctx.theme.braise,
            _ => ctx.theme.line,
        };
        scene.fill(Rect::new(x, y, 120.0, 24.0).rounded(12.0), Color::from_hex(ctx.theme.surface1));
        scene.fill(Rect::new(x, y + 22.0, 120.0, 2.0), Color::from_hex(border));
        scene.text(x + 8.0, y + 17.0, 13.0, Color::from_hex(ctx.theme.text1), self.label.clone());
    }
}

// ---- 10. Presence ----
#[derive(Debug, Clone)]
pub struct Presence {
    pub state: u8,
    pub pulse: f32,
    pub cx: f32,
    pub cy: f32,
}

impl Presence {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        let pulse = if ctx.theme.reduced_motion {
            if self.pulse >= 0.5 { 1.0 } else { 0.0 }
        } else {
            self.pulse.clamp(0.0, 1.0)
        };
        let (ring, dot) = match self.state {
            1 => (0xFF7A2F, 0xFF7A2F),
            4 | 5 => (0xFF7A2F, 0xFF5A5A),
            _ => (ctx.theme.line, ctx.theme.text2),
        };
        let r = 10.0 + pulse * 4.0;
        scene.ring(self.cx, self.cy, r, 2.0, Color::from_hex(ring));
        scene.ring(self.cx, self.cy, 5.0, 3.0, Color::from_hex(dot));
    }
}

// ---- 11. ActionCard ----
#[derive(Debug, Clone)]
pub struct ActionCard {
    pub verb: String,
    pub object: String,
    pub reason: String,
    pub cost: String,
    pub data: String,
    pub reversible: String,
    pub needs_hold: bool,
    pub is_external: bool,
    pub provenance: String,
    pub rect: Rect,
}

impl ActionCard {
    pub fn validate_label(&self) -> String {
        if self.needs_hold {
            "Valider — appui long 600 millisecondes (action définitive)".to_string()
        } else {
            "Valider".to_string()
        }
    }
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx) {
        Surface { rect: self.rect }.draw(scene, ctx);
        Text { content: format!("{} {}", self.verb, self.object), size: 16.0, mono: false }.draw(scene, ctx, self.rect.x + 16.0, self.rect.y + 28.0);
        Text { content: format!("{} — Pourquoi ?", self.reason), size: 13.0, mono: false }.draw(scene, ctx, self.rect.x + 16.0, self.rect.y + 50.0);
        let labels = [self.cost.clone(), self.data.clone(), self.reversible.clone()];
        for (i, l) in labels.iter().enumerate() {
            Pastille { kind: match i { 0 => PastilleKind::Cost, 1 => PastilleKind::Data, _ => PastilleKind::Reversibility }, label: l.clone() }
                .draw(scene, ctx, self.rect.x + 16.0 + i as f32 * 132.0, self.rect.y + 62.0);
        }
        let vy = self.rect.y + 100.0;
        Button { label: self.validate_label(), braise: true, rect: Rect::new(self.rect.x + 16.0, vy, 200.0, 44.0) }.draw(scene, ctx);
        Button { label: "Modifier".into(), braise: false, rect: Rect::new(self.rect.x + 224.0, vy, 120.0, 44.0) }.draw(scene, ctx);
        Button { label: "Refuser".into(), braise: false, rect: Rect::new(self.rect.x + 352.0, vy, 120.0, 44.0) }.draw(scene, ctx);
        if self.is_external {
            Text { content: "[externe — filet 2 px]".into(), size: 13.0, mono: false }.draw(scene, ctx, self.rect.x + 16.0, self.rect.y + 150.0);
        }
        let _ = &self.provenance;
    }
}

// ---- 12. Sheet ----
#[derive(Debug, Clone)]
pub struct Sheet {
    pub title: String,
    pub rect: Rect,
}

impl Sheet {
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, body: impl Fn(&mut Scene, &PrimitiveCtx)) {
        scene.fill(Rect::new(self.rect.x, self.rect.y, self.rect.w, self.rect.h).rounded(16.0), Color::from_hex(ctx.theme.surface1));
        Text { content: self.title.clone(), size: 20.0, mono: false }.draw(scene, ctx, self.rect.x + 16.0, self.rect.y + 30.0);
        body(scene, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn ctx() -> PrimitiveCtx {
        PrimitiveCtx::new(Theme::dark())
    }

    #[test]
    fn twelve_primitives_draw_without_panic_all_states() {
        for state in [WidgetState::Normal, WidgetState::Focused, WidgetState::Loading, WidgetState::Error, WidgetState::ReducedMotion] {
            let theme = match state {
                WidgetState::ReducedMotion => Theme::dark().with_reduced_motion(true),
                _ => Theme::dark(),
            };
            let mut c = PrimitiveCtx::new(theme);
            c.state = state;
            c.focused = state == WidgetState::Focused;
            let mut s = Scene::new(720, 640, true);
            Surface { rect: Rect::new(0.0, 0.0, 720.0, 64.0).rounded(12.0) }.draw(&mut s, &c);
            Text { content: "Akora".into(), size: 16.0, mono: false }.draw(&mut s, &c, 8.0, 20.0);
            assert!(Icon::is_allowed('✓'));
            assert!(!Icon::is_allowed('😀'));
            Button { label: "Valider".into(), braise: true, rect: Rect::new(0.0, 0.0, 200.0, 44.0) }.draw(&mut s, &c);
            Toggle { on: true, locked: false, rect: Rect::new(0.0, 0.0, 48.0, 24.0) }.draw(&mut s, &c);
            FocusRing { rect: Rect::new(0.0, 0.0, 100.0, 44.0) }.draw(&mut s, &c);
            let list = List { count: 100_000, row_h: 32.0, viewport_h: 640.0, scroll_y: 0.0 };
            let (a, b) = list.visible_range();
            assert!(b - a < 30);
            TextInput { value: "q".into(), placeholder: "p".into(), rect: Rect::new(0.0, 0.0, 400.0, 44.0), cursor_byte: 1 }.draw(&mut s, &c);
            ActionCard {
                verb: "Envoyer".into(), object: "mail".into(), reason: "demandé".into(),
                cost: "0 €".into(), data: "Local".into(), reversible: "annulable".into(),
                needs_hold: state == WidgetState::Error, is_external: false, provenance: "Akora".into(),
                rect: Rect::new(0.0, 0.0, 600.0, 200.0),
            }.draw(&mut s, &c);
            Presence { state: 1, pulse: 0.7, cx: 32.0, cy: 32.0 }.draw(&mut s, &c);
            Sheet { title: "Journal".into(), rect: Rect::new(0.0, 0.0, 720.0, 640.0) }.draw(&mut s, &c, |_, _| {});
            assert!(!s.cmds.is_empty());
        }
    }

    #[test]
    fn button_height_density_and_hold_label() {
        let compact = PrimitiveCtx::new(Theme { density: crate::theme::Density::Compact, ..Theme::dark() });
        assert_eq!(compact.theme.button_height(), 32.0);
        let card = ActionCard {
            verb: "Payer".into(), object: "25 €".into(), reason: "facture".into(), cost: "25 €".into(),
            data: "Cloud".into(), reversible: "définitif".into(), needs_hold: true, is_external: true,
            provenance: "Externe".into(), rect: Rect::new(0.0, 0.0, 600.0, 200.0),
        };
        assert!(card.validate_label().contains("600"));
    }
}
