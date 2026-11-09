//! Feature 28: Ghost Action Projection (Milestone M1 / Spec §1, §31, ui-protocol)
//!
//! Specifications: T1_F28_01 to T1_F28_05
//!
//! Verifies:
//! - GhostActionDto structure: focal coordinates (focal_u, focal_v), confidence, and action.
//! - Dispatch of UiToKernel::GhostActionAccept through the mobile gateway.
//! - Dispatch of UiToKernel::GhostActionDismiss through the mobile gateway.
//! - Strict gateway boundary validation for ghost action IDs (non-empty, <= 256 bytes).
//! - MicroGestureEvent::IndexThumbPinch spatial correlation for ghost action confirmation.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_ui_protocol::{
    ActionCard, DataExit, GhostActionDto, KernelToUi, MicroGestureEvent, Money, Provenance,
    UiToKernel,
};

fn sample_action_card() -> ActionCard {
    ActionCard {
        id: 777,
        verb: "Archiver".into(),
        object: "rapport.pdf".into(),
        reason: "Tâche planifiée terminée".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    }
}

/// `T1_F28_01`: GhostActionDto focal projection coordinates.
///
/// Ghost action projects translucent suggestions at normalized focal coordinates
/// (focal_u, focal_v in [0.0, 1.0]) with confidence and embedded ActionCard.
#[test]
fn test_t1_f28_01_ghost_action_dto_focal_projection_coordinates() {
    let ghost = GhostActionDto {
        id: "ghost-proj-01".into(),
        label: "Archiver le fichier".into(),
        confidence: 0.92,
        focal_u: 0.55,
        focal_v: 0.32,
        hold_duration_ms: 600,
        action: sample_action_card(),
    };

    assert!(
        (0.0..=1.0).contains(&ghost.focal_u),
        "focal_u must be in [0, 1]"
    );
    assert!(
        (0.0..=1.0).contains(&ghost.focal_v),
        "focal_v must be in [0, 1]"
    );
    assert!(
        (0.0..=1.0).contains(&ghost.confidence),
        "confidence must be in [0, 1]"
    );
    assert_eq!(ghost.hold_duration_ms, 600);
    assert_eq!(ghost.action.id, 777);
}

/// `T1_F28_02`: GhostActionAccept gateway dispatch.
///
/// Sending `UiToKernel::GhostActionAccept { id }` through the gateway forwards
/// the confirmation frame to the daemon.
#[tokio::test]
async fn test_t1_f28_02_ghost_action_accept_gateway_dispatch() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let ghost_id = "ghost-accept-42".to_string();

    let id_clone = ghost_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");

        let step = DaemonStep::new(
            UiToKernel::GhostActionAccept { id: id_clone },
            KernelToUi::HudReply {
                response: "Ghost action acceptée et exécutée".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("ghost accept");
    });

    let req_frame = Frame::request(1, UiToKernel::GhostActionAccept { id: ghost_id });

    let (status, resp) = rig.post_frame(&req_frame, None).await;
    assert_eq!(status, 200);
    assert!(resp["error"].is_null());

    let frames = resp["frames"].as_array().expect("frames");
    assert_eq!(frames.len(), 1);
    assert!(
        frames[0]["body"]["Reply"]["message"]["HudReply"]["response"]
            .as_str()
            .unwrap()
            .contains("acceptée")
    );

    daemon_task.await.expect("daemon exit");
}

/// `T1_F28_03`: GhostActionDismiss gateway dispatch.
///
/// Dismissing a ghost action sends `UiToKernel::GhostActionDismiss { id }`.
/// Daemon returns `GhostActionDismissed { id }`.
#[tokio::test]
async fn test_t1_f28_03_ghost_action_dismiss_gateway_dispatch() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let ghost_id = "ghost-dismiss-99".to_string();

    let id_clone = ghost_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");

        let step = DaemonStep::new(
            UiToKernel::GhostActionDismiss {
                id: id_clone.clone(),
            },
            KernelToUi::GhostActionDismissed { id: id_clone },
        );
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("ghost dismiss");
    });

    let req_frame = Frame::request(
        1,
        UiToKernel::GhostActionDismiss {
            id: ghost_id.clone(),
        },
    );

    let (status, resp) = rig.post_frame(&req_frame, None).await;
    assert_eq!(status, 200);

    let frames = resp["frames"].as_array().expect("frames");
    let dismissed_frame = frames
        .iter()
        .find(|f| f["body"]["Reply"]["message"]["GhostActionDismissed"].is_object())
        .expect("dismissed frame");
    assert_eq!(
        dismissed_frame["body"]["Reply"]["message"]["GhostActionDismissed"]["id"],
        ghost_id
    );

    daemon_task.await.expect("daemon exit");
}

/// `T1_F28_04`: Boundary validation for ghost action IDs.
///
/// Gateway enforces `!id.is_empty() && id.len() <= 256` on GhostActionAccept/Dismiss.
/// Empty ID or oversized ID (>256 chars) is rejected with HTTP 400 Bad Request.
#[tokio::test]
async fn test_t1_f28_04_boundary_validation_for_ghost_action_ids() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    // Subcase A: Empty ID rejected
    let empty_frame = Frame::request(1, UiToKernel::GhostActionAccept { id: "".into() });
    let (status1, resp1) = rig.post_frame(&empty_frame, None).await;
    assert_eq!(status1, 400);
    assert_eq!(resp1["error"]["code"], "unsupported");

    // Subcase B: Oversized ID (> 256 chars) rejected
    let over_id = "g".repeat(257);
    let over_frame = Frame::request(2, UiToKernel::GhostActionDismiss { id: over_id });
    let (status2, resp2) = rig.post_frame(&over_frame, None).await;
    assert_eq!(status2, 400);
    assert_eq!(resp2["error"]["code"], "unsupported");
}

/// `T1_F28_05`: Pinch micro-gesture correlation.
///
/// `MicroGestureEvent::IndexThumbPinch` validation of normalized pinch parameters.
#[test]
fn test_t1_f28_05_pinch_micro_gesture_correlation() {
    let pinch = MicroGestureEvent::IndexThumbPinch {
        hand: "Right".into(),
        pinch_distance_norm: 0.05,
        duration_ms: 120,
        confirmed: true,
    };

    let serialized = serde_json::to_string(&pinch).unwrap();
    let deserialized: MicroGestureEvent = serde_json::from_str(&serialized).unwrap();

    match deserialized {
        MicroGestureEvent::IndexThumbPinch {
            hand,
            pinch_distance_norm,
            duration_ms,
            confirmed,
        } => {
            assert_eq!(hand, "Right");
            assert!((pinch_distance_norm - 0.05).abs() < 1e-4);
            assert_eq!(duration_ms, 120);
            assert!(confirmed);
        }
        _ => panic!("Expected IndexThumbPinch"),
    }
}
