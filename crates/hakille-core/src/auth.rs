//! Contexte d'autorisation central — enforcement des capabilities Biscuit.
//!
//! # Contrat
//!
//! - [`AuthorizationContext`] porte l'intention complète d'une requête :
//!   identité (`subject`), audience attendue, borne d'expiration, couple
//!   scope/action, ressource normalisée, quota, nonce anti-rejeu et chaîne
//!   de délégation.
//! - Validation stricte (fail-closed) : refus `..`, chemins absolus hors-VFS,
//!   token expiré, audience mismatch, scope insuffisant, quota épuisé,
//!   nonce malformé, délégation trop profonde.
//! - Vérification cryptographique via [`CapabilityAuthority::verify_pinned`]
//!   (clé racine épinglée) ou [`verify_capability_with_root_key`] pour les
//!   substrates sans clé privée. Jamais de `unimplemented!`.

use crate::capability::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken,
    verify_capability_with_root_key,
};
use crate::error::{HakilleError, Result};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};

/// Profondeur maximale de délégation (chaîne `subject -> délégué -> …`).
pub const MAX_DELEGATION_DEPTH: usize = 4;

/// Racines VFS autorisées pour les chemins absolus côté [`CapabilityScope::Filesystem`].
/// Tout chemin absolu hors de ces préfixes est refusé (hors-VFS).
pub const ALLOWED_ABSOLUTE_PREFIXES: &[&str] = &["/vfs/", "/tmp/"];

/// Longueur maximale d'une ressource normalisée (borne anti-DoS).
pub const MAX_RESOURCE_LEN: usize = 1024;

/// Quota d'appels restants pour un contexte (décrémenté par l'appelant via
/// [`Quota::try_consume`] ; `remaining == 0` = refus explicite).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Quota {
    /// Plafond initial (audit).
    pub limit: u64,
    /// Appels restants.
    pub remaining: u64,
}

impl Quota {
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            remaining: limit,
        }
    }

    /// Refus explicite si épuisé.
    pub fn check(&self) -> Result<()> {
        if self.remaining == 0 {
            return Err(HakilleError::CapabilityDenied(
                "quota épuisé (remaining == 0)".to_string(),
            ));
        }
        Ok(())
    }

    /// Décrémente d'une unité ou refuse si épuisé.
    pub fn try_consume(&mut self) -> Result<()> {
        self.check()?;
        self.remaining -= 1;
        Ok(())
    }
}

/// Contexte d'autorisation central threadé dans tout le pipeline
/// (orchestrateur → planner → system2 → providers → substrates).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AuthorizationContext {
    /// Identité du demandeur (ex. `user:<uuid>`, `hakille-daemon`).
    pub subject: String,
    /// Audience revendiquée (service destinataire, ex. `hakille-kernel`).
    pub audience: String,
    /// Borne d'expiration de la *requête* (distincte de l'expiry du token).
    pub expires_at: Timestamp,
    /// Scope demandé.
    pub scope: CapabilityScope,
    /// Action demandée.
    pub action: CapabilityAction,
    /// Ressource normalisée (voir [`normalize_resource_for_scope`]).
    pub resource: String,
    /// Quota optionnel.
    pub quota: Option<Quota>,
    /// Nonce anti-rejeu (8..=128 chars, `[A-Za-z0-9_-]`).
    pub nonce: String,
    /// Chaîne de délégation (`["alice", "bob"]` = alice a délégué à bob).
    pub delegation_chain: Vec<String>,
}

