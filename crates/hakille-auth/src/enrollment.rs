use crate::face::FaceProfile;
use hakille_core::{EncryptedBlob, Result, Timestamp, UserId, decrypt_at_rest, encrypt_at_rest};
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
    face_profiles: HashMap<UserId, FaceProfile>,
}

/// Score de qualité déterministe d'un embedding vocal, dans [0, 1].
/// Combine la couverture dimensionnelle (`len` vs 256 dims de référence) et
/// l'énergie RMS (vecteur nul ou quasi-silencieux ⇒ score ~0).
pub fn compute_quality(embedding: &[f32]) -> f64 {
    if embedding.is_empty() {
        return 0.0;
    }
    if embedding.iter().any(|x| !x.is_finite()) {
        return 0.0;
    }
    let len = embedding.len() as f64;
    let size_factor = (len / 256.0).min(1.0);
    let sum_sq: f64 = embedding.iter().map(|x| (*x as f64) * (*x as f64)).sum();
    let rms = (sum_sq / len).sqrt();
    if rms == 0.0 {
        return 0.0;
    }
    // RMS typique d'un embedding normalisé ≈ 1/sqrt(len)..1 ; on sature à 1.
    let energy = (rms * 2.0).min(1.0);
    (0.5 * size_factor + 0.5 * energy).clamp(0.0, 1.0)
}

/// Deterministic quality score for a 512-dim face embedding in [0.0, 1.0].
pub fn compute_face_quality(embedding: &[f32]) -> f64 {
    if embedding.is_empty() {
        return 0.0;
    }
    if embedding.iter().any(|x| !x.is_finite()) {
        return 0.0;
    }
    let len = embedding.len() as f64;
    let size_factor = (len / 512.0).min(1.0);
    let sum_sq: f64 = embedding.iter().map(|x| (*x as f64) * (*x as f64)).sum();
    let norm = sum_sq.sqrt();
    if norm < 1e-4 {
        return 0.0;
    }
    // Normalization deviation penalty
    let norm_factor = (1.0 - (norm - 1.0).abs()).clamp(0.0, 1.0);
    (0.5 * size_factor + 0.5 * norm_factor).clamp(0.0, 1.0)
}

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for x in embedding {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

fn embedding_from_bytes(bytes: &[u8]) -> Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return Err(hakille_core::HakilleError::CryptoError(
            "Longueur du gabarit déchiffré invalide (pas un multiple de 4)".into(),
        ));
    }
    let (chunks, _) = bytes.as_chunks::<4>();
    Ok(chunks.iter().map(|c| f32::from_le_bytes(*c)).collect())
}

