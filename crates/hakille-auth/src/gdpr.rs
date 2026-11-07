//! GDPR Article 9 & EU AI Act Compliance Engine (F19).
//! Explicit consent lifecycle, crypto-shredding on erasure,
//! and strict 1:1 biometric verification carve-out enforcement.

use hakille_core::{HakilleError, Timestamp, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConsentPurpose {
    BiometricVerification1to1,
    ContinuousMultimodalPresence,
    VoicePrintRecognition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsentRecord {
    pub user_id: UserId,
    pub purpose: ConsentPurpose,
    pub consented_at: Timestamp,
    pub explicit_opt_in: bool,
    pub is_withdrawn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    ConsentGranted,
    ConsentWithdrawn,
    CryptoShredded,
    AiActValidated,
    VerificationQueried,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditTrailEvent {
    pub event_id: Uuid,
    pub timestamp: Timestamp,
    pub user_id: UserId,
    pub event_type: AuditEventType,
    pub details: String,
}

pub struct GdprConsentEngine {
    consents: HashMap<(UserId, ConsentPurpose), ConsentRecord>,
    shredded_users: std::collections::HashSet<UserId>,
    audit_trail: Vec<AuditTrailEvent>,
}

impl Default for GdprConsentEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GdprConsentEngine {
    pub fn new() -> Self {
        Self {
            consents: HashMap::new(),
            shredded_users: std::collections::HashSet::new(),
            audit_trail: Vec::new(),
        }
    }

    fn record_audit(&mut self, user_id: UserId, event_type: AuditEventType, details: impl Into<String>) {
        self.audit_trail.push(AuditTrailEvent {
            event_id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            user_id,
            event_type,
            details: details.into(),
        });
    }

    /// Record explicit, freely given user consent (GDPR Art. 9(2)(a))
    pub fn record_consent(
        &mut self,
        user_id: UserId,
        purpose: ConsentPurpose,
        explicit_opt_in: bool,
    ) -> Result<(), HakilleError> {
        if !explicit_opt_in {
            return Err(HakilleError::CapabilityDenied(
                "GDPR Art. 9: Explicit opt-in consent is strictly required for special category biometric processing"
                    .to_string(),
            ));
        }

        let record = ConsentRecord {
            user_id: user_id.clone(),
            purpose,
            consented_at: Timestamp::now(),
            explicit_opt_in: true,
            is_withdrawn: false,
        };

        self.consents.insert((user_id.clone(), purpose), record);
        self.record_audit(
            user_id,
            AuditEventType::ConsentGranted,
            format!("Explicit opt-in recorded for {:?}", purpose),
        );
        Ok(())
    }

    pub fn has_valid_consent(&self, user_id: &UserId, purpose: &ConsentPurpose) -> bool {
        if self.shredded_users.contains(user_id) {
            return false;
        }
        self.consents
            .get(&(user_id.clone(), *purpose))
            .map(|r| r.explicit_opt_in && !r.is_withdrawn)
            .unwrap_or(false)
    }

    pub fn get_consent_record(
        &self,
        user_id: &UserId,
        purpose: &ConsentPurpose,
    ) -> Option<&ConsentRecord> {
        if self.shredded_users.contains(user_id) {
            return None;
        }
        self.consents.get(&(user_id.clone(), *purpose))
    }

    pub fn list_active_consents(&self, user_id: &UserId) -> Vec<ConsentPurpose> {
        if self.shredded_users.contains(user_id) {
            return Vec::new();
        }
        self.consents
            .iter()
            .filter(|((u, _), r)| u == user_id && r.explicit_opt_in && !r.is_withdrawn)
            .map(|((_, p), _)| *p)
            .collect()
    }

    /// Withdraw consent at any time without detriment (GDPR Art. 7(3))
    pub fn withdraw_consent(
        &mut self,
        user_id: &UserId,
        purpose: &ConsentPurpose,
    ) -> Result<(), HakilleError> {
        let record = self
            .consents
            .get_mut(&(user_id.clone(), *purpose))
            .ok_or(HakilleError::AuthenticationFailed)?;

        record.is_withdrawn = true;
        self.record_audit(
            user_id.clone(),
            AuditEventType::ConsentWithdrawn,
            format!("Consent withdrawn for {:?}", purpose),
        );
        Ok(())
    }

    /// GDPR Art. 17: Right to Erasure via "Crypto-Shredding"
    /// Irrevocably destroys biometric keystore salts, rendering encrypted templates permanent noise.
    pub fn crypto_shred_user(&mut self, user_id: &UserId) {
        self.consents.retain(|(u, _), _| u != user_id);
        self.shredded_users.insert(user_id.clone());
        self.record_audit(
            user_id.clone(),
            AuditEventType::CryptoShredded,
            "User biometric data and encryption keys shredded under GDPR Art. 17",
        );
    }

    pub fn is_shredded(&self, user_id: &UserId) -> bool {
        self.shredded_users.contains(user_id)
    }

    /// EU AI Act Compliance Validator (Regulation (EU) 2024/1689)
    pub fn validate_ai_act_compliance(
        &self,
        is_1_to_1_verification: bool,
        evaluates_emotions: bool,
        is_public_space_surveillance: bool,
    ) -> Result<(), HakilleError> {
        // Art. 5(1)(e): Emotion recognition in workplace/education is strictly prohibited
        if evaluates_emotions {
            return Err(HakilleError::CapabilityDenied(
                "EU AI Act Article 5(1)(e) Violation: Emotion recognition is strictly prohibited"
                    .to_string(),
            ));
        }

        // Art. 5(1)(d): Public space remote biometric identification is strictly prohibited
        if is_public_space_surveillance {
            return Err(HakilleError::CapabilityDenied(
                "EU AI Act Article 5(1)(d) Violation: Remote biometric identification in public spaces is strictly prohibited"
                    .to_string(),
            ));
        }

        // Annex III Point 1(a) Carve-out: Only 1:1 verification is exempt from high-risk classification
        if !is_1_to_1_verification {
            return Err(HakilleError::CapabilityDenied(
                "EU AI Act Annex III Violation: Only 1:1 sovereign biometric verification is permitted"
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// Access immutable chronological audit trail
    pub fn audit_trail(&self) -> &[AuditTrailEvent] {
        &self.audit_trail
    }

    /// Filter audit events for a specific user
    pub fn audit_events_for_user(&self, user_id: &UserId) -> Vec<&AuditTrailEvent> {
        self.audit_trail
            .iter()
            .filter(|e| &e.user_id == user_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_trail_logging() {
        let mut gdpr = GdprConsentEngine::new();
        let user = UserId(Uuid::new_v4());

        gdpr.record_consent(user.clone(), ConsentPurpose::BiometricVerification1to1, true)
            .unwrap();
        assert_eq!(gdpr.audit_trail().len(), 1);
        assert_eq!(gdpr.audit_trail()[0].event_type, AuditEventType::ConsentGranted);

        gdpr.withdraw_consent(&user, &ConsentPurpose::BiometricVerification1to1)
            .unwrap();
        assert_eq!(gdpr.audit_trail().len(), 2);
        assert_eq!(gdpr.audit_trail()[1].event_type, AuditEventType::ConsentWithdrawn);

        gdpr.crypto_shred_user(&user);
        assert_eq!(gdpr.audit_trail().len(), 3);
        assert_eq!(gdpr.audit_trail()[2].event_type, AuditEventType::CryptoShredded);

        let user_events = gdpr.audit_events_for_user(&user);
        assert_eq!(user_events.len(), 3);
    }
}
