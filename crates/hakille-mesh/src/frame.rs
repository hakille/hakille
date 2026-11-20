//! Trames chiffrées du Réseau Maillé Souverain (§35).
//!
//! Principes souverains de Hakille :
//! - Chiffrement AEAD ChaCha20-Poly1305 par trame avec AAD d'en-tête et signature Ed25519 de l'émetteur.
//! - Garde Link-Local strict : rejet immédiat fail-closed de toute adresse WAN, routable ou metadata cloud.
//! - Nonces uniques et numéros de séquence monotones anti-rejeu.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
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
    #[error("Trame expirée, rejouée ou horodatage dans le futur")]
    FrameExpired,
    #[error("Erreur de transport maillé : {0}")]
    Transport(String),
}

/// Garde fail-closed link-local vérifiant l'absence de routage WAN et d'IMDS cloud.
pub struct LinkLocalGuard;

impl LinkLocalGuard {
    /// Vérifie si une chaîne d'adresse ou d'URI est strictement link-local ou air-gap.
    /// Rejette toute adresse IP publique routable, service metadata cloud (169.254.169.254) ou domaine WAN.
    pub fn is_link_local_safe(target: &str) -> bool {
        let clean = target.trim();
        if clean.is_empty() || clean.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return false;
        }

        // 1. Schémas virtuels ou bus local air-gap
        if clean.starts_with("airgap://")
            || clean.starts_with("virtual://")
            || clean.starts_with("mesh-local://")
        {
            return true;
        }

        let lower = clean.to_ascii_lowercase();
        // 2. Rejet immédiat des protocoles cloud, WAN ou userinfo '@'
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("ftp://")
            || lower.starts_with("ws://")
            || lower.starts_with("wss://")
            || clean.contains('@')
            || clean.contains('/')
            || clean.contains('\\')
        {
            return false;
        }

