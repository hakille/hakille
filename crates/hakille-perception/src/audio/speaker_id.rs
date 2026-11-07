use crate::audio::capture::AudioChunk;
use async_trait::async_trait;
use hakille_core::{HakilleError, Result};
use hakille_inference::CampplusOnnx;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceBiometricEmbedding {
    pub vector: Vec<f32>,
    pub dimension: usize,
}

impl VoiceBiometricEmbedding {
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.dimension != self.vector.len() || other.dimension != other.vector.len() {
            return 0.0;
        }
        hakille_inference::speaker_onnx::cosine_similarity(&self.vector, &other.vector)
    }
}

#[async_trait]
pub trait SpeakerIdentifier: Send + Sync {
    async fn extract_embedding(&self, chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding>;
}

/// Biométrie locuteur RÉELLE : embeddings CAM++ via forward `ort`
/// (`CampplusOnnx`), fbank 80×200 + 12+ couches TDNN-CAM exécutées.
/// Erreur LOUD si le checkpoint est absent — aucun fallback simulé.
pub struct CamPlusPlusIdentifier {
    embedding_dim: usize,
    model_path: Option<PathBuf>,
    cache: std::sync::Mutex<Option<CampplusOnnx>>,
}

impl CamPlusPlusIdentifier {
    pub fn new() -> Self {
        Self {
            embedding_dim: 192,
            model_path: None,
            cache: std::sync::Mutex::new(None),
        }
    }

    pub fn with_model_path(path: impl AsRef<Path>) -> Self {
        Self {
            embedding_dim: 192,
            model_path: Some(path.as_ref().to_path_buf()),
            cache: std::sync::Mutex::new(None),
        }
    }

    pub fn model_name(&self) -> &str {
        "CAM++ (zh-en 16k advanced, ONNX réel)"
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    /// Chemin effectif : explicite > défaut. Erreur LOUD si introuvable.
    fn resolve_path(&self) -> Result<PathBuf> {
        if let Some(ref p) = self.model_path {
            if p.exists() {
                return Ok(p.clone());
            }
            return Err(HakilleError::InferenceError(format!(
                "CamPlusPlusIdentifier: modèle CAM++ absent '{}' — provisionnez models/campplus_zh_en_16k_advanced.onnx (RÉEL exigé, pas de sine-hash)",
                p.display()
            )));
        }
        CampplusOnnx::default_model_path().ok_or_else(|| {
            HakilleError::InferenceError(
                "CamPlusPlusIdentifier: checkpoint CAM++ introuvable (models/campplus_zh_en_16k_advanced.onnx absent) — biométrie RÉELLE impossible, aucun fallback simulé"
                    .to_string(),
            )
        })
    }

    fn mono_16k(chunk: &AudioChunk) -> Result<Vec<f32>> {
        hakille_inference::audio_resample::try_to_mono_16k(
            &chunk.samples,
            chunk.channels,
            chunk.sample_rate,
        )
    }
}

impl Default for CamPlusPlusIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpeakerIdentifier for CamPlusPlusIdentifier {
    async fn extract_embedding(&self, chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding> {
        let pcm_16k = Self::mono_16k(chunk)?;
        if pcm_16k.is_empty() {
            return Err(HakilleError::InferenceError(
                "CamPlusPlusIdentifier: chunk vide — embedding RÉEL impossible".to_string(),
            ));
        }
        // Session mise en cache (ouverture paresseuse, réutilisée).
        // `CampplusOnnx::open` est bloquant : on l'appelle hors verrou long.
        let path = self.resolve_path()?;
        let mut guard = self.cache.lock().map_err(|e| {
            HakilleError::InferenceError(format!("CamPlusPlusIdentifier: mutex empoisonné : {e}"))
        })?;
        if guard.is_none() {
            let sess = CampplusOnnx::open(&path)?;
            *guard = Some(sess);
        }
        let vector = guard
            .as_ref()
            .expect("session CAM++ initialisée")
            .embed(&pcm_16k)?;
        Ok(VoiceBiometricEmbedding {
            dimension: self.embedding_dim,
            vector,
        })
    }
}

/// Seuil de similarité cosinus minimal exigé pour certifier l'identité souveraine.
pub const SOVEREIGN_VOICEPRINT_THRESHOLD: f32 = 0.72;

/// Résultat d'une vérification d'empreinte vocale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerVerificationResult {
    /// Vrai si et seulement si S >= seuil et données conformes (accès souverain accordé).
    pub is_verified: bool,
    /// Score de similarité cosinus mesuré S ∈ [-1.0, 1.0].
    pub similarity: f32,
    /// Seuil appliqué (ex: 0.72).
    pub threshold: f32,
    /// Identifiant souverain cible.
    pub sovereign_id: String,
}

/// Portail biométrique fail-closed authentifiant l'accès souverain par empreinte vocale CAM++.
#[derive(Debug, Clone)]
pub struct SovereignVoiceprintGate {
    enrolled_embedding: Arc<VoiceBiometricEmbedding>,
    threshold: f32,
    sovereign_id: String,
}

impl SovereignVoiceprintGate {
    /// Initialise le portail avec une empreinte souveraine enrôlée et le seuil par défaut 0.72.
    pub fn new(enrolled_embedding: VoiceBiometricEmbedding) -> Result<Self> {
        Self::with_threshold(enrolled_embedding, SOVEREIGN_VOICEPRINT_THRESHOLD)
    }

