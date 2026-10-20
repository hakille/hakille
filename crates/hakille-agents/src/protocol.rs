use hakille_core::{
    Ed25519Signature, Ed25519VerifyingKey, HakilleError, KeyPair, Result, Signature, Timestamp,
    VerifyEd25519,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Durée de validité par défaut d'un message inter-agents (anti-rejeu).
const DEFAULT_MESSAGE_TTL_SECS: i64 = 5 * 60;

/// Fenêtre anti-rejeu : un couple (message_id, nonce) déjà vu dans les
/// 5 dernières minutes est rejeté comme rejeu.
const REPLAY_WINDOW_SECS: i64 = 5 * 60;

/// Cache anti-rejeu en mémoire.
///
/// Stocke `clé(message_id + nonce) -> expiry`. `check_and_insert` rejette
/// tout doublon encore dans la fenêtre de 5 min et purge les entrées
/// expirées à chaque appel (pas de thread de fond, pas de croissance
/// non bornée tant que le trafic est régulier).
#[derive(Debug, Default)]
pub struct ReplayGuard {
    seen: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

impl ReplayGuard {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// Clé stable = message_id + nonce hex.
    fn key_for(msg: &InterAgentMessage) -> String {
        let nonce_hex = msg
            .nonce
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        format!("{}:{}", msg.message_id, nonce_hex)
    }

    /// Purge les entrées expirées puis rejette les doublons.
    ///
    /// - Si `key` déjà présent et non expiré → `Err(CapabilityDenied("replay"))`.
    /// - Sinon insère avec `expiry = now + 5min` (borné par `expires_at`
    ///   du message si plus tôt) et retourne `Ok(())`.
    pub fn check_and_insert(&mut self, msg: &InterAgentMessage) -> Result<()> {
        let now = chrono::Utc::now();
        // Purge expirés.
        self.seen.retain(|_, exp| *exp > now);
        let key = Self::key_for(msg);
        if self.seen.contains_key(&key) {
            return Err(HakilleError::CapabilityDenied(format!(
                "Message rejeté (rejeu détecté) : {}",
                msg.message_id
            )));
        }
        let window_expiry = now + chrono::Duration::seconds(REPLAY_WINDOW_SECS);
        let msg_expiry = msg.expires_at.0;
        let expiry = window_expiry.min(msg_expiry);
        // Si le message est déjà expiré, on le rejette aussi (fail-closed)
        // sans l'insérer.
        if expiry <= now {
            return Err(HakilleError::CapabilityDenied(format!(
                "Message expiré (anti-rejeu) : {}",
                msg.message_id
            )));
        }
        self.seen.insert(key, expiry);
        Ok(())
    }

    /// Nombre d'entrées non purgées (tests / observabilité).
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterAgentMessage {
    pub message_id: Uuid,
    pub sender_id: String,
    pub recipient_id: String,
    /// Audience attendue (service/terminal destinataire). Par défaut = recipient.
    pub audience: String,
    pub action: String,
    pub payload: serde_json::Value,
    pub timestamp: Timestamp,
    /// Expiration anti-rejeu (timestamp + TTL).
    pub expires_at: Timestamp,
    /// Nonce unique 128 bits (UUID v7) anti-rejeu.
    pub nonce: Vec<u8>,
    pub signature: Signature,
}

/// Vue canonique à signer/vérifier : struct sérialisée (ordre des champs
/// fixe, pas de `to_string()` libre) ; `payload` reste un `Value` dont les
/// maps serde_json sont triées (BTreeMap sans `preserve_order`).
#[derive(Serialize)]
struct CanonicalMessage<'a> {
    message_id: Uuid,
    sender_id: &'a str,
    recipient_id: &'a str,
    audience: &'a str,
    action: &'a str,
    payload: &'a serde_json::Value,
    timestamp: &'a Timestamp,
    expires_at: &'a Timestamp,
    nonce: &'a [u8],
}

impl InterAgentMessage {
    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let canonical = CanonicalMessage {
            message_id: self.message_id,
            sender_id: &self.sender_id,
            recipient_id: &self.recipient_id,
            audience: &self.audience,
            action: &self.action,
            payload: &self.payload,
            timestamp: &self.timestamp,
            expires_at: &self.expires_at,
            nonce: &self.nonce,
        };
        serde_json::to_vec(&canonical).map_err(|e| HakilleError::SerializationError(e.to_string()))
    }

    fn sign_canonical(keypair: &KeyPair, bytes: &[u8]) -> Signature {
        keypair.sign(bytes)
    }

    pub fn sign_and_create(
        sender_id: impl Into<String>,
        recipient_id: impl Into<String>,
        action: impl Into<String>,
        payload: serde_json::Value,
        keypair: &KeyPair,
    ) -> Self {
        let r_id = recipient_id.into();
        let audience = r_id.clone();
        Self::sign_and_create_full(
            sender_id,
            r_id,
            audience,
            action,
            payload,
            keypair,
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(DEFAULT_MESSAGE_TTL_SECS)),
            Uuid::now_v7().as_bytes().to_vec(),
        )
    }

    /// Constructeur complet (tests, TTL/audience/nonce explicites).
    #[allow(clippy::too_many_arguments)]
    pub fn sign_and_create_full(
        sender_id: impl Into<String>,
        recipient_id: impl Into<String>,
        audience: impl Into<String>,
        action: impl Into<String>,
        payload: serde_json::Value,
        keypair: &KeyPair,
        expires_at: Timestamp,
        nonce: Vec<u8>,
    ) -> Self {
        let msg_id = Uuid::now_v7();
        let ts = Timestamp::now();
        let s_id = sender_id.into();
        let r_id = recipient_id.into();
        let aud = audience.into();
        let act = action.into();

        let draft = Self {
            message_id: msg_id,
            sender_id: s_id,
            recipient_id: r_id,
            audience: aud,
            action: act,
            payload,
            timestamp: ts,
            expires_at,
            nonce,
            signature: Signature(vec![]),
        };
        let bytes = draft
            .canonical_bytes()
            .expect("canonical serialization must succeed");
        let sig = Self::sign_canonical(keypair, &bytes);
        Self {
            signature: sig,
            ..draft
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at.0
    }

    /// Vérifie la signature contre la clé publique de l'expéditeur (+ expiry).
    /// Le message est rejeté si la signature est invalide OU si expiré.
    pub fn verify_signature(&self, verifying_key: &Ed25519VerifyingKey) -> bool {
        if self.is_expired() {
            return false;
        }
        let bytes = match self.canonical_bytes() {
            Ok(b) => b,
            Err(_) => return false,
        };
        match Ed25519Signature::from_slice(&self.signature.0) {
            Ok(sig) => verifying_key.verify(&bytes, &sig).is_ok(),
            Err(_) => false,
        }
    }

    /// Vérifie signature + expiry + anti-rejeu (fenêtre 5 min).
    ///
    /// Intégration du `ReplayGuard` dans la vérification : le message est
    /// rejeté (`Ok(false)` + garde non mutée sur échec crypto, `Err`
    /// remontée seulement si doublon détecté après succès crypto) si :
    /// - signature invalide ou expiré (comme `verify_signature`), OU
    /// - `guard.check_and_insert` détecte un doublon (nonce+id déjà vu).
    ///
    /// Retourne `Result<bool>` : `Ok(true)` = accepté et enregistré,
    /// `Ok(false)` = signature/expiry invalide, `Err` = rejeu détecté.
    pub fn verify_signature_with_guard(
        &self,
        verifying_key: &Ed25519VerifyingKey,
        guard: &mut ReplayGuard,
    ) -> Result<bool> {
        if !self.verify_signature(verifying_key) {
            return Ok(false);
        }
        // Succès crypto → contrôle anti-rejeu (fail-closed sur doublon).
        guard.check_and_insert(self)?;
        Ok(true)
    }

    /// Vérifie signature + expiry + audience attendue.
    pub fn verify_for_audience(
        &self,
        verifying_key: &Ed25519VerifyingKey,
        expected_audience: &str,
    ) -> bool {
        self.audience == expected_audience && self.verify_signature(verifying_key)
    }
}
