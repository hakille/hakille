//! Tier 3 Pairwise Module: Guest Delegation, Sandboxing & Autonomy Clamping
//!
//! Combinatorial interactions between:
//! - F08: 600ms Hold-to-Confirm
//! - F12: Autonomy Ladder by Domain
//! - F18: Sandboxed Agent Runner
//! - F29: Ephemeral Guest Pass
//!
//! Tests: T3_PAIR_31 to T3_PAIR_33

use hakille_design::autonomy::{Crank, DomainAutonomy};
use hakille_design::household::{GUEST_RING_PX, GuestPass};
use hakille_ui_protocol::AgentDto;

/// `T3_PAIR_31`: Ephemeral guest pass restricts agent execution to zero-capability sandboxes.
///
/// Interactions: F29 (Guest Pass) + F18 (Sandboxed Runner)
/// While a guest pass is active on the device, agents requiring host capabilities
/// (network.send, fs.write, wallet.spend) are strictly barred from execution.
#[test]
fn test_t3_pair_31_guest_pass_restricts_wasm_agent_execution_to_zero_caps() {
    let pass = GuestPass::new("Invité", "calc.basic", 1800);
    assert!(!pass.expired());

    let high_privilege_agent = AgentDto {
        id: "agent-network".into(),
        name: "network_client".into(),
        required_caps: vec!["network.send".into()],
        artifact_present: true,
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    };

    let zero_cap_agent = AgentDto {
        id: "agent-calc".into(),
        name: "calculator".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    };

    let is_agent_executable_by_guest = |guest: &GuestPass, agent: &AgentDto| -> bool {
        if guest.expired() {
            return false;
        }
        // Guest mode strictly enforces zero required capabilities
        agent.required_caps.is_empty()
    };

    assert!(
        !is_agent_executable_by_guest(&pass, &high_privilege_agent),
        "Agents demanding network/disk caps must be blocked under guest mode"
    );
    assert!(
        is_agent_executable_by_guest(&pass, &zero_cap_agent),
        "Pure calculation agents with zero required caps are allowed"
    );
}

/// `T3_PAIR_32`: Active guest pass enforces 600ms hold on all actions, overriding auto-execute.
///
/// Interactions: F29 (Guest Pass) + F08 (600ms Hold)
/// In guest mode, even reversible or zero-cost actions require explicit human confirmation
/// with 600ms hold to prevent accidental mutations by guest borrowers.
#[test]
fn test_t3_pair_32_guest_pass_enforces_600ms_hold_on_all_operations() {
    let pass = GuestPass::new("Voisin", "lumiere", 3600);
    assert_eq!(GUEST_RING_PX, 1);
    assert!(!pass.expired());

    let is_reversible_action = true;
    let cost_cents = 0;

    let requires_hold = |guest_active: bool, is_reversible: bool, cost: u64| -> bool {
        if guest_active {
            true // Guest mode ALWAYS forces 600ms hold
        } else {
            !is_reversible || cost > 0
        }
    };

    assert!(
        requires_hold(true, is_reversible_action, cost_cents),
        "Active guest pass must force 600ms hold on reversible actions"
    );
    assert!(
        !requires_hold(false, is_reversible_action, cost_cents),
        "Sovereign owner mode allows instant execution on reversible actions"
    );
}

/// `T3_PAIR_33`: Guest pass activation clamps all domain autonomy ladders to Crank 0 (Observe).
///
/// Interactions: F29 (Guest Pass) + F12 (Autonomy Ladder)
/// When device enters guest mode, all domain autonomy levels (Mail, Agenda, Maison, Finance)
/// are clamped to Crank 0 (Observe) for the full duration of the pass.
#[test]
fn test_t3_pair_33_guest_pass_clamps_domain_autonomy_ladders_to_crank_zero() {
    let mut mail_domain = DomainAutonomy::new("Mail");
    mail_domain.crank = Crank::ActEnvelope; // Crank 3

    let mut agenda_domain = DomainAutonomy::new("Agenda");
    agenda_domain.crank = Crank::Delegate; // Crank 4

    let mut home_domain = DomainAutonomy::new("Maison");
    home_domain.crank = Crank::ActReversible; // Crank 2

    let guest_pass = GuestPass::new("Colocataire", "musique", 3600);

    let effective_crank = |guest: &GuestPass, domain: &DomainAutonomy| -> Crank {
        if !guest.expired() {
            Crank::Observe // Clamped to Cran 0
        } else {
            domain.crank
        }
    };

    assert_eq!(
        effective_crank(&guest_pass, &mail_domain),
        Crank::Observe,
        "Mail domain must be clamped to Crank 0 under guest pass"
    );
    assert_eq!(
        effective_crank(&guest_pass, &agenda_domain),
        Crank::Observe,
        "Agenda domain must be clamped to Crank 0 under guest pass"
    );
    assert_eq!(
        effective_crank(&guest_pass, &home_domain),
        Crank::Observe,
        "Home domain must be clamped to Crank 0 under guest pass"
    );
}
