use annora_core::{Timestamp, UserId};
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
    pub fn validate(&self) -> annora_core::Result<()> {
        if self.quorum < MIN_QUORUM {
            return Err(annora_core::AnnoraError::ConstitutionViolation(format!(
                "quorum {} below minimum {}",
                self.quorum, MIN_QUORUM
            )));
        }
        if self.review_period_days < 30 {
            return Err(annora_core::AnnoraError::ConstitutionViolation(format!(
                "review_period_days {} below minimum 30",
                self.review_period_days
            )));
        }
        if !self.public_comment_required {
            return Err(annora_core::AnnoraError::ConstitutionViolation(
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
    ) -> annora_core::Result<AmendmentStatus> {
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
                return Err(annora_core::AnnoraError::ConstitutionViolation(
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
    ) -> annora_core::Result<AmendmentStatus> {
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
}

/// Endossement simple de la fondation (sans crypto lourde) : identifiant
/// de la fondation + signature hexadécimale opaque.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FoundationEndorsement {
    pub foundation_id: String,
    pub signature_hex: String,
}
