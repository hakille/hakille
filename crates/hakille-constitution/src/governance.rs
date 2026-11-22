use hakille_core::{Timestamp, UserId};
use serde::{Deserialize, Serialize};

/// Quorum minimal requis pour tout amendement constitutionnel.
pub const MIN_QUORUM: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AmendmentProcess {
    FoundationQuorum,
    CommunityVote,
    EmergencyOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    pub amendment_process: AmendmentProcess,
    pub review_period_days: u32,
    pub public_comment_required: bool,
    /// Nombre d'approbations requises pour faire passer un amendement
    /// de `Proposed`/`UnderReview` à `Approved`.
    pub quorum: u32,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            amendment_process: AmendmentProcess::FoundationQuorum,
            review_period_days: 30,
            public_comment_required: true,
            quorum: 5,
        }
    }
}

impl GovernanceConfig {
    /// Vérifie la cohérence de la config : quorum >= MIN_QUORUM,
    /// période de revue >= 30 jours, commentaire public requis.
    pub fn validate(&self) -> hakille_core::Result<()> {
        if self.quorum < MIN_QUORUM {
            return Err(hakille_core::HakilleError::ConstitutionViolation(format!(
                "quorum {} below minimum {}",
                self.quorum, MIN_QUORUM
            )));
        }
        if self.review_period_days < 30 {
            return Err(hakille_core::HakilleError::ConstitutionViolation(format!(
                "review_period_days {} below minimum 30",
                self.review_period_days
            )));
        }
        if !self.public_comment_required {
            return Err(hakille_core::HakilleError::ConstitutionViolation(
                "public_comment_required must be true".into(),
            ));
        }
        Ok(())
    }

    /// Machine à états `propose / approve / reject` avec quorum.
    ///
    /// - `Proposed + Approve` avec `approvals >= quorum` → `Approved`
    /// - `Proposed + Approve` avec `approvals < quorum` → `UnderReview`
    /// - `UnderReview + Approve` avec `approvals >= quorum` → `Approved`
    /// - `Proposed | UnderReview + Reject` → `Rejected`
    /// - `Approved + Apply` → `Applied`
    /// - toute autre transition → `ConstitutionViolation`
    pub fn transition(
        &self,
        current: AmendmentStatus,
        action: GovernanceAction,
        approvals: u32,
    ) -> hakille_core::Result<AmendmentStatus> {
        use AmendmentStatus::*;
        use GovernanceAction::*;
        let next = match (current, action) {
            (Proposed, Approve) if approvals >= self.quorum => Approved,
            (Proposed, Approve) => UnderReview,
            (UnderReview, Approve) if approvals >= self.quorum => Approved,
            (UnderReview, Approve) => UnderReview,
            (Proposed, Reject) | (UnderReview, Reject) => Rejected,
            (Approved, Apply) => Applied,
            (Proposed, Propose) => Proposed,
            _ => {
                return Err(hakille_core::HakilleError::ConstitutionViolation(
                    "invalid governance transition".into(),
                ));
            }
        };
        Ok(next)
    }

    /// Variante contrôlée : exige en plus `days_since_proposal >=
    /// review_period_days` et `public_comments > 0` pour passer à `Approved`.
    /// Si ces conditions ne sont pas remplies, reste en `UnderReview`.
    /// Les autres transitions sont déléguées à `transition()` inchangée.
    pub fn transition_checked(
        &self,
        current: AmendmentStatus,
        action: GovernanceAction,
        approvals: u32,
        days_since_proposal: u32,
        public_comments: u32,
    ) -> hakille_core::Result<AmendmentStatus> {
        let next = self.transition(current, action, approvals)?;
        if next == AmendmentStatus::Approved
            && (days_since_proposal < self.review_period_days || public_comments == 0)
        {
            return Ok(AmendmentStatus::UnderReview);
        }
        Ok(next)
    }
}

