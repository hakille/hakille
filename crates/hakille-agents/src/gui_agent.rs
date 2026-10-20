use hakille_inference::GgufModel;
use hakille_substrate::{InputAction, Key, MouseButton, ScrollDirection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuiIntention {
    Click {
        x: f32, // normalized 0.0 to 1.0
        y: f32,
        button: MouseButton,
    },
    DoubleClick {
        x: f32,
        y: f32,
    },
    Type {
        text: String,
    },
    Hotkey {
        keys: Vec<Key>,
    },
    Scroll {
        direction: ScrollDirection,
        amount: i32,
    },
    Drag {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    },
    Wait {
        duration_ms: u64,
    },
    Terminate {
        success: bool,
        summary: String,
    },
}

pub struct UiTarsAgent {
    pub agent_name: String,
    pub model_metadata: Option<GgufModel>,
}

impl UiTarsAgent {
    pub fn new() -> Self {
        Self {
            agent_name: "UI-TARS-2B-SFT".to_string(),
            model_metadata: None,
        }
    }

    pub fn with_gguf_file(path: impl AsRef<Path>) -> hakille_core::Result<Self> {
        let model = GgufModel::read_from_file(path)?;
        Ok(Self::with_gguf_model(model))
    }

    pub fn with_gguf_model(model: GgufModel) -> Self {
        Self {
            agent_name: model.name().unwrap_or("UI-TARS-2B-SFT").to_string(),
            model_metadata: Some(model),
        }
    }

    pub fn is_model_connected(&self) -> bool {
        self.model_metadata.is_some()
    }

    pub fn tensor_count(&self) -> usize {
        self.model_metadata
            .as_ref()
            .map(|m| m.tensor_count())
            .unwrap_or(0)
    }

    /// Parseur lexical d'intention (comprendre « clique » = Click, « écris » =
    /// Type… : c'est du parsing, pas de la simulation).
    ///
    /// HONNÊTETÉ : sans capture d'écran, aucune coordonnée ne peut être
    /// mesurée. Le Click retourné porte un prior neutre (0.5, 0.5) explicite,
    /// **interdit d'exécution tel quel** : toute coordonnée exécutée DOIT
    /// provenir de [`Self::anchor_intentions_on_regions`] (régions mesurées)
    /// ou de [`Self::predict_grounded_on_frame`] (mesure embarquée).
    /// Les anciennes variantes directionnelles (top/bottom/left/right →
    /// 0.15/0.85) ont été supprimées : c'étaient des coordonnées déguisées.
    pub fn predict_intent_from_task(&self, task_description: &str) -> Vec<GuiIntention> {
        let task_lower = task_description.to_lowercase();
        if task_lower.contains("click") {
            // Prior neutre NON mesuré — ancrage obligatoire avant exécution.
            vec![GuiIntention::Click {
                x: 0.5,
                y: 0.5,
                button: MouseButton::Left,
            }]
        } else if task_lower.contains("type") || task_lower.contains("write") {
            let text = if let Some(idx) = task_description.find(':') {
                task_description[idx + 1..].trim().to_string()
            } else {
                "Hello from Hakille GUI Agent".to_string()
            };
            vec![GuiIntention::Type { text }]
        } else if task_lower.contains("scroll") {
            let direction = if task_lower.contains("up") {
                ScrollDirection::Up
            } else {
                ScrollDirection::Down
            };
            vec![GuiIntention::Scroll {
                direction,
                amount: 3,
            }]
        } else {
            vec![GuiIntention::Wait { duration_ms: 100 }]
        }
    }

    /// Converts normalized GUI intention into physical Substrate input action
    ///
    /// bornes fail-closed :
    /// - `x`, `y` clampés dans `[0,1]` (coordonnées normalisées) ;
    /// - écran nul (`screen_w == 0 || screen_h == 0`) → `None`
    ///   (voir `to_input_action_checked` pour la variante `Err`).
    pub fn to_input_action(
        &self,
        intention: &GuiIntention,
        screen_w: u32,
        screen_h: u32,
    ) -> Option<InputAction> {
        self.to_input_action_checked(intention, screen_w, screen_h, true)
            .unwrap_or(None)
    }

    /// Variante stricte :
    /// - `screen_w == 0 || screen_h == 0` → `Err` (dimension invalide) ;
    /// - `x`, `y` clampés dans `[0,1]` ;
    /// - `Type` / `Hotkey` : si `require_confirm == false` et contenu
    ///   sensible (`password` / `token`, insensible à la casse) → `Err`.
    ///   `require_confirm == true` signifie « validation chip obtenue ».
    pub fn to_input_action_checked(
        &self,
        intention: &GuiIntention,
        screen_w: u32,
        screen_h: u32,
        require_confirm: bool,
    ) -> hakille_core::Result<Option<InputAction>> {
        if screen_w == 0 || screen_h == 0 {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "Dimensions d'écran invalides (screen 0 refusé)".to_string(),
            ));
        }
        // Détection de contenu sensible pour Type.
        if let GuiIntention::Type { text } = intention
            && !require_confirm
            && Self::looks_sensitive(text)
        {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "Saisie sensible (password/token) exige une validation chip (require_confirm=true)"
                    .to_string(),
            ));
        }
        // Hotkey : action potentiellement destructive → exige confirmation
        // explicite quand require_confirm == false.
        if matches!(intention, GuiIntention::Hotkey { .. }) && !require_confirm {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "Hotkey exige une validation chip (require_confirm=true)".to_string(),
            ));
        }
        let clamp01 = |v: f32| v.clamp(0.0, 1.0);
        match intention {
            GuiIntention::Click { x, y, button } => {
                let cx = clamp01(*x);
                let cy = clamp01(*y);
                let px = (cx * screen_w as f32) as i32;
                let py = (cy * screen_h as f32) as i32;
                Ok(Some(InputAction::Click {
                    x: px,
                    y: py,
                    button: *button,
                }))
            }
            GuiIntention::DoubleClick { x, y } => {
                let cx = clamp01(*x);
                let cy = clamp01(*y);
                let px = (cx * screen_w as f32) as i32;
                let py = (cy * screen_h as f32) as i32;
                Ok(Some(InputAction::DoubleClick { x: px, y: py }))
            }
            GuiIntention::Type { text } => Ok(Some(InputAction::Type { text: text.clone() })),
            GuiIntention::Hotkey { keys } => Ok(Some(InputAction::Hotkey { keys: keys.clone() })),
            GuiIntention::Scroll { direction, amount } => Ok(Some(InputAction::Scroll {
                direction: *direction,
                amount: *amount,
            })),
            GuiIntention::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                let fx = (clamp01(*from_x) * screen_w as f32) as i32;
                let fy = (clamp01(*from_y) * screen_h as f32) as i32;
                let tx = (clamp01(*to_x) * screen_w as f32) as i32;
                let ty = (clamp01(*to_y) * screen_h as f32) as i32;
                Ok(Some(InputAction::Drag {
                    from_x: fx,
                    from_y: fy,
                    to_x: tx,
                    to_y: ty,
                }))
            }
            GuiIntention::Wait { .. } | GuiIntention::Terminate { .. } => Ok(None),
        }
    }

    /// Heuristique sensible : contient `password` ou `token` (casse ignorée).
    fn looks_sensitive(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("password") || lower.contains("token")
    }

    /// Executes GUI intention natively on the host OS via Substrate
    /// (compat : `require_confirm = true`, clamp + screen 0 → `None`→No-op).
    pub async fn execute_intention(
        &self,
        substrate: &impl hakille_substrate::Substrate,
        intention: &GuiIntention,
        cap: &hakille_core::CapabilityToken,
        screen_w: u32,
        screen_h: u32,
    ) -> hakille_core::Result<String> {
        self.execute_intention_with_confirm(substrate, intention, cap, screen_w, screen_h, true)
            .await
    }

    /// Variante stricte avec `require_confirm` :
    /// - écran 0 → `Err` ;
    /// - `Type` sensible / `Hotkey` sans `require_confirm` → `Err` fail-closed.
    pub async fn execute_intention_with_confirm(
        &self,
        substrate: &impl hakille_substrate::Substrate,
        intention: &GuiIntention,
        cap: &hakille_core::CapabilityToken,
        screen_w: u32,
        screen_h: u32,
        require_confirm: bool,
    ) -> hakille_core::Result<String> {
        let action_opt =
            self.to_input_action_checked(intention, screen_w, screen_h, require_confirm)?;
        if let Some(action) = action_opt {
            substrate.inject_input(action, cap).await?;
            Ok(format!("Executed native GUI action: {:?}", intention))
        } else {
            match intention {
                GuiIntention::Wait { duration_ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*duration_ms)).await;
                    Ok(format!("Waited {}ms", duration_ms))
                }
                GuiIntention::Terminate { success, summary } => {
                    Ok(format!("Terminated (success={}): {}", success, summary))
                }
                _ => Ok("No-op".to_string()),
            }
        }
    }

    /// Ancien chemin « neuronal » : aucun décodeur VLM n'est chargé, donc
    /// aucune coordonnée prédite par modèle ne peut être produite. Conservé
    /// comme shim de compatibilité : délègue au parseur lexical, dont le Click
    /// porte un prior neutre explicite (à ancrer sur régions mesurées).
    #[deprecated(
        note = "no neural GUI decoding available; use predict_intent_from_task + anchor_intentions_on_regions"
    )]
    pub fn predict_intentions_neural(
        &self,
        task_description: &str,
        _screen_w: u32,
        _screen_h: u32,
    ) -> Vec<GuiIntention> {
        self.predict_intent_from_task(task_description)
    }
}

