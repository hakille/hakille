use annora_core::{EncryptedBlob, Result, Timestamp, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub user_id: UserId,
    pub embedding: EncryptedBlob,
    pub enrolled_at: Timestamp,
    pub quality_score: f64,
}

#[derive(Default)]
pub struct EnrollmentManager {
    profiles: HashMap<UserId, VoiceProfile>,
}

impl EnrollmentManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enroll_voice(
        &mut self,
        user_id: UserId,
        _embedding: Vec<f32>,
        _encryption_key: &[u8],
    ) -> Result<()> {
        let profile = VoiceProfile {
            user_id: user_id.clone(),
            embedding: EncryptedBlob {
                nonce: vec![0; 12],
                ciphertext: vec![],
            },
            enrolled_at: Timestamp::now(),
            quality_score: 0.95,
        };
        self.profiles.insert(user_id, profile);
        Ok(())
    }

    pub fn get_profile(&self, user_id: &UserId) -> Option<&VoiceProfile> {
        self.profiles.get(user_id)
    }

    pub fn remove_profile(&mut self, user_id: &UserId) -> Result<()> {
        self.profiles.remove(user_id);
        Ok(())
    }

    pub fn list_enrolled_users(&self) -> Vec<UserId> {
        self.profiles.keys().cloned().collect()
    }
}
