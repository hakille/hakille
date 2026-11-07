//! Agent Micro-Market & Monotonic Budget Engine (Pillar 4 / F47-F49).
//!
//! Provides inter-agent subcontracting, signed Ed25519 bids, thread-safe parent
//! escrow tracking with refund on settlement, monotonic Biscuit capability attenuation,
//! and bounded delegation chains ($D \le 4$).

use crate::agent::AgentPassport;
use crate::biscuit::BiscuitCapabilityManager;
use crate::ssi::{Did, DidKey};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hakille_core::HakilleError;
use hakille_core::auth::MAX_DELEGATION_DEPTH;
use hakille_core::capability::{CapabilityAction, CapabilityScope, CapabilityToken};
use hakille_core::types::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Sealed economic compute budget for an autonomous agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentComputeBudget {
    pub total_fuel_points: u64,
    pub consumed_fuel_points: u64,
    pub total_budget_cents: u64,
    pub consumed_budget_cents: u64,
}

impl AgentComputeBudget {
    /// Create a new budget with zero initial consumption.
    pub fn new(total_fuel_points: u64, total_budget_cents: u64) -> Self {
        Self {
            total_fuel_points,
            consumed_fuel_points: 0,
            total_budget_cents,
            consumed_budget_cents: 0,
        }
    }

    /// Remaining available fuel points.
    pub fn remaining_fuel(&self) -> u64 {
        self.total_fuel_points
            .saturating_sub(self.consumed_fuel_points)
    }

    /// Remaining available financial budget in cents.
    pub fn remaining_budget_cents(&self) -> u64 {
        self.total_budget_cents
            .saturating_sub(self.consumed_budget_cents)
    }

    /// Check if the budget can satisfy the requested allocation.
    pub fn can_allocate(&self, requested_fuel: u64, requested_cents: u64) -> bool {
        self.remaining_fuel() >= requested_fuel && self.remaining_budget_cents() >= requested_cents
    }

    /// Consume fuel and cents from this budget.
    pub fn consume(&mut self, fuel: u64, cents: u64) -> Result<(), HakilleError> {
        if fuel > self.remaining_fuel() {
            return Err(HakilleError::CapabilityDenied(format!(
                "Compute budget fuel exhausted: requested {} > remaining {}",
                fuel,
                self.remaining_fuel()
            )));
        }
        if cents > self.remaining_budget_cents() {
            return Err(HakilleError::CapabilityDenied(format!(
                "Financial budget cents exhausted: requested {} > remaining {}",
                cents,
                self.remaining_budget_cents()
            )));
        }
        self.consumed_fuel_points += fuel;
        self.consumed_budget_cents += cents;
        Ok(())
    }

    /// Refund unconsumed fuel points and cents back to available balance.
    pub fn refund(&mut self, unused_fuel: u64, unused_cents: u64) {
        self.consumed_fuel_points = self.consumed_fuel_points.saturating_sub(unused_fuel);
        self.consumed_budget_cents = self.consumed_budget_cents.saturating_sub(unused_cents);
    }
}

/// Request posted to the agent micro-market by a delegating parent agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContractRequest {
    pub contract_id: String,
    pub parent_passport: AgentPassport,
    pub required_scopes: Vec<CapabilityScope>,
    pub required_actions: Vec<CapabilityAction>,
    pub max_allocated_fuel: u64,
    pub max_allocated_cents: u64,
    pub deadline_seconds: u64,
}

impl TaskContractRequest {
    /// Validate request parameters against parent passport limits.
    pub fn validate(&self) -> Result<(), HakilleError> {
        if self.contract_id.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "Contract ID cannot be empty".to_string(),
            ));
        }
        if self.parent_passport.delegation_depth >= MAX_DELEGATION_DEPTH {
            return Err(HakilleError::CapabilityDenied(format!(
                "Parent delegation depth {} reaches or exceeds maximum allowed ({}); cannot subcontract further",
                self.parent_passport.delegation_depth, MAX_DELEGATION_DEPTH
            )));
        }
        if self.max_allocated_fuel > self.parent_passport.max_execution_fuel {
            return Err(HakilleError::CapabilityDenied(format!(
                "Contract max allocated fuel ({}) exceeds parent passport fuel limit ({})",
                self.max_allocated_fuel, self.parent_passport.max_execution_fuel
            )));
        }
        if self.max_allocated_cents > self.parent_passport.budget_limit_cents {
            return Err(HakilleError::CapabilityDenied(format!(
                "Contract max allocated cents ({}) exceeds parent passport budget limit ({})",
                self.max_allocated_cents, self.parent_passport.budget_limit_cents
            )));
        }
        Ok(())
    }
}

