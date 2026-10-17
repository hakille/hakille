use annora_core::{
    AnnoraError, Ed25519Signature, Ed25519VerifyingKey, KeyPair, Result, Timestamp, VerifyEd25519,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Durée de validité d'un handoff (anti-rejeu temporel).
const HANDOFF_TTL_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHandoffState {
    pub source_device_id: String,
    pub target_device_id: String,
    pub last_utterance: String,
    pub pending_action: Option<String>,
    pub interaction_focus: Option<String>,
    pub handoff_token: String,
    pub timestamp: Timestamp,
    /// Expiration (timestamp + 5 min) : tout handoff expiré est rejeté.
    pub expires_at: Timestamp,
    /// Clé publique éphémère du coordinateur émetteur (32 octets Ed25519).
    pub coordinator_pubkey: Vec<u8>,
    /// Signature Ed25519 du canonique par la clé éphémère du coordinateur.
    pub signature: Vec<u8>,
}

impl ConversationHandoffState {
    /// Octets canoniques signés : longueurs explicites + champs fixes
    /// (pas de concaténation ambiguë), horodatages RFC 3339.
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        for part in [
            self.source_device_id.as_str(),
            self.target_device_id.as_str(),
            self.last_utterance.as_str(),
            self.pending_action.as_deref().unwrap_or(""),
            self.interaction_focus.as_deref().unwrap_or(""),
            self.handoff_token.as_str(),
            &self.timestamp.0.to_rfc3339(),
            &self.expires_at.0.to_rfc3339(),
        ] {
            v.extend_from_slice(&(part.len() as u64).to_le_bytes());
            v.extend_from_slice(part.as_bytes());
        }
        v.extend_from_slice(&(self.coordinator_pubkey.len() as u64).to_le_bytes());
        v.extend_from_slice(&self.coordinator_pubkey);
        v
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at.0
    }
}

pub struct HandoffCoordinator {
    device_id: String,
    /// Clé éphémère du coordinateur (générée à la construction, jamais
    /// persistée) : signe chaque handoff ; sa partie publique voyage dedans.
    signing_key: KeyPair,
    active_handoff: Option<ConversationHandoffState>,
    /// Cache anti-rejeu : signatures déjà acceptées.
    seen_signatures: HashSet<Vec<u8>>,
}

impl HandoffCoordinator {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            signing_key: KeyPair::generate(),
            active_handoff: None,
            seen_signatures: HashSet::new(),
        }
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

    /// Variante à expiration explicite (tests d'expiry ; TTL custom).
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
            target_device_id: target_device_id.to_string(),
            last_utterance: last_utterance.to_string(),
            pending_action,
            interaction_focus: focus,
            handoff_token: uuid::Uuid::now_v7().to_string(),
            timestamp: Timestamp::now(),
            expires_at,
            coordinator_pubkey: self.signing_key.verifying_key.to_bytes().to_vec(),
            signature: vec![],
        };
        let sig = self.signing_key.sign(&state.canonical_bytes());
        state.signature = sig.0;
        self.active_handoff = Some(state.clone());
        state
    }

    /// Vérification complète : destinataire, expiry, clé, signature.
    /// (Sans effet anti-rejeu : voir `receive_handoff`.)
    pub fn verify_handoff(&self, state: &ConversationHandoffState) -> Result<()> {
        if state.target_device_id != self.device_id {
            return Err(AnnoraError::AuthenticationFailed);
        }
        if state.is_expired() {
            return Err(AnnoraError::CapabilityExpired);
        }
        if state.coordinator_pubkey.len() != 32 {
            return Err(AnnoraError::CryptoError("clé coordinateur invalide".into()));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&state.coordinator_pubkey);
        let verifying = Ed25519VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| AnnoraError::CryptoError(e.to_string()))?;
        let sig = Ed25519Signature::from_slice(&state.signature)
            .map_err(|_| AnnoraError::CryptoError("signature malformée".into()))?;
        verifying
            .verify(&state.canonical_bytes(), &sig)
            .map_err(|_| AnnoraError::AuthenticationFailed)?;
        Ok(())
    }

    /// Réception : vérifie + anti-rejeu (une signature acceptée ne peut
    /// pas être rejouée). Retourne `false` en cas de rejet.
    pub fn receive_handoff(&mut self, state: ConversationHandoffState) -> bool {
        if self.verify_handoff(&state).is_err() {
            return false;
        }
        if !self.seen_signatures.insert(state.signature.clone()) {
            return false; // Rejeu : déjà vu.
        }
        self.active_handoff = Some(state);
        true
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

        let state = desktop.prepare_handoff("mobile-node-02", "Bonjour", None, None);
        assert!(mobile.receive_handoff(state.clone()));
        // Rejeu du même token signé ⇒ rejet anti-rejeu.
        assert!(!mobile.receive_handoff(state));
    }
}
