//! Provenance cryptographique Ed25519 (alignée C2PA / manifeste signé).
//!
//! Chaque artefact généré par Hakille est accompagné :
//! 1. d'un manifeste JSON signé Ed25519 (`<artefact>.provenance.json`) ;
//! 2. pour les fichiers PNG/APNG, d'un chunk `iTXt` embarqué de mot-clé
//!    [`PROVENANCE_PNG_KEYWORD`] (`hakille.provenance`) contenant le manifeste
//!    canonique signé.
//!
//! La vérification recalcule le SHA-256 du fichier binaire, vérifie la signature
//! Ed25519 sur la charge utile canonique (hors champ signature), et contrôle la
//! cohérence du chunk PNG s'il est présent. Toute altération d'un seul octet
//! renvoie [`MediaError::ProvenanceInvalid`].

use crate::catalog::{hex_decode, hex_encode, sha256_file};
use crate::error::{MediaError, MediaResult};
use crate::types::{EngineKind, MediaDataExit, MediaModality};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version du schéma de provenance média Hakille.
pub const PROVENANCE_SCHEMA_VERSION: u32 = 1;

/// Mot-clé du chunk PNG `iTXt` embarquant la provenance signée.
pub const PROVENANCE_PNG_KEYWORD: &str = "hakille.provenance";

/// Charge utile signée (canonisée avant signature Ed25519).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProvenancePayload {
    pub schema_version: u32,
    pub artifact_id: String,
    pub modality: MediaModality,
    pub mime_type: String,
    pub content_sha256: String,
    pub bytes_len: u64,
    pub prompt_sha256: String,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub model_sha256: Option<String>,
    pub seed: u64,
    pub parent_artifacts: Vec<String>,
    /// Divulgation explicite et immuable d'origine synthétique.
    pub synthetic_ai_disclosure: bool,
    pub created_at: DateTime<Utc>,
    /// Identifiant de clé publique (empreinte hexadécimale courte du signataire).
    pub signer_public_key_hex: String,
}

impl MediaProvenancePayload {
    /// Sérialisation déterministe pour signature et vérification Ed25519.
    pub fn canonical_bytes(&self) -> MediaResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| MediaError::Encoding(e.to_string()))
    }
}

/// Manifeste de provenance complet signé Ed25519.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProvenance {
    pub payload: MediaProvenancePayload,
    pub signature_hex: String,
}

/// Résultat détaillé d'une vérification cryptographique de provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceVerificationReport {
    pub valid: bool,
    pub artifact_path: PathBuf,
    pub provenance_path: PathBuf,
    pub content_sha256: String,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub synthetic_ai_disclosure: bool,
    pub embedded_png_chunk_checked: bool,
    pub parent_count: usize,
    pub signer_public_key_hex: String,
}

/// Signe une charge utile de provenance avec la clé Ed25519 du démon Hakille.
pub fn sign_provenance(
    mut payload: MediaProvenancePayload,
    signing_key: &SigningKey,
) -> MediaResult<MediaProvenance> {
    let verifying_key = signing_key.verifying_key();
    payload.signer_public_key_hex = hex_encode(verifying_key.as_bytes());
    payload.synthetic_ai_disclosure = true;
    let bytes = payload.canonical_bytes()?;
    let signature: Signature = signing_key.sign(&bytes);
    Ok(MediaProvenance {
        payload,
        signature_hex: hex_encode(&signature.to_bytes()),
    })
}

/// Chemin standard du fichier sidecar `.provenance.json` pour un artefact.
pub fn provenance_sidecar_path(artifact_path: &Path) -> PathBuf {
    let mut os = artifact_path.as_os_str().to_os_string();
    os.push(".provenance.json");
    PathBuf::from(os)
}

/// Écrit de manière atomique le sidecar `.provenance.json` sur disque.
pub fn write_provenance_sidecar(
    artifact_path: &Path,
    provenance: &MediaProvenance,
) -> MediaResult<PathBuf> {
    let sidecar = provenance_sidecar_path(artifact_path);
    if let Some(parent) = sidecar.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(provenance)?;
    let tmp = sidecar.with_extension(format!("json.{}.tmp", std::process::id()));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &sidecar)?;
    Ok(sidecar)
}

/// Lit un sidecar `.provenance.json`.
pub fn read_provenance_sidecar(sidecar_path: &Path) -> MediaResult<MediaProvenance> {
    let raw = std::fs::read(sidecar_path).map_err(|e| {
        MediaError::ProvenanceInvalid(format!(
            "sidecar introuvable ou illisible ({}) : {e}",
            sidecar_path.display()
        ))
    })?;
    serde_json::from_slice(&raw).map_err(|e| {
        MediaError::ProvenanceInvalid(format!(
            "sidecar JSON invalide ({}) : {e}",
            sidecar_path.display()
        ))
    })
}

