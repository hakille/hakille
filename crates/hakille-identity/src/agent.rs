//! Autonomous Sovereign Agent Delegations & Passports (F12).
//! DIDs, Agent Passports, bounded delegation chains (D_max = 4), and constitutional invariants.

use crate::hierarchy::TenantId;
use crate::ssi::{Did, DidKey};
use ed25519_dalek::SigningKey;
use hakille_core::HakilleError;
use hakille_core::auth::MAX_DELEGATION_DEPTH;
use hakille_core::capability::CapabilityScope;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPassport {
    pub agent_did: Did,
    pub principal_did: Did,
    pub tenant_id: TenantId,
    pub max_lifetime_seconds: u64,
    pub max_execution_fuel: u64,
    pub budget_limit_cents: u64,
    pub biscuit_token_bytes: Vec<u8>,
    pub delegation_depth: usize,
    pub constitutional_hash: [u8; 32],
}

impl AgentPassport {
    /// Compute the canonical SHA-256 hash of this passport's identity fields
    pub fn canonical_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.agent_did.as_str().as_bytes());
        hasher.update(self.principal_did.as_str().as_bytes());
        hasher.update(self.tenant_id.0.as_bytes());
        hasher.update(self.max_lifetime_seconds.to_le_bytes());
        hasher.update(self.max_execution_fuel.to_le_bytes());
        hasher.update(self.budget_limit_cents.to_le_bytes());
        hasher.update((self.delegation_depth as u64).to_le_bytes());
        hasher.update(self.constitutional_hash);
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        out
    }

    /// Sign this passport using the principal's Ed25519 signing key
    pub fn sign_with(&self, signing_key: &SigningKey) -> Vec<u8> {
        use ed25519_dalek::Signer;
        let hash = self.canonical_hash();
        signing_key.sign(&hash).to_bytes().to_vec()
    }

    /// Verify an Ed25519 signature on this passport
    pub fn verify_signature(&self, public_key_bytes: &[u8; 32], signature_bytes: &[u8]) -> bool {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let Ok(verifying_key) = VerifyingKey::from_bytes(public_key_bytes) else {
            return false;
        };
        let Ok(sig_bytes): std::result::Result<[u8; 64], _> = signature_bytes.try_into() else {
            return false;
        };
        let sig = Signature::from_bytes(&sig_bytes);
        let hash = self.canonical_hash();
        verifying_key.verify(&hash, &sig).is_ok()
    }
}

pub struct AgentFactory;

impl AgentFactory {
    /// Spawn an autonomous agent with its own keypair and DID (`did:key:z...`)
    pub fn spawn_agent() -> (SigningKey, Did) {
        let key = SigningKey::generate(&mut OsRng);
        let did = DidKey::from_ed25519_bytes(key.verifying_key().as_bytes());
        (key, did)
    }

    /// Issue an Agent Passport from a principal
    #[allow(clippy::too_many_arguments)]
    pub fn issue_passport(
        agent_did: Did,
        principal_did: Did,
        tenant_id: TenantId,
        lifetime_seconds: u64,
        fuel: u64,
        budget_cents: u64,
        biscuit_token_bytes: Vec<u8>,
        delegation_depth: usize,
    ) -> Result<AgentPassport, HakilleError> {
        // Enforce maximum delegation depth invariant (D_max = 4)
        if delegation_depth > MAX_DELEGATION_DEPTH {
            return Err(HakilleError::CapabilityDenied(format!(
                "chaîne de délégation trop profonde ({} > {})",
                delegation_depth, MAX_DELEGATION_DEPTH
            )));
        }

        // Canonical constitution hash (Hakille 10 rules)
        let constitution_text = b"HAKILLE_CONSTITUTION_SOVEREIGN_RULES_1_TO_10";
        let mut hasher = Sha256::new();
        hasher.update(constitution_text);
        let mut constitutional_hash = [0u8; 32];
        constitutional_hash.copy_from_slice(&hasher.finalize());

        Ok(AgentPassport {
            agent_did,
            principal_did,
            tenant_id,
            max_lifetime_seconds: lifetime_seconds,
            max_execution_fuel: fuel,
            budget_limit_cents: budget_cents,
            biscuit_token_bytes,
            delegation_depth,
            constitutional_hash,
        })
    }

