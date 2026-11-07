use hakille_core::{HakilleError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub const GGUF_MAGIC: u32 = 0x46554747; // "GGUF" in little endian: 0x47, 0x47, 0x55, 0x46

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufHeader {
    pub magic: u32,
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_kv_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufTensorInfo {
    pub name: String,
    pub dimensions: Vec<u64>,
    pub tensor_type: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GgufValue {
    Uint8(u8),
    Int8(i8),
    Uint16(u16),
    Int16(i16),
    Uint32(u32),
    Int32(i32),
    Float32(f32),
    Bool(bool),
    String(String),
    Array(Vec<GgufValue>),
    LargeArray { element_type: u32, len: u64 },
    Uint64(u64),
    Int64(i64),
    Float64(f64),
}

impl GgufValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Uint8(v) => Some(*v as u64),
            Self::Uint16(v) => Some(*v as u64),
            Self::Uint32(v) => Some(*v as u64),
            Self::Uint64(v) => Some(*v),
            Self::Int8(v) if *v >= 0 => Some(*v as u64),
            Self::Int16(v) if *v >= 0 => Some(*v as u64),
            Self::Int32(v) if *v >= 0 => Some(*v as u64),
            Self::Int64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int8(v) => Some(*v as i64),
            Self::Int16(v) => Some(*v as i64),
            Self::Int32(v) => Some(*v as i64),
            Self::Int64(v) => Some(*v),
            Self::Uint8(v) => Some(*v as i64),
            Self::Uint16(v) => Some(*v as i64),
            Self::Uint32(v) => Some(*v as i64),
            Self::Uint64(v) => i64::try_from(*v).ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float32(v) => Some(*v as f64),
            Self::Float64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Self::Uint8(v) => v.to_string(),
            Self::Int8(v) => v.to_string(),
            Self::Uint16(v) => v.to_string(),
            Self::Int16(v) => v.to_string(),
            Self::Uint32(v) => v.to_string(),
            Self::Int32(v) => v.to_string(),
            Self::Float32(v) => v.to_string(),
            Self::Bool(v) => v.to_string(),
            Self::String(s) => s.clone(),
            Self::Array(arr) => format!("[array of {} elements]", arr.len()),
            Self::LargeArray { element_type, len } => {
                format!("[large array: type={}, len={}]", element_type, len)
            }
            Self::Uint64(v) => v.to_string(),
            Self::Int64(v) => v.to_string(),
            Self::Float64(v) => v.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModel {
    pub header: GgufHeader,
    pub metadata: HashMap<String, String>,
    pub metadata_raw: HashMap<String, GgufValue>,
    pub tensors: HashMap<String, GgufTensorInfo>,
    pub estimated_params: u64,
}

impl GgufModel {
    /// Read and parse a real GGUF file from disk streamingly with minimal RAM footprint.
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| {
            HakilleError::InferenceError(format!(
                "Failed to open GGUF file {}: {}",
                path.display(),
                e
            ))
        })?;
        let file_len = file
            .metadata()
            .map_err(|e| HakilleError::InferenceError(format!("Taille GGUF : {e}")))?
            .len();
        let mut reader = BufReader::new(file);

        // 1. Read header (24 bytes)
        let mut header_buf = [0u8; 24];
        reader.read_exact(&mut header_buf).map_err(|e| {
            HakilleError::InferenceError(format!(
                "Failed to read GGUF header from {}: {}",
                path.display(),
                e
            ))
        })?;

        let header = GgufRuntime::parse_header(&header_buf)?;
        if header.tensor_count > 1_000_000
            || header.metadata_kv_count > 100_000
            || header.tensor_count > file_len / 24
            || header.metadata_kv_count > file_len / 12
        {
            return Err(HakilleError::InferenceError(
                "Comptages GGUF incohérents avec la taille du fichier".into(),
            ));
        }

        // 2. Read metadata KV pairs
        let mut metadata = HashMap::new();
        let mut metadata_raw = HashMap::new();

        for _ in 0..header.metadata_kv_count {
            let key = Self::read_string(&mut reader)?;
            let val_type = Self::read_u32(&mut reader)?;
            let val = Self::read_value(&mut reader, val_type, file_len)?;
            if metadata.contains_key(&key) {
                return Err(HakilleError::InferenceError(format!(
                    "Métadonnée GGUF dupliquée : {key}"
                )));
            }
            metadata.insert(key.clone(), val.to_display_string());
            metadata_raw.insert(key, val);
            if reader
                .stream_position()
                .map_err(|e| HakilleError::InferenceError(e.to_string()))?
                > 64 * 1024 * 1024
            {
                return Err(HakilleError::InferenceError(
                    "Métadonnées GGUF supérieures à 64 Mio".into(),
                ));
            }
        }

        // 3. Read tensor info headers
        let mut tensors = HashMap::new();
        let mut estimated_params = 0u64;

        for _ in 0..header.tensor_count {
            let name = Self::read_string(&mut reader)?;
            let n_dims = Self::read_u32(&mut reader)?;
            if !(1..=4).contains(&n_dims) {
                return Err(HakilleError::InferenceError(format!(
                    "Nombre de dimensions GGUF invalide : {n_dims}"
                )));
            }
            let mut dimensions = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                let dimension = Self::read_u64(&mut reader)?;
                if dimension == 0 {
                    return Err(HakilleError::InferenceError("Dimension GGUF nulle".into()));
                }
                dimensions.push(dimension);
            }
            let tensor_type = Self::read_u32(&mut reader)?;
            let offset = Self::read_u64(&mut reader)?;

            // Accumulate parameter count for weight matrices
            if !dimensions.is_empty() {
                let count = dimensions
                    .iter()
                    .try_fold(1u64, |count, dim| count.checked_mul(*dim))
                    .ok_or_else(|| {
                        HakilleError::InferenceError("Produit des dimensions GGUF débordant".into())
                    })?;
                if !name.ends_with(".bias") && !name.contains("rotary") {
                    estimated_params = estimated_params.checked_add(count).ok_or_else(|| {
                        HakilleError::InferenceError("Nombre de paramètres GGUF débordant".into())
                    })?;
                }
            }

            if tensors.contains_key(&name) {
                return Err(HakilleError::InferenceError(format!(
                    "Tenseur GGUF dupliqué : {name}"
                )));
            }
            if offset >= file_len {
                return Err(HakilleError::InferenceError(format!(
                    "Offset GGUF hors fichier : {name}"
                )));
            }
            tensors.insert(
                name.clone(),
                GgufTensorInfo {
                    name,
                    dimensions,
                    tensor_type,
                    offset,
                },
            );
        }

        Ok(Self {
            header,
            metadata,
            metadata_raw,
            tensors,
            estimated_params,
        })
    }

    pub fn architecture(&self) -> Option<&str> {
        self.metadata_raw
            .get("general.architecture")
            .and_then(|v| v.as_str())
            .or_else(|| {
                self.metadata
                    .get("general.architecture")
                    .map(|s| s.as_str())
            })
    }

    pub fn name(&self) -> Option<&str> {
        self.metadata_raw
            .get("general.name")
            .and_then(|v| v.as_str())
            .or_else(|| self.metadata.get("general.name").map(|s| s.as_str()))
    }

    pub fn context_length(&self) -> Option<u64> {
        let arch = self.architecture()?;
        let key = format!("{}.context_length", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok()))
    }

    pub fn embedding_length(&self) -> Option<u64> {
        let arch = self.architecture()?;
        let key = format!("{}.embedding_length", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok()))
    }

    pub fn block_count(&self) -> Option<u64> {
        let arch = self.architecture()?;
        let key = format!("{}.block_count", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok()))
    }

    pub fn tensor_count(&self) -> usize {
        self.tensors.len()
    }

    pub fn find_tensor(&self, name_prefix: &str) -> Option<&GgufTensorInfo> {
        self.tensors
            .iter()
            .find(|(k, _)| k.starts_with(name_prefix))
            .map(|(_, v)| v)
    }

    fn read_u8<R: Read>(reader: &mut R) -> Result<u8> {
        let mut buf = [0u8; 1];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read u8 error: {}", e)))?;
        Ok(buf[0])
    }

    fn read_u16<R: Read>(reader: &mut R) -> Result<u16> {
        let mut buf = [0u8; 2];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read u16 error: {}", e)))?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_i16<R: Read>(reader: &mut R) -> Result<i16> {
        let mut buf = [0u8; 2];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read i16 error: {}", e)))?;
        Ok(i16::from_le_bytes(buf))
    }

    fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
        let mut buf = [0u8; 4];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read u32 error: {}", e)))?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_i32<R: Read>(reader: &mut R) -> Result<i32> {
        let mut buf = [0u8; 4];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read i32 error: {}", e)))?;
        Ok(i32::from_le_bytes(buf))
    }

    fn read_f32<R: Read>(reader: &mut R) -> Result<f32> {
        let mut buf = [0u8; 4];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read f32 error: {}", e)))?;
        Ok(f32::from_le_bytes(buf))
    }

    fn read_u64<R: Read>(reader: &mut R) -> Result<u64> {
        let mut buf = [0u8; 8];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read u64 error: {}", e)))?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_i64<R: Read>(reader: &mut R) -> Result<i64> {
        let mut buf = [0u8; 8];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read i64 error: {}", e)))?;
        Ok(i64::from_le_bytes(buf))
    }

    fn read_f64<R: Read>(reader: &mut R) -> Result<f64> {
        let mut buf = [0u8; 8];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read f64 error: {}", e)))?;
        Ok(f64::from_le_bytes(buf))
    }

    fn read_string<R: Read>(reader: &mut R) -> Result<String> {
        let len = usize::try_from(Self::read_u64(reader)?).map_err(|_| {
            HakilleError::InferenceError("Longueur de chaîne GGUF débordante".into())
        })?;
        if len > 16 * 1024 * 1024 {
            return Err(HakilleError::InferenceError(format!(
                "String length {} exceeds safety threshold",
                len
            )));
        }
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .map_err(|e| HakilleError::InferenceError(format!("Read string bytes error: {}", e)))?;
        String::from_utf8(buf).map_err(|e| {
            HakilleError::InferenceError(format!("Invalid UTF-8 in GGUF string: {}", e))
        })
    }

    /// Un seek au-delà d'EOF réussit sur File : vérifier explicitement la
    /// borne pour ne pas accepter un vocabulaire GGUF tronqué.
    fn skip_exact<R: Read + Seek>(reader: &mut R, count: u64, file_len: u64) -> Result<()> {
        let current = reader
            .stream_position()
            .map_err(|e| HakilleError::InferenceError(e.to_string()))?;
        let target = current
            .checked_add(count)
            .filter(|target| *target <= file_len)
            .ok_or_else(|| {
                HakilleError::InferenceError("Tableau GGUF tronqué ou taille débordante".into())
            })?;
        reader
            .seek(SeekFrom::Start(target))
            .map_err(|e| HakilleError::InferenceError(e.to_string()))?;
        Ok(())
    }

    fn read_value<R: Read + Seek>(
        reader: &mut R,
        value_type: u32,
        file_len: u64,
    ) -> Result<GgufValue> {
        match value_type {
            0 => Ok(GgufValue::Uint8(Self::read_u8(reader)?)),
            1 => Ok(GgufValue::Int8(Self::read_u8(reader)? as i8)),
            2 => Ok(GgufValue::Uint16(Self::read_u16(reader)?)),
            3 => Ok(GgufValue::Int16(Self::read_i16(reader)?)),
            4 => Ok(GgufValue::Uint32(Self::read_u32(reader)?)),
            5 => Ok(GgufValue::Int32(Self::read_i32(reader)?)),
            6 => Ok(GgufValue::Float32(Self::read_f32(reader)?)),
            7 => Ok(GgufValue::Bool(Self::read_u8(reader)? != 0)),
            8 => Ok(GgufValue::String(Self::read_string(reader)?)),
            9 => {
                let elem_type = Self::read_u32(reader)?;
                let array_len = Self::read_u64(reader)?;
                if elem_type == 9 || elem_type > 12 || array_len > 4_000_000 {
                    return Err(HakilleError::InferenceError(
                        "Tableau GGUF imbriqué, type inconnu ou taille excessive".into(),
                    ));
                }

                // If array is large (e.g. tokenizer vocab with 150,000 items), skip seeking to avoid allocating huge memory
                if array_len > 256 {
                    let fixed_elem_size = match elem_type {
                        0 | 1 | 7 => Some(1u64),
                        2 | 3 => Some(2u64),
                        4..=6 => Some(4u64),
                        10..=12 => Some(8u64),
                        _ => None,
                    };

                    if let Some(elem_size) = fixed_elem_size {
                        let total_bytes = elem_size.checked_mul(array_len).ok_or_else(|| {
                            HakilleError::InferenceError("Taille de tableau GGUF débordante".into())
                        })?;
                        Self::skip_exact(reader, total_bytes, file_len)?;
                    } else if elem_type == 8 {
                        // String array: skip each string efficiently
                        for _ in 0..array_len {
                            let str_len = Self::read_u64(reader)?;
                            Self::skip_exact(reader, str_len, file_len)?;
                        }
                    } else {
                        return Err(HakilleError::InferenceError(format!(
                            "Unsupported nested large array element type: {}",
                            elem_type
                        )));
                    }

                    Ok(GgufValue::LargeArray {
                        element_type: elem_type,
                        len: array_len,
                    })
                } else {
                    let mut items = Vec::with_capacity(array_len as usize);
                    for _ in 0..array_len {
                        items.push(Self::read_value(reader, elem_type, file_len)?);
                    }
                    Ok(GgufValue::Array(items))
                }
            }
            10 => Ok(GgufValue::Uint64(Self::read_u64(reader)?)),
            11 => Ok(GgufValue::Int64(Self::read_i64(reader)?)),
            12 => Ok(GgufValue::Float64(Self::read_f64(reader)?)),
            other => Err(HakilleError::InferenceError(format!(
                "Unknown GGUF value type: {}",
                other
            ))),
        }
    }
}