/// Région UI mesurée (miroir de `hakille-perception::cv::DetectedBox`).
///
/// La crate `hakille-agents` ne dépend pas de `hakille-perception` (Cargo
/// gelé) : l'appelant achemine ici les régions mesurées par `cv.rs`
/// (screenshot → `detect_regions` → ces boîtes), ou utilise
/// [`UiTarsAgent::predict_grounded_on_frame`] (mesure embarquée, même
/// algorithme). Coordonnées normalisées [0,1], score mesuré [0,1].
#[derive(Debug, Clone)]
pub struct GroundedRegion {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
    pub score: f32,
}

impl GroundedRegion {
    pub fn center(&self) -> (f32, f32) {
        (
            (self.x_min + self.x_max) / 2.0,
            (self.y_min + self.y_max) / 2.0,
        )
    }
}

impl UiTarsAgent {
    /// Ancre une intention parsée sur des régions MESURÉES.
    ///
    /// - intentions à coordonnées (Click/Drag) : centre de la région au
    ///   meilleur score mesuré ; **aucune région → `Err` fail-closed**
    ///   (jamais de coordonnée inventée) ;
    /// - Type/Scroll/Wait : traversent telles quelles (aucune coordonnée
    ///   requise ; le parsing click/type/scroll reste un parseur d'intention).
    pub fn anchor_intentions_on_regions(
        &self,
        task_description: &str,
        regions: &[GroundedRegion],
    ) -> hakille_core::Result<Vec<GuiIntention>> {
        let task_lower = task_description.to_lowercase();
        if task_lower.contains("drag") {
            if regions.len() < 2 {
                return Err(hakille_core::HakilleError::Internal(
                    "grounding: drag exige 2 régions mesurées distinctes".to_string(),
                ));
            }
            let mut sorted: Vec<&GroundedRegion> = regions.iter().collect();
            sorted.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let (fx, fy) = sorted[0].center();
            let (tx, ty) = sorted[1].center();
            return Ok(vec![GuiIntention::Drag {
                from_x: fx.clamp(0.0, 1.0),
                from_y: fy.clamp(0.0, 1.0),
                to_x: tx.clamp(0.0, 1.0),
                to_y: ty.clamp(0.0, 1.0),
            }]);
        }
        if task_lower.contains("click") {
            let r = Self::best_region(regions)?;
            let (x, y) = r.center();
            return Ok(vec![GuiIntention::Click {
                x: x.clamp(0.0, 1.0),
                y: y.clamp(0.0, 1.0),
                button: MouseButton::Left,
            }]);
        }
        // Intentions sans coordonnées : parseur direct (pas de perception).
        Ok(self.predict_intent_from_task(task_description))
    }

