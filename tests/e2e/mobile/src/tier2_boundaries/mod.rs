//! Tier 2: Boundary Values & Fault Injection (Features 01 to 30, Complete 150 Tests).
//!
//! Requirement-driven opaque-box tests covering all 30 mobile features under edge cases,
//! malformed inputs, timeouts, network resets, and resource exhaustion.
//! Each feature module contains exactly 5 tests (T2_F{xx}_01 to T2_F{xx}_05).

pub mod f01_kora_boundaries;
pub mod f02_aura_boundaries;
pub mod f03_dictation_boundaries;
pub mod f04_hub_boundaries;
pub mod f05_tokendelta_boundaries;
pub mod f06_locking_boundaries;
pub mod f07_badges_boundaries;
pub mod f08_hold_boundaries;
pub mod f09_reject_boundaries;
pub mod f10_provenance_boundaries;
pub mod f11_pourquoi_boundaries;
pub mod f12_autonomy_boundaries;
pub mod f13_audit_boundaries;
pub mod f14_undo_boundaries;
pub mod f15_spo_boundaries;
pub mod f16_forgetting_boundaries;
pub mod f17_catalog_boundaries;
pub mod f18_runner_boundaries;
pub mod f19_pairing_boundaries;
pub mod f20_checkpoint_boundaries;
pub mod f21_camera_boundaries;
pub mod f22_docpicker_boundaries;
pub mod f23_gps_boundaries;
pub mod f24_launcher_boundaries;
pub mod f25_privacy_boundaries;
pub mod f26_fatigue_boundaries;
pub mod f27_accesskit_boundaries;
pub mod f28_ghost_boundaries;
pub mod f29_guest_boundaries;
pub mod f30_calm_boundaries;
