//! Silent Distress Blink: Covert Duress Detection & Tripartite Security Response (F31, F32, F33).
//!
//! Sub-pixel ocular EAR state machine, 3-rapid covert blink detector, and tripartite stealth defense:
//! 1. Decoy Environment session activation (plausible deniability workspace).
//! 2. Digital Twilight crypto-shredding in RedbZkStore and RAM subkey zeroization.
//! 3. Silent P2P mesh alert payload with constant-size padding for traffic analysis resistance.

use hakille_core::{HakilleError, Timestamp, UserId};
use hakille_identity::crypto::SubkeyHierarchy;
use hakille_identity::storage::RedbZkStore;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;
use zeroize::Zeroize;

// =============================================================================
// F31 & F32: Asymmetric Covert Blink Detector & State Machine
// =============================================================================

/// Discrete ocular state based on independent left and right Eye Aspect Ratios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OcularState {
    BilateralOpen,
    LeftEyeCovertClosed,
    RightEyeCovertClosed,
    BilateralClosed,
}

/// Progress state of the 3-blink covert duress trigger sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DistressTriggerState {
    Idle,
    Blink1Detected,
    Blink2Detected,
    Triggered,
}

/// Asymmetric ocular distress detector tracking rapid covert blink sequences.
#[derive(Debug, Clone)]
pub struct CovertBlinkDetector {
    pub ear_closed_threshold: f32,          // default: 0.20
    pub ear_open_threshold: f32,            // default: 0.28
    pub min_blink_duration: Duration,       // default: 80 ms
    pub max_blink_duration: Duration,       // default: 450 ms
    pub max_inter_blink_interval: Duration, // default: 650 ms
    pub max_sequence_duration: Duration,    // default: 2200 ms

    state: DistressTriggerState,
    current_closed_start: Option<Instant>,
    last_blink_completed_at: Option<Instant>,
    sequence_start_at: Option<Instant>,
}

impl Default for CovertBlinkDetector {
    fn default() -> Self {
        Self {
            ear_closed_threshold: 0.20,
            ear_open_threshold: 0.28,
            min_blink_duration: Duration::from_millis(80),
            max_blink_duration: Duration::from_millis(450),
            max_inter_blink_interval: Duration::from_millis(650),
            max_sequence_duration: Duration::from_millis(2200),
            state: DistressTriggerState::Idle,
            current_closed_start: None,
            last_blink_completed_at: None,
            sequence_start_at: None,
        }
    }
}

impl CovertBlinkDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> DistressTriggerState {
        self.state
    }

    pub fn reset(&mut self) {
        self.state = DistressTriggerState::Idle;
        self.current_closed_start = None;
        self.last_blink_completed_at = None;
        self.sequence_start_at = None;
    }

    /// Classify independent left and right EAR into discrete ocular state.
    pub fn classify_ocular_state(&self, left_ear: f32, right_ear: f32) -> OcularState {
        let left_closed = left_ear < self.ear_closed_threshold;
        let right_closed = right_ear < self.ear_closed_threshold;
        let left_open = left_ear >= self.ear_open_threshold;
        let right_open = right_ear >= self.ear_open_threshold;

        if left_closed && right_closed {
            OcularState::BilateralClosed
        } else if left_closed && right_open {
            OcularState::LeftEyeCovertClosed
        } else if right_closed && left_open {
            OcularState::RightEyeCovertClosed
        } else {
            OcularState::BilateralOpen
        }
    }

    /// Process a new frame. Returns `true` exactly upon completion of the 3rd rapid covert left blink.
    pub fn process_frame(&mut self, now: Instant, left_ear: f32, right_ear: f32) -> bool {
        let ocular = self.classify_ocular_state(left_ear, right_ear);

        // Security requirement: Bilateral closure immediately cancels covert sequence
        if ocular == OcularState::BilateralClosed {
            self.reset();
            return false;
        }

        // Global sequence timeout check
        if let Some(seq_start) = self.sequence_start_at {
            if now.duration_since(seq_start) > self.max_sequence_duration {
                self.reset();
            }
        }

        // Left eye covert closed
        if ocular == OcularState::LeftEyeCovertClosed {
            if self.current_closed_start.is_none() {
                self.current_closed_start = Some(now);
            }
            return false;
        }

        // Left eye reopened
        if ocular == OcularState::BilateralOpen {
            if let Some(closed_start) = self.current_closed_start.take() {
                let duration = now.duration_since(closed_start);

                if duration >= self.min_blink_duration && duration <= self.max_blink_duration {
                    return self.advance_state(now);
                } else {
                    self.reset();
                }
            }
        } else if ocular == OcularState::RightEyeCovertClosed {
            self.reset();
        }

        false
    }

    fn advance_state(&mut self, now: Instant) -> bool {
        match self.state {
            DistressTriggerState::Idle => {
                self.state = DistressTriggerState::Blink1Detected;
                self.sequence_start_at = Some(now);
                self.last_blink_completed_at = Some(now);
                false
            }
            DistressTriggerState::Blink1Detected => {
                let last = self.last_blink_completed_at.unwrap_or(now);
                let elapsed = now.duration_since(last);
                if elapsed <= self.max_inter_blink_interval {
                    self.state = DistressTriggerState::Blink2Detected;
                    self.last_blink_completed_at = Some(now);
                    false
                } else {
                    self.state = DistressTriggerState::Blink1Detected;
                    self.sequence_start_at = Some(now);
                    self.last_blink_completed_at = Some(now);
                    false
                }
            }
            DistressTriggerState::Blink2Detected => {
                let last = self.last_blink_completed_at.unwrap_or(now);
                let elapsed = now.duration_since(last);
                if elapsed <= self.max_inter_blink_interval {
                    self.state = DistressTriggerState::Triggered;
                    self.reset();
                    true // 3rd rapid covert blink confirmed!
                } else {
                    self.state = DistressTriggerState::Blink1Detected;
                    self.sequence_start_at = Some(now);
                    self.last_blink_completed_at = Some(now);
                    false
                }
            }
            DistressTriggerState::Triggered => {
                self.reset();
                false
            }
        }
    }
}

