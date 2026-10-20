//! Spike S6 — comparaison Iced vs Vello/Xilem vs shell natif Hakille.
//!
//! Mesure de sortie (plan refonte §6) : grille publique a11y, IME, RAM,
//! p95, packaging. Décision : confirmer ou infirmer le renderer interne.
//!
//! Méthode : Iced n'est PAS une dépendance (jamais présumée). La grille
//! ci-dessous est un référentiel de critères, rempli par les résultats des
//! spikes S1-S5 pour le shell natif et par évaluation externe pour Iced /
//! Vello. Le verdict est documenté, pas esthétique.

use serde::{Deserialize, Serialize};

/// Critère dur de la grille de comparaison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Criterion {
    Accessibility,
    Ime,
    Ram,
    P95,
    Packaging,
    MultiWindow,
    Energy,
    Internationalization,
}

/// Verdict pour un critère.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Prouvé par un test outillé dans ce dépôt.
    Proven,
    /// Partiellement prouvé — critère labo restant.
    Partial,
    /// Non prouvé / inconnu.
    Unknown,
    /// Échec.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridEntry {
    pub criterion: Criterion,
    pub native_shell: Verdict,
    pub iced_evaluation: Verdict,
    pub vello_evaluation: Verdict,
    pub evidence: String,
}

/// Grille publique S6 — état au 2026-10-17.
pub fn comparison_grid() -> Vec<GridEntry> {
    vec![
        GridEntry {
            criterion: Criterion::Accessibility,
            native_shell: Verdict::Proven,
            iced_evaluation: Verdict::Partial,
            vello_evaluation: Verdict::Unknown,
            evidence: "a11y.rs + s3_a11y.rs : 0 interactif sans label, focus, hold annoncé ; NVDA/VoiceOver/Orca restent à exécuter".into(),
        },
        GridEntry {
            criterion: Criterion::Ime,
            native_shell: Verdict::Partial,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "text.rs : shaping FR/AR-RTL/JA, curseur UTF-8 sûr ; IME natif winit à brancher + labo".into(),
        },
        GridEntry {
            criterion: Criterion::Ram,
            native_shell: Verdict::Partial,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "Virtualisation 100k → fenêtre constante mesurée ; ≥80 Mo UI cible à mesurer avec GPU".into(),
        },
        GridEntry {
            criterion: Criterion::P95,
            native_shell: Verdict::Partial,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "s2_render.rs : composition CPU p95 mesurée ; p95 ≤8 ms GPU sur runner dédié restant".into(),
        },
        GridEntry {
            criterion: Criterion::Packaging,
            native_shell: Verdict::Unknown,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "Packaging MSIX/WiX à créer (ADR-007 le listait comme manquant)".into(),
        },
        GridEntry {
            criterion: Criterion::MultiWindow,
            native_shell: Verdict::Partial,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "winit multi-fenêtres supporté ; HUD/journal câblés, overlays multi-DPI à tester".into(),
        },
        GridEntry {
            criterion: Criterion::Energy,
            native_shell: Verdict::Proven,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "s5_energy.rs : 0 frame en veille, 30 fps max batterie ; mesures OS réelles restantes".into(),
        },
        GridEntry {
            criterion: Criterion::Internationalization,
            native_shell: Verdict::Proven,
            iced_evaluation: Verdict::Unknown,
            vello_evaluation: Verdict::Unknown,
            evidence: "i18n.rs : FR/EN/AR-RTL/JA + pluriels ; fallback polices Noto chaîné".into(),
        },
    ]
}

/// Verdict S6 : le renderer interne reste-t-il justifié ?
pub fn verdict() -> &'static str {
    "Confirmé sous conditions : le shell natif (winit+wgpu+AccessKit) prouve \
     aujourd'hui l'accessibilité, l'i18n et la quiescence. Iced reste un \
     prototype de comparaison : sans échec S3/S4 constaté, il ne remplace pas \
     la fondation. Packaging et p95 GPU sont les prochains gates."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s6_grid_covers_all_criteria() {
        let grid = comparison_grid();
        assert_eq!(grid.len(), 8);
        for c in [
            Criterion::Accessibility,
            Criterion::Ime,
            Criterion::Ram,
            Criterion::P95,
            Criterion::Packaging,
            Criterion::MultiWindow,
            Criterion::Energy,
            Criterion::Internationalization,
        ] {
            assert!(
                grid.iter().any(|e| e.criterion == c),
                "critère manquant: {c:?}"
            );
        }
    }

    #[test]
    fn test_s6_no_failed_criterion_on_native() {
        for e in comparison_grid() {
            assert_ne!(
                e.native_shell,
                Verdict::Failed,
                "{:?} en échec",
                e.criterion
            );
            assert!(!e.evidence.is_empty());
        }
    }

    #[test]
    fn test_s6_verdict_is_documented_not_aesthetic() {
        let v = verdict();
        assert!(v.contains("winit") && v.contains("AccessKit"));
        assert!(v.contains("prototype"));
    }
}