    /// Sub-delegate from an existing agent passport to a child sub-agent
    pub fn delegate_sub_agent(
        parent_passport: &AgentPassport,
        child_did: Did,
        attenuated_token: Vec<u8>,
        child_fuel: u64,
        child_budget_cents: u64,
    ) -> Result<AgentPassport, HakilleError> {
        let next_depth = parent_passport.delegation_depth + 1;
        if next_depth > MAX_DELEGATION_DEPTH {
            return Err(HakilleError::CapabilityDenied(format!(
                "chaîne de délégation trop profonde ({} > {})",
                next_depth, MAX_DELEGATION_DEPTH
            )));
        }

        // Enforce monotonicity: child fuel <= parent fuel, child budget <= parent budget
        if child_fuel > parent_passport.max_execution_fuel {
            return Err(HakilleError::CapabilityDenied(
                "Child agent fuel cannot exceed parent agent fuel".to_string(),
            ));
        }
        if child_budget_cents > parent_passport.budget_limit_cents {
            return Err(HakilleError::CapabilityDenied(
                "Child agent budget cannot exceed parent agent budget".to_string(),
            ));
        }

        Ok(AgentPassport {
            agent_did: child_did,
            principal_did: parent_passport.agent_did.clone(),
            tenant_id: parent_passport.tenant_id,
            max_lifetime_seconds: parent_passport.max_lifetime_seconds,
            max_execution_fuel: child_fuel,
            budget_limit_cents: child_budget_cents,
            biscuit_token_bytes: attenuated_token,
            delegation_depth: next_depth,
            constitutional_hash: parent_passport.constitutional_hash,
        })
    }

    /// Constitutional Rule 7: Minor Protection Invariant
    pub fn enforce_minor_protection_invariant(
        is_minor_interaction: bool,
        requested_scope: &CapabilityScope,
    ) -> Result<(), HakilleError> {
        if is_minor_interaction
            && matches!(
                requested_scope,
                CapabilityScope::Wallet | CapabilityScope::FullSystem
            )
        {
            return Err(HakilleError::CapabilityDenied(
                "Constitutional Rule 7: Agents interacting with minors are strictly prohibited from Wallet or FullSystem access"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_passport_signature_roundtrip() {
        let principal_key = SigningKey::generate(&mut OsRng);
        let principal_pk = principal_key.verifying_key().to_bytes();
        let principal_did = DidKey::from_ed25519_bytes(&principal_pk);

        let (_, agent_did) = AgentFactory::spawn_agent();
        let passport = AgentFactory::issue_passport(
            agent_did,
            principal_did,
            TenantId::new(),
            3600,
            10_000,
            200,
            vec![1, 2, 3],
            1,
        )
        .unwrap();

        // Sign passport
        let sig = passport.sign_with(&principal_key);
        assert!(passport.verify_signature(&principal_pk, &sig));

        // Wrong public key fails
        let wrong_key = SigningKey::generate(&mut OsRng);
        assert!(!passport.verify_signature(&wrong_key.verifying_key().to_bytes(), &sig));

        // Tampered passport fails
        let mut tampered = passport.clone();
        tampered.max_execution_fuel = 999_999;
        assert!(!tampered.verify_signature(&principal_pk, &sig));
    }

    #[test]
    fn test_agent_rule7_blocks_fullsystem() {
        assert!(
            AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::FullSystem)
                .is_err()
        );
        assert!(
            AgentFactory::enforce_minor_protection_invariant(false, &CapabilityScope::FullSystem)
                .is_ok()
        );
    }
}
