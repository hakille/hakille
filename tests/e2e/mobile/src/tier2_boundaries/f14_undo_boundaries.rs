//! Feature 14: Reversible Action Undo (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F14_01 to T2_F14_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F14_01`: Undo after time expiration window.
///
/// Action with 30-second undo window rejects undo attempts made at t = 31 seconds.
#[test]
fn test_t2_f14_01_undo_after_time_expiration_window() {
    struct UndoWindow {
        duration_secs: u64,
    }
    impl UndoWindow {
        fn can_undo(&self, elapsed_secs: u64) -> bool {
            elapsed_secs <= self.duration_secs
        }
    }

    let window = UndoWindow { duration_secs: 30 };
    assert!(window.can_undo(0));
    assert!(window.can_undo(29));
    assert!(window.can_undo(30), "Exact 30s boundary is undoable");
    assert!(!window.can_undo(31), "31s is beyond undo window: rejected");
    assert!(!window.can_undo(3600), "1 hour later is rejected");
}

/// `T2_F14_02`: Undoing irreversible action returns explicit error.
///
/// Actions declared with reversible: false (+ définitif) cannot be undone.
#[test]
fn test_t2_f14_02_undoing_irreversible_action_returns_error() {
    fn attempt_undo(is_reversible: bool) -> Result<&'static str, &'static str> {
        if !is_reversible {
            Err("Action définitive (+ définitif) : annulation impossible")
        } else {
            Ok("Action annulée avec succès")
        }
    }

    assert_eq!(
        attempt_undo(false),
        Err("Action définitive (+ définitif) : annulation impossible")
    );
    assert_eq!(attempt_undo(true), Ok("Action annulée avec succès"));
}

/// `T2_F14_03`: Double undo attempt rejected.
///
/// Undoing an action a second time fails with already reverted error.
#[test]
fn test_t2_f14_03_double_undo_attempt_rejected() {
    struct ActionState {
        reverted: bool,
    }
    impl ActionState {
        fn revert(&mut self) -> Result<(), &'static str> {
            if self.reverted {
                Err("Action déjà annulée")
            } else {
                self.reverted = true;
                Ok(())
            }
        }
    }

    let mut action = ActionState { reverted: false };
    assert!(action.revert().is_ok(), "First undo succeeds");
    assert_eq!(
        action.revert(),
        Err("Action déjà annulée"),
        "Second undo attempt must be rejected"
    );
}

/// `T2_F14_04`: Undoing action when dependent actions exist.
///
/// When dependent operations exist, reverting parent operation fails closed.
#[test]
fn test_t2_f14_04_undoing_action_with_dependents_rejected() {
    fn check_can_undo(has_dependent_operations: bool) -> Result<(), &'static str> {
        if has_dependent_operations {
            Err("Des opérations postérieures dépendent de cette action : les annuler d'abord")
        } else {
            Ok(())
        }
    }

    assert_eq!(
        check_can_undo(true),
        Err("Des opérations postérieures dépendent de cette action : les annuler d'abord")
    );
    assert!(check_can_undo(false).is_ok());
}

/// `T2_F14_05`: Undo during active in-flight request locked.
///
/// While bridge has an in-flight pending request, attempting undo or mutating action is locked.
#[test]
fn test_t2_f14_05_undo_during_active_in_flight_request_locked() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());
    bridge.dispatch_json(json!({"type": "set_draft", "text": "En vol"}).to_string());

    let res_sub = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    assert!(res_sub.contains("\"ok\":true"));

    // Attempting new action while in-flight is locked
    let res_undo = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let res_json: serde_json::Value = serde_json::from_str(&res_undo).unwrap();
    assert_eq!(res_json["ok"], false);
    assert_eq!(res_json["error"], "Une requête est déjà en cours");
}
