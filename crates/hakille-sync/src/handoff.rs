//! Transfert de contexte signé entre pairs explicitement approuvés localement.
//! Aucun appairage automatique depuis le message reçu. Les clés, autorisations
//! et le cache anti-rejeu sont en mémoire : l'hôte fournit une clé durable via
//! `with_keypair`, persiste `replay_snapshot_json` avant reprise du contexte et
//! le restaure avant d'accepter après redémarrage. Aucun transfert n'exécute
//! une action métier. Le stockage protégé et l'appairage restent à fournir par l'hôte.

use hakille_core::{
    Ed25519Signature, Ed25519VerifyingKey, HakilleError, KeyPair, Result, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const HANDOFF_TTL_SECS: i64 = 5 * 60;
const FUTURE_SKEW_SECS: i64 = 30;
const MAX_APPROVED_PEERS: usize = 128;
const MAX_REPLAY_ENTRIES: usize = 4096;
const SIGNATURE_VERSION: u8 = 2;
fn legacy_signature_version() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationHandoffState {
    pub source_device_id: String,
    pub target_device_id: String,
    pub last_utterance: String,
    pub pending_action: Option<String>,
    pub interaction_focus: Option<String>,
    pub handoff_token: String,
    pub timestamp: Timestamp,
    pub expires_at: Timestamp,
    /// Copie informative de la clé : elle ne crée aucune confiance.
    pub coordinator_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
    /// Les anciens messages sont lisibles mais leurs signatures v1 sont refusées.
    #[serde(default = "legacy_signature_version")]
    pub signature_version: u8,
}

impl ConversationHandoffState {
    fn canonical_bytes(&self) -> Vec<u8> {
        fn field(bytes: &mut Vec<u8>, value: &[u8]) {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend_from_slice(value);
        }
        let mut bytes = b"hakille.conversation-handoff.v2\0".to_vec();
        bytes.push(self.signature_version);
        for value in [
            &self.source_device_id,
            &self.target_device_id,
            &self.last_utterance,
        ] {
            field(&mut bytes, value.as_bytes());
        }
        for value in [&self.pending_action, &self.interaction_focus] {
            bytes.push(u8::from(value.is_some()));
            if let Some(value) = value {
                field(&mut bytes, value.as_bytes());
            }
        }
        for value in [
            &self.handoff_token,
            &self.timestamp.0.to_rfc3339(),
            &self.expires_at.0.to_rfc3339(),
        ] {
            field(&mut bytes, value.as_bytes());
        }
        field(&mut bytes, &self.coordinator_pubkey);
        bytes
    }
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() >= self.expires_at.0
    }
}

