//! Mouvement (spec §6) : durées, easings, ressort carte, haptique, tween.
//!
//! Tout est token, jamais en dur : les valeurs viennent de
//! `tokens/motion.toml` via `generated::motion`. Mouvement réduit : les
//! pulsations deviennent des paliers d'opacité (voir `reduced_motion_opacity`).

use crate::generated::motion as tokens;

/// Durées ms : 120 / 200 / 320 / 600.
pub const DURATION_MS: [u16; 4] = [
    tokens::MOTION_DURATION_MS_T120 as u16,
    tokens::MOTION_DURATION_MS_T200 as u16,
    tokens::MOTION_DURATION_MS_T320 as u16,
    tokens::MOTION_DURATION_MS_T600 as u16,
];
/// Entrée : cubic-bezier(0.2, 0, 0, 1).
pub const EASE_IN: &str = tokens::MOTION_EASE_EASE_IN;
/// Sortie : cubic-bezier(0.3, 0, 0.8, 0.15).
pub const EASE_OUT: &str = tokens::MOTION_EASE_EASE_OUT;
/// Ressort déplacement de cartes : raideur 380, amortissement 32, masse 1.
pub const SPRING_STIFFNESS: u16 = tokens::MOTION_SPRING_CARD_STIFFNESS as u16;
pub const SPRING_DAMPING: u16 = tokens::MOTION_SPRING_CARD_DAMPING as u16;
pub const SPRING_MASS: u16 = tokens::MOTION_SPRING_CARD_MASS as u16;
/// Haptique : tick 10 ms = validé.
pub const HAPTIC_TICK_MS: u16 = tokens::MOTION_HAPTIC_MS_TICK as u16;
/// Barge-in : les 21 fils retombent en moins de 80 ms.
pub const BARGE_IN_COLLAPSE_MS: u16 = 80;
/// Silence : 600 ms après la dernière parole, les fils se replient en point.
pub const SPEECH_FOLD_SILENCE_MS: u16 = 600;
/// Appui long irréversible : 600 ms avec anneau braise.
pub const HOLD_TO_CONFIRM_MS: u16 = 600;
/// Anneau invité 1 px qui se vide (durée = durée du pass, défaut 1 h).
pub const GUEST_DEFAULT_SECONDS: u32 = 3600;

/// Pulsation Présence : (période ms, opacité min, opacité max, échelle max).
/// Veille = statique (période 0). Parole pilotée par l'enveloppe audio.
pub fn presence_rhythm(state: &str) -> (u32, f32, f32, f32) {
    match state {
        "listening" => (4000, 0.6, 1.0, 1.15),
        "acting" => (1200, 0.5, 1.0, 1.0),
        "urgent" => (800, 0.6, 1.0, 1.0),
        _ => (0, 0.4, 0.4, 1.0),
    }
}

/// Mouvement réduit : opacité par paliers, jamais d'échelle ni de
/// défilement automatique.
pub fn reduced_motion_opacity(phase01: f32) -> f32 {
    if phase01 < 0.5 { 0.6 } else { 1.0 }
}

/// Courbe d'interpolation du moteur de tween.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionCurve {
    /// Linéaire (t → t).
    Linear,
    /// Token `motion.ease.ease_in` : `cubic-bezier(0.2, 0, 0, 1)`.
    EaseIn,
    /// Token `motion.ease.ease_out` : `cubic-bezier(0.3, 0, 0.8, 0.15)`.
    EaseOut,
}

impl MotionCurve {
    /// Toutes les courbes, dans l'ordre des tokens.
    pub const ALL: [MotionCurve; 3] = [Self::Linear, Self::EaseIn, Self::EaseOut];

    /// Points de contrôle (x1, y1, x2, y2) — miroir exact des tokens.
    pub fn control_points(self) -> (f32, f32, f32, f32) {
        match self {
            Self::Linear => (0.0, 0.0, 1.0, 1.0),
            Self::EaseIn => (0.2, 0.0, 0.0, 1.0),
            Self::EaseOut => (0.3, 0.0, 0.8, 0.15),
        }
    }

    /// Évalue la courbe en progression normalisée : `0 → 0`, `1 → 1` exacts.
    pub fn ease(self, t01: f32) -> f32 {
        let t = clamp01(t01);
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        match self {
            Self::Linear => t,
            _ => {
                let (x1, y1, x2, y2) = self.control_points();
                let s = solve_bezier_time(t, x1, x2);
                bezier_axis(s, y1, y2)
            }
        }
    }
}

