pub mod continuous;
pub mod enrollment;
pub mod guest;
pub mod household;
pub mod liveness;
pub mod minor;
pub mod wallet;

pub use continuous::*;
pub use enrollment::*;
pub use guest::*;
pub use household::*;
pub use liveness::*;
pub use minor::*;
pub use wallet::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, UserId};
    use uuid::Uuid;

    #[test]
    fn test_guest_session_boundaries_and_rejections() {
        let mut manager = GuestSessionManager::new();
        let host_id = UserId(Uuid::new_v4());

        // Attempting to grant Wallet to guest MUST be rejected
        let invalid = manager.create_guest_session(
            "guest-bob",
            host_id.clone(),
            vec![CapabilityScope::Wallet],
            "Tester",
            60,
        );
        assert!(invalid.is_err(), "Granting Wallet to a guest must fail");

        // Valid guest session with Audio and Network
        let valid = manager.create_guest_session(
            "guest-alice",
            host_id,
            vec![CapabilityScope::Audio, CapabilityScope::Network],
            "Invité week-end",
            120,
        );
        assert!(valid.is_ok());

        assert!(manager.can_guest_access("guest-alice", &CapabilityScope::Audio));
        assert!(!manager.can_guest_access("guest-alice", &CapabilityScope::Filesystem));
        assert!(!manager.can_guest_access("guest-alice", &CapabilityScope::Wallet));
    }

    #[test]
    fn test_voice_liveness_detector() {
        let detector = VoiceLivenessDetector::default();

        // 1. Natural audio with realistic syllabic modulation and micro-jitter
        let mut natural_audio = Vec::with_capacity(3200);
        for i in 0..3200 {
            let syllable_envelope = ((i as f32 * 0.003).sin()).abs() + 0.1;
            let jitter = ((i as f32 * 0.07).sin()) * 0.1;
            let val = (i as f32 * 0.15 + jitter).sin() * 0.5 * syllable_envelope;
            natural_audio.push(val);
        }
        let result_live = detector.analyze_pcm_frame(&natural_audio);
        assert!(result_live.is_live_human);

        // 2. Unnatural perfectly static flat tone (synthetic vocoder artifact)
        let flat_audio: Vec<f32> = (0..3200).map(|_| 0.25).collect();
        let result_flat = detector.analyze_pcm_frame(&flat_audio);
        assert!(!result_flat.is_live_human);
        assert!(result_flat.synthetic_voice_probability > 0.6);
    }
}