impl EnrollmentManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enroll_voice(
        &mut self,
        user_id: UserId,
        embedding: Vec<f32>,
        encryption_key: &[u8],
    ) -> Result<()> {
        // Vrai chiffrement ChaCha20Poly1305 : nonce aléatoire 96 bits tiré
        // par `encrypt_at_rest`, clé 32 octets vérifiée (erreur sinon).
        let plaintext = embedding_to_bytes(&embedding);
        let blob = encrypt_at_rest(&plaintext, encryption_key)?;
        let profile = VoiceProfile {
            user_id: user_id.clone(),
            embedding: blob,
            enrolled_at: Timestamp::now(),
            quality_score: compute_quality(&embedding),
        };
        self.profiles.insert(user_id, profile);
        Ok(())
    }

    /// Enroll a protected face profile
    pub fn enroll_face(&mut self, profile: FaceProfile) -> Result<()> {
        self.face_profiles.insert(profile.user_id.clone(), profile);
        Ok(())
    }

    /// Déchiffre le gabarit vocal stocké (vérification, ré-enrôlement).
    pub fn decrypt_embedding(&self, user_id: &UserId, key: &[u8]) -> Result<Vec<f32>> {
        let profile = self
            .profiles
            .get(user_id)
            .ok_or(hakille_core::HakilleError::AuthenticationFailed)?;
        let plaintext = decrypt_at_rest(&profile.embedding, key)?;
        embedding_from_bytes(&plaintext)
    }

    pub fn get_profile(&self, user_id: &UserId) -> Option<&VoiceProfile> {
        self.profiles.get(user_id)
    }

    pub fn get_voice_profile(&self, user_id: &UserId) -> Option<&VoiceProfile> {
        self.profiles.get(user_id)
    }

    pub fn get_face_profile(&self, user_id: &UserId) -> Option<&FaceProfile> {
        self.face_profiles.get(user_id)
    }

    pub fn remove_voice_profile(&mut self, user_id: &UserId) -> Result<()> {
        self.profiles.remove(user_id);
        Ok(())
    }

    pub fn remove_face_profile(&mut self, user_id: &UserId) -> Result<()> {
        self.face_profiles.remove(user_id);
        Ok(())
    }

    /// Removes both voice and face biometric profiles for a user
    pub fn remove_profile(&mut self, user_id: &UserId) -> Result<()> {
        self.profiles.remove(user_id);
        self.face_profiles.remove(user_id);
        Ok(())
    }

    /// Lists all enrolled users across voice and face modalities
    pub fn list_enrolled_users(&self) -> Vec<UserId> {
        let mut users: std::collections::HashSet<UserId> = self.profiles.keys().cloned().collect();
        users.extend(self.face_profiles.keys().cloned());
        users.into_iter().collect()
    }

    pub fn list_enrolled_voice_users(&self) -> Vec<UserId> {
        self.profiles.keys().cloned().collect()
    }

    pub fn list_enrolled_face_users(&self) -> Vec<UserId> {
        self.face_profiles.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_embedding(len: usize, scale: f32) -> Vec<f32> {
        (0..len)
            .map(|i| ((i as f32 * 0.37).sin() + 1.0) * 0.25 * scale)
            .collect()
    }

    #[test]
    fn test_enroll_face_and_list() {
        let mut manager = EnrollmentManager::new();
        let user_id = UserId(Uuid::new_v4());
        let face_prof = FaceProfile::new(
            user_id.clone(),
            vec![0x42; 128],
            1,
            None,
            0.92,
        );

        manager.enroll_face(face_prof).unwrap();
        assert!(manager.get_face_profile(&user_id).is_some());
        assert!(manager.list_enrolled_face_users().contains(&user_id));
        assert!(manager.list_enrolled_users().contains(&user_id));

        manager.remove_face_profile(&user_id).unwrap();
        assert!(manager.get_face_profile(&user_id).is_none());
    }

    #[test]
    fn test_compute_face_quality() {
        assert_eq!(compute_face_quality(&[]), 0.0);
        let valid_512 = vec![1.0 / (512.0f32).sqrt(); 512];
        let q = compute_face_quality(&valid_512);
        assert!(q > 0.90, "Normalized 512-dim embedding should have high quality: {}", q);
    }

    #[test]
    fn test_enroll_encrypts_template_with_random_nonce() {
        let mut manager = EnrollmentManager::new();
        let key = [7u8; 32];
        let embedding = sample_embedding(256, 1.0);
        let user = UserId(Uuid::now_v7());

        manager
            .enroll_voice(user.clone(), embedding.clone(), &key)
            .unwrap();
        let profile = manager.get_profile(&user).unwrap();

        // Vrai ciphertext non vide + nonce 96 bits non nul.
        assert!(!profile.embedding.ciphertext.is_empty());
        assert_eq!(profile.embedding.nonce.len(), 12);
        assert!(profile.embedding.nonce.iter().any(|&b| b != 0));
        let first_nonce = profile.embedding.nonce.clone();
        let first_ciphertext = profile.embedding.ciphertext.clone();

        // Deux enrôlements ⇒ nonces/chiffres différents (aléa).
        let user2 = UserId(Uuid::now_v7());
        manager
            .enroll_voice(user2.clone(), embedding.clone(), &key)
            .unwrap();
        let profile2 = manager.get_profile(&user2).unwrap();
        assert_ne!(first_nonce, profile2.embedding.nonce);
        assert_ne!(first_ciphertext, profile2.embedding.ciphertext);

        // Roundtrip déchiffrement.
        let recovered = manager.decrypt_embedding(&user, &key).unwrap();
        assert_eq!(recovered.len(), embedding.len());
        for (a, b) in recovered.iter().zip(embedding.iter()) {
            assert!((a - b).abs() < 1e-6);
        }

        // Mauvaise clé ⇒ échec.
        assert!(manager.decrypt_embedding(&user, &[9u8; 32]).is_err());
    }

    #[test]
    fn test_enroll_rejects_bad_key_length() {
        let mut manager = EnrollmentManager::new();
        let bad_key = [1u8; 16];
        assert!(
            manager
                .enroll_voice(UserId(Uuid::now_v7()), sample_embedding(64, 1.0), &bad_key)
                .is_err()
        );
    }

    #[test]
    fn test_quality_score_is_computed_not_constant() {
        // Vide / nul ⇒ 0.
        assert_eq!(compute_quality(&[]), 0.0);
        assert_eq!(compute_quality(&vec![0.0; 128]), 0.0);
        assert_eq!(compute_quality(&[f32::NAN, 1.0]), 0.0);

        // Tailles/énergies différentes ⇒ scores différents, dans (0, 1].
        let small = compute_quality(&sample_embedding(32, 1.0));
        let full = compute_quality(&sample_embedding(256, 1.0));
        let loud = compute_quality(&sample_embedding(256, 4.0));
        let quiet = compute_quality(&sample_embedding(256, 0.05));
        for s in [small, full, loud, quiet] {
            assert!((0.0..=1.0).contains(&s), "score hors borne: {s}");
        }
        assert!(small < full, "la couverture dimensionnelle doit compter");
        assert!(quiet < full, "l'énergie doit compter");
        assert_ne!(full, 0.95, "plus de constante 0.95");

        let mut manager = EnrollmentManager::new();
        let key = [7u8; 32];
        let user = UserId(Uuid::now_v7());
        manager
            .enroll_voice(user.clone(), sample_embedding(256, 1.0), &key)
            .unwrap();
        assert_eq!(manager.get_profile(&user).unwrap().quality_score, full);
    }
}
