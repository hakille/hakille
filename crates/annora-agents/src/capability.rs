use annora_core::{
    AnnoraError, CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaBreachPolicy {
    Throttle,
    AutoRevoke,
}

/// Explicit frequency, volume, and time-window quota for capabilities (§31c).
/// Prevents runaway agent loops, denial of service, or rogue resource exhaustion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityQuota {
    pub max_invocations_per_minute: u32,
    pub max_total_invocations: u32,
    pub max_payload_bytes_per_call: usize,
    pub policy_on_breach: QuotaBreachPolicy,
}

impl Default for CapabilityQuota {
    fn default() -> Self {
        Self {
            max_invocations_per_minute: 60,
            max_total_invocations: 1000,
            max_payload_bytes_per_call: 10 * 1024 * 1024, // 10 MB
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        }
    }
}

/// Stateful tracker monitoring capability rate-limiting and quota compliance (§31c).
pub struct AgentQuotaTracker {
    pub quota: CapabilityQuota,
    invocations_last_minute: VecDeque<std::time::Instant>,
    total_invocations_count: u32,
    is_revoked: bool,
    revocation_reason: Option<String>,
}

impl AgentQuotaTracker {
    pub fn new(quota: CapabilityQuota) -> Self {
        Self {
            quota,
            invocations_last_minute: VecDeque::new(),
            total_invocations_count: 0,
            is_revoked: false,
            revocation_reason: None,
        }
    }

    /// Record capability usage and enforce quota limits.
    pub fn record_invocation(&mut self, payload_bytes: usize) -> Result<()> {
        if self.is_revoked {
            return Err(AnnoraError::CapabilityDenied(
                self.revocation_reason
                    .clone()
                    .unwrap_or_else(|| "Capability révoquée automatiquement pour dépassement de quota (§31c)".to_string()),
            ));
        }

        // 1. Check single call payload size
        if payload_bytes > self.quota.max_payload_bytes_per_call {
            if self.quota.policy_on_breach == QuotaBreachPolicy::AutoRevoke {
                self.is_revoked = true;
                self.revocation_reason = Some(format!(
                    "Volume par appel dépassé ({} > {} octets)",
                    payload_bytes, self.quota.max_payload_bytes_per_call
                ));
            }
            return Err(AnnoraError::CapabilityDenied(
                "Dépassement du volume de payload autorisé pour cette capability".to_string(),
            ));
        }

        let now = std::time::Instant::now();
        let one_minute_ago = now.checked_sub(std::time::Duration::from_secs(60)).unwrap_or(now);

        // 2. Clean up calls older than 60s
        while let Some(&t) = self.invocations_last_minute.front() {
            if t < one_minute_ago {
                self.invocations_last_minute.pop_front();
            } else {
                break;
            }
        }

        // 3. Check rate-limit (invocations / min)
        if self.invocations_last_minute.len() as u32 >= self.quota.max_invocations_per_minute {
            if self.quota.policy_on_breach == QuotaBreachPolicy::AutoRevoke {
                self.is_revoked = true;
                self.revocation_reason = Some(format!(
                    "Fréquence maximale dépassée (> {} appels/minute)",
                    self.quota.max_invocations_per_minute
                ));
            }
            return Err(AnnoraError::CapabilityDenied(
                "Fréquence d'appel maximale dépassée pour cette capability (§31c)".to_string(),
            ));
        }

        // 4. Check total lifetime quota
        if self.total_invocations_count >= self.quota.max_total_invocations {
            self.is_revoked = true;
            self.revocation_reason = Some("Quota total d'invocations de la session épuisé".to_string());
            return Err(AnnoraError::CapabilityExpired);
        }

        self.invocations_last_minute.push_back(now);
        self.total_invocations_count += 1;
        Ok(())
    }

    pub fn is_revoked(&self) -> bool {
        self.is_revoked
    }

    pub fn total_calls(&self) -> u32 {
        self.total_invocations_count
    }
}

pub struct AgentCapabilityVerifier {
    pub authority: CapabilityAuthority,
}

impl AgentCapabilityVerifier {
    pub fn new(authority: CapabilityAuthority) -> Self {
        Self { authority }
    }

    pub fn verify_action(
        &self,
        token: &CapabilityToken,
        scope: CapabilityScope,
        action: CapabilityAction,
    ) -> Result<()> {
        self.authority.verify(token, scope, action)
    }

    pub fn verify_attenuation(
        &self,
        _parent_token: &CapabilityToken,
        child_token: &CapabilityToken,
        child_scope: CapabilityScope,
        child_action: CapabilityAction,
    ) -> Result<()> {
        // Biscuit inherently verifies that any check added in blocks must hold
        self.authority
            .verify(child_token, child_scope, child_action)
            .map_err(|_| AnnoraError::AttenuationViolation)
    }
}