    /// Prédit une intention ANCRÉE depuis une capture RGBA réelle.
    ///
    /// Pipeline : pixels → luminance → seuil d'Otsu mesuré → composantes
    /// connexes (union-find) → scores géométriques → ancrage. Écran vide ou
    /// sans structure → `Wait` honnête (rien à ancrer, on attend au lieu
    /// d'inventer) ; voir [`Self::anchor_intentions_on_regions`] pour la
    /// variante stricte `Err`.
    ///
    /// Note de duplication honnête : la mesure est réimplémentée ici en
    /// `std` pur car cette crate ne peut dépendre d'`hakille-perception`
    /// (Cargo gelé) ; l'algorithme est le miroir de `cv::detect_regions`.
    pub fn predict_grounded_on_frame(
        &self,
        task_description: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> hakille_core::Result<Vec<GuiIntention>> {
        let regions = Self::measure_regions_rgba(rgba, width, height);
        match self.anchor_intentions_on_regions(task_description, &regions) {
            Ok(intentions) => Ok(intentions),
            Err(_) if Self::needs_coordinates(task_description) => {
                // Wait honnête : aucune région mesurée, rien à cliquer.
                Ok(vec![GuiIntention::Wait { duration_ms: 500 }])
            }
            Err(e) => Err(e),
        }
    }

    fn needs_coordinates(task_description: &str) -> bool {
        let t = task_description.to_lowercase();
        t.contains("click") || t.contains("drag")
    }

    fn best_region(regions: &[GroundedRegion]) -> hakille_core::Result<&GroundedRegion> {
        regions
            .iter()
            .max_by(|a, b| {
                a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| {
                hakille_core::HakilleError::Internal(
                    "grounding: aucune région mesurée — intention à coordonnées refusée (fail-closed)".to_string(),
                )
            })
    }

    /// Mesure embarquée : Otsu + composantes connexes sur la luminance.
    /// Retourne des boîtes normalisées triées par score décroissant ;
    /// entrée vide/incohérente → `[]`.
    fn measure_regions_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<GroundedRegion> {
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let (w, h) = (width as usize, height as usize);
        let n = match w.checked_mul(h) {
            Some(n) => n,
            None => return Vec::new(),
        };
        if rgba.len() < n * 4 {
            return Vec::new();
        }
        // 1. Luminance mesurée.
        let mut gray = Vec::with_capacity(n);
        for px in rgba.chunks_exact(4).take(n) {
            gray.push(
                (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            );
        }
        // 2. Seuil d'Otsu mesuré sur l'histogramme.
        let mut hist = [0u64; 256];
        for &v in &gray {
            hist[v as usize] += 1;
        }
        let mut sum_all = 0f64;
        for (i, &c) in hist.iter().enumerate() {
            sum_all += i as f64 * c as f64;
        }
        let (mut sum_b, mut w_b, mut thresh, mut best) = (0f64, 0u64, 0u8, -1f64);
        for (t, &c) in hist.iter().enumerate() {
            w_b += c;
            if w_b == 0 {
                continue;
            }
            let w_f = n as u64 - w_b;
            if w_f == 0 {
                break;
            }
            sum_b += t as f64 * c as f64;
            let (m_b, m_f) = (sum_b / w_b as f64, (sum_all - sum_b) / w_f as f64);
            let var = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
            if var > best {
                best = var;
                thresh = t as u8;
            }
        }
        let (global_mean, _) = {
            let m: f32 = gray.iter().map(|&v| v as f32).sum::<f32>() / n as f32;
            let v: f32 = gray.iter().map(|&v| (v as f32 - m).powi(2)).sum::<f32>() / n as f32;
            (m, v.sqrt())
        };
        // 3. Composantes connexes, double polarité (union-find local).
        let mut out = Vec::new();
        for invert in [false, true] {
            let binary: Vec<bool> = gray
                .iter()
                .map(|&v| if invert { v <= thresh } else { v > thresh })
                .collect();
            let mut parent: Vec<usize> = (0..n).collect();
            fn find(parent: &mut [usize], mut x: usize) -> usize {
                let mut r = x;
                while parent[r] != r {
                    r = parent[r];
                }
                while parent[x] != r {
                    let nxt = parent[x];
                    parent[x] = r;
                    x = nxt;
                }
                r
            }
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if !binary[i] {
                        continue;
                    }
                    if x > 0 && binary[i - 1] {
                        let (a, b) = (find(&mut parent, i), find(&mut parent, i - 1));
                        if a != b {
                            parent[b] = a;
                        }
                    }
                    if y > 0 && binary[i - w] {
                        let (a, b) = (find(&mut parent, i), find(&mut parent, i - w));
                        if a != b {
                            parent[b] = a;
                        }
                    }
                }
            }
            use std::collections::HashMap;
            let mut acc: HashMap<usize, (usize, usize, usize, usize, usize, u64)> = HashMap::new();
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if !binary[i] {
                        continue;
                    }
                    let r = find(&mut parent, i);
                    acc.entry(r)
                        .and_modify(|e| {
                            e.0 = e.0.min(x);
                            e.1 = e.1.min(y);
                            e.2 = e.2.max(x);
                            e.3 = e.3.max(y);
                            e.4 += 1;
                            e.5 += gray[i] as u64;
                        })
                        .or_insert((x, y, x, y, 1, gray[i] as u64));
                }
            }
            for (x0, y0, x1, y1, count, sum) in acc.into_values() {
                let area_rel = count as f32 / n as f32;
                // Rejet du fond documenté (> 90 % = masse de fond).
                if area_rel > 0.90 {
                    continue;
                }
                let (bx0, by0, bx1, by1) = (
                    x0 as f32 / w as f32,
                    y0 as f32 / h as f32,
                    (x1 as f32 + 1.0) / w as f32,
                    (y1 as f32 + 1.0) / h as f32,
                );
                let region_mean = sum as f32 / count as f32;
                let contrast = ((region_mean - global_mean).abs() / 255.0).clamp(0.0, 1.0);
                let size = area_rel.clamp(0.0, 1.0).sqrt();
                let (cx, cy) = ((bx0 + bx1) / 2.0, (by0 + by1) / 2.0);
                let dx = cx - 0.5;
                let dy = cy - 0.5;
                let centrality = (1.0
                    - (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2 * 2.0)
                    .clamp(0.0, 1.0);
                out.push(GroundedRegion {
                    x_min: bx0,
                    y_min: by0,
                    x_max: bx1.min(1.0),
                    y_max: by1.min(1.0),
                    score: (0.5 * contrast + 0.3 * size + 0.2 * centrality).clamp(0.0, 1.0),
                });
            }
        }
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }
}

