//! Registre ferme de providers d'inference + pastille de sortie des donnees.
//!
//! # Contrat honnete
//!
//! - `ProviderId` est **ferme** : `LocalGguf`, `LocalOllama`,
//!   `Cloud(OpenAI | Anthropic | Custom { endpoint })`. Aucun provider
//!   reseau n'est ajoute implicitement.
//! - `ProviderRegistry::default_local_only()` ne contient QUE les locaux.
//!   La selection est **persistante en memoire uniquement** (pas de FS :
//!   le choix meurt avec le processus ; TODO : persistance consentie si la
//!   spec l'exige un jour, avec capability dediee).
//! - `data_exit(weight_ko)` dit ou vont les donnees : `Local` (rien ne sort)
//!   ou `Cloud { provider }` (+ poids estime `weight_ko` en Ko, fourni par
//!   l'appelant — le registre ne mesure rien lui-meme).
//! - `System2Engine::reason()` **refuse** tout provider `Cloud` sans
//!   capability `Network` explicite (voir `system2.rs`, garde par defaut +
//!   `reason_with_network_capability`). Threading effectif :
//!   `CognitiveOrchestrator::process_user_turn_with_authorization` fait
//!   remonter `(&AuthorizationContext, &CapabilityToken, &CapabilityAuthority)`
//!   avec consentement UX par requete ; preferer l'attenuation par requete
//!   (`authority.attenuate` scope Network + Execute, expiry courte) au lieu
//!   d'un token racine longue duree.
//! - Point d'enforcement : [`require_network_capability`] et
//!   [`require_network_capability_with_context`] (cap Network/Execute valide
//!   exigee, sinon `Err` explicite — jamais de passage silencieux).

use hakille_core::{
    AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken,
    HakilleError, Result, Timestamp,
};
use serde::{Deserialize, Serialize};

/// Durée de vie du token réseau court (5 minutes, initiative bornée).
pub const SHORT_NETWORK_TOKEN_SECS: i64 = 5 * 60;

/// Forge un token réseau court (Network+Execute, expiry 5 min) au lieu d'un
/// root longue durée.
///
/// - `parent = Some` ⇒ atténuation via `authority.attenuate` (scope ⊆ parent,
///   expiry min(parent, now+5min)) — jamais d'expansion ;
/// - `parent = None` ⇒ racine courte (Network+Execute, now+5min) — périmètre
///   minimal, durée minimale, pas de root longue durée.
///
/// Échec (parent incompatible/expiré) ⇒ `Err` explicite, jamais de repli
/// silencieux vers un token large.
pub fn mint_short_network_token(
    authority: &CapabilityAuthority,
    parent: Option<&CapabilityToken>,
) -> Result<CapabilityToken> {
    let expiry =
        Timestamp(chrono::Utc::now() + chrono::Duration::seconds(SHORT_NETWORK_TOKEN_SECS));
    match parent {
        Some(p) => authority.attenuate(
            p,
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Execute],
            expiry,
        ),
        None => authority.create_root_token(
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Execute],
            expiry,
        ),
    }
}

/// Provider cloud (sous-ensemble ferme de `ProviderId::Cloud`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CloudProvider {
    OpenAI,
    Anthropic,
    Custom { endpoint: String },
}

/// Identifiant ferme de provider d'inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    /// GGUF local (moteur embarque, defaut boot).
    LocalGguf,
    /// Ollama local (daemon local, jamais de cloud).
    LocalOllama,
    /// Inference distante : exige une capability `Network` explicite.
    Cloud(CloudProvider),
}

impl ProviderId {
    /// `true` si aucune donnee ne quitte la machine avec ce provider.
    pub fn is_local(&self) -> bool {
        matches!(self, ProviderId::LocalGguf | ProviderId::LocalOllama)
    }

    /// `true` si les prompts transitent par un reseau tiers.
    pub fn is_cloud(&self) -> bool {
        !self.is_local()
    }

    /// Nom stable (logs, pastilles, flag `--provider`).
    pub fn display_name(&self) -> String {
        match self {
            ProviderId::LocalGguf => "local".to_string(),
            ProviderId::LocalOllama => "ollama".to_string(),
            ProviderId::Cloud(CloudProvider::OpenAI) => "openai".to_string(),
            ProviderId::Cloud(CloudProvider::Anthropic) => "anthropic".to_string(),
            ProviderId::Cloud(CloudProvider::Custom { endpoint }) => {
                format!("custom:{endpoint}")
            }
        }
    }

