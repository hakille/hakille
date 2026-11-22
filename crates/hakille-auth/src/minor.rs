//! Minor Profile & Constitutional Rule 7 Invariant Protection (F23).
//! Strict, unbypassable enforcement preventing wallet creation, transaction signing,
//! and full-system escalation for minors under 18 years of age.

use crate::household::{ContentFilterLevel, HouseholdMember, Restriction};
use hakille_core::capability::CapabilityScope;
use hakille_core::{HakilleError, Timestamp, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgeCategory {
    Child,    // Under 13
    Teenager, // 13 - 17
}

impl AgeCategory {
    /// Classify chronological age in years into an AgeCategory.
    /// Strictly rejects ages 18 and older (adults).
    pub fn from_age_years(age_years: u8) -> Result<Self, HakilleError> {
        match age_years {
            0..=12 => Ok(AgeCategory::Child),
            13..=17 => Ok(AgeCategory::Teenager),
            _ => Err(HakilleError::CapabilityDenied(
                "User is 18 or older and cannot be assigned a Minor profile.".to_string(),
            )),
        }
    }
}

/// Profile representing a minor dependent within a domestic family tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinorProfile {
    pub member: HouseholdMember,
    pub parent_user_ids: Vec<UserId>,
    pub age_category: AgeCategory,
}

impl MinorProfile {
    /// Absolute Constitutional Rule 7 Invariant: Minors are unconditionally barred from Wallet operations.
    pub fn is_wallet_permitted(&self) -> bool {
        false
    }

    /// Enforces Rule 7: Fails closed if any wallet operation is attempted by or for a minor.
    pub fn enforce_rule7_no_wallet(&self) -> Result<(), HakilleError> {
        Err(HakilleError::CapabilityDenied(
            "Constitutional Rule 7: Minor NoWallet invariant violated: minors under 18 are strictly prohibited from wallet creation or signing.".to_string(),
        ))
    }

    /// Evaluates if minor can access a requested capability scope.
    /// Wallet and FullSystem are hard-blocked by Constitutional Rules 1, 2, and 7.
    pub fn allows_scope(&self, scope: &CapabilityScope) -> bool {
        if matches!(scope, CapabilityScope::Wallet | CapabilityScope::FullSystem) {
            return false;
        }

        // Check if household member has explicit NoWallet restriction
        if matches!(scope, CapabilityScope::Wallet)
            && self.member.restrictions.contains(&Restriction::NoWallet)
        {
            return false;
        }

        true
    }

    /// Verify if a guardian user ID is registered in the parent/guardian set
    pub fn is_guardian(&self, user_id: &UserId) -> bool {
        self.parent_user_ids.contains(user_id)
    }

    /// Validate guardian-authorized financial co-signing.
    /// While a minor cannot hold or sign with a wallet directly (Rule 7),
    /// a legitimate parent/guardian may co-sign on behalf of the household.
    pub fn validate_guardian_cosign(
        &self,
        guardian_id: &UserId,
        approval: &GuardianApproval,
    ) -> Result<(), HakilleError> {
        if !self.is_guardian(guardian_id) {
            return Err(HakilleError::CapabilityDenied(
                "Co-signing entity is not an authorized guardian for this minor.".to_string(),
            ));
        }

        if approval.guardian_user_id != *guardian_id {
            return Err(HakilleError::CapabilityDenied(
                "Guardian approval user ID does not match the caller guardian ID.".to_string(),
            ));
        }

        if approval.minor_user_id != self.member.user_id {
            return Err(HakilleError::CapabilityDenied(
                "Guardian approval does not target this minor.".to_string(),
            ));
        }

        if approval.approved_amount_cents == 0 {
            return Err(HakilleError::CapabilityDenied(
                "Guardian approval amount must be strictly positive.".to_string(),
            ));
        }

        if approval.signature_proof.is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "Guardian approval signature proof is empty.".to_string(),
            ));
        }

        if chrono::Utc::now() > approval.expires_at.0 {
            return Err(HakilleError::CapabilityExpired);
        }

        Ok(())
    }

    /// Validate guardian co-signing with cryptographic HMAC-SHA256 verification against the guardian's key.
    pub fn validate_guardian_cosign_with_key(
        &self,
        guardian_id: &UserId,
        approval: &GuardianApproval,
        guardian_key: &[u8; 32],
    ) -> Result<(), HakilleError> {
        self.validate_guardian_cosign(guardian_id, approval)?;
        if !approval.verify_hmac(guardian_key) {
            return Err(HakilleError::AuthenticationFailed);
        }
        Ok(())
    }
}

/// Signed co-approval record from an adult guardian authorizing a controlled transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianApproval {
    pub guardian_user_id: UserId,
    pub minor_user_id: UserId,
    pub approved_amount_cents: u64,
    pub purpose: String,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub signature_proof: Vec<u8>,
}

