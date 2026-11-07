//! Self-Sovereign Identity (SSI): DIDs (`did:key`, `did:peer`, `did:jwk`)
//! and W3C Verifiable Credentials (VC v2.0).

use ed25519_dalek::{Signature as EdSignature, Signer, SigningKey, Verifier, VerifyingKey};
use hakille_core::HakilleError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// Base58BTC alphabet
const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn base58_encode(bytes: &[u8]) -> String {
    let mut zeros = 0;
    while zeros < bytes.len() && bytes[zeros] == 0 {
        zeros += 1;
    }

    let size = bytes.len() * 138 / 100 + 1;
    let mut b58 = vec![0u8; size];

    for &b in bytes.iter() {
        let mut carry = b as u32;
        for (i, it) in b58.iter_mut().rev().enumerate() {
            if carry == 0 && i >= size {
                break;
            }
            carry += (*it as u32) << 8;
            *it = (carry % 58) as u8;
            carry /= 58;
        }
    }

    let mut it = 0;
    while it < b58.len() && b58[it] == 0 {
        it += 1;
    }

    let mut str = String::new();
    for _ in 0..zeros {
        str.push('1');
    }
    for &b in &b58[it..] {
        str.push(BASE58_ALPHABET[b as usize] as char);
    }
    str
}

fn base58_decode(str: &str) -> Result<Vec<u8>, HakilleError> {
    let mut zeros = 0;
    while zeros < str.len() && str.as_bytes()[zeros] == b'1' {
        zeros += 1;
    }

    let mut b256 = vec![0u8; str.len() * 733 / 1000 + 1];
    for c in str.chars() {
        let val = match BASE58_ALPHABET.iter().position(|&x| x == c as u8) {
            Some(v) => v as u32,
            None => {
                return Err(HakilleError::CryptoError(format!(
                    "Invalid Base58 character: {c}"
                )));
            }
        };

        let mut carry = val;
        for it in b256.iter_mut().rev() {
            carry += (*it as u32) * 58;
            *it = (carry % 256) as u8;
            carry /= 256;
        }
    }

    let mut it = 0;
    while it < b256.len() && b256[it] == 0 {
        it += 1;
    }

    let mut res = vec![0u8; zeros];
    res.extend_from_slice(&b256[it..]);
    Ok(res)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Did(pub String);

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Did {
    pub fn new(val: impl Into<String>) -> Self {
        Self(val.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn method(&self) -> Option<&str> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() >= 2 && parts[0] == "did" {
            Some(parts[1])
        } else {
            None
        }
    }
}

pub struct DidKey;

impl DidKey {
    /// Generate a W3C `did:key` identifier from an Ed25519 public key.
    /// Uses Multicodec prefix `0xed01` (Ed25519 public key) and Multibase Base58BTC prefix `'z'`.
    pub fn from_ed25519_bytes(public_key: &[u8; 32]) -> Did {
        let mut multicodec = Vec::with_capacity(34);
        multicodec.push(0xed);
        multicodec.push(0x01);
        multicodec.extend_from_slice(public_key);
        let b58 = base58_encode(&multicodec);
        Did(format!("did:key:z{}", b58))
    }

    /// Extract the 32-byte Ed25519 public key from a `did:key:z...` string.
    pub fn extract_ed25519_bytes(did: &Did) -> Result<[u8; 32], HakilleError> {
        let s = did.as_str();
        if !s.starts_with("did:key:z") {
            return Err(HakilleError::CryptoError(
                "DID is not a valid did:key with Base58BTC encoding".to_string(),
            ));
        }

        let b58_payload = &s[9..];
        let bytes = base58_decode(b58_payload)?;
        if bytes.len() != 34 || bytes[0] != 0xed || bytes[1] != 0x01 {
            return Err(HakilleError::CryptoError(
                "DID payload does not contain Ed25519 multicodec (0xed01)".to_string(),
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[2..]);
        Ok(key)
    }
}

pub struct DidPeer;

impl DidPeer {
    /// Format a `did:peer:2` peer DID encoding signing and agreement keys
    pub fn create_peer2(signing_key: &[u8; 32], agreement_key: &[u8; 32]) -> Did {
        let sig_did = DidKey::from_ed25519_bytes(signing_key);
        let sig_part = sig_did.0.strip_prefix("did:key:").unwrap();

        let mut multicodec_x = Vec::with_capacity(34);
        multicodec_x.push(0xec); // X25519 multicodec
        multicodec_x.push(0x01);
        multicodec_x.extend_from_slice(agreement_key);
        let enc_part = base58_encode(&multicodec_x);

        Did(format!("did:peer:2.V{}.Ez{}", sig_part, enc_part))
    }
}

pub struct DidJwk;

impl DidJwk {
    /// Generate a W3C `did:jwk` identifier from an Ed25519 public key (RFC 8037 OKP Ed25519)
    pub fn from_ed25519_bytes(public_key: &[u8; 32]) -> Did {
        Did(hakille_core::crypto::ssi::format_did_jwk(public_key))
    }

    /// Extract the 32-byte Ed25519 public key from a `did:jwk:...` identifier
    pub fn extract_ed25519_bytes(did: &Did) -> Result<[u8; 32], HakilleError> {
        hakille_core::crypto::ssi::parse_did_jwk(did.as_str())
            .map_err(|e| HakilleError::CryptoError(e.to_string()))
    }

    pub fn from_bytes(public_key: &[u8; 32]) -> String {
        hakille_core::crypto::ssi::format_did_jwk(public_key)
    }

    pub fn to_bytes(did: &str) -> Result<[u8; 32], HakilleError> {
        hakille_core::crypto::ssi::parse_did_jwk(did)
            .map_err(|e| HakilleError::CryptoError(e.to_string()))
    }
}

// =============================================================================
// W3C Verifiable Credentials (v2.0)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VcProof {
    pub proof_type: String, // "Ed25519Signature2020"
    pub created: String,
    pub verification_method: String,
    pub proof_value: String, // Base58 signature
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiableCredential {
    pub id: String,
    pub vc_type: Vec<String>,
    pub issuer: Did,
    pub valid_from: i64,
    pub valid_until: i64,
    pub credential_subject: serde_json::Value,
    pub proof: VcProof,
}

fn canonicalize_json_value(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize_json_value(v));
            }
            serde_json::to_value(sorted).unwrap()
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json_value).collect())
        }
        other => other.clone(),
    }
}