pub struct GgufRuntime;

impl GgufRuntime {
    pub fn parse_header(bytes: &[u8]) -> Result<GgufHeader> {
        if bytes.len() < 24 {
            return Err(HakilleError::InferenceError(
                "Buffer too small for GGUF header".into(),
            ));
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != GGUF_MAGIC {
            return Err(HakilleError::InferenceError(format!(
                "Invalid GGUF magic bytes: 0x{:08X}",
                magic
            )));
        }

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 3 {
            return Err(HakilleError::InferenceError(format!(
                "Version GGUF {version} non prise en charge (v3 attendue)"
            )));
        }
        let tensor_count = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let metadata_kv_count = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        Ok(GgufHeader {
            magic,
            version,
            tensor_count,
            metadata_kv_count,
        })
    }

    pub fn load_mock_model(model_name: &str) -> GgufModel {
        let mut metadata = HashMap::new();
        let mut metadata_raw = HashMap::new();
        metadata.insert("general.architecture".to_string(), "qwen2".to_string());
        metadata_raw.insert(
            "general.architecture".to_string(),
            GgufValue::String("qwen2".to_string()),
        );
        metadata.insert("general.name".to_string(), model_name.to_string());
        metadata_raw.insert(
            "general.name".to_string(),
            GgufValue::String(model_name.to_string()),
        );
        metadata.insert("qwen2.context_length".to_string(), "32768".to_string());
        metadata_raw.insert("qwen2.context_length".to_string(), GgufValue::Uint64(32768));

        let mut tensors = HashMap::new();
        tensors.insert(
            "token_embd.weight".to_string(),
            GgufTensorInfo {
                name: "token_embd.weight".to_string(),
                dimensions: vec![4096, 151936],
                tensor_type: 2, // Q4_0
                offset: 1024,
            },
        );

        GgufModel {
            header: GgufHeader {
                magic: GGUF_MAGIC,
                version: 3,
                tensor_count: 1,
                metadata_kv_count: 3,
            },
            metadata,
            metadata_raw,
            tensors,
            estimated_params: 4096 * 151936,
        }
    }
}

/// Heuristic Template Engine over GGUF v3 metadata (inspection-only, SIMULÉ — gabarits locaux, pas de décodage neural).
#[derive(Debug, Clone)]
pub struct GgufInferenceEngine {
    pub model: GgufModel,
    pub model_path: Option<std::path::PathBuf>,
}

impl GgufInferenceEngine {
    pub fn new(model: GgufModel) -> Self {
        Self {
            model,
            model_path: None,
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref().to_path_buf();
        let model = GgufModel::read_from_file(&p)?;
        Ok(Self {
            model,
            model_path: Some(p),
        })
    }

    pub fn model_name(&self) -> &str {
        self.model.name().unwrap_or("modèle GGUF inconnu")
    }

    pub fn architecture(&self) -> &str {
        self.model.architecture().unwrap_or("architecture inconnue")
    }

    pub fn context_length(&self) -> usize {
        self.model
            .context_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0)
    }