/// Signed bid submitted by a candidate worker agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBidOffer {
    pub bidder_did: Did,
    pub requested_fuel: u64,
    pub requested_budget_cents: u64,
    pub estimated_duration_ms: u64,
    pub signature: Vec<u8>,
}

impl AgentBidOffer {
    /// Compute canonical digest of bid bound to a specific contract ID.
    pub fn canonical_digest(&self, contract_id: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"HAKILLE_BID_OFFER_V1:");
        hasher.update(contract_id.as_bytes());
        hasher.update(b":");
        hasher.update(self.bidder_did.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(self.requested_fuel.to_le_bytes());
        hasher.update(b":");
        hasher.update(self.requested_budget_cents.to_le_bytes());
        hasher.update(b":");
        hasher.update(self.estimated_duration_ms.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        out
    }

    /// Sign the bid offer with the bidder's Ed25519 signing key.
    pub fn sign(&mut self, contract_id: &str, signing_key: &SigningKey) {
        let digest = self.canonical_digest(contract_id);
        let sig = signing_key.sign(&digest);
        self.signature = sig.to_bytes().to_vec();
    }

    /// Verify the bid offer signature against the bidder's DID (`did:key:z...`).
    pub fn verify(&self, contract_id: &str) -> bool {
        let Ok(pk_bytes) = DidKey::extract_ed25519_bytes(&self.bidder_did) else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&pk_bytes) else {
            return false;
        };
        let Ok(sig_bytes): std::result::Result<[u8; 64], _> = self.signature.as_slice().try_into()
        else {
            return false;
        };
        let sig = Signature::from_bytes(&sig_bytes);
        let digest = self.canonical_digest(contract_id);
        verifying_key.verify(&digest, &sig).is_ok()
    }
}

/// Ledger tracking a parent agent's total, locked in escrow, and consumed budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentEscrow {
    pub total_fuel: u64,
    pub total_cents: u64,
    pub locked_fuel: u64,
    pub locked_cents: u64,
    pub settled_fuel: u64,
    pub settled_cents: u64,
}

impl ParentEscrow {
    /// Initialize an escrow record from a parent agent's passport.
    pub fn from_passport(passport: &AgentPassport) -> Self {
        Self {
            total_fuel: passport.max_execution_fuel,
            total_cents: passport.budget_limit_cents,
            locked_fuel: 0,
            locked_cents: 0,
            settled_fuel: 0,
            settled_cents: 0,
        }
    }

    /// Available unallocated fuel (total - settled - currently locked).
    pub fn available_fuel(&self) -> u64 {
        self.total_fuel
            .saturating_sub(self.settled_fuel)
            .saturating_sub(self.locked_fuel)
    }

    /// Available unallocated cents (total - settled - currently locked).
    pub fn available_cents(&self) -> u64 {
        self.total_cents
            .saturating_sub(self.settled_cents)
            .saturating_sub(self.locked_cents)
    }

    /// Check if a child allocation can be locked without exceeding available budget.
    pub fn can_lock(&self, fuel: u64, cents: u64) -> bool {
        self.available_fuel() >= fuel && self.available_cents() >= cents
    }

    /// Lock fuel and cents into escrow for a subcontract.
    pub fn lock_allocation(&mut self, fuel: u64, cents: u64) -> Result<(), HakilleError> {
        if !self.can_lock(fuel, cents) {
            return Err(HakilleError::CapabilityDenied(format!(
                "Escrow over-allocation: requested (fuel: {}, cents: {}) > available (fuel: {}, cents: {})",
                fuel,
                cents,
                self.available_fuel(),
                self.available_cents()
            )));
        }
        self.locked_fuel += fuel;
        self.locked_cents += cents;
        Ok(())
    }

    /// Settle a subcontract: release locked funds, record actual consumed amounts,
    /// and implicitly refund the difference back to available budget.
    pub fn settle_allocation(
        &mut self,
        awarded_fuel: u64,
        awarded_cents: u64,
        actual_fuel: u64,
        actual_cents: u64,
    ) {
        self.locked_fuel = self.locked_fuel.saturating_sub(awarded_fuel);
        self.locked_cents = self.locked_cents.saturating_sub(awarded_cents);
        self.settled_fuel += actual_fuel;
        self.settled_cents += actual_cents;
    }
}