        // 3. Extraction de l'hôte (retrait du port si présent)
        let host_str = if let Ok(sock) = clean.parse::<SocketAddr>() {
            return Self::is_ip_link_local(&sock.ip());
        } else if clean.starts_with('[') {
            if let Some(close) = clean.find(']') {
                &clean[1..close]
            } else {
                return false;
            }
        } else if clean.parse::<IpAddr>().is_ok() {
            clean
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
            host_str.eq_ignore_ascii_case("localhost")
        }
    }

    /// Contrôle précis de la plage IP (Loopback ou Link-Local, à l'exclusion des endpoints Cloud Metadata).
    pub fn is_ip_link_local(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                // Rejet strict des endpoints Cloud Instance Metadata Service (AWS/GCP/Azure/ECS)
                if octets[0] == 169
                    && octets[1] == 254
                    && ((octets[2] == 169 && (octets[3] == 254 || octets[3] == 253))
                        || (octets[2] == 170 && octets[3] == 2))
                {
                    return false;
                }
                // Loopback (127.0.0.0/8) ou IPv4 Link-Local (169.254.0.0/16)
                v4.is_loopback() || v4.is_link_local()
            }
            IpAddr::V6(v6) => {
                if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                    return Self::is_ip_link_local(&IpAddr::V4(mapped_v4));
                }
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
    fn canonical_header_aad(
        frame_id: &str,
        sequence_number: u64,
        sender_peer_id: &str,
        target_peer_id: Option<&str>,
        sender_verifying_key_bytes: &[u8; 32],
        nonce: &[u8; 12],
        timestamp_ms: u64,
    ) -> Vec<u8> {
        let mut aad = Vec::with_capacity(160);
        aad.extend_from_slice(b"hakille-mesh-frame-v2:");
        aad.extend_from_slice(&(frame_id.len() as u64).to_le_bytes());
        aad.extend_from_slice(frame_id.as_bytes());
        aad.extend_from_slice(&sequence_number.to_le_bytes());
        aad.extend_from_slice(&(sender_peer_id.len() as u64).to_le_bytes());
        aad.extend_from_slice(sender_peer_id.as_bytes());
        match target_peer_id {
            Some(target) => {
                aad.push(1);
                aad.extend_from_slice(&(target.len() as u64).to_le_bytes());
                aad.extend_from_slice(target.as_bytes());
            }
            None => {
                aad.push(0);
            }
        }
        aad.extend_from_slice(sender_verifying_key_bytes);
        aad.extend_from_slice(nonce);
        aad.extend_from_slice(&timestamp_ms.to_le_bytes());
        aad
    }

    fn canonical_signing_bytes(&self, header_aad: &[u8]) -> Vec<u8> {
        let mut msg = Vec::with_capacity(header_aad.len() + 8 + self.encrypted_payload.len());
        msg.extend_from_slice(header_aad);
        msg.extend_from_slice(&(self.encrypted_payload.len() as u64).to_le_bytes());
        msg.extend_from_slice(&self.encrypted_payload);
        msg
    }

    /// Construit, chiffre (AEAD ChaCha20-Poly1305 avec AAD d'en-tête) et signe une trame maillée.
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
        let verifying_key_bytes = signing_key.verifying_key().to_bytes();

        let header_aad = Self::canonical_header_aad(
            &frame_id,
            sequence_number,
            &sender_peer,
            target_peer_id.as_deref(),
            &verifying_key_bytes,
            &nonce_bytes,
            now_ms,
        );

        // Chiffrement AEAD ChaCha20-Poly1305 lié à l'en-tête canonique (AAD)
        let cipher = ChaCha20Poly1305::new_from_slice(symmetric_key)
            .map_err(|_| MeshError::DecryptionFailed)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let encrypted_payload = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext_payload,
                    aad: &header_aad,
                },
            )
            .map_err(|_| MeshError::DecryptionFailed)?;

        let mut frame = Self {
            frame_id,
            sequence_number,
            sender_peer_id: sender_peer,
            target_peer_id,
            sender_verifying_key_bytes: verifying_key_bytes,
            encrypted_payload,
            nonce: nonce_bytes,
            signature_bytes: Vec::new(),
            timestamp_ms: now_ms,
        };

        let msg_to_sign = frame.canonical_signing_bytes(&header_aad);
        let signature = signing_key.sign(&msg_to_sign);
        frame.signature_bytes = signature.to_bytes().to_vec();

        Ok(frame)
    }

    /// Vérifie l'authenticité de la trame (`verify_strict` + AAD) et déchiffre son contenu.
    pub fn verify_and_decrypt(&self, symmetric_key: &[u8; 32]) -> Result<Vec<u8>, MeshError> {
        let verifying_key = VerifyingKey::from_bytes(&self.sender_verifying_key_bytes)
            .map_err(|_| MeshError::InvalidVerifyingKey)?;
        if verifying_key.is_weak() {
            return Err(MeshError::InvalidVerifyingKey);
        }
        self.verify_and_decrypt_with_key(symmetric_key, &verifying_key, None)
    }

    /// Vérifie la trame contre une clé publique Ed25519 épinglée pour l'émetteur et applique
    /// un contrôle anti-rejeu monotone optionnel (`sequence_number > min_exclusive_seq`).
    pub fn verify_and_decrypt_with_key(
        &self,
        symmetric_key: &[u8; 32],
        expected_verifying_key: &VerifyingKey,
        min_exclusive_seq: Option<u64>,
    ) -> Result<Vec<u8>, MeshError> {
        if expected_verifying_key.is_weak()
            || expected_verifying_key.to_bytes() != self.sender_verifying_key_bytes
        {
            return Err(MeshError::InvalidVerifyingKey);
        }

        if let Some(last_seq) = min_exclusive_seq
            && self.sequence_number <= last_seq
        {
            return Err(MeshError::FrameExpired);
        }

        let sig_array: [u8; 64] = self
            .signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| MeshError::InvalidSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

        let header_aad = Self::canonical_header_aad(
            &self.frame_id,
            self.sequence_number,
            &self.sender_peer_id,
            self.target_peer_id.as_deref(),
            &self.sender_verifying_key_bytes,
            &self.nonce,
            self.timestamp_ms,
        );
        let msg_to_sign = self.canonical_signing_bytes(&header_aad);

        expected_verifying_key
            .verify_strict(&msg_to_sign, &signature)
            .map_err(|_| MeshError::InvalidSignature)?;

        let cipher = ChaCha20Poly1305::new_from_slice(symmetric_key)
            .map_err(|_| MeshError::DecryptionFailed)?;
        let nonce = Nonce::from_slice(&self.nonce);

        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: self.encrypted_payload.as_slice(),
                    aad: &header_aad,
                },
            )
            .map_err(|_| MeshError::DecryptionFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_local_guard_rejects_wan_and_accepts_link_local() {
        // WAN IPs and cloud metadata must be blocked fail-closed
        assert!(!LinkLocalGuard::is_link_local_safe("8.8.8.8:53"));
        assert!(!LinkLocalGuard::is_link_local_safe("1.1.1.1"));
        assert!(!LinkLocalGuard::is_link_local_safe("169.254.169.254"));
        assert!(!LinkLocalGuard::is_link_local_safe("169.254.169.254:80"));
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

        // Pinned verifying key + monotonic sequence check
        let vk = signing_key.verifying_key();
        assert!(
            frame
                .verify_and_decrypt_with_key(&sym_key, &vk, Some(0))
                .is_ok()
        );
        assert_eq!(
            frame.verify_and_decrypt_with_key(&sym_key, &vk, Some(1)),
            Err(MeshError::FrameExpired)
        );

        // Tampered payload fails
        let mut tampered = frame.clone();
        tampered.encrypted_payload[0] ^= 0xFF;
        assert!(tampered.verify_and_decrypt(&sym_key).is_err());

        // Tampered header fails both signature and AAD
        let mut tampered_hdr = frame;
        tampered_hdr.target_peer_id = Some("node-charlie".into());
        assert!(tampered_hdr.verify_and_decrypt(&sym_key).is_err());
    }
}