    /// SUPPRIMÉ (ex-`generate_instruct` simulé par gabarits) : la génération
    /// réelle est assurée par `llm_engine::QwenEngine` (forward Candle Q4_K_M).
    /// Cet inspecteur GGUF ne fait que de l'inspection (`read_from_file`).
    /// Retourne toujours `Err` honnête pour empêcher tout usage simulé.
    /// Conservé (signature) pour compatibilité d'appelants externes éventuels.
    #[deprecated(
        since = "0.1.0",
        note = "SIMULÉ supprimé — utilisez llm_engine::QwenEngine::generate (forward réel). Inspection seule ici."
    )]
    pub fn generate_instruct(
        &self,
        _user_prompt: &str,
        _system_prompt: Option<&str>,
        _max_tokens: usize,
    ) -> Result<String> {
        Err(HakilleError::InferenceError(format!(
            "SIMULÉ supprimé — GgufInferenceEngine::generate_instruct n'exécute aucun décodage neural (modèle '{}', {} tenseurs inspectés). Utilisez llm_engine::QwenEngine (forward Candle réel).",
            self.model_name(),
            self.model.tensor_count(),
        )))
    }

    /// Ancien générateur d'actions GUI « neuronal » : en réalité un parseur
    /// de mots-clés qui fabriquait des coordonnées directionnelles
    /// (`top/bottom/left/right` → 0.12/0.88/0.15/0.85). C'étaient des
    /// coordonnées déguisées : la fonction est désormais honnêtement
    /// inutilisable tant qu'aucun décodage VLM UI-TARS n'est branché.
    ///
    /// Utiliser `hakille_agents::UiTarsAgent::predict_intent_from_task`
    /// (prior neutre explicite) puis `anchor_intentions_on_regions` /
    /// `predict_grounded_on_frame` (régions MESURÉES) pour toute exécution.
    #[deprecated(
        note = "keyword-based coordinates were fabricated; anchor parsed intents on measured CV regions instead"
    )]
    pub fn generate_gui_action_plan(
        &self,
        _task_description: &str,
        _screen_w: u32,
        _screen_h: u32,
    ) -> Result<Vec<String>> {
        Err(HakilleError::InferenceError(format!(
            "SIMULÉ supprimé — GgufInferenceEngine::generate_gui_action_plan ne décode aucun VLM (modèle '{}'). Utilisez UiTarsAgent::predict_intent_from_task + ancrage sur régions CV mesurées.",
            self.model_name(),
        )))
    }
}