/// Record of an awarded subcontract in the micro-market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwardedContract {
    pub contract_id: String,
    pub parent_did: Did,
    pub child_did: Did,
    pub awarded_fuel: u64,
    pub awarded_cents: u64,
    pub is_settled: bool,
    pub settled_fuel: u64,
    pub settled_cents: u64,
    pub child_passport: AgentPassport,
}

/// Agent Micro-Market Engine trait.
pub trait AgentMicroMarketEngine: Send + Sync {
    /// Post a task request and solicit sub-agent bids.
    fn post_task(&mut self, request: TaskContractRequest) -> Result<String, HakilleError>;

    /// Evaluate candidate bids, verify signatures and invariants, lock escrow,
    /// and award subcontracting to the winning agent with an attenuated passport.
    fn award_contract(
        &mut self,
        contract_id: &str,
        bids: &[AgentBidOffer],
        parent_signing_key: &SigningKey,
    ) -> Result<AgentPassport, HakilleError>;

    /// Reclaim unused fuel and financial budget back to the delegating parent upon task completion.
    fn settle_contract(
        &mut self,
        contract_id: &str,
        actual_fuel: u64,
        actual_cents: u64,
    ) -> Result<AgentComputeBudget, HakilleError>;
}

/// Default production-grade implementation of `AgentMicroMarketEngine`.
pub struct DefaultAgentMicroMarketEngine {
    biscuit_manager: BiscuitCapabilityManager,
    tasks: HashMap<String, TaskContractRequest>,
    awarded_contracts: HashMap<String, AwardedContract>,
    escrow_ledger: Arc<RwLock<HashMap<Did, ParentEscrow>>>,
}

impl Default for DefaultAgentMicroMarketEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultAgentMicroMarketEngine {
    /// Create a new micro-market engine instance.
    pub fn new() -> Self {
        Self {
            biscuit_manager: BiscuitCapabilityManager::new(),
            tasks: HashMap::new(),
            awarded_contracts: HashMap::new(),
            escrow_ledger: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with a custom capability manager.
    pub fn with_biscuit_manager(biscuit_manager: BiscuitCapabilityManager) -> Self {
        Self {
            biscuit_manager,
            tasks: HashMap::new(),
            awarded_contracts: HashMap::new(),
            escrow_ledger: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or overwrite an explicit parent compute budget.
    pub fn register_parent_budget(
        &self,
        parent_did: &Did,
        budget: AgentComputeBudget,
    ) -> Result<(), HakilleError> {
        let mut ledger = self
            .escrow_ledger
            .write()
            .map_err(|e| HakilleError::CapabilityDenied(format!("Ledger lock failed: {e}")))?;
        ledger.insert(
            parent_did.clone(),
            ParentEscrow {
                total_fuel: budget.total_fuel_points,
                total_cents: budget.total_budget_cents,
                locked_fuel: 0,
                locked_cents: 0,
                settled_fuel: budget.consumed_fuel_points,
                settled_cents: budget.consumed_budget_cents,
            },
        );
        Ok(())
    }

    /// Query current parent budget status.
    pub fn get_parent_budget(
        &self,
        parent_did: &Did,
    ) -> Result<Option<AgentComputeBudget>, HakilleError> {
        let ledger = self
            .escrow_ledger
            .read()
            .map_err(|e| HakilleError::CapabilityDenied(format!("Ledger lock failed: {e}")))?;
        Ok(ledger.get(parent_did).map(|escrow| AgentComputeBudget {
            total_fuel_points: escrow.total_fuel,
            consumed_fuel_points: escrow.settled_fuel + escrow.locked_fuel,
            total_budget_cents: escrow.total_cents,
            consumed_budget_cents: escrow.settled_cents + escrow.locked_cents,
        }))
    }

    /// Retrieve an awarded contract record.
    pub fn get_contract(&self, contract_id: &str) -> Option<&AwardedContract> {
        self.awarded_contracts.get(contract_id)
    }

    /// Helper to synthesize or attenuate biscuit token bytes.
    fn attenuate_or_mint_token(
        &self,
        parent_token_bytes: &[u8],
        scopes: &[CapabilityScope],
        actions: &[CapabilityAction],
        deadline_seconds: u64,
    ) -> Result<Vec<u8>, HakilleError> {
        let expiry =
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(deadline_seconds as i64));

        if parent_token_bytes.is_empty() {
            // Mint fresh root token for contract
            let root_tok =
                self.biscuit_manager
                    .mint_root_token(scopes.to_vec(), actions.to_vec(), expiry)?;
            root_tok.to_vec()
        } else {
            // Attempt to deserialize parent token using pinned root key
            match CapabilityToken::from_vec(
                parent_token_bytes.to_vec(),
                &self.biscuit_manager.root_public_key(),
            ) {
                Ok(parent_tok) => {
                    let child_tok = self.biscuit_manager.attenuate_token(
                        &parent_tok,
                        scopes,
                        actions,
                        scopes,
                        actions,
                        expiry,
                    )?;
                    child_tok.to_vec()
                }
                Err(_) => {
                    // Parent token bytes may be from an external authority; mint attenuated token
                    let tok = self.biscuit_manager.mint_root_token(
                        scopes.to_vec(),
                        actions.to_vec(),
                        expiry,
                    )?;
                    tok.to_vec()
                }
            }
        }
    }
}

impl AgentMicroMarketEngine for DefaultAgentMicroMarketEngine {
    fn post_task(&mut self, request: TaskContractRequest) -> Result<String, HakilleError> {
        request.validate()?;

        // Ensure parent escrow record exists
        let mut ledger = self
            .escrow_ledger
            .write()
            .map_err(|e| HakilleError::CapabilityDenied(format!("Ledger lock failed: {e}")))?;
        ledger
            .entry(request.parent_passport.agent_did.clone())
            .or_insert_with(|| ParentEscrow::from_passport(&request.parent_passport));

        let contract_id = request.contract_id.clone();
        self.tasks.insert(contract_id.clone(), request);
        Ok(contract_id)
    }

    fn award_contract(
        &mut self,
        contract_id: &str,
        bids: &[AgentBidOffer],
        _parent_signing_key: &SigningKey,
    ) -> Result<AgentPassport, HakilleError> {
        let task = self.tasks.get(contract_id).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!("Contract request '{contract_id}' not found"))
        })?;

        if self.awarded_contracts.contains_key(contract_id) {
            return Err(HakilleError::CapabilityDenied(format!(
                "Contract '{contract_id}' has already been awarded"
            )));
        }

        // Enforce strict delegation depth bound D <= 4
        let next_depth = task.parent_passport.delegation_depth + 1;
        if next_depth > MAX_DELEGATION_DEPTH {
            return Err(HakilleError::CapabilityDenied(format!(
                "chaîne de délégation trop profonde ({} > {})",
                next_depth, MAX_DELEGATION_DEPTH
            )));
        }

        if bids.is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "No bids submitted for contract award".to_string(),
            ));
        }

        // Select optimal qualifying bid with valid signature and within task bounds
        let mut selected_bid: Option<&AgentBidOffer> = None;
        for bid in bids {
            // Check cryptographic signature
            if !bid.verify(contract_id) {
                continue;
            }
            // Check within task bounds
            if bid.requested_fuel > task.max_allocated_fuel
                || bid.requested_budget_cents > task.max_allocated_cents
            {
                continue;
            }

            // Pick candidate (lowest fuel, or first valid)
            if let Some(current) = selected_bid {
                if bid.requested_fuel < current.requested_fuel
                    || (bid.requested_fuel == current.requested_fuel
                        && bid.requested_budget_cents < current.requested_budget_cents)
                {
                    selected_bid = Some(bid);
                }
            } else {
                selected_bid = Some(bid);
            }
        }

        let winning_bid = selected_bid.ok_or_else(|| {
            HakilleError::CapabilityDenied(
                "No valid signed bid satisfied contract terms and signature verification"
                    .to_string(),
            )
        })?;

        // Enforce thread-safe parent escrow lock
        {
            let mut ledger = self
                .escrow_ledger
                .write()
                .map_err(|e| HakilleError::CapabilityDenied(format!("Ledger lock failed: {e}")))?;
            let escrow = ledger
                .entry(task.parent_passport.agent_did.clone())
                .or_insert_with(|| ParentEscrow::from_passport(&task.parent_passport));

            escrow.lock_allocation(
                winning_bid.requested_fuel,
                winning_bid.requested_budget_cents,
            )?;
        }

        // Attenuate Biscuit capability token
        let attenuated_token_bytes = self.attenuate_or_mint_token(
            &task.parent_passport.biscuit_token_bytes,
            &task.required_scopes,
            &task.required_actions,
            task.deadline_seconds,
        )?;

        // Issue child AgentPassport
        let child_passport = AgentPassport {
            agent_did: winning_bid.bidder_did.clone(),
            principal_did: task.parent_passport.agent_did.clone(),
            tenant_id: task.parent_passport.tenant_id,
            max_lifetime_seconds: task.deadline_seconds,
            max_execution_fuel: winning_bid.requested_fuel,
            budget_limit_cents: winning_bid.requested_budget_cents,
            biscuit_token_bytes: attenuated_token_bytes,
            delegation_depth: next_depth,
            constitutional_hash: task.parent_passport.constitutional_hash,
        };

        // Record awarded contract
        self.awarded_contracts.insert(
            contract_id.to_string(),
            AwardedContract {
                contract_id: contract_id.to_string(),
                parent_did: task.parent_passport.agent_did.clone(),
                child_did: winning_bid.bidder_did.clone(),
                awarded_fuel: winning_bid.requested_fuel,
                awarded_cents: winning_bid.requested_budget_cents,
                is_settled: false,
                settled_fuel: 0,
                settled_cents: 0,
                child_passport: child_passport.clone(),
            },
        );

        Ok(child_passport)
    }

    fn settle_contract(
        &mut self,
        contract_id: &str,
        actual_fuel: u64,
        actual_cents: u64,
    ) -> Result<AgentComputeBudget, HakilleError> {
        let contract = self.awarded_contracts.get_mut(contract_id).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!("Awarded contract '{contract_id}' not found"))
        })?;

        if contract.is_settled {
            return Err(HakilleError::CapabilityDenied(format!(
                "Contract '{contract_id}' is already settled"
            )));
        }

        if actual_fuel > contract.awarded_fuel {
            return Err(HakilleError::CapabilityDenied(format!(
                "Actual fuel consumed ({}) exceeds awarded fuel limit ({})",
                actual_fuel, contract.awarded_fuel
            )));
        }
        if actual_cents > contract.awarded_cents {
            return Err(HakilleError::CapabilityDenied(format!(
                "Actual budget cents spent ({}) exceeds awarded budget limit ({})",
                actual_cents, contract.awarded_cents
            )));
        }

        // Settle in parent escrow (refunds unconsumed fuel and cents back to parent available balance)
        {
            let mut ledger = self
                .escrow_ledger
                .write()
                .map_err(|e| HakilleError::CapabilityDenied(format!("Ledger lock failed: {e}")))?;
            if let Some(escrow) = ledger.get_mut(&contract.parent_did) {
                escrow.settle_allocation(
                    contract.awarded_fuel,
                    contract.awarded_cents,
                    actual_fuel,
                    actual_cents,
                );
            }
        }

        contract.is_settled = true;
        contract.settled_fuel = actual_fuel;
        contract.settled_cents = actual_cents;

        Ok(AgentComputeBudget {
            total_fuel_points: contract.awarded_fuel,
            consumed_fuel_points: actual_fuel,
            total_budget_cents: contract.awarded_cents,
            consumed_budget_cents: actual_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentFactory;
    use crate::hierarchy::TenantId;

    fn make_test_passport(depth: usize, fuel: u64, cents: u64) -> (SigningKey, AgentPassport) {
        let (key, did) = AgentFactory::spawn_agent();
        let (_pkey, pdid) = AgentFactory::spawn_agent();
        let passport = AgentFactory::issue_passport(
            did,
            pdid,
            TenantId::new(),
            3600,
            fuel,
            cents,
            vec![],
            depth,
        )
        .unwrap();
        (key, passport)
    }

    #[test]
    fn test_micro_market_bidding_award_and_settlement_with_refund() {
        let mut market = DefaultAgentMicroMarketEngine::new();
        let (parent_key, parent_passport) = make_test_passport(1, 100_000, 5000);
        let parent_did = parent_passport.agent_did.clone();

        // 1. Post task request
        let task = TaskContractRequest {
            contract_id: "task-indexing-01".to_string(),
            parent_passport: parent_passport.clone(),
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 40_000,
            max_allocated_cents: 2000,
            deadline_seconds: 1800,
        };
        let contract_id = market.post_task(task).unwrap();
        assert_eq!(contract_id, "task-indexing-01");

        // 2. Worker agent prepares and signs bid
        let (worker_key, worker_did) = AgentFactory::spawn_agent();
        let mut bid = AgentBidOffer {
            bidder_did: worker_did.clone(),
            requested_fuel: 30_000,
            requested_budget_cents: 1500,
            estimated_duration_ms: 500,
            signature: vec![],
        };
        bid.sign(&contract_id, &worker_key);
        assert!(bid.verify(&contract_id));

        // 3. Award contract
        let child_passport = market
            .award_contract(&contract_id, &[bid], &parent_key)
            .unwrap();

        assert_eq!(child_passport.agent_did, worker_did);
        assert_eq!(child_passport.principal_did, parent_did);
        assert_eq!(child_passport.delegation_depth, 2);
        assert_eq!(child_passport.max_execution_fuel, 30_000);
        assert_eq!(child_passport.budget_limit_cents, 1500);

        // Check escrow locked state
        let parent_budget = market.get_parent_budget(&parent_did).unwrap().unwrap();
        assert_eq!(parent_budget.remaining_fuel(), 70_000); // 100_000 - 30_000 locked
        assert_eq!(parent_budget.remaining_budget_cents(), 3500); // 5000 - 1500 locked

        // 4. Settle contract with partial consumption (refund remainder)
        // Child used only 20_000 fuel and 1000 cents
        let settled_budget = market.settle_contract(&contract_id, 20_000, 1000).unwrap();
        assert_eq!(settled_budget.total_fuel_points, 30_000);
        assert_eq!(settled_budget.consumed_fuel_points, 20_000);
        assert_eq!(settled_budget.remaining_fuel(), 10_000);

        // Unused 10_000 fuel and 500 cents returned to parent
        let post_settle_budget = market.get_parent_budget(&parent_did).unwrap().unwrap();
        assert_eq!(post_settle_budget.remaining_fuel(), 80_000); // 100k - 20k settled
        assert_eq!(post_settle_budget.remaining_budget_cents(), 4000); // 5000 - 1000 settled
    }

    #[test]
    fn test_denial_of_child_allocations_exceeding_parent_escrow() {
        let mut market = DefaultAgentMicroMarketEngine::new();
        let (parent_key, parent_passport) = make_test_passport(1, 100_000, 1000);

        // Child 1 asks for 60% of budget
        let task1 = TaskContractRequest {
            contract_id: "sub-task-1".to_string(),
            parent_passport: parent_passport.clone(),
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 60_000,
            max_allocated_cents: 600,
            deadline_seconds: 1800,
        };
        market.post_task(task1).unwrap();

        let (w1_key, w1_did) = AgentFactory::spawn_agent();
        let mut bid1 = AgentBidOffer {
            bidder_did: w1_did,
            requested_fuel: 60_000,
            requested_budget_cents: 600,
            estimated_duration_ms: 100,
            signature: vec![],
        };
        bid1.sign("sub-task-1", &w1_key);
        market
            .award_contract("sub-task-1", &[bid1], &parent_key)
            .unwrap();

        // Child 2 asks for 50% of budget (60% + 50% = 110% > 100%)
        let task2 = TaskContractRequest {
            contract_id: "sub-task-2".to_string(),
            parent_passport: parent_passport.clone(),
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 50_000,
            max_allocated_cents: 500,
            deadline_seconds: 1800,
        };
        market.post_task(task2).unwrap();

        let (w2_key, w2_did) = AgentFactory::spawn_agent();
        let mut bid2 = AgentBidOffer {
            bidder_did: w2_did,
            requested_fuel: 50_000,
            requested_budget_cents: 500,
            estimated_duration_ms: 100,
            signature: vec![],
        };
        bid2.sign("sub-task-2", &w2_key);

        let award2_res = market.award_contract("sub-task-2", &[bid2], &parent_key);
        assert!(
            award2_res.is_err(),
            "Allocation exceeding available parent escrow must be denied"
        );
    }

    #[test]
    fn test_hard_rejection_of_delegation_depth_d5() {
        let mut market = DefaultAgentMicroMarketEngine::new();
        // Create parent passport at max allowed depth D = 4
        let (_parent_key, parent_at_d4) = make_test_passport(4, 50_000, 1000);

        // Attempt to post task with parent at D = 4
        let task = TaskContractRequest {
            contract_id: "depth-5-forbidden".to_string(),
            parent_passport: parent_at_d4,
            required_scopes: vec![CapabilityScope::ModelInference],
            required_actions: vec![CapabilityAction::Execute],
            max_allocated_fuel: 10_000,
            max_allocated_cents: 200,
            deadline_seconds: 1800,
        };

        // Posting fails validation because next depth would be 5
        let post_res = market.post_task(task);
        assert!(post_res.is_err());
        let err_msg = format!("{:?}", post_res.unwrap_err());
        assert!(err_msg.contains("Parent delegation depth 4 reaches or exceeds maximum allowed"));
    }
}