    /// Initialise le portail avec un seuil explicite.
    pub fn with_threshold(
        enrolled_embedding: VoiceBiometricEmbedding,
        threshold: f32,
    ) -> Result<Self> {
        Self::validate_embedding(&enrolled_embedding)?;
        if !threshold.is_finite() || threshold <= 0.0 || threshold > 1.0 {
            return Err(HakilleError::InferenceError(format!(
                "SovereignVoiceprintGate: seuil invalide {threshold} (attendu dans (0.0, 1.0])"
            )));
        }
        Ok(Self {
            enrolled_embedding: Arc::new(enrolled_embedding),
            threshold,
            sovereign_id: "sovereign_owner".to_string(),
        })
    }

    /// Définit un identifiant personnalisé pour l'entité souveraine.
    pub fn with_id(
        enrolled_embedding: VoiceBiometricEmbedding,
        sovereign_id: impl Into<String>,
    ) -> Result<Self> {
        let mut gate = Self::new(enrolled_embedding)?;
        gate.sovereign_id = sovereign_id.into();
        Ok(gate)
    }

    /// Valide l'intégrité structurelle et numérique d'un embedding biométrique.
    fn validate_embedding(emb: &VoiceBiometricEmbedding) -> Result<()> {
        if emb.dimension == 0 || emb.dimension != emb.vector.len() {
            return Err(HakilleError::InferenceError(format!(
                "SovereignVoiceprintGate: dimension invalide ou incohérente (déclarée: {}, len: {})",
                emb.dimension,
                emb.vector.len()
            )));
        }
        if emb.vector.iter().any(|x| !x.is_finite()) {
            return Err(HakilleError::InferenceError(
                "SovereignVoiceprintGate: valeurs non finies dans l'embedding (NaN/Inf)".into(),
            ));
        }
        let norm_sq: f32 = emb.vector.iter().map(|x| x * x).sum();
        if norm_sq < 1e-9 {
            return Err(HakilleError::InferenceError(
                "SovereignVoiceprintGate: embedding dégénéré de norme quasi nulle".into(),
            ));
        }
        Ok(())
    }

    /// Seuil effectif.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Identifiant souverain.
    pub fn sovereign_id(&self) -> &str {
        &self.sovereign_id
    }

    /// Empreinte enrôlée.
    pub fn enrolled_embedding(&self) -> &VoiceBiometricEmbedding {
        &self.enrolled_embedding
    }

    /// Met à jour l'empreinte enrôlée.
    pub fn update_enrolled(&mut self, new_embedding: VoiceBiometricEmbedding) -> Result<()> {
        Self::validate_embedding(&new_embedding)?;
        self.enrolled_embedding = Arc::new(new_embedding);
        Ok(())
    }

