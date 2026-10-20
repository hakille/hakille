use hakille_core::{Timestamp, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub user_id: UserId,
    pub started_at: Timestamp,
    pub last_verified: Timestamp,
    pub confidence_score: f64,
    pub is_fully_authenticated: bool,
}

pub struct ContinuousAuth;

impl ContinuousAuth {
    pub fn start_session(user_id: UserId, initial_confidence: f64) -> AuthSession {
        let now = Timestamp::now();
        AuthSession {
            user_id,
            started_at: now.clone(),
            last_verified: now,
            confidence_score: initial_confidence,
            is_fully_authenticated: initial_confidence >= 0.8,
        }
    }

    pub fn update_confidence(
        session: &mut AuthSession,
        new_embedding: &[f32],
        stored: &[f32],
    ) -> f64 {
        let similarity = Self::cosine_similarity(new_embedding, stored);
        session.confidence_score = (session.confidence_score + similarity) / 2.0;
        // Toute ré-évaluation réussie rafraîchit la preuve de présence.
        session.last_verified = Timestamp::now();
        session.is_fully_authenticated = session.confidence_score >= 0.8;
        session.confidence_score
    }

    /// Session valide si `score >= min` ET preuve récente (< 15 min).
    /// Le timeout borne la réutilisation d'une session volée/inactive
    /// (fail-closed au-delà de 15 min sans re-vérification).
    pub fn is_valid(session: &AuthSession, min_confidence: f64) -> bool {
        if session.confidence_score < min_confidence {
            return false;
        }
        let elapsed = chrono::Utc::now().signed_duration_since(session.last_verified.0);
        elapsed.num_seconds() >= 0 && elapsed.num_seconds() < 15 * 60
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for (x, y) in a.iter().zip(b.iter()) {
            dot += x * y;
            norm_a += x * x;
            norm_b += y * y;
        }
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a.sqrt() * norm_b.sqrt())) as f64
    }
}
