use annora_core::{Timestamp, UserId};
use serde::{Deserialize, Serialize};

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