    /// Parse le flag daemon `--provider <local|ollama|openai|anthropic|custom:URL>`.
    /// Valeur inconnue = erreur explicite (jamais de repli silencieux local).
    pub fn parse_cli(raw: &str) -> Result<ProviderId> {
        let s = raw.trim();
        match s {
            "local" => Ok(ProviderId::LocalGguf),
            "ollama" => Ok(ProviderId::LocalOllama),
            "openai" => Ok(ProviderId::Cloud(CloudProvider::OpenAI)),
            "anthropic" => Ok(ProviderId::Cloud(CloudProvider::Anthropic)),
            _ if s.starts_with("custom:") => {
                let endpoint = s.trim_start_matches("custom:").trim().to_string();
                if endpoint.is_empty() {
                    Err(HakilleError::Internal(format!(
                        "unknown provider '{raw}' (expected local|ollama|openai|anthropic|custom:URL)"
                    )))
                } else {
                    Ok(ProviderId::Cloud(CloudProvider::Custom { endpoint }))
                }
            }
            _ => Err(HakilleError::Internal(format!(
                "unknown provider '{raw}' (expected local|ollama|openai|anthropic|custom:URL)"
            ))),
        }
    }
}

/// Ou vont (estime) les donnees d'une requete : nulle part (`Local`) ou vers
/// un tiers (`Cloud`). `weight_ko_estimate` = Ko estimes fournis par l'appelant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataExit {
    Local {
        weight_ko_estimate: u64,
    },
    Cloud {
        provider: String,
        weight_ko_estimate: u64,
    },
}

impl DataExit {
    pub fn is_local(&self) -> bool {
        matches!(self, DataExit::Local { .. })
    }

    /// Pastille donnees pour le daemon (`:providers`, boot) : texte seul,
    /// sans emoji (rendu terminal incertain), explicite sur la destination.
    pub fn badge(&self) -> String {
        match self {
            DataExit::Local { .. } => "[donnees: LOCAL — rien ne quitte cet appareil]".to_string(),
            DataExit::Cloud {
                provider,
                weight_ko_estimate,
            } => format!("[donnees: CLOUD -> {provider} (~{weight_ko_estimate} Ko estimes)]"),
        }
    }
}

/// Registre des providers disponibles + selection courante (memoire seule).
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    available: Vec<ProviderId>,
    selected: ProviderId,
}

impl ProviderRegistry {
    /// Defaut boot : locaux seuls, `LocalGguf` selectionne.
    pub fn default_local_only() -> Self {
        Self {
            available: vec![ProviderId::LocalGguf, ProviderId::LocalOllama],
            selected: ProviderId::LocalGguf,
        }
    }

    /// Registre avec clouds publics pre-enregistres (locaux + OpenAI + Anthropic).
    /// `Custom` reste a ajouter via `register` (endpoint explicite requis).
    pub fn with_public_clouds() -> Self {
        Self {
            available: vec![
                ProviderId::LocalGguf,
                ProviderId::LocalOllama,
                ProviderId::Cloud(CloudProvider::OpenAI),
                ProviderId::Cloud(CloudProvider::Anthropic),
            ],
            selected: ProviderId::LocalGguf,
        }
    }

    /// Ajoute un provider au registre (idempotent). N'elargit jamais seule
    /// la selection courante.
    pub fn register(&mut self, provider: ProviderId) {
        if !self.available.contains(&provider) {
            self.available.push(provider);
        }
    }