/// Interpole `from` → `to` selon `curve`.
///
/// `t_ms` est la **progression normalisée** 0.0 → 1.0 (0 = `from`, 1 = `to`),
/// bornée : `≤ 0` → `from`, `≥ 1` → `to`, `NaN` → `from`. Le nom du paramètre
/// est conservé pour l'API coordonnée ; pour une horloge en millisecondes,
/// utiliser [`tween_ms`].
pub fn tween(from: f32, to: f32, t_ms: f32, curve: MotionCurve) -> f32 {
    let t = clamp01(t_ms);
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    from + (to - from) * curve.ease(t)
}

/// Variante horloge : `elapsed_ms` sur `duration_ms`.
///
/// Durée `≤ 0` → arrivée immédiate (`to`) ; durée `NaN` → `from` ; durée
/// infinie → reste à `from` ; `elapsed_ms ≤ 0` → `from`.
pub fn tween_ms(from: f32, to: f32, elapsed_ms: f32, duration_ms: f32, curve: MotionCurve) -> f32 {
    if duration_ms.is_nan() {
        return from;
    }
    if duration_ms <= 0.0 {
        return to;
    }
    tween(from, to, elapsed_ms / duration_ms, curve)
}

/// Parse une courbe CSS `cubic-bezier(x1, y1, x2, y2)` en points de contrôle.
pub fn parse_cubic_bezier(spec: &str) -> Option<(f32, f32, f32, f32)> {
    let inner = spec
        .trim()
        .strip_prefix("cubic-bezier(")?
        .strip_suffix(')')?;
    let mut it = inner.split(',');
    let mut vals = [0.0f32; 4];
    for v in &mut vals {
        *v = it.next()?.trim().parse().ok()?;
    }
    if it.next().is_some() {
        return None;
    }
    Some((vals[0], vals[1], vals[2], vals[3]))
}

fn clamp01(t: f32) -> f32 {
    if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
}

