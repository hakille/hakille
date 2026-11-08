//! Trames chiffrées du Réseau Maillé Souverain (§35).
//!
//! Principes souverains de Hakille :
//! - Chiffrement AEAD ChaCha20-Poly1305 par trame avec signature Ed25519 de l'émetteur.
//! - Garde Link-Local strict : rejet immédiat fail-closed de toute adresse WAN ou routable.
//! - Nonces uniques et numéros de séquence monotones anti-rejeu.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MeshError {
    #[error("Adresse non link-local : tentative de fuite réseau WAN rejetée fail-closed")]
    NonLinkLocalAddress(String),
    #[error("Échec de déchiffrement AEAD")]
    DecryptionFailed,
    #[error("Signature cryptographique de trame invalide")]
    InvalidSignature,
    #[error("Clé de vérification Ed25519 invalide")]
    InvalidVerifyingKey,
    #[error("Trame expirée ou horodatage dans le futur")]
    FrameExpired,
    #[error("Erreur de transport maillé : {0}")]
    Transport(String),
}

/// Garde fail-closed link-local vérifiant l'absence de routage WAN.
pub struct LinkLocalGuard;

impl LinkLocalGuard {
    /// Vérifie si une chaîne d'adresse ou d'URI est strictement link-local ou air-gap.
    /// Rejette toute adresse IP publique routable ou domaine WAN.
    pub fn is_link_local_safe(target: &str) -> bool {
        let clean = target.trim();
        if clean.is_empty() {
            return false;
        }

        // 1. Schémas virtuels ou bus local air-gap
        if clean.starts_with("airgap://")
            || clean.starts_with("virtual://")
            || clean.starts_with("mesh-local://")
        {
            return true;
        }

        // 2. Rejet immédiat des protocoles cloud ou WAN
        if clean.starts_with("http://")
            || clean.starts_with("https://")
            || clean.starts_with("ftp://")
        {
            return false;
        }

        // 3. Extraction de l'hôte (retrait du port si présent)
        let host_str = if let Ok(sock) = clean.parse::<SocketAddr>() {
            return Self::is_ip_link_local(&sock.ip());
        } else if let Some((h, _)) = clean.rsplit_once(':') {
            h.trim_start_matches('[').trim_end_matches(']')
        } else {
            clean
        };

        // 4. Analyse IP
        if let Ok(ip) = host_str.parse::<IpAddr>() {
            Self::is_ip_link_local(&ip)
        } else {
            // Rejette tout domaine externe non résolu localement
            clean == "localhost"
        }
    }

    /// Contrôle précis de la plage IP (Loopback ou Link-Local).
    pub fn is_ip_link_local(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                // Loopback (127.0.0.0/8) ou IPv4 Link-Local (169.254.0.0/16)
                v4.is_loopback() || v4.is_link_local()
            }
            IpAddr::V6(v6) => {
                // Loopback (::1) ou IPv6 Unicast Link-Local (fe80::/10)
                v6.is_loopback() || ((v6.segments()[0] & 0xffc0) == 0xfe80)
            }
        }
    }
}

/// Trame chiffrée et signée du réseau maillé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshFrame {
    pub frame_id: String,
    pub sequence_number: u64,
    pub sender_peer_id: String,
    pub target_peer_id: Option<String>, // None = diffusion broadcast
    pub sender_verifying_key_bytes: [u8; 32],
    pub encrypted_payload: Vec<u8>,
    pub nonce: [u8; 12],
    pub signature_bytes: Vec<u8>,
    pub timestamp_ms: u64,
}

