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
            return Err(AnnoraError::CapabilityDenied(
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
            return Err(AnnoraError::CapabilityDenied(
                "Fréquence d'appel maximale dépassée pour cette capability (§31c)".to_string(),
            ));
        }

        // 4. Check total lifetime quota
        if self.total_invocations_count >= self.quota.max_total_invocations {
            self.is_revoked = true;
            self.revocation_reason =
                Some("Quota total d'invocations de la session épuisé".to_string());
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
        parent_token: &CapabilityToken,
        child_token: &CapabilityToken,
        child_scope: CapabilityScope,
        child_action: CapabilityAction,
    ) -> Result<()> {
        // 1. Le jeton enfant doit dériver du parent : même bloc d'autorité
        //    (Biscuit `append` conserve le bloc 0) et au moins un bloc de plus.
        if child_token.biscuit.block_count() <= parent_token.biscuit.block_count() {
            return Err(AnnoraError::AttenuationViolation);
        }
        let parent_authority = parent_token
            .biscuit
            .print_block_source(0)
            .map_err(|_| AnnoraError::AttenuationViolation)?;
        let child_authority = child_token
            .biscuit
            .print_block_source(0)
            .map_err(|_| AnnoraError::AttenuationViolation)?;
        if parent_authority != child_authority {
            return Err(AnnoraError::AttenuationViolation);
        }

        // 2. Décodage des droits effectifs des deux tokens : enfant ⊆ parent.
        //    Expiry : les checks temporels ajoutés par l'enfant (blocs au-delà
        //    de ceux du parent) doivent être <= expiry du parent — comparer le
        //    min global serait vide de sens car l'enfant embarque les checks
        //    du parent (min global toujours <= parent). Fail-closed si absent.
        let (parent_rights, _) = effective_rights(parent_token)?;
        let (child_rights, _) = effective_rights(child_token)?;
        if !child_rights.is_subset(&parent_rights) {
            return Err(AnnoraError::AttenuationViolation);
        }
        let parent_expiry =
            min_expiry_in_blocks(parent_token, 0..parent_token.biscuit.block_count())?;
        let child_added_expiry = min_expiry_in_blocks(
            child_token,
            parent_token.biscuit.block_count()..child_token.biscuit.block_count(),
        )?;
        match (parent_expiry, child_added_expiry) {
            (Some(p), Some(c)) if c <= p => {}
            _ => return Err(AnnoraError::AttenuationViolation),
        }

        // 3. Le couple (scope, action) demandé doit appartenir aux droits
        //    effectifs de l'enfant ET passer la vérification cryptographique.
        let s = format!("{:?}", child_scope).to_lowercase();
        let a = format!("{:?}", child_action).to_lowercase();
        if !child_rights.contains(&(s, a)) {
            return Err(AnnoraError::AttenuationViolation);
        }
        self.authority
            .verify(child_token, child_scope, child_action)
            .map_err(|_| AnnoraError::AttenuationViolation)
    }
}

/// Droits effectifs d'un token Biscuit décodés depuis ses blocs :
/// `rights` = paires (scope, action) autorisées, `expiry` = plus petit
/// `time <= N` trouvé dans tous les blocs (fail-closed si absent).
fn effective_rights(
    token: &CapabilityToken,
) -> Result<(std::collections::HashSet<(String, String)>, Option<i64>)> {
    let mut root_rights = std::collections::HashSet::new();
    let mut allowed_scopes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut allowed_actions: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut expiry: Option<i64> = None;

    for i in 0..token.biscuit.block_count() {
        let src = token
            .biscuit
            .print_block_source(i)
            .map_err(|_| AnnoraError::AttenuationViolation)?;
        for (s, a) in parse_right_facts(&src) {
            root_rights.insert((s, a));
        }
        let (scopes, actions) = parse_allowed_facts(&src);
        allowed_scopes.extend(scopes);
        allowed_actions.extend(actions);
        for exp in parse_expiry_checks(&src) {
            expiry = Some(expiry.map_or(exp, |e: i64| e.min(exp)));
        }
    }

    // Sans bloc d'atténuation, l'effectif = les droits racine. Sinon,
    // intersection avec le produit scopes×actions autorisés.
    let effective = if allowed_scopes.is_empty() || allowed_actions.is_empty() {
        root_rights
    } else {
        root_rights
            .into_iter()
            .filter(|(s, a)| allowed_scopes.contains(s) && allowed_actions.contains(a))
            .collect()
    };
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
            .map_err(|_| AnnoraError::AttenuationViolation)?;
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
