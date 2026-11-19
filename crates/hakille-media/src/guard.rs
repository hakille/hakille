//! Garde constitutionnel et vérification Biscuit v5 pour la génération média.
//!
//! Applique de manière fail-closed :
//! - **Règle 1** : rejet de toute requête issue d'un canal [`TrustLevel::Untrusted`]
//!   (page web, contenu non fiable) afin d'empêcher les injections indirectes ;
//! - **Règle 2 / 4** : vérification épinglée du jeton Biscuit v5 (`MediaGeneration`
//!   et `Execute`, avec liaison de ressource optionnelle + `Network` obligatoire
//!   pour toute sortie [`MediaDataExit::Cloud`]) ;
//! - **Constitution v1** : filtrage via [`Constitution::check_action`] ;
//! - **Règle 7** : restrictions profil mineur (cloud interdit, plafonds réduits) ;
//! - **Règle 8** : interdiction d'envoyer un fichier de référence local (I2I,
//!   inpaint, audio-reactive) vers un fournisseur cloud.

use crate::error::{MediaError, MediaResult};
use crate::policy::MediaPolicy;
use crate::types::{ImageGenMode, MediaDataExit, MediaRequest, VideoGenMode};
use hakille_constitution::Constitution;
use hakille_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, TrustLevel,
};

/// Contexte d'autorisation fourni par l'appelant (démon, kernel ou tests).
#[derive(Debug, Clone)]
pub struct MediaAuthContext {
    pub trust_level: TrustLevel,
    pub minor_profile: bool,
    pub cloud_explicitly_approved: bool,
    pub expected_resource: Option<String>,
}

impl Default for MediaAuthContext {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::User,
            minor_profile: false,
            cloud_explicitly_approved: false,
            expected_resource: None,
        }
    }
}

/// Garde unifié combinant la Constitution Hakille, la [`MediaPolicy`] et Biscuit v5.
pub struct MediaConstitutionGuard {
    constitution: Constitution,
    policy: MediaPolicy,
}

impl MediaConstitutionGuard {
    pub fn new(policy: MediaPolicy) -> Self {
        Self {
            constitution: Constitution::default_v1(),
            policy,
        }
    }

    pub fn policy(&self) -> &MediaPolicy {
        &self.policy
    }

    /// Vérifie la requête avant toute inférence ou sortie réseau.
    ///
    /// Lorsque `authority` et `token` sont fournis, vérifie la capability
    /// Biscuit v5 de manière épinglée (avec liaison ressource si présente dans
    /// `ctx.expected_resource`, plus `CapabilityScope::Network` si `data_exit`
    /// est [`MediaDataExit::Cloud`]).
    pub fn authorize(
        &self,
        request: &MediaRequest,
        data_exit: &MediaDataExit,
        ctx: &MediaAuthContext,
        authority: Option<&CapabilityAuthority>,
        token: Option<&CapabilityToken>,
    ) -> MediaResult<()> {
        // Règle 1 — Aucune donnée externe non fiable ne peut déclencher une génération.
        if ctx.trust_level == TrustLevel::Untrusted {
            return Err(MediaError::ConstitutionBlocked(
                "Règle 1 : une source Untrusted ne peut pas déclencher de génération média".into(),
            ));
        }

        // Politique effective (profil mineur applique les plafonds renforcés).
        let effective_policy = if ctx.minor_profile || self.policy.minor_profile {
            MediaPolicy::for_minor()
        } else {
            self.policy.clone()
        };
        effective_policy.validate_request(request)?;

        // Constitution v1 : détection de directives interdites dans le prompt.
        let prompt = request.prompt();
        if let Err(err) = self.constitution.check_action(prompt) {
            return Err(MediaError::ConstitutionBlocked(err.to_string()));
        }
        if let MediaRequest::Image(img) = request
            && let Some(neg) = &img.negative_prompt
            && let Err(err) = self.constitution.check_action(neg)
        {
            return Err(MediaError::ConstitutionBlocked(err.to_string()));
        }

        // Règles 7 & 8 pour toute sortie Cloud.
        if data_exit.is_cloud() {
            if ctx.minor_profile || effective_policy.minor_profile {
                return Err(MediaError::ConstitutionBlocked(
                    "Règle 7 : sortie cloud interdite pour un profil mineur".into(),
                ));
            }
            if !ctx.cloud_explicitly_approved && !effective_policy.allow_cloud_by_default {
                return Err(MediaError::CapabilityDenied(
                    "une génération cloud exige une approbation explicite (ActionCard) et un jeton Network".into(),
                ));
            }
            if has_local_reference_media(request) {
                return Err(MediaError::ConstitutionBlocked(
                    "Règle 8 : envoi de média local de référence (I2I/Inpaint/Audio) vers le cloud interdit".into(),
                ));
            }
        }

        // Vérification Biscuit v5 épinglée si une autorité est branchée.
        if let Some(auth) = authority {
            let tok = token.ok_or_else(|| {
                MediaError::CapabilityDenied(
                    "jeton Biscuit MediaGeneration manquant pour cette requête".into(),
                )
            })?;

            if let Some(res) = ctx.expected_resource.as_deref() {
                auth.verify_pinned_with_resource(
                    tok,
                    CapabilityScope::MediaGeneration,
                    CapabilityAction::Execute,
                    res,
                )?;
            } else {
                auth.verify_pinned(
                    tok,
                    CapabilityScope::MediaGeneration,
                    CapabilityAction::Execute,
                )?;
            }

            if data_exit.is_cloud() {
                auth.verify_pinned(tok, CapabilityScope::Network, CapabilityAction::Execute)?;
            }
        }

        Ok(())
    }
}

/// Détecte si la requête référence des fichiers médias locaux sensibles (I2I,
/// masque, image ou audio local) qui ne doivent jamais quitter la machine.
fn has_local_reference_media(request: &MediaRequest) -> bool {
    match request {
        MediaRequest::Image(r) => matches!(
            r.mode,
            ImageGenMode::ImageToImage { .. } | ImageGenMode::Inpaint { .. }
        ),
        MediaRequest::Video(r) => matches!(
            r.mode,
            VideoGenMode::ImageToVideo { .. } | VideoGenMode::AudioReactive { .. }
        ),
        MediaRequest::Audio(_) => false,
    }
}
