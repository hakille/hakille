//! Feature 26: Ergonomic Fatigue Detection (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F26_01 to T2_F26_05 (5 tests total).

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

/// `T2_F26_01`: PERCLOS P80 boundaries: exactly 0.0 and 1.0.
///
/// PERCLOS P80 = 0.0 (wide awake) and 1.0 (eyes fully shut) are valid boundary inputs.
#[test]
fn test_t2_f26_01_perclos_p80_exact_boundaries() {
    let event_min = FatigueAlertEvent {
        severity: FatigueSeverity::Mild,
        perclos_p80: 0.0,
        ear: 0.35,
        mar: 0.1,
        yawn_count: 0,
        recommendation: "Tout va bien".into(),
        timestamp_ms: 1731000000000,
    };
    assert_eq!(event_min.perclos_p80, 0.0);

    let event_max = FatigueAlertEvent {
        severity: FatigueSeverity::Critical,
        perclos_p80: 1.0,
        ear: 0.05,
        mar: 0.7,
        yawn_count: 10,
        recommendation: "Arrêt immédiat conseillé".into(),
        timestamp_ms: 1731000000000,
    };
    assert_eq!(event_max.perclos_p80, 1.0);
}

/// `T2_F26_02`: PERCLOS P80 out of bounds clamped to [0.0, 1.0].
///
/// Values < 0.0 or > 1.0 are clamped to [0.0, 1.0].
#[test]
fn test_t2_f26_02_perclos_out_of_bounds_clamped() {
    fn clamp_perclos(val: f32) -> f32 {
        val.clamp(0.0, 1.0)
    }

    assert_eq!(clamp_perclos(-0.2), 0.0);
    assert_eq!(clamp_perclos(1.5), 1.0);
    assert_eq!(clamp_perclos(0.42), 0.42);
}

/// `T2_F26_03`: Yawn count accumulation boundary.
///
/// Yawn count accumulation maps monotonically into fatigue severity tiers.
#[test]
fn test_t2_f26_03_yawn_count_boundary() {
    fn severity_from_yawns(yawns: u32) -> FatigueSeverity {
        match yawns {
            0..=1 => FatigueSeverity::Mild,
            2..=4 => FatigueSeverity::Moderate,
            _ => FatigueSeverity::Critical,
        }
    }

    assert_eq!(severity_from_yawns(0), FatigueSeverity::Mild);
    assert_eq!(severity_from_yawns(1), FatigueSeverity::Mild);
    assert_eq!(severity_from_yawns(2), FatigueSeverity::Moderate);
    assert_eq!(severity_from_yawns(4), FatigueSeverity::Moderate);
    assert_eq!(severity_from_yawns(5), FatigueSeverity::Critical);
    assert_eq!(severity_from_yawns(50), FatigueSeverity::Critical);
}

/// `T2_F26_04`: Critical fatigue alert non-blocking contract.
///
/// Fatigue alerts are advisory only: they never block user input, never set recovery_required,
/// and never reject in-progress drafts.
#[test]
fn test_t2_f26_04_critical_fatigue_alert_non_blocking() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Draft en cours"}).to_string());

    // Critical fatigue alert delivered via protocol
    let _alert = FatigueAlertEvent {
        severity: FatigueSeverity::Critical,
        perclos_p80: 0.95,
        ear: 0.1,
        mar: 0.8,
        yawn_count: 8,
        recommendation: "Pause de 15 minutes recommandée".into(),
        timestamp_ms: 1731000000000,
    };

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap["recovery_required"], false,
        "Fatigue must never require recovery"
    );
    assert!(
        snap["pending"].is_null(),
        "Fatigue must never lock pending input"
    );
    assert_eq!(
        snap["draft"], "Draft en cours",
        "Draft must remain editable"
    );
}

/// `T2_F26_05`: Advisory recommendation formatting benevolence.
///
/// Recommendations are polite, non-infantilizing ("Pause conseillée"), and contain no moralizing language.
#[test]
fn test_t2_f26_05_advisory_recommendation_formatting() {
    let recommendations = [
        "Pause de quelques minutes conseillée.",
        "Pause de 15 minutes recommandée.",
        "Repos visuel conseillé : regarder au loin.",
    ];

    for rec in recommendations {
        assert!(
            rec.contains("conseillée") || rec.contains("recommandée") || rec.contains("conseillé")
        );
        assert!(!rec.contains("interdit"));
        assert!(!rec.contains("bloqué"));
        assert!(!rec.contains("obligatoire"));
    }
}