    /// Vérifie directement un embedding candidat.
    ///
    /// Fail-closed : retourne Err en cas d'incohérence de dimension ou de données non finies.
    pub fn verify(&self, candidate: &VoiceBiometricEmbedding) -> Result<SpeakerVerificationResult> {
        Self::validate_embedding(candidate)?;
        if candidate.dimension != self.enrolled_embedding.dimension {
            return Err(HakilleError::InferenceError(format!(
                "SovereignVoiceprintGate: dimension candidate {} incompatible avec l'enrôlement {}",
                candidate.dimension, self.enrolled_embedding.dimension
            )));
        }

        let similarity = self.enrolled_embedding.cosine_similarity(candidate);
        let is_verified = similarity >= self.threshold;

        Ok(SpeakerVerificationResult {
            is_verified,
            similarity,
            threshold: self.threshold,
            sovereign_id: self.sovereign_id.clone(),
        })
    }

    /// Extrait l'empreinte d'un tronçon audio brut et la vérifie.
    ///
    /// Fail-closed : tout chunk vide, corrompu, ou échantillon non-fini
    /// est immédiatement rejeté avec une erreur explicite.
    pub async fn verify_audio(
        &self,
        identifier: &dyn SpeakerIdentifier,
        chunk: &AudioChunk,
    ) -> Result<SpeakerVerificationResult> {
        if chunk.samples.is_empty() {
            return Err(HakilleError::InferenceError(
                "SovereignVoiceprintGate: chunk audio vide — rejet fail-closed".into(),
            ));
        }
        if chunk.samples.iter().any(|s| !s.is_finite()) {
            return Err(HakilleError::InferenceError(
                "SovereignVoiceprintGate: échantillons audio corrompus (NaN/Inf) — rejet fail-closed".into(),
            ));
        }
        if chunk.samples.len() < hakille_inference::speaker_onnx::CAMP_WIN {
            return Err(HakilleError::InferenceError(
                "SovereignVoiceprintGate: chunk audio trop court pour extraction CAM++ — rejet fail-closed".into(),
            ));
        }

        let candidate = identifier.extract_embedding(chunk).await?;
        self.verify(&candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vector(seed: f32, dim: usize) -> Vec<f32> {
        let mut v: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.1 + seed).sin()).collect();
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
        v
    }

    struct MockIdentifier {
        embedding: VoiceBiometricEmbedding,
    }

    #[async_trait]
    impl SpeakerIdentifier for MockIdentifier {
        async fn extract_embedding(&self, _chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding> {
            Ok(self.embedding.clone())
        }
    }

    #[test]
    fn test_sovereign_gate_verified_acceptance() {
        let v = create_test_vector(1.0, 192);
        let enrolled = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v.clone(),
        };
        let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

        // 1. Identical candidate: similarity == 1.0 >= 0.72
        let candidate_exact = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v.clone(),
        };
        let res_exact = gate.verify(&candidate_exact).expect("exact verification");
        assert!(res_exact.is_verified);
        assert!((res_exact.similarity - 1.0).abs() < 1e-4);
        assert_eq!(res_exact.threshold, SOVEREIGN_VOICEPRINT_THRESHOLD);
        assert_eq!(res_exact.sovereign_id, "sovereign_owner");

        // 2. High correlation candidate (S ≈ 0.85 >= 0.72)
        let v_perturbed: Vec<f32> = v
            .iter()
            .enumerate()
            .map(|(i, &x)| if i % 4 == 0 { x * 0.8 } else { x })
            .collect();
        let norm = v_perturbed.iter().map(|x| x * x).sum::<f32>().sqrt();
        let v_perturbed: Vec<f32> = v_perturbed.into_iter().map(|x| x / norm).collect();
        let candidate_perturbed = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v_perturbed,
        };
        let res_perturbed = gate
            .verify(&candidate_perturbed)
            .expect("perturbed verification");
        assert!(res_perturbed.is_verified);
        assert!(res_perturbed.similarity >= 0.72);
    }

    #[test]
    fn test_sovereign_gate_impostor_rejection() {
        let v = create_test_vector(1.0, 192);
        let enrolled = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v.clone(),
        };
        let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

