//! Vision classique RÉELLE — pur Rust, `std` uniquement, zéro dépendance.
//!
//! Ce module remplace toute heuristique à boîtes fixes : chaque boîte, chaque
//! score et chaque statistique est calculé depuis les pixels d'entrée.
//!
//! Contenu :
//! - conversion RGB(A)/BGR vers luminance (formule de perception
//!   `0.299·R + 0.587·G + 0.114·B`, appliquée aux octets mesurés) ;
//! - seuillage adaptatif d'Otsu (seuil dérivé de l'histogramme mesuré,
//!   jamais fixé en dur) ;
//! - contours de Sobel + densité de contours à seuil adaptatif
//!   (`moyenne + écart-type` de la magnitude, mesurés sur l'image) ;
//! - segmentation en régions par composantes connexes (union-find, 4-connexité)
//!   sur la carte binaire **et** son inverse (pour capter aussi les éléments
//!   sombres sur fond clair), triées par aire décroissante ;
//! - template matching par corrélation croisée normalisée (NCC) multi-échelle,
//!   avec images intégrales (complexité O(1) par position) ;
//! - `detect_text_regions()` : candidats **géométriques** (fort gradient
//!   horizontal dense) — **pas de reconnaissance de caractères sans modèle
//!   OCR** ; ces boîtes localisent des zones « probablement textuelles »
//!   sans en lire le contenu.
//!
//! Garantie anti-hallucination : image vide ou uniforme → vecteur vide.
//! Aucune boîte n'est jamais inventée.
//!
//! Conventions d'honnêteté numérique : les divisions par 255.0 sont des
//! conversions d'unité (octet → [0,1]), pas des seuils ; les seuls paramètres
//! de calibrage (rejet du fond > 90 %, planchers de bruit, pondérations du
//! score) sont documentés comme tels à chaque usage.

/// Boîte détectée par mesure, coordonnées normalisées dans `[0,1]`.
#[derive(Debug, Clone)]
pub struct DetectedBox {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
    /// Fraction de la surface image couverte (`pixels / (w*h)`), mesurée.
    pub area_rel: f32,
    /// Score géométrique mesuré dans `[0,1]` : contraste local × taille × centralité.
    pub score: f32,
    /// Nombre de pixels de la composante, mesuré.
    pub pixels: usize,
}

impl DetectedBox {
    pub fn center(&self) -> (f32, f32) {
        (
            (self.x_min + self.x_max) / 2.0,
            (self.y_min + self.y_max) / 2.0,
        )
    }
}

/// Description de l'ordre des canaux d'un pixel packé.
#[derive(Debug, Clone, Copy)]
pub struct PixelLayout {
    pub bytes_per_pixel: usize,
    pub r: usize,
    pub g: usize,
    pub b: usize,
}

impl PixelLayout {
    pub const RGB: Self = Self {
        bytes_per_pixel: 3,
        r: 0,
        g: 1,
        b: 2,
    };
    pub const RGBA: Self = Self {
        bytes_per_pixel: 4,
        r: 0,
        g: 1,
        b: 2,
    };
    pub const BGRA: Self = Self {
        bytes_per_pixel: 4,
        r: 2,
        g: 1,
        b: 0,
    };
}

/// Convertit des pixels packés en carte de luminance.
///
/// Retourne `None` si les dimensions sont nulles ou incohérentes avec les données
/// (jamais de pixels inventés : pas de remplissage silencieux).
pub fn to_grayscale(data: &[u8], width: u32, height: u32, layout: PixelLayout) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    let n = w.checked_mul(h)?;
    if data.len() < n.checked_mul(layout.bytes_per_pixel)? {
        return None;
    }
    if layout.r >= layout.bytes_per_pixel
        || layout.g >= layout.bytes_per_pixel
        || layout.b >= layout.bytes_per_pixel
    {
        return None;
    }
    let mut gray = Vec::with_capacity(n);
    for px in data.chunks_exact(layout.bytes_per_pixel).take(n) {
        let r = px[layout.r] as f32;
        let g = px[layout.g] as f32;
        let b = px[layout.b] as f32;
        gray.push(
            (0.299 * r + 0.587 * g + 0.114 * b)
                .round()
                .clamp(0.0, 255.0) as u8,
        );
    }
    if gray.len() != n {
        return None;
    }
    Some(gray)
}

/// (moyenne, écart-type) de la luminance — mesures pures.
pub fn luminance_stats(gray: &[u8]) -> (f32, f32) {
    if gray.is_empty() {
        return (0.0, 0.0);
    }
    let n = gray.len() as f32;
    let mean = gray.iter().map(|&v| v as f32).sum::<f32>() / n;
    let var = gray.iter().map(|&v| (v as f32 - mean).powi(2)).sum::<f32>() / n;
    (mean, var.sqrt())
}