impl AuthorizationContext {
    /// Construit un contexte en normalisant la ressource et en validant la
    /// structure (sujet/audience/nonce/délégation). L'expiration n'est PAS
    /// rejetée ici pour laisser les tests construire des contextes expirés ;
    /// [`Self::validate`] / [`Self::authorize`] la refusent (fail-closed).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subject: impl Into<String>,
        audience: impl Into<String>,
        expires_at: Timestamp,
        scope: CapabilityScope,
        action: CapabilityAction,
        resource_raw: &str,
        quota: Option<Quota>,
        nonce: impl Into<String>,
        delegation_chain: Vec<String>,
    ) -> Result<Self> {
        let subject = subject.into();
        let audience = audience.into();
        let nonce = nonce.into();
        if subject.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "subject (identité) vide".to_string(),
            ));
        }
        if subject.len() > 256 {
            return Err(HakilleError::CapabilityDenied(
                "subject trop long (>256)".to_string(),
            ));
        }
        if audience.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied("audience vide".to_string()));
        }
        if audience.len() > 256 {
            return Err(HakilleError::CapabilityDenied(
                "audience trop longue (>256)".to_string(),
            ));
        }
        validate_nonce(&nonce)?;
        validate_delegation(&delegation_chain)?;
        if let Some(ref q) = quota
            && q.remaining > q.limit
        {
            return Err(HakilleError::CapabilityDenied(
                "quota incohérent (remaining > limit)".to_string(),
            ));
        }
        let resource = normalize_resource_for_scope(resource_raw, &scope)?;
        Ok(Self {
            subject,
            audience,
            expires_at,
            scope,
            action,
            resource,
            quota,
            nonce,
            delegation_chain,
        })
    }

    /// `true` si la borne d'expiration de la requête est dépassée.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.expires_at.0.timestamp()
    }

    /// Validation complète fail-closed : ressource (re-normalisée en défense
    /// profonde), expiry, nonce, délégation, quota, sujet/audience non vides.
    pub fn validate(&self) -> Result<()> {
        if self.subject.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "subject (identité) vide".to_string(),
            ));
        }
        if self.audience.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied("audience vide".to_string()));
        }
        validate_nonce(&self.nonce)?;
        validate_delegation(&self.delegation_chain)?;
        // Re-normalise : toute divergence = tentative de contournement.
        let renormalized = normalize_resource_for_scope(&self.resource, &self.scope)?;
        if renormalized != self.resource {
            return Err(HakilleError::CapabilityDenied(format!(
                "ressource non canonique (attendu '{}', reçu '{}')",
                renormalized, self.resource
            )));
        }
        if self.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }
        if let Some(ref q) = self.quota {
            q.check()?;
        }
        Ok(())
    }

    /// Refuse tout mismatch d'audience (`expected` = identité du service qui
    /// applique la décision, ex. `hakille-kernel`).
    pub fn check_audience(&self, expected: &str) -> Result<()> {
        if self.audience != expected {
            return Err(HakilleError::CapabilityDenied(format!(
                "audience mismatch: attendu '{}', reçu '{}'",
                expected, self.audience
            )));
        }
        Ok(())
    }

    /// Le couple scope/action du contexte couvre-t-il l'exigence ?
    /// `FullSystem` couvre tous les scopes, `Admin` toutes les actions.
    pub fn covers(
        &self,
        required_scope: &CapabilityScope,
        required_action: &CapabilityAction,
    ) -> bool {
        let scope_ok = self.scope == *required_scope || self.scope == CapabilityScope::FullSystem;
        let action_ok = self.action == *required_action || self.action == CapabilityAction::Admin;
        scope_ok && action_ok
    }

    /// Décrémente le quota (refus explicite si absent-épuisé : `None` = pas
    /// de quota = OK, `Some(0)` = refus).
    pub fn try_consume_quota(&mut self) -> Result<()> {
        if let Some(ref mut q) = self.quota {
            q.try_consume()?;
        }
        Ok(())
    }

    /// Autorisation complète : validation du contexte + audience + expiry du
    /// token + vérification Biscuit épinglée + quota.
    pub fn authorize(
        &self,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
        expected_audience: &str,
    ) -> Result<()> {
        self.validate()?;
        self.check_audience(expected_audience)?;
        if token.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }
        if !self.covers(&self.scope, &self.action) {
            // Garde interne (infaillible en pratique) : le contexte doit se
            // couvrir lui-même, sinon construction corrompue.
            return Err(HakilleError::CapabilityDenied(
                "contexte incohérent (auto-couverture impossible)".to_string(),
            ));
        }
        authority
            .verify_pinned(token, self.scope.clone(), self.action.clone())
            .map_err(|e| match e {
                HakilleError::CapabilityExpired => HakilleError::CapabilityExpired,
                HakilleError::CapabilityDenied(msg) => HakilleError::CapabilityDenied(format!(
                    "scope insuffisant pour {:?}/{:?} sur '{}': {}",
                    self.scope, self.action, self.resource, msg
                )),
                other => other,
            })?;
        if let Some(ref q) = self.quota {
            q.check()?;
        }
        Ok(())
    }

    /// Variante substrates : vérification avec la seule clé publique racine
    /// (sans clé privée d'autorité).
    pub fn authorize_with_root_key(
        &self,
        token: &CapabilityToken,
        root_public_key: &biscuit_auth::PublicKey,
        expected_audience: &str,
    ) -> Result<()> {
        self.validate()?;
        self.check_audience(expected_audience)?;
        if token.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }
        verify_capability_with_root_key(
            token,
            root_public_key,
            self.scope.clone(),
            self.action.clone(),
        )
        .map_err(|e| match e {
            HakilleError::CapabilityExpired => HakilleError::CapabilityExpired,
            HakilleError::CapabilityDenied(msg) => HakilleError::CapabilityDenied(format!(
                "scope insuffisant pour {:?}/{:?} sur '{}': {}",
                self.scope, self.action, self.resource, msg
            )),
            other => other,
        })?;
        if let Some(ref q) = self.quota {
            q.check()?;
        }
        Ok(())
    }
}

