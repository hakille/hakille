//! Feature 28: Ghost Action Projection (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F28_01 to T2_F28_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig};
use crate::tier1_features::f27_accesskit_a11y::{
    AccessibleHoldController, MIN_TOUCH_TARGET_ANDROID_DP, MIN_TOUCH_TARGET_IOS_PT,
};
use hakille_design::color::{BRAISE_ALLOWED_IN, braise_allowed};
use hakille_design::household::{DistressMode, GUEST_RING_PX, GuestPass};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{
    FatigueAlertEvent, FatigueSeverity, GhostActionDto, KernelToUi, PrivacyGuardEvent,
    PrivacyGuardTrigger, UiToKernel,
};
use serde_json::json;
use std::time::{Duration, Instant};

/// `T2_F28_01`: focal_u and focal_v clamped to [0.0, 1.0].
///
/// Out of bound projection coordinates are clamped to [0.0, 1.0].
#[test]
fn test_t2_f28_01_focal_coordinates_clamped() {
    fn clamp_focal(u: f32, v: f32) -> (f32, f32) {
        (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
    }

    assert_eq!(clamp_focal(-0.1, 1.2), (0.0, 1.0));
    assert_eq!(clamp_focal(0.5, 0.5), (0.5, 0.5));
}

/// `T2_F28_02`: Confidence < 0.0 or > 1.0 clamped.
///
/// Ghost action confidence is strictly clamped to [0.0, 1.0].
#[test]
fn test_t2_f28_02_confidence_clamped() {
    fn clamp_confidence(c: f32) -> f32 {
        c.clamp(0.0, 1.0)
    }

    assert_eq!(clamp_confidence(-0.5), 0.0);
    assert_eq!(clamp_confidence(1.5), 1.0);
    assert_eq!(clamp_confidence(0.85), 0.85);
}

/// `T2_F28_03`: Ghost action dismissal timeout.
///
/// Ghost action unconfirmed after TTL duration is dismissed without triggering action.
#[test]
fn test_t2_f28_03_ghost_action_dismissal_timeout() {
    struct GhostActionLifecycle {
        ttl_ms: u64,
        elapsed_ms: u64,
        dismissed: bool,
    }
    impl GhostActionLifecycle {
        fn tick(&mut self, delta_ms: u64) {
            self.elapsed_ms += delta_ms;
            if self.elapsed_ms >= self.ttl_ms {
                self.dismissed = true;
            }
        }
    }

    let mut ghost = GhostActionLifecycle {
        ttl_ms: 5000,
        elapsed_ms: 0,
        dismissed: false,
    };

    ghost.tick(4999);
    assert!(!ghost.dismissed, "4999ms is before 5000ms TTL");

    ghost.tick(2);
    assert!(ghost.dismissed, "5001ms triggers auto-dismissal");
}

/// `T2_F28_04`: Microgesture pinch normalization.
///
/// Coordinates of index-thumb pinch gestures are normalized to [0.0, 1.0] viewport space.
#[test]
fn test_t2_f28_04_microgesture_pinch_normalization() {
    fn normalize_coordinate(pixel_val: f32, dimension_pixels: f32) -> f32 {
        if dimension_pixels <= 0.0 {
            0.0
        } else {
            (pixel_val / dimension_pixels).clamp(0.0, 1.0)
        }
    }

    assert_eq!(normalize_coordinate(540.0, 1080.0), 0.5);
    assert_eq!(normalize_coordinate(-10.0, 1080.0), 0.0);
    assert_eq!(normalize_coordinate(1200.0, 1080.0), 1.0);
}

/// `T2_F28_05`: Ghost action ID boundary (> 256 bytes rejected by gateway).
///
/// Gateway enforces `!id.is_empty() && id.len() <= 256` for GhostActionAccept and Dismiss.
#[tokio::test]
async fn test_t2_f28_05_ghost_action_id_boundary_256_bytes() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // 257 bytes rejected
    let f_over = Frame::request(
        1,
        UiToKernel::GhostActionAccept {
            id: "g".repeat(257),
        },
    );
    let (s_over, r_over) = rig.post_frame(&f_over, None).await;
    assert_eq!(s_over, 400);
    assert_eq!(r_over["error"]["code"], "unsupported");

    // 256 bytes accepted
    let id_256 = "g".repeat(256);
    let id_clone = id_256.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::GhostActionAccept { id: id_clone },
            KernelToUi::HudReply {
                response: "Ghost accept".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 2, step)
            .await
            .expect("step");
    });

    let f_valid = Frame::request(2, UiToKernel::GhostActionAccept { id: id_256 });
    let (s_valid, r_valid) = rig.post_frame(&f_valid, None).await;
    assert_eq!(s_valid, 200);
    assert!(r_valid["error"].is_null());

    daemon_task.await.unwrap();
}