/// Moyenne par canal (R, G, B) — mesures pures.
pub fn channel_means(
    data: &[u8],
    width: u32,
    height: u32,
    layout: PixelLayout,
) -> Option<(f32, f32, f32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let n = (width as usize).checked_mul(height as usize)?;
    if data.len() < n.checked_mul(layout.bytes_per_pixel)? {
        return None;
    }
    let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
    for px in data.chunks_exact(layout.bytes_per_pixel).take(n) {
        sr += px[layout.r] as u64;
        sg += px[layout.g] as u64;
        sb += px[layout.b] as u64;
    }
    let n = n as f32;
    Some((sr as f32 / n, sg as f32 / n, sb as f32 / n))
}

/// Nom de dominante couleur par comparaison exacte des moyennes mesurées
/// (aucun seuil : le canal strictement maximal gagne, égalité exacte → neutre).
pub fn dominant_color_name(r_mean: f32, g_mean: f32, b_mean: f32) -> &'static str {
    if r_mean == g_mean && g_mean == b_mean {
        "Gris-Neutre"
    } else if r_mean >= g_mean && r_mean >= b_mean {
        "Dominante-Rouge"
    } else if g_mean >= r_mean && g_mean >= b_mean {
        "Dominante-Vert"
    } else {
        "Dominante-Bleu"
    }
}

/// Seuil d'Otsu dérivé de l'histogramme mesuré (maximise la variance
/// inter-classe). Image uniforme → retourne le niveau unique (la binarisation
/// `v > seuil` produira alors zéro pixel : pas de région inventée).
pub fn otsu_threshold(gray: &[u8]) -> u8 {
    if gray.is_empty() {
        return 0;
    }
    let mut hist = [0u64; 256];
    for &v in gray {
        hist[v as usize] += 1;
    }
    let total = gray.len() as f64;
    let mut sum_all = 0.0;
    for (i, &h) in hist.iter().enumerate() {
        sum_all += i as f64 * h as f64;
    }
    let (mut sum_b, mut w_b, mut best_t, mut best_var) = (0.0, 0u64, 0u8, -1.0f64);
    for (t, &h) in hist.iter().enumerate() {
        w_b += h;
        if w_b == 0 {
            continue;
        }
        let w_f = gray.len() as u64 - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as f64 * h as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_all - sum_b) / w_f as f64;
        let var_between = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
        if var_between > best_var {
            best_var = var_between;
            best_t = t as u8;
        }
    }
    let _ = total;
    best_t
}

/// Binarisation stricte `v > seuil`.
pub fn binarize(gray: &[u8], threshold: u8) -> Vec<bool> {
    gray.iter().map(|&v| v > threshold).collect()
}

/// Trouve-racine avec compression de chemin sur un vecteur parent
/// (étiquette 0 = sentinelle fond, jamais utilisée comme label).
fn dsu_find(parent: &mut Vec<usize>, mut x: usize) -> usize {
    let mut root = x;
    while parent[root] != root {
        root = parent[root];
    }
    while parent[x] != root {
        let next = parent[x];
        parent[x] = root;
        x = next;
    }
    root
}