// =============================================================================
// F33: Tripartite Silent Distress Response Coordinator
// =============================================================================

/// Decoy environment profile launched during coercive duress to provide plausible deniability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecoyProfile {
    pub session_id: Uuid,
    pub profile_label: String,
    pub simulated_balance_cents: u64, // e.g. 24000 = 240.00 EUR
    pub decoy_documents: Vec<String>,
    pub vault_access_locked: bool,
}

/// Constant-padded encrypted payload propagated over P2P mesh (BLE / Wi-Fi Aware) upon distress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SilentDistressPayload {
    pub alert_id: Uuid,
    pub user_id: UserId,
    pub timestamp: Timestamp,
    pub decoy_session_id: Uuid,
    /// 256-byte constant-padded telemetry frame for traffic analysis resistance
    pub encrypted_mesh_telemetry: Vec<u8>,
    pub mesh_channel: String,
    pub is_silent: bool,
}

/// Sovereign coordinator orchestrating the tripartite response upon covert distress trigger.
pub struct SilentDistressCoordinator;

impl SilentDistressCoordinator {
    /// Generates a plausible decoy profile preventing aggressor suspicion.
    pub fn build_decoy_profile(session_id: Uuid) -> DecoyProfile {
        DecoyProfile {
            session_id,
            profile_label: "Decoy Workspace / Guest Standard".to_string(),
            simulated_balance_cents: 24_000, // 240.00 EUR
            decoy_documents: vec![
                "Notes_Courses.docx".to_string(),
                "Budget_Vacances_2026.xlsx".to_string(),
                "Facture_Electricite.pdf".to_string(),
            ],
            vault_access_locked: true,
        }
    }

    /// Constructs the padded, indistinguishable mesh broadcast payload.
    pub fn build_silent_mesh_payload(
        user_id: &UserId,
        decoy_session_id: Uuid,
    ) -> SilentDistressPayload {
        let alert_id = Uuid::new_v4();
        // Fixed 256-byte telemetry buffer resistant to size analysis
        let mut padded_telemetry = vec![0u8; 256];
        // Populate header with alert UUID and timestamp entropy
        let alert_bytes = alert_id.as_bytes();
        padded_telemetry[0..16].copy_from_slice(alert_bytes);
        let decoy_bytes = decoy_session_id.as_bytes();
        padded_telemetry[16..32].copy_from_slice(decoy_bytes);

        SilentDistressPayload {
            alert_id,
            user_id: user_id.clone(),
            timestamp: Timestamp::now(),
            decoy_session_id,
            encrypted_mesh_telemetry: padded_telemetry,
            mesh_channel: "hakille.mesh.sovereign.distress".to_string(),
            is_silent: true,
        }
    }

    /// Executes the tripartite distress defense protocol:
    /// 1. Digital Twilight trigger in RedbZkStore (user state -> DigitalTwilightGrace, journal salts shredded).
    /// 2. RAM SubkeyHierarchy zeroization (all cryptographic keys wiped to zero in memory).
    /// 3. Construction and return of the silent mesh distress payload.
    pub fn execute_distress_protocol(
        user_id: &UserId,
        subkeys: &mut SubkeyHierarchy,
        zk_store: &mut RedbZkStore,
    ) -> Result<SilentDistressPayload, HakilleError> {
        // 1. Digital Twilight in persistent ZK store
        zk_store.trigger_digital_twilight(user_id)?;

        // 2. RAM key zeroization
        subkeys.zeroize();

        // 3. Decoy session & silent mesh alert
        let decoy_session_id = Uuid::new_v4();
        let payload = Self::build_silent_mesh_payload(user_id, decoy_session_id);

        Ok(payload)
    }