/// Événement de la machine à états de gouvernance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GovernanceAction {
    Propose,
    Approve,
    Reject,
    Apply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AmendmentStatus {
    Proposed,
    UnderReview,
    Approved,
    Rejected,
    Applied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstitutionAmendment {
    pub proposed_by: UserId,
    pub change_description: String,
    pub affected_rules: Vec<String>,
    pub proposed_at: Timestamp,
    pub status: AmendmentStatus,
    #[serde(default)]
    pub approvers: Vec<UserId>,
}

impl ConstitutionAmendment {
    /// Enregistre le vote d'approbation d'un votant unique (anti-Sybil).
    /// Retourne `false` si `voter` est le proposant ou a déjà voté.
    pub fn record_approval(&mut self, voter: UserId) -> bool {
        if voter == self.proposed_by || self.approvers.contains(&voter) {
            return false;
        }
        self.approvers.push(voter);
        true
    }

    /// Nombre de votants distincts ayant approuvé l'amendement.
    pub fn unique_approvals(&self) -> u32 {
        self.approvers.len() as u32
    }

    /// Payload canonique à signer pour un endossement Ed25519 de la fondation.
    pub fn canonical_signing_bytes(&self, foundation_id: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"hakille-foundation-endorsement-v1\0");
        out.extend_from_slice(&(foundation_id.len() as u32).to_le_bytes());
        out.extend_from_slice(foundation_id.as_bytes());
        out.extend_from_slice(self.proposed_by.0.as_bytes());
        out.extend_from_slice(&(self.change_description.len() as u32).to_le_bytes());
        out.extend_from_slice(self.change_description.as_bytes());
        out.extend_from_slice(&(self.affected_rules.len() as u32).to_le_bytes());
        for rule in &self.affected_rules {
            out.extend_from_slice(&(rule.len() as u32).to_le_bytes());
            out.extend_from_slice(rule.as_bytes());
        }
        out.extend_from_slice(&self.proposed_at.0.timestamp().to_le_bytes());
        out
    }
}

/// Endossement cryptographique Ed25519 de la fondation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FoundationEndorsement {
    pub foundation_id: String,
    pub signature_hex: String,
}

impl FoundationEndorsement {
    /// Signe un amendement avec la paire de clés Ed25519 de la fondation.
    pub fn sign(
        foundation_id: impl Into<String>,
        amendment: &ConstitutionAmendment,
        keypair: &hakille_core::KeyPair,
    ) -> Self {
        let foundation_id = foundation_id.into();
        let payload = amendment.canonical_signing_bytes(&foundation_id);
        let sig = keypair.sign(&payload);
        let signature_hex: String = sig.0.iter().map(|b| format!("{b:02x}")).collect();
        Self {
            foundation_id,
            signature_hex,
        }
    }

    /// Vérifie mathématiquement la signature Ed25519 (`verify_strict`) de l'endossement.
    pub fn verify(
        &self,
        amendment: &ConstitutionAmendment,
        verifying_key: &hakille_core::Ed25519VerifyingKey,
    ) -> hakille_core::Result<()> {
        if self.foundation_id.trim().is_empty() {
            return Err(hakille_core::HakilleError::ConstitutionViolation(
                "foundation_id must not be empty".into(),
            ));
        }
        if self.signature_hex.len() != 128 {
            return Err(hakille_core::HakilleError::ConstitutionViolation(
                "invalid foundation signature hex length (expected 128 hex chars)".into(),
            ));
        }
        let mut sig_bytes = [0u8; 64];
        for (i, chunk) in self
            .signature_hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
        {
            let hex_str = std::str::from_utf8(chunk).map_err(|_| {
                hakille_core::HakilleError::ConstitutionViolation(
                    "non-UTF8 foundation signature hex".into(),
                )
            })?;
            sig_bytes[i] = u8::from_str_radix(hex_str, 16).map_err(|_| {
                hakille_core::HakilleError::ConstitutionViolation(
                    "invalid hex byte in foundation signature".into(),
                )
            })?;
        }
        let sig = hakille_core::Ed25519Signature::from_bytes(&sig_bytes);
        let payload = amendment.canonical_signing_bytes(&self.foundation_id);
        verifying_key.verify_strict(&payload, &sig).map_err(|e| {
            hakille_core::HakilleError::ConstitutionViolation(format!(
                "foundation endorsement Ed25519 verification failed: {e}"
            ))
        })
    }
}