#[cfg(test)]
mod gguf_safety_tests {
    use super::*;
    use std::io::Cursor;

    fn header(tensors: u64, metadata: u64) -> Vec<u8> {
        let mut bytes = GGUF_MAGIC.to_le_bytes().to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(tensors.to_le_bytes());
        bytes.extend(metadata.to_le_bytes());
        bytes
    }

    fn string(bytes: &mut Vec<u8>, text: &str) {
        bytes.extend((text.len() as u64).to_le_bytes());
        bytes.extend(text.as_bytes());
    }

    fn parse_fixture(bytes: &[u8]) -> Result<GgufModel> {
        let path = std::env::temp_dir().join(format!("hakille-gguf-{}.gguf", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).unwrap();
        let result = GgufModel::read_from_file(&path);
        std::fs::remove_file(path).unwrap();
        result
    }

    #[test]
    fn versions_and_unsigned_conversion_are_checked() {
        let mut bytes = header(0, 0);
        bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert!(GgufRuntime::parse_header(&bytes).is_err());
        assert_eq!(GgufValue::Uint64(u64::MAX).as_i64(), None);
        assert_eq!(GgufValue::Uint64(42).as_i64(), Some(42));
    }

    #[test]
    fn truncated_large_arrays_and_nested_arrays_are_refused() {
        for (kind, count) in [(4u32, 300u64), (8, 300), (9, 1), (4, u64::MAX)] {
            let mut bytes = kind.to_le_bytes().to_vec();
            bytes.extend(count.to_le_bytes());
            let len = bytes.len() as u64;
            assert!(GgufModel::read_value(&mut Cursor::new(bytes), 9, len).is_err());
        }
        let mut bytes = 4u32.to_le_bytes().to_vec();
        bytes.extend(300u64.to_le_bytes());
        bytes.extend(vec![0u8; 1200]);
        let len = bytes.len() as u64;
        assert!(matches!(
            GgufModel::read_value(&mut Cursor::new(bytes), 9, len).unwrap(),
            GgufValue::LargeArray { len: 300, .. }
        ));
    }

    #[test]
    fn invalid_dimensions_and_overflow_are_refused_before_allocation() {
        let mut bytes = header(1, 0);
        string(&mut bytes, "weight");
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(
            parse_fixture(&bytes)
                .unwrap_err()
                .to_string()
                .contains("dimensions")
        );
        let mut bytes = header(1, 0);
        string(&mut bytes, "weight");
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(u64::MAX.to_le_bytes());
        bytes.extend(2u64.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        assert!(
            parse_fixture(&bytes)
                .unwrap_err()
                .to_string()
                .contains("débordant")
        );
    }

    #[test]
    fn duplicate_architecture_cannot_silently_replace_ui_tars_metadata() {
        let mut bytes = header(0, 2);
        for arch in ["qwen2vl", "qwen2"] {
            string(&mut bytes, "general.architecture");
            bytes.extend(8u32.to_le_bytes());
            string(&mut bytes, arch);
        }
        assert!(
            parse_fixture(&bytes)
                .unwrap_err()
                .to_string()
                .contains("dupliquée")
        );
        let unknown = parse_fixture(&header(0, 0)).unwrap();
        assert_eq!(unknown.context_length(), None);
        let inspector = GgufInferenceEngine::new(unknown);
        assert_eq!(inspector.architecture(), "architecture inconnue");
        assert_eq!(inspector.context_length(), 0);
    }
}