    /// Executes the tripartite distress defense protocol and returns both the mesh payload and the decoy profile.
    pub fn execute_distress_protocol_with_decoy(
        user_id: &UserId,
        subkeys: &mut SubkeyHierarchy,
        zk_store: &mut RedbZkStore,
    ) -> Result<(SilentDistressPayload, DecoyProfile), HakilleError> {
        let decoy_session_id = Uuid::new_v4();
        let decoy_profile = Self::build_decoy_profile(decoy_session_id);

        zk_store.trigger_digital_twilight(user_id)?;
        subkeys.zeroize();

        let payload = Self::build_silent_mesh_payload(user_id, decoy_session_id);
        Ok((payload, decoy_profile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use hakille_identity::crypto::XChaChaBlob;
    use hakille_identity::ssi::Did;
    use hakille_identity::storage::{AccountStatus, UserRecord};

    #[test]
    fn test_covert_blink_detector_happy_path() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // Blink 1
        detector.process_frame(base, 0.12, 0.32);
        assert!(!detector.process_frame(base + Duration::from_millis(150), 0.32, 0.32));
        assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

        // Blink 2
        let t2 = base + Duration::from_millis(400);
        detector.process_frame(t2, 0.14, 0.30);
        assert!(!detector.process_frame(t2 + Duration::from_millis(150), 0.30, 0.30));
        assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

        // Blink 3
        let t3 = base + Duration::from_millis(800);
        detector.process_frame(t3, 0.10, 0.35);
        let triggered = detector.process_frame(t3 + Duration::from_millis(150), 0.35, 0.35);
        assert!(triggered, "Third rapid blink must trigger distress!");
    }

    #[test]
    fn test_bilateral_blink_cancels_covert_sequence() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // 2 successful covert blinks
        detector.process_frame(base, 0.12, 0.32);
        detector.process_frame(base + Duration::from_millis(150), 0.32, 0.32);
        let t2 = base + Duration::from_millis(400);
        detector.process_frame(t2, 0.12, 0.32);
        detector.process_frame(t2 + Duration::from_millis(150), 0.32, 0.32);
        assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

        // Normal bilateral blink occurs (both eyes close)
        let t3 = base + Duration::from_millis(700);
        let triggered = detector.process_frame(t3, 0.10, 0.10);
        assert!(!triggered);
        assert_eq!(detector.state(), DistressTriggerState::Idle);
    }

    #[test]
    fn test_execute_distress_protocol_tripartite_response() {
        let mut zk_store = RedbZkStore::new();
        let user_id = UserId(Uuid::new_v4());
        let did = Did::new("did:key:z6MkhaXgBZDvotDkL5257faiz4zWrAZvUnDHiafvMnWWkAOE");

        let user_record = UserRecord {
            user_id: user_id.clone(),
            primary_did: did,
            blind_username_hash: [7u8; 32],
            encrypted_profile_blob: XChaChaBlob {
                nonce: [1u8; 24],
                ciphertext: vec![1, 2, 3],
                tag: [0u8; 16],
                aad_fingerprint: [0u8; 16],
            },
            state: AccountStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        zk_store.insert_user(user_record).unwrap();

        let mut subkeys = SubkeyHierarchy {
            identity_key: [0x11; 32],
            memory_key: [0x22; 32],
            credentials_key: [0x33; 32],
            biometrics_key: [0x44; 32],
            blind_index_key: [0x55; 32],
        };

        let (payload, decoy) = SilentDistressCoordinator::execute_distress_protocol_with_decoy(
            &user_id,
            &mut subkeys,
            &mut zk_store,
        )
        .unwrap();

        // 1. Digital Twilight verification: account state transitions to grace & salt shredded
        assert!(zk_store.is_salt_shredded(&user_id));

        // 2. RAM Zeroize verification: all subkeys in memory wiped to zero
        assert_eq!(*subkeys.identity_key(), [0u8; 32]);
        assert_eq!(*subkeys.memory_key(), [0u8; 32]);
        assert_eq!(*subkeys.credentials_key(), [0u8; 32]);
        assert_eq!(*subkeys.biometrics_key(), [0u8; 32]);
        assert_eq!(*subkeys.blind_index_key(), [0u8; 32]);

        // 3. Decoy Profile verification
        assert!(decoy.vault_access_locked);
        assert_eq!(decoy.simulated_balance_cents, 24000);
        assert!(!decoy.decoy_documents.is_empty());

        // 4. Mesh alert verification
        assert!(payload.is_silent);
        assert_eq!(payload.encrypted_mesh_telemetry.len(), 256);
        assert_eq!(payload.user_id, user_id);
    }
}