        // 1. Inverted vector (S == -1.0)
        let v_inv: Vec<f32> = v.iter().map(|&x| -x).collect();
        let candidate_inv = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v_inv,
        };
        let res_inv = gate.verify(&candidate_inv).expect("inv verification");
        assert!(!res_inv.is_verified);
        assert!((res_inv.similarity + 1.0).abs() < 1e-4);

        // 2. Different voiceprint with S < 0.72
        let v_other = create_test_vector(5.5, 192);
        let candidate_other = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v_other,
        };
        let res_other = gate.verify(&candidate_other).expect("other verification");
        assert!(!res_other.is_verified);
        assert!(res_other.similarity < 0.72);
    }

    #[test]
    fn test_sovereign_gate_boundary_precision() {
        // Construct two 2D-like embeddings in 192 dimensions to precisely hit 0.72 ± epsilon
        let mut e1 = vec![0.0f32; 192];
        e1[0] = 1.0;

        let enrolled = VoiceBiometricEmbedding {
            dimension: 192,
            vector: e1,
        };
        let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

        // Target S = 0.72005 >= 0.72 -> verified true
        let s_above = 0.72005f32;
        let mut e_above = vec![0.0f32; 192];
        e_above[0] = s_above;
        e_above[1] = (1.0 - s_above * s_above).sqrt();
        let candidate_above = VoiceBiometricEmbedding {
            dimension: 192,
            vector: e_above,
        };
        let res_above = gate.verify(&candidate_above).expect("verify above");
        assert!(res_above.is_verified, "0.72005 must be verified");

        // Target S = 0.71995 < 0.72 -> verified false
        let s_below = 0.71995f32;
        let mut e_below = vec![0.0f32; 192];
        e_below[0] = s_below;
        e_below[1] = (1.0 - s_below * s_below).sqrt();
        let candidate_below = VoiceBiometricEmbedding {
            dimension: 192,
            vector: e_below,
        };
        let res_below = gate.verify(&candidate_below).expect("verify below");
        assert!(!res_below.is_verified, "0.71995 must be rejected");
    }

    #[tokio::test]
    async fn test_sovereign_gate_fail_closed_validation() {
        let v = create_test_vector(1.0, 192);
        let enrolled = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v.clone(),
        };
        let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

        // Dimension mismatch
        let mismatch = VoiceBiometricEmbedding {
            dimension: 128,
            vector: vec![0.1f32; 128],
        };
        assert!(gate.verify(&mismatch).is_err());

        // NaN in candidate
        let mut v_nan = v.clone();
        v_nan[10] = f32::NAN;
        let nan_candidate = VoiceBiometricEmbedding {
            dimension: 192,
            vector: v_nan,
        };
        assert!(gate.verify(&nan_candidate).is_err());

        // Zero-norm vector
        let zero_candidate = VoiceBiometricEmbedding {
            dimension: 192,
            vector: vec![0.0f32; 192],
        };
        assert!(gate.verify(&zero_candidate).is_err());

        // Invalid threshold
        assert!(
            SovereignVoiceprintGate::with_threshold(
                VoiceBiometricEmbedding {
                    dimension: 192,
                    vector: v.clone()
                },
                0.0
            )
            .is_err()
        );
        assert!(
            SovereignVoiceprintGate::with_threshold(
                VoiceBiometricEmbedding {
                    dimension: 192,
                    vector: v.clone()
                },
                1.5
            )
            .is_err()
        );
        assert!(
            SovereignVoiceprintGate::with_threshold(
                VoiceBiometricEmbedding {
                    dimension: 192,
                    vector: v.clone()
                },
                f32::NAN
            )
            .is_err()
        );

        // AudioChunk verification fail-closed
        let mock_id = MockIdentifier {
            embedding: VoiceBiometricEmbedding {
                dimension: 192,
                vector: v.clone(),
            },
        };

        // Empty chunk
        let empty_chunk = AudioChunk {
            samples: Vec::new(),
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(gate.verify_audio(&mock_id, &empty_chunk).await.is_err());

        // Corrupted chunk (NaN sample)
        let nan_chunk = AudioChunk {
            samples: vec![f32::NAN; 500],
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(gate.verify_audio(&mock_id, &nan_chunk).await.is_err());

        // Too short chunk (< CAMP_WIN = 400)
        let short_chunk = AudioChunk {
            samples: vec![0.1f32; 200],
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(gate.verify_audio(&mock_id, &short_chunk).await.is_err());

        // Valid chunk passes to identifier
        let valid_chunk = AudioChunk {
            samples: vec![0.1f32; 500],
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        let valid_res = gate
            .verify_audio(&mock_id, &valid_chunk)
            .await
            .expect("valid chunk");
        assert!(valid_res.is_verified);
    }
}
