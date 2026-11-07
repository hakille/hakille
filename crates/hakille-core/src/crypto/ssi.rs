use crate::crypto::error::CryptoError;
use ed25519_dalek::{Signature as EdSignature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// =============================================================================
// Base58BTC and Base64URL Primitives
// =============================================================================

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub fn base58_encode(bytes: &[u8]) -> String {
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

pub fn base58_decode(str: &str) -> Result<Vec<u8>, CryptoError> {
    let mut zeros = 0;
    while zeros < str.len() && str.as_bytes()[zeros] == b'1' {
        zeros += 1;
    }

    let mut b256 = vec![0u8; str.len() * 733 / 1000 + 1];
    for c in str.chars() {
        let val = match BASE58_ALPHABET.iter().position(|&x| x == c as u8) {
            Some(v) => v as u32,
            None => {
                return Err(CryptoError::InvalidDid(format!(
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

const BASE64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64url_encode(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        let idx0 = (b0 >> 2) as usize;
        let idx1 = (((b0 & 0x03) << 4) | (b1 >> 4)) as usize;
        let idx2 = (((b1 & 0x0f) << 2) | (b2 >> 6)) as usize;
        let idx3 = (b2 & 0x3f) as usize;

        result.push(BASE64URL_CHARS[idx0] as char);
        result.push(BASE64URL_CHARS[idx1] as char);

        if i + 1 < bytes.len() {
            result.push(BASE64URL_CHARS[idx2] as char);
        }
        if i + 2 < bytes.len() {
            result.push(BASE64URL_CHARS[idx3] as char);
        }

        i += 3;
    }
    result
}

pub fn base64url_decode(s: &str) -> Result<Vec<u8>, CryptoError> {
    fn decode_char(c: u8) -> Result<u8, CryptoError> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'-' => Ok(62),
            b'_' => Ok(63),
            _ => Err(CryptoError::InvalidDid(format!(
                "Invalid base64url character: {}",
                c as char
            ))),
        }
    }

    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rem = bytes.len() - i;
        if rem == 1 {
            return Err(CryptoError::InvalidDid(
                "Invalid base64url length".to_string(),
            ));
        }

        let c0 = decode_char(bytes[i])?;
        let c1 = decode_char(bytes[i + 1])?;
        out.push((c0 << 2) | (c1 >> 4));

        if rem >= 3 {
            let c2 = decode_char(bytes[i + 2])?;
            out.push(((c1 & 0x0f) << 4) | (c2 >> 2));

            if rem >= 4 {
                let c3 = decode_char(bytes[i + 3])?;
                out.push(((c2 & 0x03) << 6) | c3);
                i += 4;
            } else {
                i += 3;
            }
        } else {
            i += 2;
        }
    }

    Ok(out)
}

// =============================================================================
// F07: did:key Implementation
// =============================================================================

/// Format an Ed25519 32-byte public key as a W3C did:key identifier.
/// Uses Multicodec prefix `0xed01` and Multibase Base58BTC prefix `'z'`.
pub fn format_did_key(public_key: &[u8; 32]) -> String {
    let mut multicodec = Vec::with_capacity(34);
    multicodec.push(0xed);
    multicodec.push(0x01);
    multicodec.extend_from_slice(public_key);
    let b58 = base58_encode(&multicodec);
    format!("did:key:z{b58}")
}

/// Parse an Ed25519 public key from a `did:key:z6Mk...` identifier
pub fn parse_did_key(did: &str) -> Result<[u8; 32], CryptoError> {
    if !did.starts_with("did:key:z") {
        return Err(CryptoError::InvalidDid(
            "DID is not a valid did:key with multibase Base58BTC prefix".to_string(),
        ));
    }

    let b58_payload = &did[9..];
    let decoded = base58_decode(b58_payload)?;
    if decoded.len() != 34 || decoded[0] != 0xed || decoded[1] != 0x01 {
        return Err(CryptoError::InvalidDid(
            "DID payload does not contain Ed25519 multicodec header 0xed01".to_string(),
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded[2..]);
    Ok(key)
}

pub struct DidKey;

impl DidKey {
    pub fn from_bytes(public_key: &[u8; 32]) -> String {
        format_did_key(public_key)
    }

    pub fn to_bytes(did: &str) -> Result<[u8; 32], CryptoError> {
        parse_did_key(did)
    }
}

// =============================================================================
// F07: did:jwk Implementation (RFC 8037 OKP Ed25519)
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Ed25519Jwk {
    kty: String,
    crv: String,
    x: String,
}

/// Format an Ed25519 public key as a did:jwk identifier
pub fn format_did_jwk(public_key: &[u8; 32]) -> String {
    let x = base64url_encode(public_key);
    let jwk = Ed25519Jwk {
        kty: "OKP".to_string(),
        crv: "Ed25519".to_string(),
        x,
    };
    let json_bytes = serde_json::to_vec(&jwk).expect("JWK serialization infallible");
    let encoded_jwk = base64url_encode(&json_bytes);
    format!("did:jwk:{encoded_jwk}")
}

/// Parse an Ed25519 public key from a did:jwk identifier
pub fn parse_did_jwk(did: &str) -> Result<[u8; 32], CryptoError> {
    if !did.starts_with("did:jwk:") {
        return Err(CryptoError::InvalidDid(
            "DID does not use jwk method".to_string(),
        ));
    }

    let b64_payload = &did[8..];
    let json_bytes = base64url_decode(b64_payload)?;
    let jwk: Ed25519Jwk = serde_json::from_slice(&json_bytes)
        .map_err(|e| CryptoError::InvalidDid(format!("Malformed JWK JSON: {e}")))?;

    if jwk.kty != "OKP" || jwk.crv != "Ed25519" {
        return Err(CryptoError::InvalidDid(
            "JWK is not an OKP Ed25519 key".to_string(),
        ));
    }

    let key_bytes = base64url_decode(&jwk.x)?;
    if key_bytes.len() != 32 {
        return Err(CryptoError::InvalidDid(
            "JWK x coordinate is not 32 bytes".to_string(),
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

pub struct DidJwk;

impl DidJwk {
    pub fn from_bytes(public_key: &[u8; 32]) -> String {
        format_did_jwk(public_key)
    }

    pub fn to_bytes(did: &str) -> Result<[u8; 32], CryptoError> {
        parse_did_jwk(did)
    }
}

// =============================================================================
// F07: W3C Verifiable Credentials (v2.0)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialProof {
    #[serde(rename = "type")]
    pub proof_type: String, // "Ed25519Signature2020"
    pub created: String,
    pub verification_method: String,
    pub proof_purpose: String,
    pub proof_value: String, // Base58BTC signature
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifiableCredential {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub credential_type: Vec<String>,
    pub issuer: String,     // did:key or did:jwk
    pub valid_from: String, // ISO 8601 UTC
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    pub credential_subject: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<CredentialProof>,
}

impl VerifiableCredential {
    /// Compute canonical claims digest for signing and verification
    pub fn compute_digest(&self) -> Result<[u8; 32], CryptoError> {
        // Clone with proof omitted for canonical signature verification
        let mut canonical = self.clone();
        canonical.proof = None;

        let canonical_json = serde_json::to_vec(&canonical)
            .map_err(|e| CryptoError::SerializationError(e.to_string()))?;

        let digest = Sha256::digest(&canonical_json);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Ok(out)
    }

    /// Sign and attach an Ed25519 cryptographic proof to the credential
    pub fn issue(&mut self, signing_key: &SigningKey) -> Result<(), CryptoError> {
        let verifying_key = signing_key.verifying_key();
        let issuer_did = format_did_key(verifying_key.as_bytes());

        self.issuer = issuer_did.clone();
        self.proof = None;

        let digest = self.compute_digest()?;
        let sig: EdSignature = signing_key.sign(&digest);
        let proof_val = base58_encode(&sig.to_bytes());

        self.proof = Some(CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            verification_method: format!("{issuer_did}#key-1"),
            proof_purpose: "assertionMethod".to_string(),
            proof_value: proof_val,
        });

        Ok(())
    }

    /// Verify the Ed25519 cryptographic proof on this credential
    pub fn verify(&self) -> Result<bool, CryptoError> {
        let proof = match &self.proof {
            Some(p) => p,
            None => return Err(CryptoError::InvalidCredential("Missing proof".to_string())),
        };

        // Extract issuer public key (supports both did:key and did:jwk)
        let pub_key_bytes = if self.issuer.starts_with("did:key:") {
            parse_did_key(&self.issuer)?
        } else if self.issuer.starts_with("did:jwk:") {
            parse_did_jwk(&self.issuer)?
        } else {
            return Err(CryptoError::InvalidDid(format!(
                "Unsupported issuer DID method: {}",
                self.issuer
            )));
        };

        let vk = VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|e| CryptoError::SignatureVerificationFailed(e.to_string()))?;

        let sig_bytes = base58_decode(&proof.proof_value)?;
        if sig_bytes.len() != 64 {
            return Err(CryptoError::SignatureVerificationFailed(
                "Invalid Ed25519 signature length in proof".to_string(),
            ));
        }

        let sig = EdSignature::from_slice(&sig_bytes)
            .map_err(|e| CryptoError::SignatureVerificationFailed(e.to_string()))?;

        let digest = self.compute_digest()?;
        vk.verify(&digest, &sig)
            .map(|_| true)
            .map_err(|e| CryptoError::SignatureVerificationFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_did_key_roundtrip() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let vk = signing_key.verifying_key();

        let did = format_did_key(vk.as_bytes());
        assert!(did.starts_with("did:key:z6Mk"));

        let extracted_bytes = parse_did_key(&did).unwrap();
        assert_eq!(&extracted_bytes, vk.as_bytes());
    }

    #[test]
    fn test_did_jwk_roundtrip() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let vk = signing_key.verifying_key();

        let did = format_did_jwk(vk.as_bytes());
        assert!(did.starts_with("did:jwk:"));

        let extracted_bytes = parse_did_jwk(&did).unwrap();
        assert_eq!(&extracted_bytes, vk.as_bytes());
    }

    #[test]
    fn test_verifiable_credential_issuance_and_verification() {
        let mut csprng = OsRng;
        let issuer_key = SigningKey::generate(&mut csprng);

        let mut vc = VerifiableCredential {
            context: vec![
                "https://www.w3.org/ns/credentials/v2".to_string(),
                "https://schema.hakille.org/identity/v1".to_string(),
            ],
            id: "urn:uuid:7f3b890a-1122-4455-8899-aabbccddeeff".to_string(),
            credential_type: vec![
                "VerifiableCredential".to_string(),
                "SovereignAgentPassport".to_string(),
            ],
            issuer: String::new(),
            valid_from: "2026-01-01T00:00:00Z".to_string(),
            valid_until: Some("2027-01-01T00:00:00Z".to_string()),
            credential_subject: serde_json::json!({
                "agent_id": "agent:assistant-01",
                "tier": "enterprise",
                "max_delegation_depth": 4
            }),
            proof: None,
        };

        vc.issue(&issuer_key).unwrap();
        assert!(vc.proof.is_some());
        assert!(vc.verify().unwrap());

        // Tampering with subject claims must invalidate proof
        vc.credential_subject = serde_json::json!({
            "agent_id": "agent:assistant-01",
            "tier": "enterprise",
            "max_delegation_depth": 99 // Tampered escalation!
        });
        assert!(vc.verify().is_err());
    }
}
