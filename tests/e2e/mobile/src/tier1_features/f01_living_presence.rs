//! Feature 01: Living Presence & 21 Kora Wires (Milestone M1 / Spec §1, §2, §7, §20)
//!
//! Specifications: T1_F01_01 to T1_F01_05
//!
//! Verifies:
//! - 21-wire telemetry vs 7-wire compact fallback based on viewport size.
//! - Brand identity invariants (lowercase name, clearspace, signature, tracking).
//! - Speech rate clamping and brevity thresholds for sovereign voice.
//! - Uncertainty-proportioned voice hedging (no emojis, honest hedges).
//! - Real-time PresenceState propagation into MobileSnapshot.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_design::brand::{
    BRAND_NAME, BRAND_SIGNATURE, CLEARSPACE_DIAMETERS, LOGOTYPE_TRACKING_PCT, MIN_SIZE_LOGOTYPE_PX,
    MIN_SIZE_SIGN_PX, WIRES_FULL, WIRES_SMALL, WIRES_SMALL_BELOW_PX, wire_count,
};
use hakille_design::voice::{
    DEFAULT_SPEECH_RATE, NO_ANSWER_SILENCE_SECS, SPEECH_RATE_MAX, SPEECH_RATE_MIN,
    clamp_speech_rate, speak_short, spoken_list,
};
use hakille_ui_protocol::{KernelToUi, PresenceState};
use serde_json::json;

/// `T1_F01_01`: Wire count resolution based on viewport bounds.
///
/// Full kora waveform renders 21 wires when viewport width is >= 24 px.
/// Falls back to 7 wires when viewport width is < 24 px.
#[test]
fn test_t1_f01_01_wire_count_resolution_based_on_viewport_bounds() {
    assert_eq!(WIRES_FULL, 21, "Full kora presence must have 21 wires");
    assert_eq!(WIRES_SMALL, 7, "Compact presence must have 7 wires");
    assert_eq!(
        WIRES_SMALL_BELOW_PX, 24,
        "Threshold for compact presence is 24 px"
    );

    // Viewport >= 24 px -> 21 wires
    assert_eq!(wire_count(24), 21, "At exact 24 px threshold: 21 wires");
    assert_eq!(wire_count(48), 21, "At 48 px viewport: 21 wires");
    assert_eq!(wire_count(360), 21, "At mobile width 360 px: 21 wires");
    assert_eq!(wire_count(1080), 21, "At desktop width 1080 px: 21 wires");

    // Viewport < 24 px -> 7 wires
    assert_eq!(
        wire_count(23),
        7,
        "At 23 px (just below threshold): 7 wires"
    );
    assert_eq!(wire_count(16), 7, "At 16 px icon size: 7 wires");
    assert_eq!(wire_count(0), 7, "At 0 px edge case: 7 wires");
}

/// `T1_F01_02`: Brand identity constraints and clearspace rules.
///
/// Brand name is strictly lowercase "hakille", signature contains sovereign intelligence,
/// clearspace is 2x diameter, and min sizes prevent illegible scaling.
#[test]
fn test_t1_f01_02_brand_identity_constraints_and_clearspace() {
    assert_eq!(
        BRAND_NAME, "hakille",
        "Brand name must strictly be all-lowercase 'hakille'"
    );
    assert_eq!(
        BRAND_NAME,
        BRAND_NAME.to_lowercase(),
        "Brand name must contain no uppercase letters"
    );

    assert_eq!(
        BRAND_SIGNATURE, "Hakille — intelligence souveraine",
        "Brand signature must declare sovereign intelligence"
    );

    assert_eq!(
        CLEARSPACE_DIAMETERS, 2,
        "Protection clearspace must be 2x the lueur diameter"
    );
    assert_eq!(
        MIN_SIZE_SIGN_PX, 16,
        "Minimum standalone sign size is 16 px"
    );
    assert_eq!(MIN_SIZE_LOGOTYPE_PX, 64, "Minimum logotype size is 64 px");
    assert_eq!(
        LOGOTYPE_TRACKING_PCT, -1,
        "Logotype letter-spacing tracking is -1%"
    );
}

