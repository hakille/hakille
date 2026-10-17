//! Spikes Phase 0 (spec Hakille §10) : hypothèses prouvées avant engagement.
//!
//! - **S1 accessibilité** : l'arbre AccessKit construit depuis nos VMs
//!   (cartes, journal, pouvoirs) porte rôles + labels + états complets.
//!   Critère labo restant : 3 lecteurs d'écran parcourent Carte + Journal
//!   sur macOS/Windows/Linux (plan B : Xilem/Masonry).
//! - **S2 rendu** : présence, carte, journal virtualisé 100 000 lignes ;
//!   p95 GPU sur runner dédié restant.
//! - **S3 accessibilité nouveau shell** : arbre du shell natif complet,
//!   parcours lecteurs réels restant (gate bloquant).
//! - **S4 événementiel** : en Veille (Idle + HUD masqué), aucun travail en
//!   attente — 0 réveil/s mesurable côté OS (voir `quiescent Pérenne`).
//! - **S5 énergie** : 0 frame en veille, 30 fps max batterie, arrêt si
//!   mouvement réduit.
//! - **S6 comparaison** : grille publique a11y/IME/RAM/p95/packaging ;
//!   Iced reste un prototype de comparaison, jamais une dépendance présumée.

pub mod s1_tree;
pub mod s2_render;
pub mod s3_a11y;
pub mod s4_quiescence;
pub mod s5_energy;
pub mod s6_compare;

pub use s1_tree::*;
pub use s2_render::*;
pub use s3_a11y::*;
pub use s4_quiescence::*;
pub use s5_energy::*;
pub use s6_compare::*;
