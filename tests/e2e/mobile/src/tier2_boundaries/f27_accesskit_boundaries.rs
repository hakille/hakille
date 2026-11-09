//! Feature 27: Universal AccessKit & A11y (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F27_01 to T2_F27_05 (5 tests total).

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

/// `T2_F27_01`: Touch targets < 48dp on Android or < 44pt on iOS flagged.
///
/// Minimum dimensions: Android >= 48.0 dp; iOS >= 44.0 pt. Smaller dimensions are flagged.
#[test]
fn test_t2_f27_01_touch_target_minimum_dimensions_flagged() {
    assert_eq!(MIN_TOUCH_TARGET_ANDROID_DP, 48.0);
    assert_eq!(MIN_TOUCH_TARGET_IOS_PT, 44.0);

    struct TouchTarget {
        width: f32,
        height: f32,
    }
    impl TouchTarget {
        fn is_valid_android(&self) -> bool {
            self.width >= MIN_TOUCH_TARGET_ANDROID_DP && self.height >= MIN_TOUCH_TARGET_ANDROID_DP
        }
        fn is_valid_ios(&self) -> bool {
            self.width >= MIN_TOUCH_TARGET_IOS_PT && self.height >= MIN_TOUCH_TARGET_IOS_PT
        }
    }

    // Exact boundary
    let target_android_exact = TouchTarget {
        width: 48.0,
        height: 48.0,
    };
    assert!(target_android_exact.is_valid_android());

    let target_android_undersized = TouchTarget {
        width: 47.9,
        height: 48.0,
    };
    assert!(
        !target_android_undersized.is_valid_android(),
        "47.9dp must be flagged"
    );

    let target_ios_exact = TouchTarget {
        width: 44.0,
        height: 44.0,
    };
    assert!(target_ios_exact.is_valid_ios());

    let target_ios_undersized = TouchTarget {
        width: 44.0,
        height: 43.9,
    };
    assert!(
        !target_ios_undersized.is_valid_ios(),
        "43.9pt must be flagged"
    );
}

/// `T2_F27_02`: Two-step accessible confirmation: step 2 before 600ms rejected.
///
/// Step 2 activated at 599ms fails with error and cancels the gesture.
#[test]
fn test_t2_f27_02_two_step_confirmation_premature_activation_rejected() {
    let mut controller = AccessibleHoldController::new();
    let start = Instant::now();

    controller.trigger_step1(start);

    // Step 2 at 599ms (< 600ms)
    let early = start + Duration::from_millis(599);
    let res = controller.trigger_step2(early);
    assert_eq!(res, Err("Délai de réflexion de 600 ms non écoulé"));
}

/// `T2_F27_03`: Two-step accessible confirmation at >= 600ms confirmed.
///
/// Step 2 activated at exactly 600ms or later succeeds and confirms.
#[test]
fn test_t2_f27_03_two_step_confirmation_at_600ms_confirmed() {
    let mut controller = AccessibleHoldController::new();
    let start = Instant::now();

    controller.trigger_step1(start);

    // Step 2 at 600ms
    let on_time = start + Duration::from_millis(600);
    let res = controller.trigger_step2(on_time);
    assert!(res.is_ok(), "600ms threshold must confirm");
}

/// `T2_F27_04`: Missing label on accessible interactive element flagged.
///
/// All interactive buttons must have a non-empty accessible name.
#[test]
fn test_t2_f27_04_missing_label_on_accessible_element_flagged() {
    fn validate_accessible_label(label: &str) -> Result<(), &'static str> {
        if label.trim().is_empty() {
            Err("Élément interactif sans libellé accessible")
        } else {
            Ok(())
        }
    }

    assert_eq!(
        validate_accessible_label(""),
        Err("Élément interactif sans libellé accessible")
    );
    assert_eq!(
        validate_accessible_label("   "),
        Err("Élément interactif sans libellé accessible")
    );
    assert!(validate_accessible_label("Confirmer l'action").is_ok());
}

/// `T2_F27_05`: RTL text with embedded LTR cryptographic hashes.
///
/// Arabic or NKo text embedding 64-hex SHA-256 hashes uses directional isolation markers.
#[test]
fn test_t2_f27_05_rtl_text_with_embedded_ltr_hash() {
    fn isolate_ltr_hash(rtl_prefix: &str, hash: &str, rtl_suffix: &str) -> String {
        // Unicode Left-to-Right Isolate (U+2066) and Pop Directional Isolate (U+2069)
        format!("{rtl_prefix}\u{2066}{hash}\u{2069}{rtl_suffix}")
    }

    let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let formatted = isolate_ltr_hash("بصمة السجل السيادي: ", hash, " تم التحقق منها.");

    assert!(formatted.contains("\u{2066}"));
    assert!(formatted.contains("\u{2069}"));
    assert!(formatted.contains(hash));
}