impl GuardianApproval {
    /// Computes canonical HMAC-SHA256 over the approval fields.
    pub fn compute_hmac(&self, key: &[u8; 32]) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        let mut ipad = [0x36u8; 64];
        let mut opad = [0x5cu8; 64];
        for i in 0..32 {
            ipad[i] ^= key[i];
            opad[i] ^= key[i];
        }

        let mut inner = Sha256::new();
        inner.update(ipad);
        inner.update(b"hakille-guardian-approval-v1:");
        inner.update(self.guardian_user_id.0.as_bytes());
        inner.update(self.minor_user_id.0.as_bytes());
        inner.update(self.approved_amount_cents.to_le_bytes());
        inner.update((self.purpose.len() as u64).to_le_bytes());
        inner.update(self.purpose.as_bytes());
        inner.update(self.created_at.0.timestamp_millis().to_le_bytes());
        inner.update(self.expires_at.0.timestamp_millis().to_le_bytes());
        let inner_digest = inner.finalize();

        let mut outer = Sha256::new();
        outer.update(opad);
        outer.update(inner_digest);
        let digest = outer.finalize();

        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    /// Populates `self.signature_proof` with the HMAC-SHA256 digest under `key`.
    pub fn sign_hmac(&mut self, key: &[u8; 32]) {
        self.signature_proof = self.compute_hmac(key).to_vec();
    }

    /// Constant-time verification of `self.signature_proof` against `key`.
    pub fn verify_hmac(&self, key: &[u8; 32]) -> bool {
        if self.signature_proof.len() != 32 {
            return false;
        }
        let expected = self.compute_hmac(key);
        let mut diff = 0u8;
        for (a, b) in expected.iter().zip(self.signature_proof.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

pub fn default_minor_restrictions(category: &AgeCategory) -> Vec<Restriction> {
    let mut restrictions = vec![Restriction::NoWallet];

    match category {
        AgeCategory::Child => {
            restrictions.push(Restriction::ContentFilter(ContentFilterLevel::Strict));
            restrictions.push(Restriction::SupervisedCategories(vec!["all".to_string()]));
        }
        AgeCategory::Teenager => {
            restrictions.push(Restriction::ContentFilter(ContentFilterLevel::Moderate));
            restrictions.push(Restriction::SupervisedCategories(vec![
                "finance".to_string(),
                "mature_content".to_string(),
            ]));
        }
    }

    restrictions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::household::HouseholdRole;
    use uuid::Uuid;

    #[test]
    fn test_rule7_unconditional_rejection() {
        let minor = MinorProfile {
            member: HouseholdMember {
                user_id: UserId(Uuid::new_v4()),
                display_name: "Tim".to_string(),
                role: HouseholdRole::Minor,
                restrictions: default_minor_restrictions(&AgeCategory::Child),
            },
            parent_user_ids: vec![UserId(Uuid::new_v4())],
            age_category: AgeCategory::Child,
        };

        assert!(!minor.is_wallet_permitted());
        assert!(minor.enforce_rule7_no_wallet().is_err());
        assert!(!minor.allows_scope(&CapabilityScope::Wallet));
        assert!(!minor.allows_scope(&CapabilityScope::FullSystem));
        assert!(minor.allows_scope(&CapabilityScope::Audio));
    }

    #[test]
    fn test_age_category_classification() {
        assert_eq!(AgeCategory::from_age_years(8).unwrap(), AgeCategory::Child);
        assert_eq!(
            AgeCategory::from_age_years(15).unwrap(),
            AgeCategory::Teenager
        );
        assert!(AgeCategory::from_age_years(18).is_err());
        assert!(AgeCategory::from_age_years(25).is_err());
    }

    #[test]
    fn test_guardian_cosign_validation() {
        let parent = UserId(Uuid::new_v4());
        let stranger = UserId(Uuid::new_v4());
        let minor_id = UserId(Uuid::new_v4());

        let minor = MinorProfile {
            member: HouseholdMember {
                user_id: minor_id.clone(),
                display_name: "Emma".to_string(),
                role: HouseholdRole::Minor,
                restrictions: default_minor_restrictions(&AgeCategory::Teenager),
            },
            parent_user_ids: vec![parent.clone()],
            age_category: AgeCategory::Teenager,
        };

        let now = chrono::Utc::now();
        let valid_approval = GuardianApproval {
            guardian_user_id: parent.clone(),
            minor_user_id: minor_id.clone(),
            approved_amount_cents: 1500,
            purpose: "School Supplies".to_string(),
            created_at: Timestamp(now),
            expires_at: Timestamp(now + chrono::Duration::hours(1)),
            signature_proof: vec![1, 2, 3],
        };

        // Authorized guardian succeeds
        assert!(
            minor
                .validate_guardian_cosign(&parent, &valid_approval)
                .is_ok()
        );

        // Unauthorized entity fails
        assert!(
            minor
                .validate_guardian_cosign(&stranger, &valid_approval)
                .is_err()
        );
    }
}