/// `T1_F01_03`: Speech rate clamping and spoken brevity thresholds.
///
/// Default speech rate is 1.0, clamped between [0.7, 1.6]. Spoken lists are capped
/// at 3 items with journal overflow. Silence without answer times out at 8 seconds.
#[test]
fn test_t1_f01_03_speech_rate_clamping_and_spoken_brevity() {
    assert_eq!(DEFAULT_SPEECH_RATE, 1.0);
    assert_eq!(SPEECH_RATE_MIN, 0.7);
    assert_eq!(SPEECH_RATE_MAX, 1.6);
    assert_eq!(NO_ANSWER_SILENCE_SECS, 8);

    // Clamping boundaries
    assert_eq!(clamp_speech_rate(0.1), 0.7, "Underflow clamped to 0.7");
    assert_eq!(clamp_speech_rate(0.7), 0.7, "Lower bound preserved");
    assert_eq!(clamp_speech_rate(1.2), 1.2, "Normal rate preserved");
    assert_eq!(clamp_speech_rate(1.6), 1.6, "Upper bound preserved");
    assert_eq!(clamp_speech_rate(2.5), 1.6, "Overflow clamped to 1.6");

    // Spoken list cap
    let short_list = vec!["Courrier A".into(), "Courrier B".into()];
    let (spoken_short, overflow_short) = spoken_list(&short_list);
    assert_eq!(spoken_short.len(), 2);
    assert!(
        overflow_short.is_none(),
        "Lists <= 3 items must have no overflow"
    );

    let long_list = vec![
        "Tâche 1".into(),
        "Tâche 2".into(),
        "Tâche 3".into(),
        "Tâche 4".into(),
        "Tâche 5".into(),
    ];
    let (spoken_long, overflow_long) = spoken_list(&long_list);
    assert_eq!(spoken_long.len(), 3, "Spoken list must be truncated to 3");
    let overflow_msg = overflow_long.expect("Overflow message must be generated");
    assert!(
        overflow_msg.contains("Journal"),
        "Overflow must refer to Journal: {overflow_msg}"
    );
}

/// `T1_F01_04`: Uncertainty-proportioned voice hedging without emojis or AI excuses.
///
/// Tone reflects true confidence: assertive if >= 0.85, conditional if >= 0.50,
/// explicitly uncertain if < 0.50. Never uses emojis or 'En tant qu'IA'.
#[test]
fn test_t1_f01_04_uncertainty_proportioned_voice_hedging() {
    // High confidence (>= 0.85)
    let high = speak_short("Portail fermé", 0.95, "Rien ne quitte ton appareil.");
    assert!(
        high.starts_with("Portail fermé."),
        "High confidence states result first"
    );
    assert!(
        high.contains("Rien ne quitte ton appareil."),
        "High confidence appends detail hint"
    );
    assert!(
        !high.contains("Je pense que") && !high.contains("Je ne suis pas sûr"),
        "High confidence must not hedge unnecessarily"
    );

    // Medium confidence (0.50 <= c < 0.85)
    let med = speak_short("la réunion commence à 14 h", 0.72, "Détail au calendrier.");
    assert!(
        med.contains("Je pense que"),
        "Medium confidence must hedge: {med}"
    );
    assert!(
        med.contains("Tu peux confirmer ?"),
        "Medium confidence asks user confirmation: {med}"
    );

    // Low confidence (< 0.50)
    let low = speak_short("le contact est Paul", 0.35, "Vérification au carnet.");
    assert!(
        low.starts_with("Je ne suis pas sûr."),
        "Low confidence begins with honest uncertainty: {low}"
    );
    assert!(
        low.contains("Tu peux confirmer ?"),
        "Low confidence asks user confirmation: {low}"
    );

    // Zero simulated politeness / honesty constraints
    for text in [&high, &med, &low] {
        assert!(
            !text.contains("En tant qu'IA"),
            "Must never contain AI trope: {text}"
        );
        assert!(
            !text.contains("Désolé") && !text.contains("Pardon"),
            "Must never apologize: {text}"
        );
        assert!(
            !text.chars().any(|c| c >= '\u{1F600}' && c <= '\u{1F64F}'),
            "Must never contain emojis: {text}"
        );
    }
}

/// `T1_F01_05`: End-to-end presence state mapping across MobileBridge.
///
/// Gateway dispatches presence transitions (Listening -> Acting -> Idle)
/// and verifies they are faithfully updated in the MobileSnapshot.
#[tokio::test]
async fn test_t1_f01_05_presence_state_propagation_to_mobile_snapshot() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    // Verify initial presence is None
    let initial_snapshot = rig.snapshot(&bridge);
    assert!(
        initial_snapshot["presence"].is_null(),
        "Initial presence state must be null"
    );

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept gateway stream");
        let step = DaemonStep::status()
            .before(vec![KernelToUi::Presence(PresenceState::Listening)])
            .after(vec![KernelToUi::Presence(PresenceState::Acting)]);
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("execute status with presence transitions");
    });

    let (status_code, result) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    assert_eq!(status_code, 200);
    assert_eq!(result["ok"], true);

    // Bridge applies frames in order: Acting was in after frames
    let snapshot = rig.snapshot(&bridge);
    assert_eq!(
        snapshot["presence"], "Acting",
        "Presence must transition to Acting in snapshot"
    );

    daemon_task.await.expect("daemon task completion");
}