/// Vérifie la signature Ed25519 interne d'un manifeste [`MediaProvenance`].
///
/// Si `expected_key` est fourni, exige que la clé signataire corresponde
/// exactement à la clé épinglée.
pub fn verify_provenance_signature(
    provenance: &MediaProvenance,
    expected_key: Option<&VerifyingKey>,
) -> MediaResult<VerifyingKey> {
    if !provenance.payload.synthetic_ai_disclosure {
        return Err(MediaError::ProvenanceInvalid(
            "le marqueur obligatoire synthetic_ai_disclosure est absent ou faux".into(),
        ));
    }
    let pk_bytes = hex_decode(&provenance.payload.signer_public_key_hex).ok_or_else(|| {
        MediaError::ProvenanceInvalid("clé publique hexadécimale malformée".into())
    })?;
    let pk_arr: [u8; 32] = pk_bytes.try_into().map_err(|_| {
        MediaError::ProvenanceInvalid("longueur de clé publique != 32 octets".into())
    })?;
    let verifying_key = VerifyingKey::from_bytes(&pk_arr).map_err(|e| {
        MediaError::ProvenanceInvalid(format!("clé publique Ed25519 invalide : {e}"))
    })?;

    if let Some(pinned) = expected_key
        && verifying_key.as_bytes() != pinned.as_bytes()
    {
        return Err(MediaError::ProvenanceInvalid(
            "la clé publique du manifeste ne correspond pas à la clé épinglée".into(),
        ));
    }

    let sig_bytes = hex_decode(&provenance.signature_hex)
        .ok_or_else(|| MediaError::ProvenanceInvalid("signature hexadécimale malformée".into()))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| MediaError::ProvenanceInvalid("longueur de signature != 64 octets".into()))?;
    let signature = Signature::from_bytes(&sig_arr);

    let canonical = provenance.payload.canonical_bytes()?;
    verifying_key
        .verify(&canonical, &signature)
        .map_err(|e| MediaError::ProvenanceInvalid(format!("signature Ed25519 invalide : {e}")))?;
    Ok(verifying_key)
}

/// Extrait le chunk PNG `iTXt` de mot-clé `hakille.provenance` s'il est présent.
pub fn extract_png_provenance_chunk(png_bytes: &[u8]) -> MediaResult<Option<String>> {
    // Vérifie la signature PNG 8 octets.
    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if png_bytes.len() < 8 || png_bytes[..8] != PNG_SIG {
        return Ok(None);
    }
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let reader = decoder
        .read_info()
        .map_err(|e| MediaError::ProvenanceInvalid(format!("décodage PNG impossible : {e}")))?;
    let info = reader.info();
    for chunk in &info.utf8_text {
        if chunk.keyword == PROVENANCE_PNG_KEYWORD {
            let text = chunk.get_text().map_err(|e| {
                MediaError::ProvenanceInvalid(format!(
                    "chunk iTXt '{}' illisible : {e}",
                    PROVENANCE_PNG_KEYWORD
                ))
            })?;
            return Ok(Some(text));
        }
    }
    Ok(None)
}

