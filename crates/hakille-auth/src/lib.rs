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
    use hakille_core::{CapabilityScope, UserId};
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

        // 3. sample_rate != 16000 refusé (avec warn), pas de faux spectral.
        assert!(
            detector
                .analyze_pcm_frame_with_rate(&natural_audio, 8000)
                .is_err()
        );
        assert!(
            detector
                .analyze_pcm_frame_with_rate(&natural_audio, 16000)
                .is_ok()
        );
        assert!(!result_flat.explanation.contains("spectrales"));
    }

    #[test]
    fn test_guest_duration_cap_and_duplicate() {
        let mut manager = GuestSessionManager::new();
        let host = UserId(Uuid::new_v4());
        // > 24h refusé.
        assert!(
            manager
                .create_guest_session(
                    "g-long",
                    host.clone(),
                    vec![CapabilityScope::Audio],
                    "trop long",
                    1441,
                )
                .is_err()
        );
        assert!(
            manager
                .create_guest_session(
                    "g-ok",
                    host.clone(),
                    vec![CapabilityScope::Audio],
                    "ok",
                    1440,
                )
                .is_ok()
        );
        // Doublon refusé.
        assert!(
            manager
                .create_guest_session(
                    "g-ok",
                    host.clone(),
                    vec![CapabilityScope::Audio],
                    "dup",
                    60,
                )
                .is_err()
        );
        // Id vide → UUID unique généré.
        let auto = manager
            .create_guest_session("", host, vec![CapabilityScope::Audio], "auto", 60)
            .unwrap();
        assert!(!auto.guest_id.is_empty());
    }

    #[test]
    fn test_continuous_timeout_and_last_verified() {
        use hakille_core::Timestamp;
        let user = UserId(Uuid::new_v4());
        let mut sess = ContinuousAuth::start_session(user, 0.9);
        assert!(ContinuousAuth::is_valid(&sess, 0.8));
        // update_confidence rafraîchit last_verified.
        let before = sess.last_verified.clone();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let v = vec![1.0, 0.0];
        ContinuousAuth::update_confidence(&mut sess, &v, &v);
        assert!(sess.last_verified.0 >= before.0);
        // Session périmée (> 15 min) invalide même avec bon score.
        sess.last_verified = Timestamp(chrono::Utc::now() - chrono::Duration::minutes(16));
        sess.confidence_score = 0.95;
        assert!(!ContinuousAuth::is_valid(&sess, 0.8));
    }

    #[test]
    fn test_household_enforce_owner() {
        use hakille_core::NodeId;
        let mut hh = Household::new(NodeId(Uuid::new_v4()));
        let owner_id = UserId(Uuid::new_v4());
        let other_id = UserId(Uuid::new_v4());
        // Bootstrap : premier Owner inséré directement (foyer vide).
        hh.members.insert(
            owner_id.clone(),
            HouseholdMember {
                user_id: owner_id.clone(),
                display_name: "Owner".to_string(),
                role: HouseholdRole::Owner,
                restrictions: vec![],
            },
        );
        assert!(hh.enforce_owner(&owner_id).is_ok());
        assert!(hh.enforce_owner(&other_id).is_err());
        assert!(
            hh.add_member(
                &other_id,
                HouseholdMember {
                    user_id: other_id.clone(),
                    display_name: "X".to_string(),
                    role: HouseholdRole::Adult,
                    restrictions: vec![],
                },
            )
            .is_err()
        );
        assert!(
            hh.add_member(
                &owner_id,
                HouseholdMember {
                    user_id: other_id.clone(),
                    display_name: "X".to_string(),
                    role: HouseholdRole::Adult,
                    restrictions: vec![],
                },
            )
            .is_ok()
        );
        assert!(hh.remove_member(&other_id, &other_id).is_err());
        assert!(hh.remove_member(&owner_id, &other_id).is_ok());
    }

    #[test]
    fn test_wallet_persist_and_daily_cap() {
        use hakille_core::{SpendingCategory, Timestamp};
        let mut manager = WalletManager::new();
        let user = UserId(Uuid::new_v4());
        let mut wallet = manager.create_wallet_cap(
            user,
            10000,
            "EUR".into(),
            SpendingCategory::Food,
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
        );
        manager
            .record_spend(
                &mut wallet,
                3000,
                "Lunch".into(),
                "EUR",
                SpendingCategory::Food,
            )
            .unwrap();
        assert!(manager.check_daily_cap(1000, 5000).is_ok());
        assert!(manager.check_daily_cap(3000, 5000).is_err());
        let path = std::env::temp_dir().join(format!("wallet_{}.json", Uuid::new_v4()));
        manager.save_to_file(&path).unwrap();
        let reloaded = WalletManager::load_from_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(reloaded.remaining_budget(&wallet), 7000);
        assert_eq!(reloaded.history().len(), 1);
    }
}