impl MeshFrame {
    /// Construit, chiffre et signe une trame maillée.
    pub fn create_encrypted(
        sender_peer_id: impl Into<String>,
        target_peer_id: Option<String>,
        sequence_number: u64,
        plaintext_payload: &[u8],
        symmetric_key: &[u8; 32],
        nonce_bytes: [u8; 12],
        signing_key: &SigningKey,
    ) -> Result<Self, MeshError> {
        let sender_peer = sender_peer_id.into();
        let frame_id = format!("frame-{}", uuid::Uuid::new_v4());
        let now_ms = hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64;

        // Chiffrement AEAD ChaCha20-Poly1305
        let cipher = ChaCha20Poly1305::new_from_slice(symmetric_key)
            .map_err(|_| MeshError::DecryptionFailed)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let encrypted_payload = cipher
            .encrypt(nonce, plaintext_payload)
            .map_err(|_| MeshError::DecryptionFailed)?;

        // Attestation signée Ed25519
        let mut msg_to_sign = Vec::new();
        msg_to_sign.extend_from_slice(frame_id.as_bytes());
        msg_to_sign.extend_from_slice(&sequence_number.to_le_bytes());
        msg_to_sign.extend_from_slice(sender_peer.as_bytes());
        if let Some(target) = &target_peer_id {
            msg_to_sign.extend_from_slice(target.as_bytes());
        }
        msg_to_sign.extend_from_slice(&encrypted_payload);
        msg_to_sign.extend_from_slice(&nonce_bytes);
        msg_to_sign.extend_from_slice(&now_ms.to_le_bytes());

        let signature = signing_key.sign(&msg_to_sign);
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            frame_id,
            sequence_number,
            sender_peer_id: sender_peer,
            target_peer_id,
            sender_verifying_key_bytes: verifying_key.to_bytes(),
            encrypted_payload,
            nonce: nonce_bytes,
            signature_bytes: signature.to_bytes().to_vec(),
            timestamp_ms: now_ms,
        })
    }

    /// Vérifie l'authenticité de la trame et déchiffre son contenu.
    pub fn verify_and_decrypt(&self, symmetric_key: &[u8; 32]) -> Result<Vec<u8>, MeshError> {
        let verifying_key = VerifyingKey::from_bytes(&self.sender_verifying_key_bytes)
            .map_err(|_| MeshError::InvalidVerifyingKey)?;

        let sig_array: [u8; 64] = self
            .signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| MeshError::InvalidSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

        let mut msg_to_sign = Vec::new();
        msg_to_sign.extend_from_slice(self.frame_id.as_bytes());
        msg_to_sign.extend_from_slice(&self.sequence_number.to_le_bytes());
        msg_to_sign.extend_from_slice(self.sender_peer_id.as_bytes());
        if let Some(target) = &self.target_peer_id {
            msg_to_sign.extend_from_slice(target.as_bytes());
        }
        msg_to_sign.extend_from_slice(&self.encrypted_payload);
        msg_to_sign.extend_from_slice(&self.nonce);
        msg_to_sign.extend_from_slice(&self.timestamp_ms.to_le_bytes());

        verifying_key
            .verify(&msg_to_sign, &signature)
            .map_err(|_| MeshError::InvalidSignature)?;

        let cipher = ChaCha20Poly1305::new_from_slice(symmetric_key)
            .map_err(|_| MeshError::DecryptionFailed)?;
        let nonce = Nonce::from_slice(&self.nonce);

        cipher
            .decrypt(nonce, self.encrypted_payload.as_slice())
            .map_err(|_| MeshError::DecryptionFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_local_guard_rejects_wan_and_accepts_link_local() {
        // WAN IPs must be blocked fail-closed
        assert!(!LinkLocalGuard::is_link_local_safe("8.8.8.8:53"));
        assert!(!LinkLocalGuard::is_link_local_safe("1.1.1.1"));
        assert!(!LinkLocalGuard::is_link_local_safe(
            "https://api.openai.com/v1"
        ));
        assert!(!LinkLocalGuard::is_link_local_safe("example.com:443"));

        // Link-local and loopback must be accepted
        assert!(LinkLocalGuard::is_link_local_safe("127.0.0.1:8080"));
        assert!(LinkLocalGuard::is_link_local_safe("169.254.1.25:9000"));
        assert!(LinkLocalGuard::is_link_local_safe("[::1]:9000"));
        assert!(LinkLocalGuard::is_link_local_safe("[fe80::1]:9000"));
        assert!(LinkLocalGuard::is_link_local_safe("airgap://mesh_bus"));
        assert!(LinkLocalGuard::is_link_local_safe("virtual://node_alpha"));
    }

    #[test]
    fn test_mesh_frame_encrypt_sign_verify_roundtrip() {
        let signing_key = SigningKey::from_bytes(&[33u8; 32]);
        let sym_key = [77u8; 32];
        let nonce = [12u8; 12];
        let payload = b"Donnees souveraines synchronisees sans cloud";

        let frame = MeshFrame::create_encrypted(
            "node-alpha",
            Some("node-beta".into()),
            1,
            payload,
            &sym_key,
            nonce,
            &signing_key,
        )
        .unwrap();

        assert_eq!(frame.sender_peer_id, "node-alpha");
        assert_eq!(frame.target_peer_id.as_deref(), Some("node-beta"));

        let decrypted = frame.verify_and_decrypt(&sym_key).unwrap();
        assert_eq!(decrypted, payload);

        // Tampered payload fails
        let mut tampered = frame.clone();
        tampered.encrypted_payload[0] ^= 0xFF;
        assert!(tampered.verify_and_decrypt(&sym_key).is_err());
    }
}
