//! Rendu 3D du Palais de la Mémoire dans le shell desktop Hakille (§33).
//!
//! Projection perspective 3D pure sans dépendance GPU obligatoire,
//! tri en profondeur (Painter's algorithm), arêtes vectorielles,
//! et halo pulsant Braise (#FF7A2F) sur les contradictions cognitives actives.

use crate::components::PrimitiveCtx;
use crate::scene::{Color, Rect, Scene};
use serde::{Deserialize, Serialize};

/// Caméra 3D pour la navigation dans le Palais de la Mémoire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalaceCamera3D {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_deg: f32,
    pub near_clip: f32,
    pub far_clip: f32,
}

impl Default for PalaceCamera3D {
    fn default() -> Self {
        Self {
            eye: [0.0, 40.0, 160.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_deg: 60.0,
            near_clip: 1.0,
            far_clip: 1000.0,
        }
    }
}

impl PalaceCamera3D {
    pub fn new(eye: [f32; 3], target: [f32; 3]) -> Self {
        Self {
            eye,
            target,
            ..Self::default()
        }
    }

    /// Projette un point 3D monde vers les coordonnées écran (pixel_x, pixel_y, depth_z).
    /// Retourne `None` si le point est derrière le plan proche de la caméra.
    pub fn project(&self, p: [f32; 3], screen_w: f32, screen_h: f32) -> Option<(f32, f32, f32)> {
        // Vecteur forward (visée) normalisé
        let fx = self.target[0] - self.eye[0];
        let fy = self.target[1] - self.eye[1];
        let fz = self.target[2] - self.eye[2];
        let f_len = (fx * fx + fy * fy + fz * fz).sqrt();
        if f_len < 0.001 {
            return None;
        }
        let f = [fx / f_len, fy / f_len, fz / f_len];

        // Vecteur right = cross(forward, up)
        let rx = f[1] * self.up[2] - f[2] * self.up[1];
        let ry = f[2] * self.up[0] - f[0] * self.up[2];
        let rz = f[0] * self.up[1] - f[1] * self.up[0];
        let r_len = (rx * rx + ry * ry + rz * rz).sqrt();
        if r_len < 0.001 {
            return None;
        }
        let r = [rx / r_len, ry / r_len, rz / r_len];

        // Vecteur up corrigé = cross(right, forward)
        let u = [
            r[1] * f[2] - r[2] * f[1],
            r[2] * f[0] - r[0] * f[2],
            r[0] * f[1] - r[1] * f[0],
        ];

        // Vecteur relatif caméra
        let dx = p[0] - self.eye[0];
        let dy = p[1] - self.eye[1];
        let dz = p[2] - self.eye[2];

        // Coordonnées dans le repère caméra
        let cam_x = dx * r[0] + dy * r[1] + dz * r[2];
        let cam_y = dx * u[0] + dy * u[1] + dz * u[2];
        let cam_z = dx * f[0] + dy * f[1] + dz * f[2];

        if cam_z < self.near_clip || cam_z > self.far_clip {
            return None;
        }

        let fov_rad = self.fov_deg.to_radians();
        let focal = (screen_h / 2.0) / (fov_rad / 2.0).tan();

        let screen_x = screen_w / 2.0 + (cam_x / cam_z) * focal;
        let screen_y = screen_h / 2.0 - (cam_y / cam_z) * focal; // axe Y inversé à l'écran

        Some((screen_x, screen_y, cam_z))
    }
}

/// Nœud de rendu pour le Palais de la Mémoire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalaceRenderNode {
    pub id: String,
    pub label: String,
    pub position: [f32; 3],
    pub weight: f32,
    pub contradiction_level: f32, // [0.0, 1.0]
}

/// Arête de rendu reliant deux nœuds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalaceRenderEdge {
    pub source: String,
    pub target: String,
    pub is_contradiction: bool,
}

/// Moteur de rendu de scène 3D pour le Palais de la Mémoire.
#[derive(Debug, Clone)]
pub struct PalaceSceneRenderer {
    pub camera: PalaceCamera3D,
    pub nodes: Vec<PalaceRenderNode>,
    pub edges: Vec<PalaceRenderEdge>,
    pub pulse_time_s: f32,
}

impl PalaceSceneRenderer {
    pub fn new() -> Self {
        Self {
            camera: PalaceCamera3D::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            pulse_time_s: 0.0,
        }
    }

    pub fn set_pulse_time(&mut self, time_s: f32) {
        self.pulse_time_s = time_s;
    }

