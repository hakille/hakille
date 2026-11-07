//! Fixtures et générateurs synthétiques pour la validation CI sans GPU ni poids lourds.
//!
//! Conforme à la Règle 8 de la Constitution : permet de tester l'intégralité du pipeline
//! en local et en CI sans dépendre du téléchargement de modèles volumineux.

/// Génère une image RGB unie de dimensions données.
pub fn synthetic_blank_rgb(width: usize, height: usize, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut buf = vec![0u8; width * height * 3];
    for chunk in buf.chunks_exact_mut(3) {
        chunk[0] = r;
        chunk[1] = g;
        chunk[2] = b;
    }
    buf
}

/// Génère un dégradé horizontal et vertical continu RGB.
pub fn synthetic_gradient_rgb(width: usize, height: usize) -> Vec<u8> {
    let mut buf = vec![0u8; width * height * 3];
    for y in 0..height {
        let vy = (y as f32 / height.max(1) as f32 * 255.0) as u8;
        for x in 0..width {
            let vx = (x as f32 / width.max(1) as f32 * 255.0) as u8;
            let idx = (y * width + x) * 3;
            buf[idx] = vx;
            buf[idx + 1] = vy;
            buf[idx + 2] = ((vx as u16 + vy as u16) / 2) as u8;
        }
    }
    buf
}

/// Génère une image en damier RGB.
pub fn synthetic_checkerboard_rgb(width: usize, height: usize, cell_size: usize) -> Vec<u8> {
    let mut buf = vec![0u8; width * height * 3];
    let cs = cell_size.max(1);
    for y in 0..height {
        let cy = (y / cs) % 2;
        for x in 0..width {
            let cx = (x / cs) % 2;
            let val = if cx == cy { 240 } else { 20 };
            let idx = (y * width + x) * 3;
            buf[idx] = val;
            buf[idx + 1] = val;
            buf[idx + 2] = val;
        }
    }
    buf
}

/// Génère un recadrage facial synthétique schématique (ovale chair, yeux et bouche).
pub fn synthetic_face_crop(width: usize, height: usize) -> Vec<u8> {
    let mut buf = synthetic_blank_rgb(width, height, 220, 220, 230); // Fond gris clair
    let cx = width as f32 * 0.5;
    let cy = height as f32 * 0.5;
    let rx = width as f32 * 0.38;
    let ry = height as f32 * 0.45;

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 - cx) / rx;
            let dy = (y as f32 - cy) / ry;
            let d_ellipse = dx * dx + dy * dy;

            if d_ellipse <= 1.0 {
                let idx = (y * width + x) * 3;
                // Peau ton chair
                buf[idx] = 235;
                buf[idx + 1] = 195;
                buf[idx + 2] = 175;

                // Oeil gauche (~35% w, 40% h)
                let eye_l = ((x as f32 - width as f32 * 0.35).powi(2)
                    + (y as f32 - height as f32 * 0.40).powi(2))
                .sqrt();
                if eye_l < (width as f32 * 0.06) {
                    buf[idx] = 30;
                    buf[idx + 1] = 40;
                    buf[idx + 2] = 60;
                }

                // Oeil droit (~65% w, 40% h)
                let eye_r = ((x as f32 - width as f32 * 0.65).powi(2)
                    + (y as f32 - height as f32 * 0.40).powi(2))
                .sqrt();
                if eye_r < (width as f32 * 0.06) {
                    buf[idx] = 30;
                    buf[idx + 1] = 40;
                    buf[idx + 2] = 60;
                }

                // Bouche (~50% w, 70% h)
                let mouth_dx = (x as f32 - width as f32 * 0.50).abs();
                let mouth_dy = (y as f32 - height as f32 * 0.70).abs();
                if mouth_dx < (width as f32 * 0.16) && mouth_dy < (height as f32 * 0.04) {
                    buf[idx] = 190;
                    buf[idx + 1] = 50;
                    buf[idx + 2] = 60;
                }
            }
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_generators() {
        let blank = synthetic_blank_rgb(10, 10, 100, 150, 200);
        assert_eq!(blank.len(), 300);
        assert_eq!(blank[0], 100);
        assert_eq!(blank[1], 150);
        assert_eq!(blank[2], 200);

        let grad = synthetic_gradient_rgb(16, 16);
        assert_eq!(grad.len(), 16 * 16 * 3);

        let checker = synthetic_checkerboard_rgb(8, 8, 4);
        assert_eq!(checker.len(), 8 * 8 * 3);

        let face = synthetic_face_crop(64, 64);
        assert_eq!(face.len(), 64 * 64 * 3);
    }
}