/// Injecte un chunk PNG `iTXt` (`hakille.provenance`) dans un flux PNG existant
/// juste avant le chunk `IEND` sans altérer les données pixels.
///
/// Utile pour lier cryptographiquement le manifeste à un PNG/APNG déjà encodé,
/// tout en conservant le hachage du fichier final comme référence du sidecar.
pub fn inject_png_itxt_chunk(png_bytes: &[u8], keyword: &str, text: &str) -> MediaResult<Vec<u8>> {
    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if png_bytes.len() < 8 + 12 || png_bytes[..8] != PNG_SIG {
        return Err(MediaError::Encoding(
            "flux d'entrée non reconnu comme PNG valide".into(),
        ));
    }

    // Construit la charge utile du chunk iTXt :
    // keyword\0 + compression_flag(0) + compression_method(0) + lang_tag\0 + translated_kw\0 + text
    let mut data = Vec::with_capacity(keyword.len() + 5 + text.len());
    data.extend_from_slice(keyword.as_bytes());
    data.push(0); // NUL après keyword
    data.push(0); // compression flag = 0 (non compressé)
    data.push(0); // compression method = 0
    data.push(0); // language tag vide + NUL
    data.push(0); // translated keyword vide + NUL
    data.extend_from_slice(text.as_bytes());

    let chunk_type = *b"iTXt";
    let crc = crc32_png(&chunk_type, &data);

    let mut chunk_buf = Vec::with_capacity(12 + data.len());
    chunk_buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk_buf.extend_from_slice(&chunk_type);
    chunk_buf.extend_from_slice(&data);
    chunk_buf.extend_from_slice(&crc.to_be_bytes());

    // Localise le chunk IHDR et insère le chunk iTXt immédiatement après
    // (avant IDAT) afin que tout décodeur PNG standard (`read_info()`) lise
    // les métadonnées de provenance sans avoir à décompresser l'image.
    let mut pos = 8usize;
    let mut after_ihdr_pos = None;
    while pos + 12 <= png_bytes.len() {
        let len = u32::from_be_bytes([
            png_bytes[pos],
            png_bytes[pos + 1],
            png_bytes[pos + 2],
            png_bytes[pos + 3],
        ]) as usize;
        let ctype = &png_bytes[pos + 4..pos + 8];
        let next = pos.saturating_add(12).saturating_add(len);
        if next > png_bytes.len() {
            return Err(MediaError::Encoding("chunk PNG tronqué".into()));
        }
        if ctype == b"IHDR" {
            after_ihdr_pos = Some(next);
            break;
        }
        pos = next;
    }

    let insert_at = after_ihdr_pos
        .ok_or_else(|| MediaError::Encoding("chunk IHDR introuvable dans le PNG".into()))?;
    let mut out = Vec::with_capacity(png_bytes.len() + chunk_buf.len());
    out.extend_from_slice(&png_bytes[..insert_at]);
    out.extend_from_slice(&chunk_buf);
    out.extend_from_slice(&png_bytes[insert_at..]);
    Ok(out)
}

/// Vérifie intégralement un artefact sur disque contre son sidecar signé Ed25519
/// (et son chunk PNG embarqué le cas échéant).
pub fn verify_artifact_provenance(
    artifact_path: &Path,
    expected_key: Option<&VerifyingKey>,
) -> MediaResult<ProvenanceVerificationReport> {
    if !artifact_path.is_file() {
        return Err(MediaError::ProvenanceInvalid(format!(
            "artefact introuvable : {}",
            artifact_path.display()
        )));
    }
    let sidecar_path = provenance_sidecar_path(artifact_path);
    let provenance = read_provenance_sidecar(&sidecar_path)?;
    verify_provenance_signature(&provenance, expected_key)?;

    let actual_sha = sha256_file(artifact_path)?;
    if actual_sha != provenance.payload.content_sha256 {
        return Err(MediaError::ProvenanceInvalid(format!(
            "altération détectée : SHA-256 disque={} != manifeste={}",
            actual_sha, provenance.payload.content_sha256
        )));
    }

    let meta = std::fs::metadata(artifact_path)?;
    if meta.len() != provenance.payload.bytes_len {
        return Err(MediaError::ProvenanceInvalid(format!(
            "taille modifiée : disque={} != manifeste={}",
            meta.len(),
            provenance.payload.bytes_len
        )));
    }

    // Si PNG/APNG, vérifie aussi la présence/cohérence d'un éventuel chunk iTXt embarqué.
    let mut embedded_checked = false;
    let ext = artifact_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "png" | "apng") {
        let bytes = std::fs::read(artifact_path)?;
        if let Some(chunk_json) = extract_png_provenance_chunk(&bytes)? {
            let embedded: serde_json::Value = serde_json::from_str(&chunk_json).map_err(|e| {
                MediaError::ProvenanceInvalid(format!("chunk PNG iTXt non JSON : {e}"))
            })?;
            if embedded.get("artifact_id").and_then(|v| v.as_str())
                != Some(provenance.payload.artifact_id.as_str())
            {
                return Err(MediaError::ProvenanceInvalid(
                    "incohérence entre artifact_id du chunk PNG et du sidecar signé".into(),
                ));
            }
            embedded_checked = true;
        }
    }

    Ok(ProvenanceVerificationReport {
        valid: true,
        artifact_path: artifact_path.to_path_buf(),
        provenance_path: sidecar_path,
        content_sha256: actual_sha,
        engine_id: provenance.payload.engine_id,
        engine_kind: provenance.payload.engine_kind,
        synthetic_ai_disclosure: provenance.payload.synthetic_ai_disclosure,
        embedded_png_chunk_checked: embedded_checked,
        parent_count: provenance.payload.parent_artifacts.len(),
        signer_public_key_hex: provenance.payload.signer_public_key_hex,
    })
}

/// Calcule le CRC-32 IEEE (polynôme 0xEDB88320) conforme à la spécification PNG.
fn crc32_png(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