    /// Dessine la scène 3D sur le canvas 2D [`Scene`].
    pub fn draw(&self, scene: &mut Scene, ctx: &PrimitiveCtx, viewport: Rect) {
        let mut projected_nodes = Vec::new();
        let mut id_to_screen = std::collections::HashMap::new();

        // 1. Projection 3D -> 2D
        for node in &self.nodes {
            if let Some((sx, sy, depth)) =
                self.camera.project(node.position, viewport.w, viewport.h)
            {
                let screen_pos = (viewport.x + sx, viewport.y + sy);
                projected_nodes.push((screen_pos.0, screen_pos.1, depth, node));
                id_to_screen.insert(node.id.clone(), (screen_pos.0, screen_pos.1, depth));
            }
        }

        // Tri des nœuds du fond vers le premier plan (Painter's Algorithm)
        projected_nodes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // 2. Dessin des arêtes
        for edge in &self.edges {
            if let (Some(&(x1, y1, _)), Some(&(x2, y2, _))) = (
                id_to_screen.get(&edge.source),
                id_to_screen.get(&edge.target),
            ) {
                let edge_color = if edge.is_contradiction {
                    Color::from_hex(ctx.theme.braise).with_alpha(180)
                } else {
                    Color::from_hex(ctx.theme.line).with_alpha(100)
                };

                let min_x = x1.min(x2);
                let min_y = y1.min(y2);
                let w = (x2 - x1).abs().max(1.0);
                let h = (y2 - y1).abs().max(1.0);
                scene.fill(Rect::new(min_x, min_y, w, h), edge_color);
            }
        }

        // 3. Dessin des nœuds avec halo Braise en cas de contradiction
        for (sx, sy, depth, node) in projected_nodes {
            let scale_factor = (80.0 / depth.max(10.0)).clamp(0.4, 2.5);
            let base_radius = (8.0 + node.weight * 6.0) * scale_factor;

            let is_contradiction = node.contradiction_level > 0.05;

            if is_contradiction {
                // Pulsation dynamique Braise (#FF7A2F)
                let pulse = (self.pulse_time_s * 4.0).sin().abs();
                let glow_radius = base_radius + 4.0 + pulse * 6.0;
                scene.shadow(
                    Rect::new(
                        sx - glow_radius,
                        sy - glow_radius,
                        glow_radius * 2.0,
                        glow_radius * 2.0,
                    ),
                    12.0,
                    Color::from_hex(ctx.theme.braise)
                        .with_alpha((120.0 * node.contradiction_level) as u8),
                );

                scene.ring(
                    sx,
                    sy,
                    base_radius + 2.0,
                    2.0,
                    Color::from_hex(ctx.theme.braise),
                );
            }

            // Corps du nœud
            let body_color = if is_contradiction {
                Color::from_hex(ctx.theme.braise).with_alpha(200)
            } else {
                Color::from_hex(ctx.theme.surface1)
            };

            scene.fill(
                Rect::new(
                    sx - base_radius,
                    sy - base_radius,
                    base_radius * 2.0,
                    base_radius * 2.0,
                )
                .rounded(base_radius),
                body_color,
            );

            // Contour
            scene.ring(sx, sy, base_radius, 1.5, Color::from_hex(ctx.theme.line));

            // Étiquette textuelle si suffisamment proche
            if scale_factor > 0.6 {
                scene.text(
                    sx + base_radius + 4.0,
                    sy + 4.0,
                    (11.0 * scale_factor).clamp(9.0, 14.0),
                    Color::from_hex(ctx.theme.text1),
                    node.label.clone(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn test_camera_projection_and_clipping() {
        let camera = PalaceCamera3D::default();
        // Point centered in front of camera (eye = (0, 40, 160), target = (0, 0, 0))
        let projected = camera.project([0.0, 0.0, 0.0], 800.0, 600.0);
        assert!(projected.is_some());
        let (sx, sy, depth) = projected.unwrap();
        assert!((sx - 400.0).abs() < 50.0);
        assert!((sy - 300.0).abs() < 50.0);
        assert!(depth > 100.0);

        // Point strictly behind camera (z = 250, eye = 160 looking towards 0)
        let behind = camera.project([0.0, 40.0, 250.0], 800.0, 600.0);
        assert!(behind.is_none());
    }

    #[test]
    fn test_palace_scene_renderer_draws_cmds() {
        let mut renderer = PalaceSceneRenderer::new();
        renderer.nodes.push(PalaceRenderNode {
            id: "n1".into(),
            label: "Souveraineté".into(),
            position: [0.0, 0.0, 0.0],
            weight: 0.9,
            contradiction_level: 0.0,
        });
        renderer.nodes.push(PalaceRenderNode {
            id: "n2".into(),
            label: "Cloud Leak Conflict".into(),
            position: [20.0, 10.0, -10.0],
            weight: 0.8,
            contradiction_level: 0.8, // Should generate Braise pulse shadow
        });
        renderer.edges.push(PalaceRenderEdge {
            source: "n1".into(),
            target: "n2".into(),
            is_contradiction: true,
        });
        renderer.set_pulse_time(1.2);

        let ctx = PrimitiveCtx::new(Theme::dark());
        let mut scene = Scene::new(800, 600, true);
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);

        renderer.draw(&mut scene, &ctx, viewport);
        assert!(!scene.cmds.is_empty());

        let has_shadow = scene
            .cmds
            .iter()
            .any(|cmd| matches!(cmd, crate::scene::DrawCmd::Shadow { .. }));
        assert!(
            has_shadow,
            "Contradiction node must trigger shadow glow command"
        );
    }
}