/// Normalise et valide une ressource pour un scope donné (fail-closed).
///
/// - Rejette : vide, NUL, `\`, `..` (segment), longueur > [`MAX_RESOURCE_LEN`].
/// - [`CapabilityScope::Network`] : endpoints/URLs (`https://…`, noms de
///   providers) — `://` autorisé, `..` toujours refusé.
/// - Autres scopes : `://` interdit sauf préfixe `vfs://` (alias `/vfs/`) ;
///   `//` interdit ; `:` interdit en relatif (drive/scheme) ; chemin absolu
///   (`/…`) autorisé UNIQUEMENT sous [`ALLOWED_ABSOLUTE_PREFIXES`], sinon
///   refus `chemin absolu hors-VFS` ; segments `.` repliés, forme canonique
///   retournée.
pub fn normalize_resource_for_scope(raw: &str, scope: &CapabilityScope) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(HakilleError::CapabilityDenied("ressource vide".to_string()));
    }
    if trimmed.len() > MAX_RESOURCE_LEN {
        return Err(HakilleError::CapabilityDenied(format!(
            "ressource trop longue (>{})",
            MAX_RESOURCE_LEN
        )));
    }
    if trimmed.contains('\0') {
        return Err(HakilleError::CapabilityDenied(
            "NUL byte dans la ressource".to_string(),
        ));
    }
    if trimmed.contains('\\') {
        return Err(HakilleError::CapabilityDenied(
            "séparateur windows (\\) interdit dans la ressource".to_string(),
        ));
    }
    // Traversal `..` en segment (split `/`), valable pour TOUS les scopes
    // y compris Network (`https://h/a/../b` = refus).
    for seg in trimmed.split('/') {
        if seg == ".." {
            return Err(HakilleError::CapabilityDenied(
                "parent traversal (..) rejeté".to_string(),
            ));
        }
    }

    if *scope == CapabilityScope::Network {
        // Endpoints : on accepte `://`, noms simples, chemins URL.
        // Nettoyage minimal : trim déjà fait ; rejette les espaces internes.
        if trimmed.chars().any(|c| c.is_whitespace()) {
            return Err(HakilleError::CapabilityDenied(
                "espace interdit dans la ressource réseau".to_string(),
            ));
        }
        return Ok(trimmed.to_string());
    }

    // --- Scopes non-réseau : VFS strict ---
    let mut path = trimmed.to_string();
    if path.contains("://") {
        if let Some(rest) = path.strip_prefix("vfs://") {
            if rest.is_empty() {
                return Err(HakilleError::CapabilityDenied(
                    "ressource vfs:// vide".to_string(),
                ));
            }
            path = format!("/vfs/{}", rest.trim_start_matches('/'));
        } else {
            return Err(HakilleError::CapabilityDenied(
                "scheme (://) interdit hors scope Network".to_string(),
            ));
        }
    }
    if path.contains("//") {
        return Err(HakilleError::CapabilityDenied(
            "segment vide (//) interdit".to_string(),
        ));
    }
    if path.starts_with('/') {
        let allowed = ALLOWED_ABSOLUTE_PREFIXES
            .iter()
            .any(|p| path.starts_with(p));
        if !allowed {
            return Err(HakilleError::CapabilityDenied(format!(
                "chemin absolu hors-VFS refusé ('{}' ; autorisés : {:?})",
                path, ALLOWED_ABSOLUTE_PREFIXES
            )));
        }
        let canonical = canonicalize_vfs_path(&path)?;
        return Ok(canonical);
    }
    // Relatif : interdit `:` (drive `C:` / scheme déguisé).
    if path.contains(':') {
        return Err(HakilleError::CapabilityDenied(
            "':' interdit en chemin relatif (drive/scheme)".to_string(),
        ));
    }
    let canonical = canonicalize_vfs_path(&path)?;
    if canonical.is_empty() {
        return Err(HakilleError::CapabilityDenied(
            "ressource vide après normalisation".to_string(),
        ));
    }
    Ok(canonical)
}

