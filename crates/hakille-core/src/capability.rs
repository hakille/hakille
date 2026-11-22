use crate::error::{HakilleError, Result};
use crate::types::{Timestamp, UserId};
use biscuit_auth::{Biscuit, KeyPair};
use serde::{Deserialize, Serialize};

/// Clé publique racine Biscuit épinglée (re-export pour `hakille-audit` qui
/// doit épingler sans dépendance directe `biscuit-auth`).
/// Les substrates / le journal vérifient les tokens contre CETTE clé,
/// jamais contre la clé embarquée dans le token (auto-signés rejetés).
pub use biscuit_auth::PublicKey as PinnedRootPublicKey;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityScope {
    Filesystem,
    Network,
    GuiInput,
    Wallet,
    Sensor,
    ModelInference,
    Audio,
    Camera,
    FullSystem,
    /// Données biométriques brutes ou dérivées directes (repères faciaux 478 pts, iris, pitch fondamental, embeddings).
    Biometrics,
    /// Géométrie spatiale 3D de l'utilisateur (Point3D, HeadPose6D, Squelette 17 points, GazeRay 3D).
    Spatial,
    /// Inférences cognitives et affectives de haut niveau (émotions, âge estimé, attention, fatigue, posture).
    CognitiveInference,
    /// Émission et gestion des notifications système et alertes utilisateur (SMNF).
    Notification,
    /// Synthèse multimodale (image, vidéo, audio) gouvernée par provenance signée.
    MediaGeneration,
}

impl CapabilityScope {
    /// Nom canonique Datalog Biscuit v5 (snake_case).
    pub fn datalog_name(&self) -> &'static str {
        match self {
            CapabilityScope::Filesystem => "filesystem",
            CapabilityScope::Network => "network",
            CapabilityScope::GuiInput => "guiinput",
            CapabilityScope::Wallet => "wallet",
            CapabilityScope::Sensor => "sensor",
            CapabilityScope::ModelInference => "modelinference",
            CapabilityScope::Audio => "audio",
            CapabilityScope::Camera => "camera",
            CapabilityScope::FullSystem => "fullsystem",
            CapabilityScope::Biometrics => "biometrics",
            CapabilityScope::Spatial => "spatial",
            CapabilityScope::CognitiveInference => "cognitive_inference",
            CapabilityScope::Notification => "notification",
            CapabilityScope::MediaGeneration => "media_generation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityAction {
    Read,
    Write,
    Execute,
    Delete,
    Admin,
}

#[derive(Debug, Clone)]
pub struct CapabilityToken {
    pub biscuit: Biscuit,
}

impl CapabilityToken {
    /// Sérialise le token en bytes (format Biscuit).
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        self.biscuit
            .to_vec()
            .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))
    }

    /// Désérialise un token en épinglant la clé publique racine attendue.
    /// Rejette les tokens auto-signés par un attaquant (clé différente).
    pub fn from_vec(bytes: Vec<u8>, root_public_key: &biscuit_auth::PublicKey) -> Result<Self> {
        let biscuit = Biscuit::from(bytes, *root_public_key)
            .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;
        Ok(Self { biscuit })
    }

    /// Borne d'expiration minimale déclarée dans tous les blocs
    /// (`check … time <= N`). `None` si aucun check trouvé (fail-closed).
    pub fn min_expiry_timestamp(&self) -> Option<i64> {
        min_expiry_in_token(self)
    }

    /// `true` si le token est expiré par rapport à l'horloge actuelle.
    pub fn is_expired(&self) -> bool {
        match self.min_expiry_timestamp() {
            Some(exp) => chrono::Utc::now().timestamp() > exp,
            None => true,
        }
    }
}

pub struct CapabilityAuthority {
    pub root_keypair: KeyPair,
}

impl Default for CapabilityAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityAuthority {
    pub fn new() -> Self {
        Self {
            root_keypair: KeyPair::new(),
        }
    }

    pub fn with_keypair(root_keypair: KeyPair) -> Self {
        Self { root_keypair }
    }

    /// Public key pinned by substrates to verify tokens.
    /// Substrates MUST pin this key — verifying a token against its own
    /// embedded key alone would accept attacker self-signed tokens.
    pub fn root_public_key(&self) -> biscuit_auth::PublicKey {
        self.root_keypair.public()
    }

    pub fn create_root_token(
        &self,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        expiry: Timestamp,
    ) -> Result<CapabilityToken> {
        self.create_root_token_with_binding(scopes, actions, expiry, None, None, None)
    }