struct ComponentAcc {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    count: usize,
    sum_lum: u64,
}
/// Composantes connexes (4-connexité) des pixels `true`, avec accumulateurs
/// de boîte englobante et de luminance cumulée (pour le contraste local).
///
/// Implémentation RLE (run-length) : balayage ligne par ligne en segments
/// contigus + union-find sur les étiquettes de segments (bien moins
/// nombreuses que les pixels), puis accumulation par segment. Strictement
/// équivalent à un union-find par pixel en 4-connexité (segments horizontaux
/// + recouvrement vertical), en beaucoup moins d'opérations — même mesures,
/// mêmes boîtes.
fn connected_components(
    binary: &[bool],
    gray: &[u8],
    width: usize,
    height: usize,
) -> Vec<ComponentAcc> {
    if binary.len() != width * height || gray.len() != width * height {
        return Vec::new();
    }
    // Passe 1 : segments par ligne, unions inter-lignes par recouvrement.
    let mut parent: Vec<usize> = vec![0]; // sentinelle 0 = fond
    // (début, fin_exclue, étiquette), triés par construction.
    let mut prev: Vec<(usize, usize, usize)> = Vec::new();
    let mut rows: Vec<Vec<(usize, usize, usize)>> = Vec::with_capacity(height);
    for y in 0..height {
        let base = y * width;
        let mut cur: Vec<(usize, usize, usize)> = Vec::new();
        let mut x = 0;
        while x < width {
            if !binary[base + x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && binary[base + x] {
                x += 1;
            }
            cur.push((start, x, 0));
        }
        // Balayage fusionné : les deux listes sont triées.
        let mut j = 0;
        for run in cur.iter_mut() {
            while j < prev.len() && prev[j].1 <= run.0 {
                j += 1;
            }
            let mut k = j;
            let mut first: Option<usize> = None;
            while k < prev.len() && prev[k].0 < run.1 {
                let pl = prev[k].2;
                match first {
                    None => first = Some(pl),
                    Some(f) => {
                        let (rf, rp) = (dsu_find(&mut parent, f), dsu_find(&mut parent, pl));
                        if rf != rp {
                            parent[rp] = rf;
                        }
                    }
                }
                k += 1;
            }
            run.2 = match first {
                Some(f) => f,
                None => {
                    parent.push(parent.len());
                    parent.len() - 1
                }
            };
        }
        rows.push(cur);
        prev = rows[rows.len() - 1].clone();
    }
    // Passe 2 : résolution + accumulation (par segment, jamais par pixel
    // sauf la somme de luminance, boucle serrée sans branchement).
    let mut acc: std::collections::HashMap<usize, ComponentAcc> = std::collections::HashMap::new();
    for (y, row) in rows.iter().enumerate() {
        for &(start, end, label) in row {
            let root = dsu_find(&mut parent, label);
            let sum: u64 = gray[y * width + start..y * width + end]
                .iter()
                .map(|&v| v as u64)
                .sum();
            let count = end - start;
            acc.entry(root)
                .and_modify(|c| {
                    c.min_x = c.min_x.min(start);
                    c.min_y = c.min_y.min(y);
                    c.max_x = c.max_x.max(end - 1);
                    c.max_y = c.max_y.max(y);
                    c.count += count;
                    c.sum_lum += sum;
                })
                .or_insert(ComponentAcc {
                    min_x: start,
                    min_y: y,
                    max_x: end - 1,
                    max_y: y,
                    count,
                    sum_lum: sum,
                });
        }
    }
    acc.into_values().collect()
}

/// Score géométrique 100 % fonction de mesures :
/// contraste local (|luminance moyenne région − moyenne globale| / 255,
/// conversion d'unité, pas un seuil), taille (`√aire_relative`, mathématique
/// exacte) et centralité (distance du centre région au centre image
/// normalisée par la demi-diagonale exacte).
///
/// Pondérations 0.5/0.3/0.2 : paramètre de calibrage documenté — la valeur
/// du score varie pour chaque entrée, elle n'est jamais fixée.
fn region_score(region_mean: f32, global_mean: f32, area_rel: f32, cx: f32, cy: f32) -> f32 {
    let contrast = ((region_mean - global_mean).abs() / 255.0).clamp(0.0, 1.0);
    let size = area_rel.clamp(0.0, 1.0).sqrt();
    let dx = cx - 0.5;
    let dy = cy - 0.5;
    // Demi-diagonale du carré unité : sqrt(0.5). Géométrie exacte.
    let centrality =
        (1.0 - (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2 * 2.0).clamp(0.0, 1.0);
    (0.5 * contrast + 0.3 * size + 0.2 * centrality).clamp(0.0, 1.0)
}

fn acc_to_box(acc: &ComponentAcc, width: usize, height: usize, global_mean: f32) -> DetectedBox {
    let x_min = acc.min_x as f32 / width as f32;
    let y_min = acc.min_y as f32 / height as f32;
    // Bornes exclusives → +1 pour couvrir le dernier pixel.
    let x_max = (acc.max_x as f32 + 1.0) / width as f32;
    let y_max = (acc.max_y as f32 + 1.0) / height as f32;
    let area_rel = acc.count as f32 / (width * height) as f32;
    let region_mean = acc.sum_lum as f32 / acc.count as f32;
    let cx = (x_min + x_max) / 2.0;
    let cy = (y_min + y_max) / 2.0;
    DetectedBox {
        x_min,
        y_min,
        x_max: x_max.min(1.0),
        y_max: y_max.min(1.0),
        area_rel,
        score: region_score(region_mean, global_mean, area_rel, cx, cy),
        pixels: acc.count,
    }
}

/// Segmentation RÉELLE : binarisation d'Otsu + composantes connexes sur la
/// carte **et** son inverse (éléments clairs sur fond sombre **et** sombres
/// sur fond clair), boîtes normalisées triées par aire décroissante.
///
/// Rejet du fond : toute composante couvrant plus de 90 % de l'image est
/// considérée comme masse de fond (mur, fond d'écran uni) — paramètre de
/// calibrage documenté, pas une mesure. Image vide/uniforme → `[]`.
pub fn detect_regions(gray: &[u8], width: u32, height: u32) -> Vec<DetectedBox> {
    if gray.is_empty() || width == 0 || height == 0 {
        return Vec::new();
    }
    let w = width as usize;
    let h = height as usize;
    if gray.len() != w * h {
        return Vec::new();
    }
    // Paramètre de calibrage documenté : au-delà, c'est le fond, pas un élément.
    const BACKGROUND_REJECTION_REL_AREA: f32 = 0.90;
    let (global_mean, _) = luminance_stats(gray);
    let thresh = otsu_threshold(gray);
    let fg = binarize(gray, thresh);
    let mut boxes: Vec<DetectedBox> = Vec::new();
    // Foreground + polarité inverse (deux passes réelles, pas de duplication de score).
    let polarities: [Vec<bool>; 2] = [fg, gray.iter().map(|&v| v <= thresh).collect()];
    for binary in &polarities {
        for acc in connected_components(binary, gray, w, h) {
            let area_rel = acc.count as f32 / (w * h) as f32;
            if area_rel > BACKGROUND_REJECTION_REL_AREA {
                continue;
            }
            boxes.push(acc_to_box(&acc, w, h, global_mean));
        }
    }
    // Déduplication des boîtes quasi-identiques issues des deux polarités
    // (recouvrement IoU > 0.9 mesuré → garde le meilleur score).
    let mut kept: Vec<DetectedBox> = Vec::new();
    'outer: for b in boxes {
        for k in &mut kept {
            if iou(&b, k) > 0.9 {
                if b.score > k.score {
                    *k = b.clone();
                }
                continue 'outer;
            }
        }
        kept.push(b);
    }
    kept.sort_by(|a, b| {
        b.area_rel
            .partial_cmp(&a.area_rel)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    kept
}

fn iou(a: &DetectedBox, b: &DetectedBox) -> f32 {
    let ix0 = a.x_min.max(b.x_min);
    let iy0 = a.y_min.max(b.y_min);
    let ix1 = a.x_max.min(b.x_max);
    let iy1 = a.y_max.min(b.y_max);
    if ix1 <= ix0 || iy1 <= iy0 {
        return 0.0;
    }
    let inter = (ix1 - ix0) * (iy1 - iy0);
    let ua = (a.x_max - a.x_min) * (a.y_max - a.y_min) + (b.x_max - b.x_min) * (b.y_max - b.y_min)
        - inter;
    if ua <= 0.0 { 0.0 } else { inter / ua }
}

/// Magnitude de Sobel (|Gx| + |Gy|, approximation entière exacte aux bords
/// repliés) pour chaque pixel intérieur ; bordures à 0.
pub fn sobel_magnitude(gray: &[u8], width: usize, height: usize) -> Vec<u16> {
    let mut mag = vec![0u16; width * height];
    if gray.len() != width * height || width < 3 || height < 3 {
        return mag;
    }
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let at = |dx: isize, dy: isize| {
                gray[((y as isize + dy) as usize) * width + ((x as isize + dx) as usize)] as i32
            };
            let gx = -at(-1, -1) - 2 * at(-1, 0) - at(-1, 1) + at(1, -1) + 2 * at(1, 0) + at(1, 1);
            let gy = -at(-1, -1) - 2 * at(0, -1) - at(1, -1) + at(-1, 1) + 2 * at(0, 1) + at(1, 1);
            mag[y * width + x] = (gx.abs() + gy.abs()) as u16;
        }
    }
    mag
}

/// Statistiques de contours : densité = fraction de pixels dont la magnitude
/// dépasse `moyenne + écart-type` (seuil adaptatif mesuré sur l'image même).
#[derive(Debug, Clone)]
pub struct EdgeStats {
    pub density: f32,
    pub mean_magnitude: f32,
    pub std_magnitude: f32,
    pub edge_pixels: usize,
}

pub fn edge_stats(gray: &[u8], width: u32, height: u32) -> EdgeStats {
    let w = width as usize;
    let h = height as usize;
    if gray.is_empty() || gray.len() != w * h {
        return EdgeStats {
            density: 0.0,
            mean_magnitude: 0.0,
            std_magnitude: 0.0,
            edge_pixels: 0,
        };
    }
    let mag = sobel_magnitude(gray, w, h);
    let n = mag.len() as f32;
    let mean = mag.iter().map(|&v| v as f32).sum::<f32>() / n;
    let var = mag.iter().map(|&v| (v as f32 - mean).powi(2)).sum::<f32>() / n;
    let std = var.sqrt();
    let thresh = mean + std;
    let edge_pixels = mag.iter().filter(|&&v| v as f32 > thresh).count();
    EdgeStats {
        density: edge_pixels as f32 / n,
        mean_magnitude: mean,
        std_magnitude: std,
        edge_pixels,
    }
}

/// Densité de contours dans un sous-rectangle normalisé [0,1] (mesure locale
/// réutilisant la carte de Sobel déjà calculée).
pub fn edge_density_in_rect(
    sobel: &[u16],
    width: usize,
    height: usize,
    threshold: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
) -> f32 {
    if sobel.len() != width * height || width == 0 || height == 0 {
        return 0.0;
    }
    let xa = (x0.clamp(0.0, 1.0) * width as f32) as usize;
    let xb = ((x1.clamp(0.0, 1.0) * width as f32) as usize)
        .max(xa + 1)
        .min(width);
    let ya = (y0.clamp(0.0, 1.0) * height as f32) as usize;
    let yb = ((y1.clamp(0.0, 1.0) * height as f32) as usize)
        .max(ya + 1)
        .min(height);
    let (mut edges, mut total) = (0usize, 0usize);
    for y in ya..yb {
        for x in xa..xb {
            total += 1;
            if sobel[y * width + x] as f32 > threshold {
                edges += 1;
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        edges as f32 / total as f32
    }
}

/// Régions candidates « probablement textuelles » : fort gradient horizontal
/// **dense** (lignes de texte = traits horizontaux rapprochés).
///
/// Pipeline réel : |Gx| → flou moyen horizontal (agrégation de densité) →
/// seuil adaptatif (moyenne + écart-type mesurés) → composantes connexes →
/// filtre géométrique (largeur ≥ 2× hauteur : a priori de forme documenté,
/// pas une mesure).
///
/// **Honnêteté OCR : ces boîtes sont des candidats géométriques. Aucun
/// caractère n'est reconnu sans modèle OCR.**
pub fn detect_text_regions(gray: &[u8], width: u32, height: u32) -> Vec<DetectedBox> {
    let w = width as usize;
    let h = height as usize;
    if gray.is_empty() || gray.len() != w * h || w < 8 || h < 3 {
        return Vec::new();
    }
    // 1. Gradient horizontal |Gx| (noyau central [-1 0 1]).
    let mut gx = vec![0u16; w * h];
    for y in 0..h {
        for x in 1..w - 1 {
            let d = (gray[y * w + x + 1] as i32 - gray[y * w + x - 1] as i32).abs();
            gx[y * w + x] = d as u16;
        }
    }
    // 2. Densité : flou moyen horizontal, rayon 6 (paramètre de forme
    //    documenté : agglomère les traits d'une même ligne de texte).
    const BLUR_RADIUS: usize = 6;
    let mut dense = vec![0f32; w * h];
    for y in 0..h {
        // Fenêtre glissante [lo, hi] : ajouts/retraits monotones (pas de
        // soustraction de pixel jamais ajouté) ; normalisation par la largeur
        // réelle de fenêtre (bords gérés exactement).
        let mut window_sum: u32 = 0;
        let mut prev_lo = 0usize;
        let mut prev_hi: Option<usize> = None;
        for x in 0..w {
            let lo = x.saturating_sub(BLUR_RADIUS);
            let hi = (x + BLUR_RADIUS).min(w - 1);
            match prev_hi {
                None => {
                    for i in lo..=hi {
                        window_sum += gx[y * w + i] as u32;
                    }
                }
                Some(ph) => {
                    for i in (ph + 1)..=hi {
                        window_sum += gx[y * w + i] as u32;
                    }
                    for i in prev_lo..lo {
                        window_sum -= gx[y * w + i] as u32;
                    }
                }
            }
            prev_lo = lo;
            prev_hi = Some(hi);
            dense[y * w + x] = window_sum as f32 / (hi - lo + 1) as f32;
        }
    }
    // 3. Seuil adaptatif mesuré sur la carte de densité.
    let n = dense.len() as f32;
    let mean = dense.iter().sum::<f32>() / n;
    let var = dense.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
    let thresh = mean + var.sqrt();
    let binary: Vec<bool> = dense.iter().map(|&v| v > thresh).collect();
    // 4. Composantes + filtre géométrique (a priori documenté).
    let (global_mean, _) = luminance_stats(gray);
    let mut out = Vec::new();
    for acc in connected_components(&binary, gray, w, h) {
        let bw = (acc.max_x - acc.min_x + 1) as f32;
        let bh = (acc.max_y - acc.min_y + 1) as f32;
        if bw < 2.0 * bh {
            continue; // pas une forme de ligne de texte
        }
        let area_rel = acc.count as f32 / (w * h) as f32;
        if area_rel > 0.5 {
            continue; // masse de fond, pas une ligne
        }
        out.push(acc_to_box(&acc, w, h, global_mean));
    }
    out.sort_by(|a: &DetectedBox, b: &DetectedBox| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Résultat de template matching : boîte en pixels image + score NCC.
#[derive(Debug, Clone)]
pub struct TemplateHit {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Corrélation croisée normalisée dans [-1, 1] (1.0 = identique).
    /// Gabarit uniforme (variance nulle) → NCC indéfini → repli documenté
    /// sur similarité de luminance moyenne dans [0,1].
    pub score: f32,
    /// Échelle du gabarit ayant donné le meilleur score (1.0 = taille native).
    pub scale: f32,
    /// `true` si le score vient du repli « gabarit uniforme », pas du NCC.
    pub uniform_fallback: bool,
}

/// Réduit une image en niveaux de gris par plus proche voisin (facteur < 1).
fn downscale_gray(gray: &[u8], w: usize, h: usize, scale: f32) -> (Vec<u8>, usize, usize) {
    let nw = ((w as f32 * scale).round() as usize).max(1);
    let nh = ((h as f32 * scale).round() as usize).max(1);
    let mut out = vec![0u8; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let sx = ((x as f32 / scale).floor() as usize).min(w - 1);
            let sy = ((y as f32 / scale).floor() as usize).min(h - 1);
            out[y * nw + x] = gray[sy * w + sx];
        }
    }
    (out, nw, nh)
}

/// NCC exhaustif mono-échelle par images intégrales (moyenne/variance O(1)
/// par position). Retourne le meilleur hit à cette échelle, ou `None` si le
/// gabarit ne tient pas dans l'image.
fn ncc_single_scale(
    image: &[u8],
    iw: usize,
    ih: usize,
    tmpl: &[u8],
    tw: usize,
    th: usize,
) -> Option<TemplateHit> {
    if tw == 0 || th == 0 || tw > iw || th > ih {
        return None;
    }
    if tmpl.len() != tw * th || image.len() != iw * ih {
        return None;
    }
    // Statistiques du gabarit (mesurées).
    let n = (tw * th) as f64;
    let t_sum: f64 = tmpl.iter().map(|&v| v as f64).sum();
    let t_sq: f64 = tmpl.iter().map(|&v| (v as f64) * (v as f64)).sum();
    let t_mean = t_sum / n;
    let t_var = (t_sq - t_sum * t_sum / n) / n;
    if t_var <= 1e-9 {
        // Gabarit uniforme : NCC indéfini (division par zéro).
        // Repli honnête : similarité de luminance moyenne.
        let mut best = TemplateHit {
            x: 0,
            y: 0,
            w: tw as u32,
            h: th as u32,
            score: -1.0,
            scale: 1.0,
            uniform_fallback: true,
        };
        for y in 0..=ih - th {
            for x in 0..=iw - tw {
                let mut s = 0u64;
                for dy in 0..th {
                    let row = (y + dy) * iw + x;
                    for dx in 0..tw {
                        s += image[row + dx] as u64;
                    }
                }
                let m = s as f64 / n;
                let score = (1.0 - ((m - t_mean).abs() / 255.0)) as f32;
                if score > best.score {
                    best.score = score;
                    best.x = x as u32;
                    best.y = y as u32;
                }
            }
        }
        return Some(best);
    }
    let t_std = t_var.sqrt();
    // Images intégrales (somme et somme des carrés).
    let sw = iw + 1;
    let mut integ = vec![0f64; sw * (ih + 1)];
    let mut integ_sq = vec![0f64; sw * (ih + 1)];
    for y in 0..ih {
        let mut row_sum = 0f64;
        let mut row_sq = 0f64;
        for x in 0..iw {
            let v = image[y * iw + x] as f64;
            row_sum += v;
            row_sq += v * v;
            integ[(y + 1) * sw + (x + 1)] = integ[y * sw + (x + 1)] + row_sum;
            integ_sq[(y + 1) * sw + (x + 1)] = integ_sq[y * sw + (x + 1)] + row_sq;
        }
    }
    let rect_sum = |integ: &[f64], x: usize, y: usize| -> f64 {
        integ[(y + th) * sw + (x + tw)] - integ[y * sw + (x + tw)] - integ[(y + th) * sw + x]
            + integ[y * sw + x]
    };
    let mut best: Option<(f64, usize, usize)> = None;
    for y in 0..=ih - th {
        for x in 0..=iw - tw {
            let s = rect_sum(&integ, x, y);
            let sq = rect_sum(&integ_sq, x, y);
            let i_mean = s / n;
            let i_var = ((sq - s * s / n) / n).max(0.0);
            let i_std = i_var.sqrt();
            if i_std <= 1e-9 {
                continue; // fenêtre image uniforme : corrélation indéfinie
            }
            // Covariance croisée (somme des produits centrés).
            let mut cross = 0f64;
            for dy in 0..th {
                let img_row = (y + dy) * iw + x;
                let t_row = dy * tw;
                for dx in 0..tw {
                    cross +=
                        (image[img_row + dx] as f64 - i_mean) * (tmpl[t_row + dx] as f64 - t_mean);
                }
            }
            let score = cross / (n * i_std * t_std);
            if best.map(|(b, _, _)| score > b).unwrap_or(true) {
                best = Some((score, x, y));
            }
        }
    }
    best.map(|(score, x, y)| TemplateHit {
        x: x as u32,
        y: y as u32,
        w: tw as u32,
        h: th as u32,
        score: (score as f32).clamp(-1.0, 1.0),
        scale: 1.0,
        uniform_fallback: false,
    })
}

/// Localisation de gabarit par NCC multi-échelle : le gabarit est cherché à
/// sa taille native puis réduit (×0.75, ×0.5 — échelles documentées, couvrant
/// les rendus plus petits) ; le meilleur score toutes échelles gagne.
///
/// Coordonnées et taille retournées exprimées en pixels de l'image d'origine.
pub fn locate_template(
    image: &[u8],
    image_w: usize,
    image_h: usize,
    template: &[u8],
    template_w: usize,
    template_h: usize,
) -> Option<TemplateHit> {
    // Échelles documentées : natif + deux réductions (rendus plus petits).
    const SCALES: [f32; 3] = [1.0, 0.75, 0.5];
    if image.is_empty() || template.is_empty() {
        return None;
    }
    let mut best: Option<TemplateHit> = None;
    for &scale in &SCALES {
        let (t_scaled, tw, th) = if scale == 1.0 {
            (template.to_vec(), template_w, template_h)
        } else {
            downscale_gray(template, template_w, template_h, scale)
        };
        if tw == 0 || th == 0 || tw > image_w || th > image_h {
            continue;
        }
        if let Some(mut hit) = ncc_single_scale(image, image_w, image_h, &t_scaled, tw, th) {
            hit.scale = scale;
            // En cas d'égalité de repli uniforme, préférer l'échelle native.
            let better = match &best {
                None => true,
                Some(b) => hit.score > b.score || (hit.score == b.score && scale > b.scale),
            };
            if better {
                best = Some(hit);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Image synthétique : rectangle blanc sur fond noir (aucun screenshot OS).
    fn synthetic_rect(
        w: usize,
        h: usize,
        rx0: usize,
        ry0: usize,
        rx1: usize,
        ry1: usize,
    ) -> Vec<u8> {
        let mut gray = vec![0u8; w * h];
        for y in ry0..ry1.min(h) {
            for x in rx0..rx1.min(w) {
                gray[y * w + x] = 255;
            }
        }
        gray
    }

    #[test]
    fn test_detect_rectangle_bbox_epsilon() {
        let (w, h) = (160u32, 120u32);
        let (rx0, ry0, rx1, ry1) = (40usize, 30usize, 120usize, 90usize);
        let gray = synthetic_rect(w as usize, h as usize, rx0, ry0, rx1, ry1);
        let t0 = std::time::Instant::now();
        let regions = detect_regions(&gray, w, h);
        let dt = t0.elapsed();
        eprintln!("detect_regions 160x120: {:?}", dt);
        assert!(!regions.is_empty(), "le rectangle doit être détecté");
        let (ex0, ey0, ex1, ey1) = (
            rx0 as f32 / w as f32,
            ry0 as f32 / h as f32,
            rx1 as f32 / w as f32,
            ry1 as f32 / h as f32,
        );
        let eps = 0.02;
        let found = regions.iter().any(|b| {
            (b.x_min - ex0).abs() <= eps
                && (b.y_min - ey0).abs() <= eps
                && (b.x_max - ex1).abs() <= eps
                && (b.y_max - ey1).abs() <= eps
        });
        assert!(
            found,
            "bbox attendue ~({ex0:.3},{ey0:.3},{ex1:.3},{ey1:.3}), obtenues: {:?}",
            regions
                .iter()
                .map(|b| (b.x_min, b.y_min, b.x_max, b.y_max, b.score))
                .collect::<Vec<_>>()
        );
        // Tri par aire décroissante vérifié.
        for pair in regions.windows(2) {
            assert!(pair[0].area_rel >= pair[1].area_rel);
        }
    }

    #[test]
    fn test_empty_and_uniform_yield_no_boxes() {
        assert!(detect_regions(&vec![0u8; 64 * 64], 64, 64).is_empty());
        assert!(detect_regions(&vec![140u8; 64 * 64], 64, 64).is_empty());
        assert!(detect_regions(&[], 0, 0).is_empty());
        assert!(detect_template_empty());
    }

    fn detect_template_empty() -> bool {
        locate_template(&vec![0u8; 16], 4, 4, &[], 0, 0).is_none()
    }

    #[test]
    fn test_ncc_identical_template_scores_one() {
        // Gabarit structuré (croix) découpé dans l'image même.
        let (w, h) = (64usize, 48usize);
        let mut img = vec![20u8; w * h];
        for y in 10..30 {
            for x in 8..40 {
                img[y * w + x] = 200;
            }
        }
        for y in 14..26 {
            for x in 20..28 {
                img[y * w + x] = 40;
            }
        }
        // Gabarit = crop exact incluant fond + motif (variance non nulle).
        let (tx, ty, tw, th) = (4usize, 6usize, 40usize, 28usize);
        let mut tmpl = vec![0u8; tw * th];
        for dy in 0..th {
            for dx in 0..tw {
                tmpl[dy * tw + dx] = img[(ty + dy) * w + (tx + dx)];
            }
        }
        let hit = locate_template(&img, w, h, &tmpl, tw, th)
            .expect("le gabarit découpé doit être retrouvé");
        eprintln!("NCC hit: {:?} (attendu x=4,y=6,score~1.0)", hit);
        assert_eq!(hit.x, tx as u32);
        assert_eq!(hit.y, ty as u32);
        assert!(
            (hit.score - 1.0).abs() < 1e-4,
            "score NCC={} attendu ~1.0",
            hit.score
        );
        assert!(!hit.uniform_fallback);
    }

    #[test]
    fn test_text_candidates_honest() {
        // Barre horizontale blanche = fort gradient horizontal dense.
        let (w, h) = (80u32, 40u32);
        let mut gray = vec![0u8; 80 * 40];
        for y in 18..22 {
            for x in 5..75 {
                gray[y as usize * 80 + x] = 255;
            }
        }
        let cands = detect_text_regions(&gray, w, h);
        assert!(
            !cands.is_empty(),
            "la barre doit produire un candidat texte"
        );
        assert!(detect_text_regions(&vec![0u8; 80 * 40], w, h).is_empty());
    }

    #[test]
    fn test_perf_1080p_under_2s() {
        // 1080p synthétique de type UI (fond + fenêtre + boutons + barres de
        // texte, ~20 régions : charge de travail représentative, pas d'OS).
        let (w, h) = (1920u32, 1080u32);
        let mut gray = vec![0u8; 1920 * 1080];
        for y in 0..1080 {
            let bg = 30 + (y * 20 / 1080) as u8;
            for x in 0..1920 {
                gray[y * 1920 + x] = bg;
            }
        }
        // Fenêtre claire centrée.
        for y in 200..880 {
            for x in 360..1560 {
                gray[y * 1920 + x] = 210;
            }
        }
        // Barre de titre + boutons sombres.
        for y in 200..248 {
            for x in 360..1560 {
                gray[y * 1920 + x] = 60;
            }
        }
        for b in 0..3 {
            for y in 300..340 {
                for x in (500 + b * 220)..(660 + b * 220) {
                    gray[y * 1920 + x] = 90;
                }
            }
        }
        // Lignes de texte (barres sombres sur fond clair).
        for line in 0..8 {
            for y in (420 + line * 48)..(436 + line * 48) {
                for x in 420..1100 {
                    gray[y * 1920 + x] = 40;
                }
            }
        }
        let t0 = std::time::Instant::now();
        let regions = detect_regions(&gray, w, h);
        let dt_regions = t0.elapsed();
        let t1 = std::time::Instant::now();
        let edges = edge_stats(&gray, w, h);
        let dt_edges = t1.elapsed();
        eprintln!(
            "PERF 1080p: detect_regions={:?} ({} régions), edge_stats={:?} (densité={:.4})",
            dt_regions,
            regions.len(),
            dt_edges,
            edges.density
        );
        assert!(!regions.is_empty());
        // La fenêtre (39 % de la surface) doit ressortir comme masse principale.
        assert!(
            regions.iter().any(|b| b.area_rel > 0.2),
            "fenêtre 1080p non détectée parmi {} régions",
            regions.len()
        );
        assert!(
            dt_regions.as_secs_f32() < 2.0,
            "detect_regions 1080p trop lent: {:?}",
            dt_regions
        );
        assert!(
            dt_edges.as_secs_f32() < 2.0,
            "edge_stats 1080p trop lent: {:?}",
            dt_edges
        );
    }
}