/// Replie les segments `.`, rejette `..` (déjà filtré en amont, ceinture +
/// bretelles) et les segments vides ; conserve le caractère absolu/relatif.
fn canonicalize_vfs_path(path: &str) -> Result<String> {
    let is_absolute = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return Err(HakilleError::CapabilityDenied(
                "parent traversal (..) rejeté".to_string(),
            ));
        }
        if seg.contains('\0') || seg.contains('\\') {
            return Err(HakilleError::CapabilityDenied(
                "segment de chemin invalide".to_string(),
            ));
        }
        out.push(seg);
    }
    if out.is_empty() {
        return Err(HakilleError::CapabilityDenied(
            "chemin vide après normalisation".to_string(),
        ));
    }
    let joined = out.join("/");
    if is_absolute {
        Ok(format!("/{}", joined))
    } else {
        Ok(joined)
    }
}

/// Nonce : 8..=128 chars, alphabet `[A-Za-z0-9_-]`.
pub fn validate_nonce(nonce: &str) -> Result<()> {
    if nonce.len() < 8 || nonce.len() > 128 {
        return Err(HakilleError::CapabilityDenied(
            "nonce invalide (8..=128 caractères requis)".to_string(),
        ));
    }
    if !nonce
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(HakilleError::CapabilityDenied(
            "nonce invalide (alphabet [A-Za-z0-9_-])".to_string(),
        ));
    }
    Ok(())
}

