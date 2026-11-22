use hakille_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, HakilleError, Result,
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
            return Err(HakilleError::CapabilityDenied(
                self.revocation_reason.clone().unwrap_or_else(|| {
                    "Capability révoquée automatiquement pour dépassement de quota (§31c)"
                        .to_string()
                }),
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
            return Err(HakilleError::CapabilityDenied(
                "Dépassement du volume de payload autorisé pour cette capability".to_string(),
            ));
        }

        let now = std::time::Instant::now();
        let one_minute_ago = now
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(now);

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
            return Err(HakilleError::CapabilityDenied(
                "Fréquence d'appel maximale dépassée pour cette capability (§31c)".to_string(),
            ));
        }

        // 4. Check total lifetime quota
        if self.total_invocations_count >= self.quota.max_total_invocations {
            self.is_revoked = true;
            self.revocation_reason =
                Some("Quota total d'invocations de la session épuisé".to_string());
            return Err(HakilleError::CapabilityExpired);
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
        parent_token: &CapabilityToken,
        child_token: &CapabilityToken,
        child_scope: CapabilityScope,
        child_action: CapabilityAction,
    ) -> Result<()> {
        // 1. Le parent doit être cryptographiquement valide sous l'autorité épinglée
        //    et couvrir le couple (child_scope, child_action).
        self.authority
            .verify(parent_token, child_scope.clone(), child_action.clone())
            .map_err(|_| HakilleError::AttenuationViolation)?;

        // 2. Le jeton enfant doit dériver du parent : tous les blocs du parent
        //    doivent être préservés à l'identique en préfixe, et l'enfant doit
        //    porter au moins un bloc d'atténuation supplémentaire.
        let parent_blocks = parent_token.biscuit.block_count();
        let child_blocks = child_token.biscuit.block_count();
        if child_blocks <= parent_blocks {
            return Err(HakilleError::AttenuationViolation);
        }
        for idx in 0..parent_blocks {
            let parent_src = parent_token
                .biscuit
                .print_block_source(idx)
                .map_err(|_| HakilleError::AttenuationViolation)?;
            let child_src = child_token
                .biscuit
                .print_block_source(idx)
                .map_err(|_| HakilleError::AttenuationViolation)?;
            if parent_src != child_src {
                return Err(HakilleError::AttenuationViolation);
            }
        }

        // 3. Décodage par intersection monotone des droits effectifs : enfant ⊆ parent.
        let (parent_rights, _) = effective_rights(parent_token)?;
        let (child_rights, _) = effective_rights(child_token)?;
        if !child_rights.is_subset(&parent_rights) {
            return Err(HakilleError::AttenuationViolation);
        }
        let parent_expiry = min_expiry_in_blocks(parent_token, 0..parent_blocks)?;
        let child_added_expiry = min_expiry_in_blocks(child_token, parent_blocks..child_blocks)?;
        match (parent_expiry, child_added_expiry) {
            (Some(p), Some(c)) if c <= p => {}
            _ => return Err(HakilleError::AttenuationViolation),
        }

        // 4. Le couple (scope, action) demandé doit appartenir aux droits
        //    effectifs de l'enfant ET passer la vérification cryptographique.
        let s = format!("{:?}", child_scope).to_lowercase();
        let a = format!("{:?}", child_action).to_lowercase();
        if !child_rights.contains(&(s, a)) {
            return Err(HakilleError::AttenuationViolation);
        }
        self.authority
            .verify(child_token, child_scope, child_action)
            .map_err(|_| HakilleError::AttenuationViolation)
    }
}

/// Paires (scope, action) autorisées + plus petit `time <= N` (fail-closed
/// si absent).
type EffectiveRights = (std::collections::HashSet<(String, String)>, Option<i64>);

/// Droits effectifs d'un token Biscuit décodés depuis ses blocs :
/// chaque bloc d'atténuation restreint par **intersection** les scopes/actions
/// autorisés par les blocs précédents (jamais d'union entre blocs).
fn effective_rights(token: &CapabilityToken) -> Result<EffectiveRights> {
    let mut effective: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut expiry: Option<i64> = None;

    for i in 0..token.biscuit.block_count() {
        let src = token
            .biscuit
            .print_block_source(i)
            .map_err(|_| HakilleError::AttenuationViolation)?;
        if i == 0 {
            for (s, a) in parse_right_facts(&src) {
                effective.insert((s, a));
            }
        }
        let (scopes, actions) = parse_allowed_facts(&src);
        if !scopes.is_empty() {
            let scope_set: std::collections::HashSet<String> = scopes.into_iter().collect();
            effective.retain(|(s, _)| scope_set.contains(s));
        }
        if !actions.is_empty() {
            let action_set: std::collections::HashSet<String> = actions.into_iter().collect();
            effective.retain(|(_, a)| action_set.contains(a));
        }
        for exp in parse_expiry_checks(&src) {
            expiry = Some(expiry.map_or(exp, |e: i64| e.min(exp)));
        }
    }

    Ok((effective, expiry))
}

/// Borne temporelle minimale déclarée dans une plage de blocs
/// (les `check … time <= N`). `None` si aucun check trouvé (fail-closed).
fn min_expiry_in_blocks(
    token: &CapabilityToken,
    range: std::ops::Range<usize>,
) -> Result<Option<i64>> {
    let mut expiry: Option<i64> = None;
    for i in range {
        let src = token
            .biscuit
            .print_block_source(i)
            .map_err(|_| HakilleError::AttenuationViolation)?;
        for exp in parse_expiry_checks(&src) {
            expiry = Some(expiry.map_or(exp, |e: i64| e.min(exp)));
        }
    }
    Ok(expiry)
}

/// Extrait les faits `right("scope", "action")` d'une source de bloc.
fn parse_right_facts(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(idx) = rest.find("right(\"") {
        let after = &rest[idx + 7..];
        if let Some(end1) = after.find('"') {
            let scope = &after[..end1];
            let after2 = &after[end1 + 1..];
            if let Some(q2) = after2.find('"') {
                let after3 = &after2[q2 + 1..];
                if let Some(end2) = after3.find('"') {
                    let action = &after3[..end2];
                    out.push((scope.to_string(), action.to_string()));
                    rest = &after3[end2 + 1..];
                    continue;
                }
            }
            break;
        } else {
            break;
        }
    }
    out
}

/// Extrait les faits `allowed_scope("s")` / `allowed_action("a")`.
fn parse_allowed_facts(src: &str) -> (Vec<String>, Vec<String>) {
    fn collect(src: &str, pred: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = src;
        let needle = format!("{}(\"", pred);
        while let Some(idx) = rest.find(needle.as_str()) {
            let after = &rest[idx + needle.len()..];
            if let Some(end) = after.find('"') {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
        out
    }
    (
        collect(src, "allowed_scope"),
        collect(src, "allowed_action"),
    )
}

/// Extrait toutes les bornes `time($t), ... <= N` / `< N` d'une source.
fn parse_expiry_checks(src: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let is_le = bytes[i] == b'<' && bytes[i + 1] == b'=';
        let is_lt = bytes[i] == b'<' && bytes[i + 1] != b'=';
        if (is_le || is_lt) && src[..i].rfind("time").is_some_and(|p| i - p < 40) {
            let mut j = if is_le { i + 2 } else { i + 1 };
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            let mut k = j;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                k += 1;
            }
            if k > j {
                if let Ok(n) = src[j..k].parse::<i64>() {
                    out.push(n);
                }
                i = k;
                continue;
            }
        }
        i += 1;
    }
    out
}