fn valid_device_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaySnapshot {
    version: u8,
    target_device_id: String,
    entries: Vec<ReplayEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayEntry {
    source_device_id: String,
    token: String,
    expires_at: Timestamp,
}

pub struct HandoffCoordinator {
    device_id: String,
    signing_key: KeyPair,
    active_handoff: Option<ConversationHandoffState>,
    approved_peers: HashMap<String, Ed25519VerifyingKey>,
    /// Token et identité, pas seulement signature : une seconde signature du même
    /// token ne contourne pas l'anti-rejeu. Entrées conservées jusqu'à expiration.
    seen_tokens: HashMap<(String, String), Timestamp>,
}

impl HandoffCoordinator {
    /// Clé éphémère : un nouveau coordinateur doit être explicitement réapprouvé.
    pub fn new(device_id: impl Into<String>) -> Self {
        Self::with_keypair(device_id, KeyPair::generate())
    }
    pub fn with_keypair(device_id: impl Into<String>, signing_key: KeyPair) -> Self {
        Self {
            device_id: device_id.into(),
            signing_key,
            active_handoff: None,
            approved_peers: HashMap::new(),
            seen_tokens: HashMap::new(),
        }
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key.to_bytes()
    }

    /// Décision de confiance locale hors bande : ne jamais appeler à partir de
    /// la seule clé incluse dans un handoff. Une rotation exige d'abord révocation.
    pub fn approve_peer(&mut self, device_id: &str, public_key: [u8; 32]) -> Result<()> {
        if !valid_device_id(device_id) {
            return Err(HakilleError::AuthenticationFailed);
        }
        let key = Ed25519VerifyingKey::from_bytes(&public_key)
            .map_err(|error| HakilleError::CryptoError(error.to_string()))?;
        if key.is_weak() {
            return Err(HakilleError::AuthenticationFailed);
        }
        if let Some(current) = self.approved_peers.get(device_id) {
            if current != &key {
                return Err(HakilleError::AuthenticationFailed);
            }
            return Ok(());
        }
        if self.approved_peers.len() >= MAX_APPROVED_PEERS {
            return Err(HakilleError::Internal(
                "Nombre maximal de pairs approuvés atteint".into(),
            ));
        }
        self.approved_peers.insert(device_id.into(), key);
        Ok(())
    }
    pub fn revoke_peer(&mut self, device_id: &str) -> bool {
        let removed = self.approved_peers.remove(device_id).is_some();
        if self
            .active_handoff
            .as_ref()
            .is_some_and(|state| state.source_device_id == device_id)
        {
            self.active_handoff = None;
        }
        removed
    }
    pub fn prepare_handoff(
        &mut self,
        target_device_id: &str,
        last_utterance: &str,
        pending_action: Option<String>,
        focus: Option<String>,
    ) -> ConversationHandoffState {
        let expires_at =
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(HANDOFF_TTL_SECS));
        self.prepare_handoff_expiring_at(
            target_device_id,
            last_utterance,
            pending_action,
            focus,
            expires_at,
        )
    }
    /// Un TTL explicite hors bornes peut être produit pour diagnostic, mais sera
    /// toujours refusé par la vérification distante.
    pub fn prepare_handoff_expiring_at(
        &mut self,
        target_device_id: &str,
        last_utterance: &str,
        pending_action: Option<String>,
        focus: Option<String>,
        expires_at: Timestamp,
    ) -> ConversationHandoffState {
        let mut state = ConversationHandoffState {
            source_device_id: self.device_id.clone(),
            target_device_id: target_device_id.into(),
            last_utterance: last_utterance.into(),
            pending_action,
            interaction_focus: focus,
            handoff_token: uuid::Uuid::new_v4().to_string(),
            timestamp: Timestamp::now(),
            expires_at,
            coordinator_pubkey: self.public_key().to_vec(),
            signature: Vec::new(),
            signature_version: SIGNATURE_VERSION,
        };
        state.signature = self.signing_key.sign(&state.canonical_bytes()).0;
        self.active_handoff = Some(state.clone());
        state
    }

    /// Validation sans mutation : cible, pair approuvé, bornes, temps, signature
    /// stricte et token non encore reçu. `receive_handoff` consomme le token.
    pub fn verify_handoff(&self, state: &ConversationHandoffState) -> Result<()> {
        if state.signature_version != SIGNATURE_VERSION
            || state.target_device_id != self.device_id
            || !valid_device_id(&self.device_id)
            || !valid_device_id(&state.source_device_id)
            || state.last_utterance.len() > 65_536
            || state
                .pending_action
                .as_ref()
                .is_some_and(|value| value.len() > 8192)
            || state
                .interaction_focus
                .as_ref()
                .is_some_and(|value| value.len() > 8192)
            || state.coordinator_pubkey.len() != 32
            || state.signature.len() != 64
            || state.handoff_token.len() != 36
            || !uuid::Uuid::parse_str(&state.handoff_token)
                .is_ok_and(|id| !id.is_nil() && id.to_string() == state.handoff_token)
        {
            return Err(HakilleError::AuthenticationFailed);
        }
        let now = chrono::Utc::now();
        let lifetime = state.expires_at.0.signed_duration_since(state.timestamp.0);
        if now >= state.expires_at.0
            || lifetime <= chrono::Duration::zero()
            || lifetime > chrono::Duration::seconds(HANDOFF_TTL_SECS)
            || state.timestamp.0 > now + chrono::Duration::seconds(FUTURE_SKEW_SECS)
        {
            return Err(HakilleError::CapabilityExpired);
        }
        let approved = self
            .approved_peers
            .get(&state.source_device_id)
            .ok_or(HakilleError::AuthenticationFailed)?;
        if approved.as_bytes().as_slice() != state.coordinator_pubkey.as_slice()
            || self
                .seen_tokens
                .contains_key(&(state.source_device_id.clone(), state.handoff_token.clone()))
        {
            return Err(HakilleError::AuthenticationFailed);
        }
        let signature = Ed25519Signature::from_slice(&state.signature)
            .map_err(|_| HakilleError::AuthenticationFailed)?;
        approved
            .verify_strict(&state.canonical_bytes(), &signature)
            .map_err(|_| HakilleError::AuthenticationFailed)
    }

    pub fn receive_handoff(&mut self, state: ConversationHandoffState) -> bool {
        if self.verify_handoff(&state).is_err() {
            return false;
        }
        let now = chrono::Utc::now();
        if state.expires_at.0 <= now {
            return false;
        }
        self.seen_tokens.retain(|_, expires_at| expires_at.0 > now);
        if self.seen_tokens.len() >= MAX_REPLAY_ENTRIES {
            return false;
        }
        self.seen_tokens.insert(
            (state.source_device_id.clone(), state.handoff_token.clone()),
            state.expires_at.clone(),
        );
        self.active_handoff = Some(state);
        true
    }
    /// À écrire durablement dans un stockage protégé avant d'utiliser le contexte
    /// reçu. Le snapshot n'approuve aucune clé et ne contient aucune clé privée.
    pub fn replay_snapshot_json(&self) -> Result<Vec<u8>> {
        let mut entries: Vec<_> = self
            .seen_tokens
            .iter()
            .map(|((source, token), expires_at)| ReplayEntry {
                source_device_id: source.clone(),
                token: token.clone(),
                expires_at: expires_at.clone(),
            })
            .collect();
        entries.sort_by(|left, right| {
            (&left.source_device_id, &left.token).cmp(&(&right.source_device_id, &right.token))
        });
        serde_json::to_vec(&ReplaySnapshot {
            version: 1,
            target_device_id: self.device_id.clone(),
            entries,
        })
        .map_err(|error| HakilleError::SerializationError(error.to_string()))
    }

    /// Restauration validée et additive : ne retire jamais une entrée encore
    /// vivante. Le contenu doit provenir du stockage protégé de l'hôte ; cette
    /// API ne prouve ni l'intégrité du disque ni l'absence de rollback du snapshot.
    pub fn restore_replay_snapshot_json(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(HakilleError::SerializationError(
                "Cache anti-rejeu trop volumineux".into(),
            ));
        }
        let snapshot: ReplaySnapshot = serde_json::from_slice(bytes)
            .map_err(|error| HakilleError::SerializationError(error.to_string()))?;
        if snapshot.version != 1
            || snapshot.target_device_id != self.device_id
            || snapshot.entries.len() > MAX_REPLAY_ENTRIES
        {
            return Err(HakilleError::AuthenticationFailed);
        }
        let now = chrono::Utc::now();
        let mut merged = self.seen_tokens.clone();
        merged.retain(|_, expires| expires.0 > now);
        let mut seen = std::collections::HashSet::new();
        for entry in snapshot.entries {
            if !valid_device_id(&entry.source_device_id)
                || !uuid::Uuid::parse_str(&entry.token)
                    .is_ok_and(|id| !id.is_nil() && id.to_string() == entry.token)
                || entry.expires_at.0
                    > now + chrono::Duration::seconds(HANDOFF_TTL_SECS + FUTURE_SKEW_SECS)
                || !seen.insert((entry.source_device_id.clone(), entry.token.clone()))
            {
                return Err(HakilleError::AuthenticationFailed);
            }
            if entry.expires_at.0 <= now {
                continue;
            }
            let key = (entry.source_device_id, entry.token);
            merged
                .entry(key)
                .and_modify(|existing| {
                    if entry.expires_at > *existing {
                        *existing = entry.expires_at.clone();
                    }
                })
                .or_insert(entry.expires_at);
        }
        if merged.len() > MAX_REPLAY_ENTRIES {
            return Err(HakilleError::Internal("Cache anti-rejeu saturé".into()));
        }
        self.seen_tokens = merged;
        Ok(())
    }

    pub fn current_state(&self) -> Option<&ConversationHandoffState> {
        self.active_handoff.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_handoff_lifecycle() {
        let mut desktop = HandoffCoordinator::new("desktop-node-01");
        let mut mobile = HandoffCoordinator::new("mobile-node-02");
        mobile
            .approve_peer("desktop-node-01", desktop.public_key())
            .unwrap();
        let mut rogue_device = HandoffCoordinator::new("unauthorized-node-03");

        let handoff_state = desktop.prepare_handoff(
            "mobile-node-02",
            "Peux-tu resumer ce document ?",
            Some("SummarizeDocument".to_string()),
            Some("DocumentViewer".to_string()),
        );

        assert_eq!(handoff_state.source_device_id, "desktop-node-01");
        assert_eq!(handoff_state.target_device_id, "mobile-node-02");
        assert!(!handoff_state.handoff_token.is_empty());
        // Token signé : clé + signature présentes, non expiré.
        assert_eq!(handoff_state.coordinator_pubkey.len(), 32);
        assert_eq!(handoff_state.signature.len(), 64);
        assert!(!handoff_state.is_expired());

        // Unauthorized node cannot receive handoff
        let intercepted = rogue_device.receive_handoff(handoff_state.clone());
        assert!(!intercepted);
        assert!(rogue_device.current_state().is_none());

        // Legitimate target successfully receives handoff and resumes context
        let received = mobile.receive_handoff(handoff_state);
        assert!(received);
        let active = mobile.current_state().expect("State should be active");
        assert_eq!(active.last_utterance, "Peux-tu resumer ce document ?");
        assert_eq!(active.interaction_focus, Some("DocumentViewer".to_string()));
    }

    #[test]
    fn test_handoff_tampering_rejected() {
        let mut desktop = HandoffCoordinator::new("desktop-node-01");
        let mut mobile = HandoffCoordinator::new("mobile-node-02");
        mobile
            .approve_peer("desktop-node-01", desktop.public_key())
            .unwrap();

        let mut state = desktop.prepare_handoff(
            "mobile-node-02",
            "Transfère 100 €",
            Some("PayInvoice".to_string()),
            None,
        );
        // Altération post-signature (montant/action) ⇒ rejet.
        state.last_utterance = "Transfère 10000 €".to_string();
        assert!(!mobile.receive_handoff(state));
        assert!(mobile.current_state().is_none());
    }

    #[test]
    fn test_handoff_expiry_rejected() {
        let mut desktop = HandoffCoordinator::new("desktop-node-01");
        let mut mobile = HandoffCoordinator::new("mobile-node-02");
        mobile
            .approve_peer("desktop-node-01", desktop.public_key())
            .unwrap();

        let past = Timestamp(chrono::Utc::now() - chrono::Duration::seconds(60));
        let expired = desktop.prepare_handoff_expiring_at(
            "mobile-node-02",
            "Vieux contexte",
            None,
            None,
            past,
        );
        assert!(expired.is_expired());
        assert!(mobile.verify_handoff(&expired).is_err());
        assert!(!mobile.receive_handoff(expired));
    }

    #[test]
    fn test_handoff_replay_rejected() {
        let mut desktop = HandoffCoordinator::new("desktop-node-01");
        let mut mobile = HandoffCoordinator::new("mobile-node-02");
        mobile
            .approve_peer("desktop-node-01", desktop.public_key())
            .unwrap();

        let state = desktop.prepare_handoff("mobile-node-02", "Bonjour", None, None);
        assert!(mobile.receive_handoff(state.clone()));
        // Rejeu du même token signé ⇒ rejet anti-rejeu.
        assert!(!mobile.receive_handoff(state));
    }
    #[test]
    fn self_signed_unknown_and_spoofed_sources_are_rejected() {
        let mut trusted = HandoffCoordinator::new("desktop");
        let mut receiver = HandoffCoordinator::new("mobile");
        let state = trusted.prepare_handoff("mobile", "hello", None, None);
        assert!(receiver.verify_handoff(&state).is_err());
        assert!(!receiver.receive_handoff(state.clone()));
        receiver
            .approve_peer("desktop", trusted.public_key())
            .unwrap();
        let mut attacker = HandoffCoordinator::new("desktop");
        let spoof = attacker.prepare_handoff("mobile", "spoofed", None, None);
        assert!(!receiver.receive_handoff(spoof));
        assert!(
            receiver
                .approve_peer("desktop", attacker.public_key())
                .is_err()
        );
        assert!(receiver.receive_handoff(state));
        receiver.revoke_peer("desktop");
        assert!(receiver.current_state().is_none());
        let next = trusted.prepare_handoff("mobile", "revoked", None, None);
        assert!(!receiver.receive_handoff(next));
    }

    #[test]
    fn optional_fields_and_legacy_signatures_cannot_be_reinterpreted() {
        let mut sender = HandoffCoordinator::new("desktop");
        let mut receiver = HandoffCoordinator::new("mobile");
        receiver
            .approve_peer("desktop", sender.public_key())
            .unwrap();
        let state = sender.prepare_handoff("mobile", "hello", None, None);
        let mut altered = state.clone();
        altered.pending_action = Some(String::new());
        assert!(receiver.verify_handoff(&altered).is_err());
        let mut legacy = serde_json::to_value(&state).unwrap();
        legacy.as_object_mut().unwrap().remove("signature_version");
        let decoded: ConversationHandoffState = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.signature_version, 1);
        assert!(receiver.verify_handoff(&decoded).is_err());
        assert!(receiver.receive_handoff(state));
    }

    #[test]
    fn signed_future_expiry_and_resigned_replay_are_rejected() {
        let mut sender = HandoffCoordinator::new("desktop");
        let mut receiver = HandoffCoordinator::new("mobile");
        receiver
            .approve_peer("desktop", sender.public_key())
            .unwrap();
        let normal = sender.prepare_handoff("mobile", "hello", None, None);
        let mut future = normal.clone();
        future.timestamp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        future.expires_at = Timestamp(future.timestamp.0 + chrono::Duration::minutes(1));
        future.signature = sender.signing_key.sign(&future.canonical_bytes()).0;
        assert!(receiver.verify_handoff(&future).is_err());
        let too_long = sender.prepare_handoff_expiring_at(
            "mobile",
            "hello",
            None,
            None,
            Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        );
        assert!(receiver.verify_handoff(&too_long).is_err());
        assert!(receiver.receive_handoff(normal.clone()));
        let mut resigned = normal;
        resigned.last_utterance = "same token, new signature".into();
        resigned.signature = sender.signing_key.sign(&resigned.canonical_bytes()).0;
        assert!(receiver.verify_handoff(&resigned).is_err());
        assert!(!receiver.receive_handoff(resigned));
    }

    #[test]
    fn replay_cache_saturation_never_evicts_live_tokens() {
        let mut sender = HandoffCoordinator::new("desktop");
        let mut receiver = HandoffCoordinator::new("mobile");
        receiver
            .approve_peer("desktop", sender.public_key())
            .unwrap();
        let expires = Timestamp(chrono::Utc::now() + chrono::Duration::minutes(5));
        for index in 0..MAX_REPLAY_ENTRIES {
            receiver
                .seen_tokens
                .insert(("other".into(), format!("token-{index}")), expires.clone());
        }
        let state = sender.prepare_handoff("mobile", "hello", None, None);
        assert!(!receiver.receive_handoff(state.clone()));
        assert_eq!(receiver.seen_tokens.len(), MAX_REPLAY_ENTRIES);
        assert!(receiver.current_state().is_none());
        for expiry in receiver.seen_tokens.values_mut() {
            *expiry = Timestamp(chrono::Utc::now() - chrono::Duration::seconds(1));
        }
        assert!(receiver.receive_handoff(state));
        assert_eq!(receiver.seen_tokens.len(), 1);
    }
    #[test]
    fn replay_snapshot_restores_rejection_without_approving_a_peer() {
        let mut sender = HandoffCoordinator::new("desktop");
        let mut receiver = HandoffCoordinator::new("mobile");
        receiver
            .approve_peer("desktop", sender.public_key())
            .unwrap();
        let accepted = sender.prepare_handoff("mobile", "received", None, None);
        assert!(receiver.receive_handoff(accepted.clone()));
        let snapshot = receiver.replay_snapshot_json().unwrap();
        let mut restarted = HandoffCoordinator::new("mobile");
        restarted.restore_replay_snapshot_json(&snapshot).unwrap();
        assert!(restarted.approved_peers.is_empty());
        restarted
            .approve_peer("desktop", sender.public_key())
            .unwrap();
        assert!(!restarted.receive_handoff(accepted));
        let next = sender.prepare_handoff("mobile", "fresh", None, None);
        assert!(restarted.receive_handoff(next));
        let before = restarted.replay_snapshot_json().unwrap();
        let mut invalid: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
        invalid["target_device_id"] = "other-recipient".into();
        assert!(
            restarted
                .restore_replay_snapshot_json(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
        assert_eq!(restarted.replay_snapshot_json().unwrap(), before);
        // Une ancienne sauvegarde valide ne doit pas effacer les réceptions plus récentes.
        restarted.restore_replay_snapshot_json(&snapshot).unwrap();
        assert_eq!(restarted.replay_snapshot_json().unwrap(), before);
    }
}