/// Délégation : profondeur ≤ [`MAX_DELEGATION_DEPTH`], maillons non vides
/// (≤256 chars), sans doublon (anti-cycle simple).
pub fn validate_delegation(chain: &[String]) -> Result<()> {
    if chain.len() > MAX_DELEGATION_DEPTH {
        return Err(HakilleError::CapabilityDenied(format!(
            "chaîne de délégation trop profonde ({} > {})",
            chain.len(),
            MAX_DELEGATION_DEPTH
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for link in chain {
        let t = link.trim();
        if t.is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "maillon de délégation vide".to_string(),
            ));
        }
        if t.len() > 256 {
            return Err(HakilleError::CapabilityDenied(
                "maillon de délégation trop long (>256)".to_string(),
            ));
        }
        if !seen.insert(t.to_string()) {
            return Err(HakilleError::CapabilityDenied(
                "cycle de délégation (doublon)".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn future() -> Timestamp {
        Timestamp(chrono::Utc::now() + Duration::hours(1))
    }

    fn past() -> Timestamp {
        Timestamp(chrono::Utc::now() - Duration::hours(1))
    }

    fn ctx_for(resource: &str, scope: CapabilityScope) -> AuthorizationContext {
        AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            future(),
            scope,
            CapabilityAction::Read,
            resource,
            None,
            "nonce-1234_ABCD",
            vec![],
        )
        .expect("contexte valide")
    }

    #[test]
    fn traversal_dotdot_is_refused() {
        assert!(normalize_resource_for_scope("../evil", &CapabilityScope::Filesystem).is_err());
        assert!(
            normalize_resource_for_scope("a/../../evil", &CapabilityScope::Filesystem).is_err()
        );
        assert!(
            normalize_resource_for_scope("/vfs/a/../evil", &CapabilityScope::Filesystem).is_err()
        );
        // Via constructeur : le traversal ne construit jamais un contexte.
        assert!(
            AuthorizationContext::new(
                "u",
                "hakille-kernel",
                future(),
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                "../evil",
                None,
                "nonce-1234_ABCD",
                vec![],
            )
            .is_err()
        );
    }

    #[test]
    fn absolute_path_outside_vfs_is_refused() {
        for p in ["/etc/passwd", "/evil", "/", "/var/log/x", "C:/evil"] {
            assert!(
                normalize_resource_for_scope(p, &CapabilityScope::Filesystem).is_err(),
                "aurait dû refuser {p}"
            );
        }
        for p in ["/vfs/notes/todo.md", "/tmp/hakille-dryrun"] {
            assert!(
                normalize_resource_for_scope(p, &CapabilityScope::Filesystem).is_ok(),
                "aurait dû accepter {p}"
            );
        }
    }

    #[test]
    fn network_scope_accepts_endpoints_but_refuses_traversal() {
        assert!(
            normalize_resource_for_scope(
                "https://api.openai.com/v1/chat",
                &CapabilityScope::Network
            )
            .is_ok()
        );
        assert!(normalize_resource_for_scope("openai", &CapabilityScope::Network).is_ok());
        assert!(
            normalize_resource_for_scope("https://h/a/../b", &CapabilityScope::Network).is_err()
        );
    }

    #[test]
    fn expired_context_is_refused() {
        let ctx = AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            past(),
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            "notes/todo.md",
            None,
            "nonce-1234_ABCD",
            vec![],
        )
        .expect("construction OK même expirée");
        assert!(ctx.is_expired());
        assert!(matches!(
            ctx.validate(),
            Err(HakilleError::CapabilityExpired)
        ));
    }

    #[test]
    fn audience_mismatch_is_refused() {
        let ctx = ctx_for("notes/todo.md", CapabilityScope::Filesystem);
        assert!(ctx.check_audience("hakille-kernel").is_ok());
        let err = ctx.check_audience("hakille-daemon").unwrap_err();
        assert!(err.to_string().contains("audience mismatch"));
    }

    #[test]
    fn quota_exhausted_is_refused_and_consumable() {
        let mut ctx = AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            future(),
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            "notes/todo.md",
            Some(Quota::new(1)),
            "nonce-1234_ABCD",
            vec![],
        )
        .unwrap();
        assert!(ctx.validate().is_ok());
        ctx.try_consume_quota().unwrap();
        assert!(ctx.validate().is_err());
        assert!(ctx.try_consume_quota().is_err());
    }

    #[test]
    fn bad_nonce_and_deep_delegation_are_refused() {
        assert!(validate_nonce("short").is_err());
        assert!(validate_nonce("ok-nonce_1234").is_ok());
        assert!(validate_delegation(&vec!["a".to_string(); MAX_DELEGATION_DEPTH + 1]).is_err());
        assert!(validate_delegation(&["a".to_string(), "a".to_string()]).is_err());
    }

    #[test]
    fn covers_fullsystem_and_admin() {
        let ctx = AuthorizationContext::new(
            "root",
            "hakille-kernel",
            future(),
            CapabilityScope::FullSystem,
            CapabilityAction::Admin,
            "notes/todo.md",
            None,
            "nonce-1234_ABCD",
            vec![],
        )
        .unwrap();
        assert!(ctx.covers(&CapabilityScope::Wallet, &CapabilityAction::Execute));
        let narrow = ctx_for("notes/todo.md", CapabilityScope::Filesystem);
        assert!(!narrow.covers(&CapabilityScope::Network, &CapabilityAction::Read));
        assert!(narrow.covers(&CapabilityScope::Filesystem, &CapabilityAction::Read));
    }

    #[test]
    fn authorize_refuses_expired_token_and_wrong_scope() {
        let authority = CapabilityAuthority::new();
        let ctx = AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            future(),
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            "notes/todo.md",
            None,
            "nonce-1234_ABCD",
            vec![],
        )
        .unwrap();
        // Token Filesystem/Read valide : autorisé.
        let ok_token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                future(),
            )
            .unwrap();
        assert!(
            ctx.authorize(&ok_token, &authority, "hakille-kernel")
                .is_ok()
        );
        // Hors-scope (Network) : refus scope insuffisant.
        let net_token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Read],
                future(),
            )
            .unwrap();
        let err = ctx
            .authorize(&net_token, &authority, "hakille-kernel")
            .unwrap_err();
        assert!(err.to_string().contains("scope insuffisant"));
        // Expiré : refus CapabilityExpired.
        let expired_token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                past(),
            )
            .unwrap();
        assert!(matches!(
            ctx.authorize(&expired_token, &authority, "hakille-kernel"),
            Err(HakilleError::CapabilityExpired)
        ));
        // Audience mismatch : refus explicite.
        let err = ctx
            .authorize(&ok_token, &authority, "hakille-daemon")
            .unwrap_err();
        assert!(err.to_string().contains("audience mismatch"));
    }
}