/// Composante 1D d'une cubic-bezier, extrémités fixes (0, 0) et (1, 1).
fn bezier_axis(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

fn bezier_axis_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    3.0 * (1.0 - t) * (1.0 - t) * p1 + 6.0 * (1.0 - t) * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

/// Résout `s` tel que `B_x(s) = t` (Newton-Raphson, repli bissection).
/// Pur et déterministe : mêmes entrées → même sortie.
fn solve_bezier_time(t: f32, x1: f32, x2: f32) -> f32 {
    let mut s = t;
    for _ in 0..8 {
        let err = bezier_axis(s, x1, x2) - t;
        if err.abs() < 1e-6 {
            return s.clamp(0.0, 1.0);
        }
        let d = bezier_axis_derivative(s, x1, x2);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if bezier_axis(mid, x1, x2) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_tokens() {
        assert_eq!(DURATION_MS, [120, 200, 320, 600]);
        assert_eq!(HOLD_TO_CONFIRM_MS, 600);
        assert_eq!(BARGE_IN_COLLAPSE_MS, 80);
        let (period, lo, hi, _) = presence_rhythm("listening");
        assert_eq!(period, 4000);
        assert!((lo - 0.6).abs() < 1e-6 && (hi - 1.0).abs() < 1e-6);
        assert_eq!(presence_rhythm("idle").0, 0);
        assert_eq!(reduced_motion_opacity(0.2), 0.6);
        assert_eq!(reduced_motion_opacity(0.9), 1.0);
        // Les constantes du module reflètent bien les tokens générés.
        assert_eq!(DURATION_MS[3] as i64, tokens::MOTION_DURATION_MS_T600);
        assert_eq!(
            SPRING_STIFFNESS as i64,
            tokens::MOTION_SPRING_CARD_STIFFNESS
        );
        assert_eq!(HAPTIC_TICK_MS as i64, tokens::MOTION_HAPTIC_MS_TICK);
    }

    #[test]
    fn test_curves_match_tokens() {
        let ease_in = parse_cubic_bezier(EASE_IN).expect("token ease_in");
        let ease_out = parse_cubic_bezier(EASE_OUT).expect("token ease_out");
        assert_eq!(ease_in, MotionCurve::EaseIn.control_points());
        assert_eq!(ease_out, MotionCurve::EaseOut.control_points());
        assert_eq!(
            parse_cubic_bezier("cubic-bezier(0.2, 0, 0, 1)"),
            Some(ease_in)
        );
        assert_eq!(parse_cubic_bezier("linear"), None);
        assert_eq!(parse_cubic_bezier("cubic-bezier(a, b, c, d)"), None);
        assert_eq!(parse_cubic_bezier("cubic-bezier(0.2, 0, 0, 1, 9)"), None);
    }

    #[test]
    fn test_ease_endpoints_and_bounds() {
        for curve in MotionCurve::ALL {
            assert_eq!(curve.ease(0.0), 0.0, "{curve:?}");
            assert_eq!(curve.ease(1.0), 1.0, "{curve:?}");
            assert_eq!(curve.ease(-2.0), 0.0, "{curve:?}");
            assert_eq!(curve.ease(42.0), 1.0, "{curve:?}");
            assert_eq!(curve.ease(f32::NAN), 0.0, "{curve:?}");
            assert!((0.0..=1.0).contains(&curve.ease(0.37)), "{curve:?}");
        }
        assert_eq!(MotionCurve::Linear.ease(0.25), 0.25);
    }

    #[test]
    fn test_tween_endpoints_and_bounds() {
        for curve in MotionCurve::ALL {
            assert_eq!(tween(2.0, 8.0, 0.0, curve), 2.0, "{curve:?}");
            assert_eq!(tween(2.0, 8.0, 1.0, curve), 8.0, "{curve:?}");
            assert_eq!(tween(2.0, 8.0, -3.0, curve), 2.0, "{curve:?}");
            assert_eq!(tween(2.0, 8.0, 7.5, curve), 8.0, "{curve:?}");
            assert_eq!(tween(2.0, 8.0, f32::NAN, curve), 2.0, "{curve:?}");
            let v = tween(2.0, 8.0, 0.5, curve);
            assert!(v.is_finite() && (2.0..=8.0).contains(&v), "{curve:?} : {v}");
        }
        // Valeurs identiques : tween constant, y compris au milieu.
        assert_eq!(tween(5.0, 5.0, 0.5, MotionCurve::EaseIn), 5.0);
    }

    #[test]
    fn test_tween_monotonic_and_varied() {
        for curve in MotionCurve::ALL {
            let mut prev = f32::NEG_INFINITY;
            let mut mid = 0.0f32;
            for i in 0..=1000 {
                let t = i as f32 / 1000.0;
                let v = tween(0.0, 1.0, t, curve);
                assert!(v >= prev, "{curve:?} non monotone à t={t} : {prev} → {v}");
                assert!((0.0..=1.0).contains(&v), "{curve:?} hors bornes : {v}");
                if i == 500 {
                    mid = v;
                }
                prev = v;
            }
            if curve != MotionCurve::Linear {
                assert!(
                    (mid - 0.5).abs() > 1e-3,
                    "{curve:?} identique au linéaire au milieu : {mid}"
                );
            }
        }
    }

    #[test]
    fn test_tween_ms_clock() {
        assert_eq!(
            tween_ms(0.0, 1.0, 160.0, 320.0, MotionCurve::EaseOut),
            tween(0.0, 1.0, 0.5, MotionCurve::EaseOut)
        );
        assert_eq!(tween_ms(0.0, 1.0, 999.0, 320.0, MotionCurve::Linear), 1.0);
        assert_eq!(tween_ms(0.0, 1.0, 0.0, 0.0, MotionCurve::Linear), 1.0);
        assert_eq!(tween_ms(0.0, 1.0, -5.0, 320.0, MotionCurve::Linear), 0.0);
        assert_eq!(
            tween_ms(0.0, 1.0, 100.0, f32::NAN, MotionCurve::Linear),
            0.0
        );
        assert_eq!(
            tween_ms(0.0, 1.0, 100.0, f32::INFINITY, MotionCurve::Linear),
            0.0
        );
        // Déterministe, bit à bit.
        let a = tween(0.25, 8.5, 0.37, MotionCurve::EaseOut);
        let b = tween(0.25, 8.5, 0.37, MotionCurve::EaseOut);
        assert_eq!(a.to_bits(), b.to_bits());
    }
}