impl VerifiableCredential {
    /// Compute canonical claims digest for signing and verification.
    /// Employs RFC 8785 JSON Canonicalization Scheme and explicit field length framing
    /// to guarantee collision-free hashing.
    pub fn compute_claims_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"hakille:vc:claims:v1:");

        // Explicit 4-byte framing for id
        hasher.update((self.id.len() as u32).to_le_bytes());
        hasher.update(self.id.as_bytes());

        // Explicit 4-byte framing for vc_type count and each type element
        hasher.update((self.vc_type.len() as u32).to_le_bytes());
        for t in &self.vc_type {
            hasher.update((t.len() as u32).to_le_bytes());
            hasher.update(t.as_bytes());
        }

        // Explicit 4-byte framing for issuer DID
        let issuer_bytes = self.issuer.as_str().as_bytes();
        hasher.update((issuer_bytes.len() as u32).to_le_bytes());
        hasher.update(issuer_bytes);

        hasher.update(self.valid_from.to_le_bytes());
        hasher.update(self.valid_until.to_le_bytes());

        // RFC 8785 JSON Canonicalization Scheme for credential_subject
        let canonical_subject = canonicalize_json_value(&self.credential_subject);
        let subject_canonical = serde_json::to_vec(&canonical_subject).unwrap_or_default();
        hasher.update((subject_canonical.len() as u32).to_le_bytes());
        hasher.update(&subject_canonical);

        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        out
    }

    pub fn issue(
        id: impl Into<String>,
        vc_type: Vec<String>,
        issuer_keypair: &SigningKey,
        valid_from: i64,
        valid_until: i64,
        credential_subject: serde_json::Value,
    ) -> Self {
        let issuer_vk = issuer_keypair.verifying_key();
        let issuer_did = DidKey::from_ed25519_bytes(issuer_vk.as_bytes());

        let mut vc = Self {
            id: id.into(),
            vc_type,
            issuer: issuer_did.clone(),
            valid_from,
            valid_until,
            credential_subject,
            proof: VcProof {
                proof_type: "Ed25519Signature2020".to_string(),
                created: chrono::Utc::now().to_rfc3339(),
                verification_method: format!("{}#key-1", issuer_did),
                proof_value: String::new(),
            },
        };

        let digest = vc.compute_claims_hash();
        let sig: EdSignature = issuer_keypair.sign(&digest);
        vc.proof.proof_value = base58_encode(&sig.to_bytes());
        vc
    }

    pub fn verify(&self, current_timestamp: i64) -> Result<(), HakilleError> {
        // Expiry checks
        if current_timestamp < self.valid_from {
            return Err(HakilleError::CapabilityDenied(
                "Credential not yet valid".to_string(),
            ));
        }
        if current_timestamp > self.valid_until {
            return Err(HakilleError::CapabilityExpired);
        }

        // Issuer public key extraction (supports did:key and did:jwk)
        let issuer_pk = match self.issuer.method() {
            Some("key") => DidKey::extract_ed25519_bytes(&self.issuer)?,
            Some("jwk") => DidJwk::extract_ed25519_bytes(&self.issuer)?,
            _ => return Err(HakilleError::CryptoError(format!(
                "Unsupported or missing DID method for issuer: {}",
                self.issuer
            ))),
        };
        let vk = VerifyingKey::from_bytes(&issuer_pk)
            .map_err(|e| HakilleError::CryptoError(format!("Invalid issuer verifying key: {e}")))?;

        // Verify signature over digest
        let sig_bytes = base58_decode(&self.proof.proof_value)?;
        if sig_bytes.len() != 64 {
            return Err(HakilleError::CryptoError(
                "Invalid signature length".to_string(),
            ));
        }
        let sig = EdSignature::from_slice(&sig_bytes)
            .map_err(|e| HakilleError::CryptoError(format!("Malformed Ed25519 signature: {e}")))?;

        let digest = self.compute_claims_hash();
        vk.verify(&digest, &sig)
            .map_err(|_| HakilleError::AuthenticationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_did_jwk_roundtrip() {
        let keypair = SigningKey::generate(&mut OsRng);
        let pk = keypair.verifying_key().to_bytes();

        let did = DidJwk::from_ed25519_bytes(&pk);
        assert!(did.as_str().starts_with("did:jwk:"));
        assert_eq!(did.method(), Some("jwk"));

        let extracted_pk = DidJwk::extract_ed25519_bytes(&did).unwrap();
        assert_eq!(extracted_pk, pk);

        let did_str = DidJwk::from_bytes(&pk);
        let extracted_from_str = DidJwk::to_bytes(&did_str).unwrap();
        assert_eq!(extracted_from_str, pk);
    }

    #[test]
    fn test_vc_with_did_jwk_issuer() {
        let keypair = SigningKey::generate(&mut OsRng);
        let pk = keypair.verifying_key().to_bytes();
        let jwk_did = DidJwk::from_ed25519_bytes(&pk);

        let now = chrono::Utc::now().timestamp();
        let mut vc = VerifiableCredential {
            id: "urn:uuid:jwk-cred-1".to_string(),
            vc_type: vec!["VerifiableCredential".to_string()],
            issuer: jwk_did.clone(),
            valid_from: now - 10,
            valid_until: now + 3600,
            credential_subject: serde_json::json!({
                "sub": "user:jwk-holder",
                "role": "Auditor"
            }),
            proof: VcProof {
                proof_type: "Ed25519Signature2020".to_string(),
                created: chrono::Utc::now().to_rfc3339(),
                verification_method: format!("{}#key-1", jwk_did),
                proof_value: String::new(),
            },
        };

        let digest = vc.compute_claims_hash();
        let sig: EdSignature = keypair.sign(&digest);
        vc.proof.proof_value = base58_encode(&sig.to_bytes());

        assert!(vc.verify(now).is_ok());
    }

    #[test]
    fn test_vc_claims_hash_rfc8785_jcs_key_invariance() {
        let keypair = SigningKey::generate(&mut OsRng);
        let now = chrono::Utc::now().timestamp();

        let obj1 = serde_json::json!({
            "zebra": "stripes",
            "alpha": "first",
            "nested": {
                "z": 100,
                "a": 1
            }
        });

        let obj2 = serde_json::json!({
            "alpha": "first",
            "nested": {
                "a": 1,
                "z": 100
            },
            "zebra": "stripes"
        });

        let vc1 = VerifiableCredential::issue(
            "urn:id:jcs",
            vec!["Type1".into()],
            &keypair,
            now,
            now + 1000,
            obj1,
        );

        let vc2 = VerifiableCredential::issue(
            "urn:id:jcs",
            vec!["Type1".into()],
            &keypair,
            now,
            now + 1000,
            obj2,
        );

        assert_eq!(
            vc1.compute_claims_hash(),
            vc2.compute_claims_hash(),
            "JCS key ordering must be invariant to JSON map insertion order"
        );
    }

    #[test]
    fn test_vc_claims_hash_field_framing_collision_resistance() {
        let keypair = SigningKey::generate(&mut OsRng);
        let now = chrono::Utc::now().timestamp();
        let claims = serde_json::json!({ "status": "active" });

        let vc_a = VerifiableCredential::issue(
            "urn:id:1",
            vec!["Alpha".into(), "BetaGamma".into()],
            &keypair,
            now,
            now + 1000,
            claims.clone(),
        );

        let vc_b = VerifiableCredential::issue(
            "urn:id:1",
            vec!["AlphaBeta".into(), "Gamma".into()],
            &keypair,
            now,
            now + 1000,
            claims,
        );

        assert_ne!(
            vc_a.compute_claims_hash(),
            vc_b.compute_claims_hash(),
            "Length prefixes prevent delimiter concatenation collisions"
        );
    }
}