    /// Émission avec liaison optionnelle F4 : `resource` (objet visé),
    /// `nonce` (anti-rejeu / jti : identifiant unique du token, à révoquer
    /// via `RevocationStore`), `aud` (audience). `None` = pas de fait
    /// (compatibilité ascendante). Les valeurs sont validées fail-closed
    /// (guillemets / backslash / retours refusés : anti-injection Datalog).
    pub fn create_root_token_with_binding(
        &self,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        expiry: Timestamp,
        resource: Option<&str>,
        nonce: Option<&str>,
        aud: Option<&str>,
    ) -> Result<CapabilityToken> {
        let mut builder = Biscuit::builder();
        for scope in &scopes {
            let s = scope.datalog_name();
            for action in &actions {
                let a = format!("{:?}", action).to_lowercase();
                let fact = format!("right(\"{}\", \"{}\")", s, a);
                builder
                    .add_fact(fact.as_str())
                    .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
            }
        }
        if let Some(r) = resource {
            validate_binding_value(r)?;
            builder
                .add_fact(format!("resource(\"{r}\")").as_str())
                .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        }
        if let Some(n) = nonce {
            validate_binding_value(n)?;
            builder
                .add_fact(format!("token_nonce(\"{n}\")").as_str())
                .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        }
        if let Some(a) = aud {
            validate_binding_value(a)?;
            builder
                .add_fact(format!("token_aud(\"{a}\")").as_str())
                .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        }
        let exp_ts = expiry.0.timestamp();
        let check = format!("check if time($time), $time <= {}", exp_ts);
        builder
            .add_check(check.as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let biscuit = builder
            .build(&self.root_keypair)
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        Ok(CapabilityToken { biscuit })
    }

    pub fn attenuate(
        &self,
        parent_token: &CapabilityToken,
        restricted_scopes: Vec<CapabilityScope>,
        restricted_actions: Vec<CapabilityAction>,
        new_expiry: Timestamp,
    ) -> Result<CapabilityToken> {
        // Monotonie : l'enfant doit être ⊆ parent (scopes × actions).
        // Un ensemble vide = deny-all (le plus restrictif) : toujours valide.
        if !restricted_scopes.is_empty() || !restricted_actions.is_empty() {
            let parent_rights = effective_rights(parent_token)?;
            let parent_scopes: std::collections::HashSet<String> =
                parent_rights.iter().map(|(s, _)| s.clone()).collect();
            let parent_actions: std::collections::HashSet<String> =
                parent_rights.iter().map(|(_, a)| a.clone()).collect();
            for scope in &restricted_scopes {
                let s = scope.datalog_name();
                if !parent_scopes.contains(s)
                    && !(s == "cognitive_inference" && parent_scopes.contains("cognitiveinference"))
                    && !parent_scopes.contains("fullsystem")
                {
                    return Err(HakilleError::AttenuationViolation);
                }
            }
            for action in &restricted_actions {
                let a = format!("{:?}", action).to_lowercase();
                if !parent_actions.contains(&a) && !parent_actions.contains("admin") {
                    return Err(HakilleError::AttenuationViolation);
                }
            }
        }
        // Monotonie temporelle : l'enfant ne peut pas vivre plus longtemps
        // que le parent (fail-closed si bornes absentes).
        let new_exp_ts = new_expiry.0.timestamp();
        match min_expiry_in_token(parent_token) {
            Some(parent_exp) if new_exp_ts <= parent_exp => {}
            _ => return Err(HakilleError::AttenuationViolation),
        }

        let mut block = biscuit_auth::builder::BlockBuilder::new();
        if restricted_scopes.is_empty() || restricted_actions.is_empty() {
            block
                .add_check("check if false")
                .map_err(|_e| HakilleError::AttenuationViolation)?;
        } else {
            for scope in &restricted_scopes {
                let s = scope.datalog_name();
                let fact = format!("allowed_scope(\"{}\")", s);
                block
                    .add_fact(fact.as_str())
                    .map_err(|_e| HakilleError::AttenuationViolation)?;
            }
            for action in &restricted_actions {
                let a = format!("{:?}", action).to_lowercase();
                let fact = format!("allowed_action(\"{}\")", a);
                block
                    .add_fact(fact.as_str())
                    .map_err(|_e| HakilleError::AttenuationViolation)?;
            }
            block
                .add_check("check if operation($s, $a), allowed_scope($s), allowed_action($a)")
                .map_err(|_e| HakilleError::AttenuationViolation)?;
        }
        let exp_ts = new_expiry.0.timestamp();
        let time_check = format!("check if time($time), $time <= {}", exp_ts);
        block
            .add_check(time_check.as_str())
            .map_err(|_e| HakilleError::AttenuationViolation)?;

        let attenuated = parent_token
            .biscuit
            .append(block)
            .map_err(|_e| HakilleError::AttenuationViolation)?;
        Ok(CapabilityToken {
            biscuit: attenuated,
        })
    }

    pub fn verify(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
    ) -> Result<()> {
        self.verify_pinned(token, required_scope, required_action)
    }

    /// Verify a token against THIS authority's pinned root public key.
    /// Serializes + re-deserializes via `Biscuit::from` so the signature is
    /// checked against the expected root key, not the key embedded in the token.
    pub fn verify_pinned(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
    ) -> Result<()> {
        // Pin the root key: re-parse the token with our public key.
        // This rejects attacker self-signed tokens (different root key).
        // Fast-path expiration : distingue `Expired` de `Denied`.
        if token.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }
        let bytes = token
            .biscuit
            .to_vec()
            .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))?;
        let pinned = Biscuit::from(bytes, self.root_keypair.public())
            .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;
        let mut authorizer = pinned
            .authorizer()
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        let s = required_scope.datalog_name();
        let a = format!("{:?}", required_action).to_lowercase();
        let now_ts = chrono::Utc::now().timestamp();

        let time_fact = format!("time({})", now_ts);
        authorizer
            .add_fact(time_fact.as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let op_fact = format!("operation(\"{}\", \"{}\")", s, a);
        authorizer
            .add_fact(op_fact.as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        let check = format!("check if right(\"{}\", \"{}\")", s, a);
        authorizer
            .add_check(check.as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
        authorizer
            .allow()
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let limits = biscuit_auth::AuthorizerLimits {
            max_time: std::time::Duration::from_millis(500),
            ..Default::default()
        };
        authorizer.authorize_with_limits(limits).map_err(|e| {
            // Distingue l'expiration du refus : si l'horloge dépasse la borne
            // `time <= exp` du token, émettre `CapabilityExpired`.
            if token.is_expired() {
                HakilleError::CapabilityExpired
            } else {
                HakilleError::CapabilityDenied(e.to_string())
            }
        })?;
        Ok(())
    }

    /// Vérification épinglée + liaison ressource F4 (fail-closed).
    /// Le token DOIT contenir `resource("<expected>")` sinon refus, même si
    /// scope/action/expiration sont valides. `resource` vide refusée.
    /// L'ensemble (scope, action, resource, temps) est vérifié dans UN SEUL
    /// passage authorizer (pas de double `authorize` : anti-TOCTOU).
    pub fn verify_pinned_with_resource(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        required_resource: &str,
    ) -> Result<()> {
        verify_capability_with_root_key_and_resource(
            token,
            &self.root_keypair.public(),
            required_scope,
            required_action,
            required_resource,
        )
    }

    /// Alias d'enforcement F4 : `enforce_with_resource(scope, action, resource)`.
    pub fn enforce_with_resource(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        required_resource: &str,
    ) -> Result<()> {
        self.verify_pinned_with_resource(token, required_scope, required_action, required_resource)
    }

    /// Vérification épinglée + liaisons cryptographiques optionnelles (`resource`, `token_aud`, `token_nonce`)
    /// dans UN SEUL passage authorizer Biscuit (fail-closed, anti-TOCTOU).
    pub fn verify_pinned_with_bindings(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        required_resource: Option<&str>,
        required_audience: Option<&str>,
        required_nonce: Option<&str>,
    ) -> Result<()> {
        verify_capability_with_root_key_and_bindings(
            token,
            &self.root_keypair.public(),
            required_scope,
            required_action,
            required_resource,
            required_audience,
            required_nonce,
        )
    }

    /// Vérification épinglée + révocation par nonce/jti ET `revocation_identifiers()` Biscuit :
    /// épingle la signature AVANT toute lecture de fait, puis refuse si le nonce ou l'un
    /// des identifiants de révocation Biscuit (résistant à l'atténuation hors-ligne) est révoqué.
    pub fn verify_pinned_with_revocation(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        is_revoked: &dyn Fn(&str) -> bool,
    ) -> Result<()> {
        verify_capability_with_root_key_and_revocation(
            token,
            &self.root_keypair.public(),
            required_scope,
            required_action,
            is_revoked,
        )
    }

    /// Atténue un token parent pour restreindre STRICTEMENT l'accès aux
    /// inférences cognitives (`CognitiveInference`), interdisant toute fuite
    /// des données brutes (`Biometrics`, `Spatial`, `Camera`, `Audio`).
    pub fn attenuate_cognitive_inference(
        &self,
        parent_token: &CapabilityToken,
        allowed_resources: &[&str],
        new_expiry: Timestamp,
    ) -> Result<CapabilityToken> {
        attenuate_cognitive_inference_token(parent_token, allowed_resources, new_expiry)
    }

    /// Vérifie l'accès à une ressource cognitive visée contre la clé publique racine épinglée.
    pub fn verify_cognitive_inference(
        &self,
        token: &CapabilityToken,
        target_resource: &str,
    ) -> Result<()> {
        verify_cognitive_inference_with_root_key(token, &self.root_public_key(), target_resource)
    }
}

/// Verify a capability token given only the pinned root *public* key.
/// Intended for substrates / point-of-effect enforcement which must not
/// hold the private root key.
pub fn verify_capability_with_root_key(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
) -> Result<()> {
    // Fast-path expiration : distingue `Expired` de `Denied` sans attendre
    // l'échec de l'authorizer.
    if token.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    let bytes = token
        .biscuit
        .to_vec()
        .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))?;
    let pinned = Biscuit::from(bytes, *root_public_key)
        .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;
    let mut authorizer = pinned
        .authorizer()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let s = required_scope.datalog_name();
    let a = format!("{:?}", required_action).to_lowercase();
    let now_ts = chrono::Utc::now().timestamp();
    let time_fact = format!("time({now_ts})");
    authorizer
        .add_fact(time_fact.as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let op_fact = format!("operation(\"{s}\", \"{a}\")");
    authorizer
        .add_fact(op_fact.as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let check = format!("check if right(\"{s}\", \"{a}\")");
    authorizer
        .add_check(check.as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .allow()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let limits = biscuit_auth::AuthorizerLimits {
        max_time: std::time::Duration::from_millis(500),
        ..Default::default()
    };
    authorizer.authorize_with_limits(limits).map_err(|e| {
        if token.is_expired() {
            HakilleError::CapabilityExpired
        } else {
            HakilleError::CapabilityDenied(e.to_string())
        }
    })?;
    Ok(())
}

/// Vérification épinglée + liaison ressource (fonction libre pour les
/// substrates / points d'effet qui ne détiennent que la clé publique).
/// Fail-closed : `required_resource` vide ou token sans `resource`
/// correspondant => `CapabilityDenied`. Expiration distinguée (`Expired`).
/// Scope/action/resource/temps vérifiés dans UN SEUL passage authorizer.
pub fn verify_capability_with_root_key_and_resource(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
    required_resource: &str,
) -> Result<()> {
    if required_resource.is_empty() || required_resource.len() > 256 {
        return Err(HakilleError::CapabilityDenied(
            "required_resource vide ou trop longue (refus)".to_string(),
        ));
    }
    validate_binding_value(required_resource)?;
    if token.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    let bytes = token
        .biscuit
        .to_vec()
        .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))?;
    let pinned = Biscuit::from(bytes, *root_public_key)
        .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;
    let mut authorizer = pinned
        .authorizer()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let s = required_scope.datalog_name();
    let a = format!("{:?}", required_action).to_lowercase();
    let now_ts = chrono::Utc::now().timestamp();
    authorizer
        .add_fact(format!("time({now_ts})").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_fact(format!("operation(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_check(format!("check if right(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    // Liaison ressource : le token doit porter exactement ce fait.
    authorizer
        .add_check(format!("check if resource(\"{required_resource}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .allow()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let limits = biscuit_auth::AuthorizerLimits {
        max_time: std::time::Duration::from_millis(500),
        ..Default::default()
    };
    authorizer.authorize_with_limits(limits).map_err(|e| {
        if token.is_expired() {
            HakilleError::CapabilityExpired
        } else {
            HakilleError::CapabilityDenied(e.to_string())
        }
    })?;
    Ok(())
}

/// Vérification épinglée + liaisons cryptographiques optionnelles (`resource`,
/// `token_aud`, `token_nonce`) vérifiées atomiquement dans un seul passage
/// d'authorizer Biscuit contre le bloc d'autorité épinglé.
pub fn verify_capability_with_root_key_and_bindings(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
    required_resource: Option<&str>,
    required_audience: Option<&str>,
    required_nonce: Option<&str>,
) -> Result<()> {
    if let Some(res) = required_resource {
        validate_binding_value(res)?;
    }
    if let Some(aud) = required_audience {
        validate_binding_value(aud)?;
    }
    if let Some(nonce) = required_nonce {
        validate_binding_value(nonce)?;
    }
    if token.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    let bytes = token
        .biscuit
        .to_vec()
        .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))?;
    let pinned = Biscuit::from(bytes, *root_public_key)
        .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;
    let mut authorizer = pinned
        .authorizer()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let s = required_scope.datalog_name();
    let a = format!("{:?}", required_action).to_lowercase();
    let now_ts = chrono::Utc::now().timestamp();
    authorizer
        .add_fact(format!("time({now_ts})").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_fact(format!("operation(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_check(format!("check if right(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    if let Some(res) = required_resource {
        authorizer
            .add_check(format!("check if resource(\"{res}\")").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    }
    if let Some(aud) = required_audience {
        authorizer
            .add_check(format!("check if token_aud(\"{aud}\")").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    }
    if let Some(nonce) = required_nonce {
        authorizer
            .add_check(format!("check if token_nonce(\"{nonce}\")").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    }
    authorizer
        .allow()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let limits = biscuit_auth::AuthorizerLimits {
        max_time: std::time::Duration::from_millis(500),
        ..Default::default()
    };
    authorizer.authorize_with_limits(limits).map_err(|e| {
        if token.is_expired() {
            HakilleError::CapabilityExpired
        } else {
            HakilleError::CapabilityDenied(e.to_string())
        }
    })?;
    Ok(())
}

/// Retourne les identifiants cryptographiques de révocation Biscuit (un par
/// bloc) encodés en hexadécimal minuscule.
pub fn token_revocation_ids_hex(token: &CapabilityToken) -> Result<Vec<String>> {
    Ok(token
        .biscuit
        .revocation_identifiers()
        .into_iter()
        .map(|bytes| {
            let mut s = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                use std::fmt::Write;
                let _ = write!(&mut s, "{b:02x}");
            }
            s
        })
        .collect())
}

/// Alias d'enforcement F4 (fonction libre) : même garantie que
/// `verify_capability_with_root_key_and_resource`.
pub fn enforce_with_resource(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
    required_resource: &str,
) -> Result<()> {
    verify_capability_with_root_key_and_resource(
        token,
        root_public_key,
        required_scope,
        required_action,
        required_resource,
    )
}

/// Vérification épinglée avec clé publique racine + vérification de révocation
/// (identifiants natifs Biscuit ET `token_nonce` du bloc d'autorité 0).
/// La signature cryptographique contre `root_public_key` est vérifiée AVANT
/// toute lecture de faits ou de liste de révocation.
pub fn verify_capability_with_root_key_and_revocation(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
    is_revoked: &dyn Fn(&str) -> bool,
) -> Result<()> {
    verify_capability_with_root_key(token, root_public_key, required_scope, required_action)?;
    for rev_id in token_revocation_ids_hex(token)? {
        if is_revoked(&rev_id) {
            return Err(HakilleError::CapabilityDenied(format!(
                "token révoqué (revocation_id {rev_id})"
            )));
        }
    }
    let (_, nonce, _) = token_binding_facts(token);
    if let Some(n) = nonce
        && is_revoked(&n)
    {
        return Err(HakilleError::CapabilityDenied(format!(
            "token révoqué (nonce/jti {n})"
        )));
    }
    Ok(())
}

/// Alias d'enforcement avec révocation.
pub fn enforce_with_revocation(
    token: &CapabilityToken,
    root_public_key: &biscuit_auth::PublicKey,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
    is_revoked: &dyn Fn(&str) -> bool,
) -> Result<()> {
    verify_capability_with_root_key_and_revocation(
        token,
        root_public_key,
        required_scope,
        required_action,
        is_revoked,
    )
}

/// Atténue un token pour limiter STRICTEMENT l'accès aux inférences cognitives.
/// Interdit toute fuite de données brutes (Camera, Audio, Biometrics, Spatial).
pub fn attenuate_cognitive_inference_token(
    parent_token: &CapabilityToken,
    allowed_resources: &[&str],
    new_expiry: Timestamp,
) -> Result<CapabilityToken> {
    // 1. Monotonie : Le token parent DOIT détenir le droit CognitiveInference:Read (ou FullSystem:Admin)
    let parent_rights = effective_rights(parent_token)?;
    let has_cog_read = parent_rights
        .contains(&("cognitive_inference".to_string(), "read".to_string()))
        || parent_rights.contains(&("cognitiveinference".to_string(), "read".to_string()))
        || parent_rights.contains(&("fullsystem".to_string(), "admin".to_string()));
    if !has_cog_read {
        return Err(HakilleError::AttenuationViolation);
    }

    // 2. Monotonie temporelle
    let new_exp_ts = new_expiry.0.timestamp();
    match min_expiry_in_token(parent_token) {
        Some(parent_exp) if new_exp_ts <= parent_exp => {}
        _ => return Err(HakilleError::AttenuationViolation),
    }

    // 3. Validation stricte des noms de ressources (anti-injection Datalog)
    for res in allowed_resources {
        validate_binding_value(res)?;
    }

    // 4. Construction du bloc d'atténuation Biscuit v5
    let mut block = biscuit_auth::builder::BlockBuilder::new();

    // Règle 1 : Verrouillage strict de l'opération (ANTI-FUITE DES DONNÉES BRUTES)
    // Interdit formellement toute tentative d'opération sur camera, audio, biometrics, spatial.
    block
        .add_check("check if operation(\"cognitive_inference\", \"read\")")
        .map_err(|_| HakilleError::AttenuationViolation)?;

    // Règle 2 : Filtrage fail-closed des ressources cognitives
    if allowed_resources.is_empty() {
        // Liste vide = Deny-all strict
        block
            .add_check("check if false")
            .map_err(|_| HakilleError::AttenuationViolation)?;
    } else {
        block
            .add_check("check if target_resource($r), allowed_cognitive_resource($r)")
            .map_err(|_| HakilleError::AttenuationViolation)?;
        for res in allowed_resources {
            let fact = format!("allowed_cognitive_resource(\"{res}\")");
            block
                .add_fact(fact.as_str())
                .map_err(|_| HakilleError::AttenuationViolation)?;
        }
    }

    // Règle 3 : Borne temporelle
    let time_check = format!("check if time($time), $time <= {new_exp_ts}");
    block
        .add_check(time_check.as_str())
        .map_err(|_| HakilleError::AttenuationViolation)?;

    let attenuated = parent_token
        .biscuit
        .append(block)
        .map_err(|_| HakilleError::AttenuationViolation)?;
    Ok(CapabilityToken {
        biscuit: attenuated,
    })
}

/// Vérifie l'accès à une inférence cognitive avec la seule clé publique racine épinglée.
pub fn verify_cognitive_inference_with_root_key(
    token: &CapabilityToken,
    root_public_key: &PinnedRootPublicKey,
    target_resource: &str,
) -> Result<()> {
    if target_resource.is_empty() || target_resource.len() > 256 {
        return Err(HakilleError::CapabilityDenied(
            "target_resource vide ou trop longue".to_string(),
        ));
    }
    validate_binding_value(target_resource)?;
    if token.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    let bytes = token
        .biscuit
        .to_vec()
        .map_err(|e| HakilleError::CapabilityDenied(format!("token serialization: {e}")))?;
    let pinned = Biscuit::from(bytes, *root_public_key)
        .map_err(|e| HakilleError::CapabilityDenied(format!("root key pin mismatch: {e}")))?;

    let mut authorizer = pinned
        .authorizer()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

    let now_ts = chrono::Utc::now().timestamp();
    authorizer
        .add_fact(format!("time({now_ts})").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_fact("operation(\"cognitive_inference\", \"read\")")
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_fact(format!("target_resource(\"{target_resource}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_check("check if right(\"cognitive_inference\", \"read\")")
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .allow()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

    let limits = biscuit_auth::AuthorizerLimits {
        max_time: std::time::Duration::from_millis(500),
        ..Default::default()
    };
    authorizer.authorize_with_limits(limits).map_err(|e| {
        if token.is_expired() {
            HakilleError::CapabilityExpired
        } else {
            HakilleError::CapabilityDenied(e.to_string())
        }
    })?;
    Ok(())
}

/// Extrait les faits de liaison `(resource, token_nonce, token_aud)` depuis le bloc
/// autorité 0 uniquement (un bloc d'atténuation tiers ne peut jamais forger un fait racine).
pub fn token_binding_facts(
    token: &CapabilityToken,
) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(src) = token.biscuit.print_block_source(0) else {
        return (None, None, None);
    };
    let resource = parse_single_string_fact(&src, "resource");
    let nonce = parse_single_string_fact(&src, "token_nonce");
    let aud = parse_single_string_fact(&src, "token_aud");
    (resource, nonce, aud)
}

/// Validation fail-closed des valeurs de liaison (anti-injection Datalog).
fn validate_binding_value(v: &str) -> Result<()> {
    if v.is_empty() || v.len() > 256 {
        return Err(HakilleError::CapabilityDenied(
            "binding value vide ou trop longue (max 256)".to_string(),
        ));
    }
    if v.contains('"') || v.contains('\\') || v.contains('\n') || v.contains('\r') {
        return Err(HakilleError::CapabilityDenied(
            "binding value invalide (guillemets / backslash / retours refusés)".to_string(),
        ));
    }
    Ok(())
}

/// Extrait `name("value")` (première occurrence) depuis une source de bloc.
fn parse_single_string_fact(src: &str, name: &str) -> Option<String> {
    let needle = format!("{name}(\"");
    let idx = src.find(&needle)?;
    let after = &src[idx + needle.len()..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Extrait toutes les occurrences de `name("value")` dans une source de bloc.
fn parse_all_string_facts(src: &str, name: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let needle = format!("{name}(\"");
    let mut rest = src;
    while let Some(idx) = rest.find(&needle) {
        let after = &rest[idx + needle.len()..];
        if let Some(end) = after.find('"') {
            out.insert(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Legacy structural verification WITHOUT root-key pinning.
///
/// Weaker: rejects wrong scope/action + expiry, but accepts any self-signed
/// root. KEPT ONLY for `#[cfg(test)]` + migration des substrates existants
/// (qui doivent passer à `verify_capability_with_root_key`) ; le journal
/// (`revert_with_cap`) ne l'appelle PLUS en code prod (pinning obligatoire).
/// Production must use `verify_capability_with_root_key` or
/// `CapabilityAuthority::verify_pinned`.
///
/// DEPRECATED: ne pas utiliser dans le nouveau code — l'absence de pinning
/// accepte les tokens auto-signés par un attaquant.
#[deprecated(
    since = "0.1.0",
    note = "Unpinned verification accepts self-signed tokens; use `verify_capability_with_root_key` or `CapabilityAuthority::verify_pinned` instead."
)]
pub fn verify_capability_unpinned(
    token: &CapabilityToken,
    required_scope: CapabilityScope,
    required_action: CapabilityAction,
) -> Result<()> {
    if token.is_expired() {
        return Err(HakilleError::CapabilityExpired);
    }
    let mut authorizer = token
        .biscuit
        .authorizer()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let s = required_scope.datalog_name();
    let a = format!("{:?}", required_action).to_lowercase();
    let now_ts = chrono::Utc::now().timestamp();
    authorizer
        .add_fact(format!("time({now_ts})").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_fact(format!("operation(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .add_check(format!("check if right(\"{s}\", \"{a}\")").as_str())
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    authorizer
        .allow()
        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;
    let limits = biscuit_auth::AuthorizerLimits {
        max_time: std::time::Duration::from_millis(500),
        ..Default::default()
    };
    authorizer.authorize_with_limits(limits).map_err(|e| {
        if token.is_expired() {
            HakilleError::CapabilityExpired
        } else {
            HakilleError::CapabilityDenied(e.to_string())
        }
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers internes : décodage des droits effectifs et bornes temporelles.
// ---------------------------------------------------------------------------

/// Droits effectifs `(scope, action)` décodés depuis le bloc autorité 0
/// puis intersectés avec chaque bloc d'atténuation `1..block_count()`.
fn effective_rights(
    token: &CapabilityToken,
) -> Result<std::collections::HashSet<(String, String)>> {
    if token.biscuit.block_count() == 0 {
        return Ok(std::collections::HashSet::new());
    }
    let auth_src = token
        .biscuit
        .print_block_source(0)
        .map_err(|_| HakilleError::AttenuationViolation)?;
    let mut rights: std::collections::HashSet<(String, String)> =
        parse_right_facts(&auth_src).into_iter().collect();

    for i in 1..token.biscuit.block_count() {
        let src = token
            .biscuit
            .print_block_source(i)
            .map_err(|_| HakilleError::AttenuationViolation)?;
        if src.contains("check if false") {
            return Ok(std::collections::HashSet::new());
        }
        let allowed_scopes = parse_all_string_facts(&src, "allowed_scope");
        let allowed_actions = parse_all_string_facts(&src, "allowed_action");
        if !allowed_scopes.is_empty() {
            rights.retain(|(s, _)| {
                allowed_scopes.contains(s) || (s == "fullsystem" && !allowed_scopes.is_empty())
            });
            // Si le parent détenait fullsystem, matérialise l'atténuation sur les scopes restreints
            let had_full = rights.iter().any(|(s, _)| s == "fullsystem");
            if had_full {
                let actions_for_full: Vec<String> = rights
                    .iter()
                    .filter(|(s, _)| s == "fullsystem")
                    .map(|(_, a)| a.clone())
                    .collect();
                rights.retain(|(s, _)| s != "fullsystem");
                for sc in &allowed_scopes {
                    for ac in &actions_for_full {
                        rights.insert((sc.clone(), ac.clone()));
                    }
                }
            }
        }
        if !allowed_actions.is_empty() {
            let had_admin_scopes: Vec<String> = rights
                .iter()
                .filter(|(_, a)| a == "admin")
                .map(|(s, _)| s.clone())
                .collect();
            rights.retain(|(_, a)| allowed_actions.contains(a));
            for sc in had_admin_scopes {
                for ac in &allowed_actions {
                    rights.insert((sc.clone(), ac.clone()));
                }
            }
        }
    }
    Ok(rights)
}

/// Borne temporelle minimale (`time <= N`) sur l'ensemble des blocs.
fn min_expiry_in_token(token: &CapabilityToken) -> Option<i64> {
    let mut expiry: Option<i64> = None;
    for i in 0..token.biscuit.block_count() {
        let Ok(src) = token.biscuit.print_block_source(i) else {
            continue;
        };
        for exp in parse_expiry_checks(&src) {
            expiry = Some(expiry.map_or(exp, |e: i64| e.min(exp)));
        }
    }
    expiry
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SpendingCategory {
    Food,
    Transport,
    Entertainment,
    Shopping,
    Bills,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WalletCapability {
    pub scope: CapabilityScope, // Should be CapabilityScope::Wallet
    pub max_amount_cents: u64,
    pub currency: String,
    pub category: SpendingCategory,
    pub expires_at: Timestamp,
    /// Propriétaire lié à l'émission F7 (fail-closed : `None` = cap legacy
    /// sans liaison, refusé par `check_cap_validity`). `#[serde(default)]`
    /// pour relire les snapshots JSON antérieurs (migrer par réémission).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<UserId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn future_ts() -> Timestamp {
        Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600))
    }

    #[test]
    fn resource_binding_roundtrip_and_enforcement() {
        let authority = CapabilityAuthority::new();
        let root = authority.root_public_key();
        let tok = authority
            .create_root_token_with_binding(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                future_ts(),
                Some("file:/docs/rapport.pdf"),
                Some("jti-test-001"),
                Some("hakille-daemon"),
            )
            .unwrap();
        let (res, nonce, aud) = token_binding_facts(&tok);
        assert_eq!(res.as_deref(), Some("file:/docs/rapport.pdf"));
        assert_eq!(nonce.as_deref(), Some("jti-test-001"));
        assert_eq!(aud.as_deref(), Some("hakille-daemon"));
        // Bonne ressource : ok (méthode + fonction libre + alias).
        assert!(
            authority
                .verify_pinned_with_resource(
                    &tok,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                    "file:/docs/rapport.pdf"
                )
                .is_ok()
        );
        assert!(
            verify_capability_with_root_key_and_resource(
                &tok,
                &root,
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                "file:/docs/rapport.pdf"
            )
            .is_ok()
        );
        assert!(
            enforce_with_resource(
                &tok,
                &root,
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                "file:/docs/rapport.pdf"
            )
            .is_ok()
        );
        // Mauvaise ressource : refus (liaison, pas de contournement).
        assert!(
            authority
                .verify_pinned_with_resource(
                    &tok,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                    "file:/autre.pdf"
                )
                .is_err()
        );
        // Token legacy sans resource : enforce_with_resource refuse (fail-closed).
        let legacy = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                future_ts(),
            )
            .unwrap();
        assert!(
            authority
                .verify_pinned_with_resource(
                    &legacy,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                    "file:/docs/rapport.pdf"
                )
                .is_err()
        );
        // Mais la vérification non liée reste ok (compatibilité).
        assert!(
            authority
                .verify(&legacy, CapabilityScope::Filesystem, CapabilityAction::Read)
                .is_ok()
        );
    }

    #[test]
    fn binding_injection_rejected() {
        let authority = CapabilityAuthority::new();
        assert!(
            authority
                .create_root_token_with_binding(
                    vec![CapabilityScope::Filesystem],
                    vec![CapabilityAction::Read],
                    future_ts(),
                    Some("evil\", right(\"x"),
                    None,
                    None,
                )
                .is_err()
        );
        assert!(
            authority
                .create_root_token_with_binding(
                    vec![CapabilityScope::Filesystem],
                    vec![CapabilityAction::Read],
                    future_ts(),
                    Some(""),
                    None,
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn revocation_by_nonce_denies() {
        let authority = CapabilityAuthority::new();
        let tok = authority
            .create_root_token_with_binding(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Read],
                future_ts(),
                None,
                Some("jti-revoked-1"),
                None,
            )
            .unwrap();
        // Non révoqué : ok.
        assert!(
            authority
                .verify_pinned_with_revocation(
                    &tok,
                    CapabilityScope::Network,
                    CapabilityAction::Read,
                    &|_| false
                )
                .is_ok()
        );
        // Révoqué : refus explicite.
        assert!(
            authority
                .verify_pinned_with_revocation(
                    &tok,
                    CapabilityScope::Network,
                    CapabilityAction::Read,
                    &|jti| jti == "jti-revoked-1"
                )
                .is_err()
        );

        // Test with pinned root public key directly
        let root_pub = authority.root_public_key();
        assert!(
            enforce_with_revocation(
                &tok,
                &root_pub,
                CapabilityScope::Network,
                CapabilityAction::Read,
                &|_| false
            )
            .is_ok()
        );
        assert!(
            enforce_with_revocation(
                &tok,
                &root_pub,
                CapabilityScope::Network,
                CapabilityAction::Read,
                &|jti| jti == "jti-revoked-1"
            )
            .is_err()
        );
    }

    #[test]
    fn test_cognitive_inference_attenuation_fail_closed_and_no_raw_leak() {
        let authority = CapabilityAuthority::new();
        let root = authority.root_public_key();
        let exp = future_ts();

        // 1. Émission d'un token parent disposant de tous les accès capteurs et inférences
        let parent = authority
            .create_root_token(
                vec![
                    CapabilityScope::Camera,
                    CapabilityScope::Audio,
                    CapabilityScope::Biometrics,
                    CapabilityScope::Spatial,
                    CapabilityScope::CognitiveInference,
                ],
                vec![CapabilityAction::Read],
                exp.clone(),
            )
            .unwrap();

        // Le parent peut lire les données brutes
        assert!(
            authority
                .verify(&parent, CapabilityScope::Camera, CapabilityAction::Read)
                .is_ok()
        );
        assert!(
            authority
                .verify(&parent, CapabilityScope::Biometrics, CapabilityAction::Read)
                .is_ok()
        );

        // 2. Atténuation stricte pour un agent tiers : autorise UNIQUEMENT "emotion" et "presence"
        let agent_token = authority
            .attenuate_cognitive_inference(&parent, &["emotion", "presence"], exp.clone())
            .unwrap();

        // A. Accès autorisés : OK
        assert!(
            authority
                .verify_cognitive_inference(&agent_token, "emotion")
                .is_ok()
        );
        assert!(
            authority
                .verify_cognitive_inference(&agent_token, "presence")
                .is_ok()
        );
        assert!(verify_cognitive_inference_with_root_key(&agent_token, &root, "emotion").is_ok());

        // B. Ressource cognitive non autorisée ("fatigue", "estimated_age") : REFUS STRICT (Fail-Closed)
        assert!(
            authority
                .verify_cognitive_inference(&agent_token, "fatigue")
                .is_err()
        );
        assert!(
            authority
                .verify_cognitive_inference(&agent_token, "estimated_age")
                .is_err()
        );

        // C. Tentative d'accès aux flux caméra bruts : REFUS STRICT (Fail-Closed, anti-fuite)
        assert!(
            authority
                .verify(
                    &agent_token,
                    CapabilityScope::Camera,
                    CapabilityAction::Read
                )
                .is_err()
        );

        // D. Tentative d'accès aux repères biométriques bruts : REFUS STRICT
        assert!(
            authority
                .verify(
                    &agent_token,
                    CapabilityScope::Biometrics,
                    CapabilityAction::Read
                )
                .is_err()
        );

        // E. Tentative d'accès au squelette 3D spatial brut : REFUS STRICT
        assert!(
            authority
                .verify(
                    &agent_token,
                    CapabilityScope::Spatial,
                    CapabilityAction::Read
                )
                .is_err()
        );

        // F. Token forgé avec mauvaise clé racine : REFUS STRICT
        let rogue_authority = CapabilityAuthority::new();
        let rogue_token = rogue_authority
            .create_root_token(
                vec![CapabilityScope::CognitiveInference],
                vec![CapabilityAction::Read],
                exp.clone(),
            )
            .unwrap();
        assert!(verify_cognitive_inference_with_root_key(&rogue_token, &root, "emotion").is_err());

        // G. Atténuation sans droit parent : REJET
        let camera_only_parent = authority
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![CapabilityAction::Read],
                exp.clone(),
            )
            .unwrap();
        assert!(
            authority
                .attenuate_cognitive_inference(&camera_only_parent, &["emotion"], exp.clone())
                .is_err()
        );

        // H. Atténuation avec liste vide : Deny-all (aucun accès)
        let deny_all = authority
            .attenuate_cognitive_inference(&parent, &[], exp.clone())
            .unwrap();
        assert!(
            authority
                .verify_cognitive_inference(&deny_all, "emotion")
                .is_err()
        );

        // I. Tentative d'injection Datalog dans le nom de ressource : REJET IMMÉDIAT
        assert!(
            authority
                .attenuate_cognitive_inference(&parent, &["emotion\", evil"], exp)
                .is_err()
        );
    }

    #[test]
    fn test_notification_capability_verification_and_isolation() {
        let authority = CapabilityAuthority::new();
        let root_pub = authority.root_public_key();
        let exp = future_ts();

        // 1. Création d'un token Notification Write (envoi) et Read (écoute)
        let notif_token = authority
            .create_root_token(
                vec![CapabilityScope::Notification],
                vec![CapabilityAction::Write, CapabilityAction::Read],
                exp.clone(),
            )
            .expect("notification root token");

        // 2. Vérification contre la clé publique racine : OK
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Notification,
                    CapabilityAction::Write
                )
                .is_ok()
        );
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Notification,
                    CapabilityAction::Read
                )
                .is_ok()
        );
        assert!(
            verify_capability_with_root_key(
                &notif_token,
                &root_pub,
                CapabilityScope::Notification,
                CapabilityAction::Write,
            )
            .is_ok()
        );

        // 3. Action non accordée (Delete / Admin) : REFUS STRICT
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Notification,
                    CapabilityAction::Delete
                )
                .is_err()
        );
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Notification,
                    CapabilityAction::Admin
                )
                .is_err()
        );

        // 4. Isolation de périmètre : Un token Notification ne peut PAS lire le système de fichiers, la caméra ou le wallet
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read
                )
                .is_err()
        );
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Camera,
                    CapabilityAction::Read
                )
                .is_err()
        );
        assert!(
            authority
                .verify(
                    &notif_token,
                    CapabilityScope::Wallet,
                    CapabilityAction::Write
                )
                .is_err()
        );

        // 5. Clé racine contrefaite : REJET
        let rogue = CapabilityAuthority::new();
        let rogue_token = rogue
            .create_root_token(
                vec![CapabilityScope::Notification],
                vec![CapabilityAction::Write],
                exp,
            )
            .expect("rogue token");
        assert!(
            verify_capability_with_root_key(
                &rogue_token,
                &root_pub,
                CapabilityScope::Notification,
                CapabilityAction::Write,
            )
            .is_err()
        );
    }
}
