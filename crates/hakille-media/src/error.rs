//! Erreurs explicites du moteur génératif multimodal Hakille.
//!
//! Contrat d'honnêteté : aucune erreur n'est masquée par un repli silencieux.
//! Un checkpoint absent renvoie [`MediaError::ModelMissing`], un muxer absent
//! renvoie [`MediaError::MuxerUnavailable`], un refus de capability renvoie
//! [`MediaError::CapabilityDenied`].

use thiserror::Error;

pub type MediaResult<T> = std::result::Result<T, MediaError>;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("requête média invalide : {0}")]
    InvalidRequest(String),

    #[error("limite de politique média dépassée : {0}")]
    PolicyExceeded(String),

    #[error("capability Biscuit refusée : {0}")]
    CapabilityDenied(String),

    #[error("action bloquée par la Constitution : {0}")]
    ConstitutionBlocked(String),

    #[error("modèle neuronal absent pour '{model_id}' (fichiers manquants : {missing:?}) — {hint}")]
    ModelMissing {
        model_id: String,
        missing: Vec<String>,
        hint: String,
    },

    #[error("moteur '{engine_id}' indisponible : {reason}")]
    BackendUnavailable { engine_id: String, reason: String },

    #[error("muxer externe indisponible ({binary}) : {reason}")]
    MuxerUnavailable { binary: String, reason: String },

    #[error("échec d'inférence ou de synthèse ({engine_id}) : {reason}")]
    InferenceFailed { engine_id: String, reason: String },

    #[error("génération annulée par l'utilisateur (job {job_id})")]
    Cancelled { job_id: String },

    #[error("échéance dépassée après {elapsed_ms} ms (budget {budget_ms} ms)")]
    DeadlineExceeded { elapsed_ms: u64, budget_ms: u64 },

    #[error("provenance cryptographique invalide : {0}")]
    ProvenanceInvalid(String),

    #[error("cycle détecté dans le graphe DAG multimodal : {0}")]
    DagCycleDetected(String),

    #[error("erreur réseau ou fournisseur externe : {0}")]
    Network(String),

    #[error("erreur de stockage média : {0}")]
    Storage(String),

    #[error("erreur d'encodage média : {0}")]
    Encoding(String),
}

impl From<std::io::Error> for MediaError {
    fn from(err: std::io::Error) -> Self {
        Self::Storage(err.to_string())
    }
}

impl From<serde_json::Error> for MediaError {
    fn from(err: serde_json::Error) -> Self {
        Self::Encoding(err.to_string())
    }
}

impl From<hakille_core::HakilleError> for MediaError {
    fn from(err: hakille_core::HakilleError) -> Self {
        match err {
            hakille_core::HakilleError::CapabilityDenied(m) => Self::CapabilityDenied(m),
            hakille_core::HakilleError::CapabilityExpired => {
                Self::CapabilityDenied("jeton de capability expiré".into())
            }
            hakille_core::HakilleError::AttenuationViolation => {
                Self::CapabilityDenied("violation de monotonie d'atténuation Biscuit".into())
            }
            hakille_core::HakilleError::ConstitutionViolation(m) => Self::ConstitutionBlocked(m),
            hakille_core::HakilleError::TrustViolation(m) => Self::ConstitutionBlocked(m),
            other => Self::InferenceFailed {
                engine_id: "hakille-core".into(),
                reason: other.to_string(),
            },
        }
    }
}
