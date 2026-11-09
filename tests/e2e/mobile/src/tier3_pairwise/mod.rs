//! Tier 3: Pairwise Cross-Feature Interaction Test Suite.
//!
//! Systematic combinatorial testing across 10 functional interaction modules:
//! - `presence_streaming`: F01 (Living Presence), F02 (Aura), F04 (Voice), F05 (Streaming)
//! - `action_card_governance`: F07 (ActionCard), F08 (Hold 600ms), F09 (Reject), F10 (Provenance), F13 (Signed Audit), F14 (Undo)
//! - `voice_draft_checkpoint`: F03 (Dictation), F04 (Voice), F06 (Input Lock), F20 (Checkpoint v2)
//! - `explainability_memory`: F11 ("Pourquoi ?"), F13 (Audit), F15 (SPO Graph), F16 (Fact Forgetting)
//! - `autonomy_approval`: F12 (Autonomy Ladder), F07 (Cards), F08 (Hold), F14 (Undo Demotion)
//! - `agent_execution_provenance`: F17 (Agent Catalog), F18 (Sandboxed Runner), F10 (Provenance), F13 (Audit)
//! - `pairing_recovery_locking`: F19 (Gateway Pairing), F20 (Checkpoint Recovery), F06 (Honest Wait)
//! - `guest_delegation_sandbox`: F29 (Guest Pass), F18 (Sandbox Limits), F08 (Hold), F12 (Autonomy Clamp)
//! - `calm_distress_safety`: F30 (Calm Distress), F01 (Presence Mute), F08 (Zero Braise), F25 (Sensor Suppression)
//! - `sensors_multimodal_bounds`: F21 (Camera), F23 (GPS), F24 (Launcher), F27 (AccessKit), F28 (Ghost Action)

pub mod action_card_governance;
pub mod agent_execution_provenance;
pub mod autonomy_approval;
pub mod calm_distress_safety;
pub mod explainability_memory;
pub mod guest_delegation_sandbox;
pub mod pairing_recovery_locking;
pub mod presence_streaming;
pub mod sensors_multimodal_bounds;
pub mod voice_draft_checkpoint;

pub const TIER3_TEST_COUNT: usize = 41;