impl Default for UiTarsAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod grounded_tests {
    use super::*;

    /// Capture synthétique RGBA (aucun screenshot OS) : rectangle blanc sur fond noir.
    fn synthetic_rgba(w: u32, h: u32, rect: Option<(u32, u32, u32, u32)>) -> Vec<u8> {
        let mut data = vec![0u8; (w * h) as usize * 4];
        if let Some((x0, y0, x1, y1)) = rect {
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    data[i] = 255;
                    data[i + 1] = 255;
                    data[i + 2] = 255;
                    data[i + 3] = 255;
                }
            }
        }
        data
    }

    #[test]
    fn test_grounded_click_anchored_to_measured_region() {
        let agent = UiTarsAgent::new();
        // Rectangle (40,30)-(120,90) sur 160x120 → centre attendu (0.5, 0.5).
        let rgba = synthetic_rgba(160, 120, Some((40, 30, 120, 90)));
        let t0 = std::time::Instant::now();
        let intents = agent
            .predict_grounded_on_frame("Click on the Save button", &rgba, 160, 120)
            .unwrap();
        eprintln!("predict_grounded_on_frame 160x120: {:?}", t0.elapsed());
        assert_eq!(intents.len(), 1);
        match &intents[0] {
            GuiIntention::Click { x, y, .. } => {
                eprintln!("click ancré mesuré: ({x:.4}, {y:.4}) — attendu (0.5, 0.5)");
                assert!((x - 0.5).abs() <= 0.03, "x={x} ancré sur la région mesurée");
                assert!((y - 0.5).abs() <= 0.03, "y={y} ancré sur la région mesurée");
            }
            other => panic!("Click ancré attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn test_empty_screen_yields_honest_wait_and_anchor_errs() {
        let agent = UiTarsAgent::new();
        let empty = synthetic_rgba(160, 120, None);
        // Couche stricte : Err fail-closed (jamais de coordonnée inventée).
        assert!(
            agent
                .anchor_intentions_on_regions("Click here", &[])
                .is_err()
        );
        // Couche intention : Wait honnête.
        let intents = agent
            .predict_grounded_on_frame("Click here", &empty, 160, 120)
            .unwrap();
        assert_eq!(intents.len(), 1);
        assert!(
            matches!(intents[0], GuiIntention::Wait { .. }),
            "écran vide → Wait honnête, obtenu {:?}",
            intents[0]
        );
        // Dimensions nulles → pareil.
        let intents = agent
            .predict_grounded_on_frame("Click here", &[], 0, 0)
            .unwrap();
        assert!(matches!(intents[0], GuiIntention::Wait { .. }));
    }

    #[test]
    fn test_parser_kept_for_coordless_intents() {
        let agent = UiTarsAgent::new();
        // Type/Scroll : aucune coordonnée requise, le parseur suffit.
        let rgba = synthetic_rgba(160, 120, None);
        let intents = agent
            .predict_grounded_on_frame("Type: hello", &rgba, 160, 120)
            .unwrap();
        assert!(matches!(&intents[0], GuiIntention::Type { text } if text == "hello"));
        let intents = agent
            .predict_grounded_on_frame("scroll down", &rgba, 160, 120)
            .unwrap();
        assert!(matches!(&intents[0], GuiIntention::Scroll { .. }));
    }
}