    /// Selectionne un provider **deja enregistre**. Refus explicite sinon
    /// (pas de bascule silencieuse, surtout pas vers un cloud).
    pub fn select(&mut self, provider: ProviderId) -> Result<()> {
        if self.available.contains(&provider) {
            self.selected = provider;
            Ok(())
        } else {
            Err(HakilleError::Internal(format!(
                "provider '{}' is not registered (available: {})",
                provider.display_name(),
                self.available
                    .iter()
                    .map(|p| p.display_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    pub fn selected(&self) -> &ProviderId {
        &self.selected
    }

    pub fn available(&self) -> &[ProviderId] {
        &self.available
    }

    /// Sortie des donnees pour la selection courante. `weight_ko` = estimation
    /// fournie par l'appelant (ex. prompt+contexte en Ko) ; 0 = pastille seule.
    pub fn data_exit(&self, weight_ko: u64) -> DataExit {
        match &self.selected {
            local if local.is_local() => DataExit::Local {
                weight_ko_estimate: weight_ko,
            },
            cloud => DataExit::Cloud {
                provider: cloud.display_name(),
                weight_ko_estimate: weight_ko,
            },
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::default_local_only()
    }
}

/// Enforcement réseau : exige une capability `Network`/`Execute` valide.
/// - `None` → `Err` explicite (consentement manquant, jamais silencieux).
/// - Token expiré → `Err(CapabilityExpired)` explicite.
/// - Scope/action insuffisants ou signature non épinglée → `Err` explicite.
pub fn require_network_capability(
    authority: &CapabilityAuthority,
    network_capability: Option<&CapabilityToken>,
) -> Result<()> {
    match network_capability {
        None => Err(HakilleError::CapabilityDenied(
            "cloud provider requires explicit Network capability + consent (aucun token fourni)"
                .to_string(),
        )),
        Some(token) => {
            if token.is_expired() {
                return Err(HakilleError::CapabilityExpired);
            }
            authority
                .verify_pinned(token, CapabilityScope::Network, CapabilityAction::Execute)
                .map_err(|e| match e {
                    HakilleError::CapabilityExpired => HakilleError::CapabilityExpired,
                    HakilleError::CapabilityDenied(msg) => HakilleError::CapabilityDenied(format!(
                        "capability Network/Execute invalide ou insuffisante: {msg}"
                    )),
                    other => other,
                })
        }
    }
}

/// Variante avec [`AuthorizationContext`] : valide le contexte (ressource,
/// expiry, audience, quota, nonce, délégation), exige qu'il couvre
/// `Network`/`Execute`, puis vérifie le token épinglé.
pub fn require_network_capability_with_context(
    authority: &CapabilityAuthority,
    network_capability: Option<&CapabilityToken>,
    ctx: &AuthorizationContext,
    expected_audience: &str,
) -> Result<()> {
    ctx.validate().map_err(|e| {
        HakilleError::CapabilityDenied(format!("contexte d'autorisation invalide: {e}"))
    })?;
    ctx.check_audience(expected_audience)?;
    if ctx.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    if !ctx.covers(&CapabilityScope::Network, &CapabilityAction::Execute) {
        return Err(HakilleError::CapabilityDenied(format!(
            "contexte {:?}/{:?} ne couvre pas Network/Execute (ressource '{}')",
            ctx.scope, ctx.action, ctx.resource
        )));
    }
    require_network_capability(authority, network_capability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_is_local_only() {
        let reg = ProviderRegistry::default_local_only();
        assert_eq!(
            reg.available(),
            &[ProviderId::LocalGguf, ProviderId::LocalOllama]
        );
        assert_eq!(reg.selected(), &ProviderId::LocalGguf);
        assert!(!reg.selected().is_cloud());
        let exit = reg.data_exit(12);
        assert_eq!(
            exit,
            DataExit::Local {
                weight_ko_estimate: 12
            }
        );
        assert!(exit.is_local());
        assert!(exit.badge().contains("LOCAL"));
    }

    #[test]
    fn select_local_ollama_succeeds() {
        let mut reg = ProviderRegistry::default_local_only();
        reg.select(ProviderId::LocalOllama)
            .expect("ollama registered");
        assert_eq!(reg.selected(), &ProviderId::LocalOllama);
        assert!(reg.data_exit(0).is_local());
    }

    #[test]
    fn select_unregistered_cloud_is_refused() {
        let mut reg = ProviderRegistry::default_local_only();
        let err = reg
            .select(ProviderId::Cloud(CloudProvider::OpenAI))
            .expect_err("cloud must not be selectable when unregistered");
        let msg = err.to_string();
        assert!(
            msg.contains("openai") && msg.contains("not registered"),
            "{msg}"
        );
        // La selection n'a pas bouge (pas de bascule partielle).
        assert_eq!(reg.selected(), &ProviderId::LocalGguf);
    }

    #[test]
    fn cloud_data_exit_names_provider_and_weight() {
        let mut reg = ProviderRegistry::with_public_clouds();
        reg.select(ProviderId::Cloud(CloudProvider::Anthropic))
            .expect("anthropic registered");
        let exit = reg.data_exit(64);
        assert_eq!(
            exit,
            DataExit::Cloud {
                provider: "anthropic".to_string(),
                weight_ko_estimate: 64,
            }
        );
        assert!(!exit.is_local());
        let badge = exit.badge();
        assert!(
            badge.contains("CLOUD") && badge.contains("anthropic"),
            "{badge}"
        );
    }

    #[test]
    fn custom_endpoint_survives_roundtrip() {
        let mut reg = ProviderRegistry::default_local_only();
        let custom = ProviderId::Cloud(CloudProvider::Custom {
            endpoint: "https://inference.intranet:11400".to_string(),
        });
        reg.register(custom.clone());
        reg.select(custom).expect("custom registered");
        match reg.data_exit(3) {
            DataExit::Cloud {
                provider,
                weight_ko_estimate,
            } => {
                assert!(
                    provider.contains("https://inference.intranet:11400"),
                    "{provider}"
                );
                assert_eq!(weight_ko_estimate, 3);
            }
            other => panic!("expected Cloud exit, got {other:?}"),
        }
    }

    #[test]
    fn network_capability_none_is_explicitly_refused() {
        let authority = CapabilityAuthority::new();
        let err = require_network_capability(&authority, None).unwrap_err();
        assert!(err.to_string().contains("explicit Network capability"));
    }

    #[test]
    fn network_capability_expired_is_refused_as_expired() {
        let authority = CapabilityAuthority::new();
        let past = hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                past,
            )
            .unwrap();
        assert!(matches!(
            require_network_capability(&authority, Some(&token)),
            Err(hakille_core::HakilleError::CapabilityExpired)
        ));
    }

    #[test]
    fn network_capability_wrong_scope_is_refused() {
        let authority = CapabilityAuthority::new();
        let future = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                future,
            )
            .unwrap();
        let err = require_network_capability(&authority, Some(&token)).unwrap_err();
        assert!(err.to_string().contains("Network/Execute"));
    }

    #[test]
    fn network_capability_valid_passes() {
        let authority = CapabilityAuthority::new();
        let future = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                future,
            )
            .unwrap();
        require_network_capability(&authority, Some(&token)).expect("cap valide");
    }

    #[test]
    fn network_capability_with_context_enforces_audience_and_coverage() {
        let authority = CapabilityAuthority::new();
        let future = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                future.clone(),
            )
            .unwrap();
        let net_ctx = AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            future.clone(),
            CapabilityScope::Network,
            CapabilityAction::Execute,
            "https://api.openai.com/v1/chat",
            None,
            "nonce-ctx-net-01",
            vec![],
        )
        .unwrap();
        require_network_capability_with_context(
            &authority,
            Some(&token),
            &net_ctx,
            "hakille-kernel",
        )
        .expect("contexte réseau valide");
        // Audience mismatch.
        let err = require_network_capability_with_context(
            &authority,
            Some(&token),
            &net_ctx,
            "hakille-daemon",
        )
        .unwrap_err();
        assert!(err.to_string().contains("audience mismatch"));
        // Couverture insuffisante (Filesystem ne couvre pas Network).
        let fs_ctx = AuthorizationContext::new(
            "user:test",
            "hakille-kernel",
            future,
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            "notes/todo.md",
            None,
            "nonce-ctx-fs-02x",
            vec![],
        )
        .unwrap();
        let err = require_network_capability_with_context(
            &authority,
            Some(&token),
            &fs_ctx,
            "hakille-kernel",
        )
        .unwrap_err();
        assert!(err.to_string().contains("ne couvre pas Network"));
    }

    #[test]
    fn parse_cli_table() {
        assert_eq!(
            ProviderId::parse_cli("local").unwrap(),
            ProviderId::LocalGguf
        );
        assert_eq!(
            ProviderId::parse_cli("ollama").unwrap(),
            ProviderId::LocalOllama
        );
        assert_eq!(
            ProviderId::parse_cli("openai").unwrap(),
            ProviderId::Cloud(CloudProvider::OpenAI)
        );
        assert_eq!(
            ProviderId::parse_cli("anthropic").unwrap(),
            ProviderId::Cloud(CloudProvider::Anthropic)
        );
        assert_eq!(
            ProviderId::parse_cli("custom:https://x:11400").unwrap(),
            ProviderId::Cloud(CloudProvider::Custom {
                endpoint: "https://x:11400".to_string(),
            })
        );
        assert!(
            ProviderId::parse_cli("azure").is_err(),
            "unknown => error, never silent fallback"
        );
        assert!(
            ProviderId::parse_cli("custom:").is_err(),
            "empty endpoint => error"
        );
        assert!(ProviderId::parse_cli("").is_err());
    }
}
