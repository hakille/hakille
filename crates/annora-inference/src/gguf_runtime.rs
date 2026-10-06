use annora_core::{AnnoraError, Result};
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
            Self::Uint64(v) => Some(*v as i64),
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
            AnnoraError::InferenceError(format!("Failed to open GGUF file {}: {}", path.display(), e))
        })?;
        let mut reader = BufReader::new(file);

        // 1. Read header (24 bytes)
        let mut header_buf = [0u8; 24];
        reader.read_exact(&mut header_buf).map_err(|e| {
            AnnoraError::InferenceError(format!("Failed to read GGUF header from {}: {}", path.display(), e))
        })?;

        let header = GgufRuntime::parse_header(&header_buf)?;

        // 2. Read metadata KV pairs
        let mut metadata = HashMap::new();
        let mut metadata_raw = HashMap::new();

        for _ in 0..header.metadata_kv_count {
            let key = Self::read_string(&mut reader)?;
            let val_type = Self::read_u32(&mut reader)?;
            let val = Self::read_value(&mut reader, val_type)?;
            metadata.insert(key.clone(), val.to_display_string());
            metadata_raw.insert(key, val);
        }

        // 3. Read tensor info headers
        let mut tensors = HashMap::new();
        let mut estimated_params = 0u64;

        for _ in 0..header.tensor_count {
            let name = Self::read_string(&mut reader)?;
            let n_dims = Self::read_u32(&mut reader)?;
            let mut dimensions = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                dimensions.push(Self::read_u64(&mut reader)?);
            }
            let tensor_type = Self::read_u32(&mut reader)?;
            let offset = Self::read_u64(&mut reader)?;

            // Accumulate parameter count for weight matrices
            if !dimensions.is_empty() {
                let count: u64 = dimensions.iter().product();
                if !name.ends_with(".bias") && !name.contains("rotary") {
                    estimated_params += count;
                }
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
            .or_else(|| self.metadata.get("general.architecture").map(|s| s.as_str()))
    }

    pub fn name(&self) -> Option<&str> {
        self.metadata_raw
            .get("general.name")
            .and_then(|v| v.as_str())
            .or_else(|| self.metadata.get("general.name").map(|s| s.as_str()))
    }

    pub fn context_length(&self) -> Option<u64> {
        let arch = self.architecture().unwrap_or("qwen2");
        let key = format!("{}.context_length", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| {
                self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok())
            })
    }

    pub fn embedding_length(&self) -> Option<u64> {
        let arch = self.architecture().unwrap_or("qwen2");
        let key = format!("{}.embedding_length", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| {
                self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok())
            })
    }

    pub fn block_count(&self) -> Option<u64> {
        let arch = self.architecture().unwrap_or("qwen2");
        let key = format!("{}.block_count", arch);
        self.metadata_raw
            .get(&key)
            .and_then(|v| v.as_u64())
            .or_else(|| {
                self.metadata.get(&key).and_then(|s| s.parse::<u64>().ok())
            })
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
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read u8 error: {}", e)))?;
        Ok(buf[0])
    }

    fn read_u16<R: Read>(reader: &mut R) -> Result<u16> {
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read u16 error: {}", e)))?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_i16<R: Read>(reader: &mut R) -> Result<i16> {
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read i16 error: {}", e)))?;
        Ok(i16::from_le_bytes(buf))
    }

    fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read u32 error: {}", e)))?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_i32<R: Read>(reader: &mut R) -> Result<i32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read i32 error: {}", e)))?;
        Ok(i32::from_le_bytes(buf))
    }

    fn read_f32<R: Read>(reader: &mut R) -> Result<f32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read f32 error: {}", e)))?;
        Ok(f32::from_le_bytes(buf))
    }

    fn read_u64<R: Read>(reader: &mut R) -> Result<u64> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read u64 error: {}", e)))?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_i64<R: Read>(reader: &mut R) -> Result<i64> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read i64 error: {}", e)))?;
        Ok(i64::from_le_bytes(buf))
    }

    fn read_f64<R: Read>(reader: &mut R) -> Result<f64> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read f64 error: {}", e)))?;
        Ok(f64::from_le_bytes(buf))
    }

    fn read_string<R: Read>(reader: &mut R) -> Result<String> {
        let len = Self::read_u64(reader)? as usize;
        if len > 50 * 1024 * 1024 {
            return Err(AnnoraError::InferenceError(format!("String length {} exceeds safety threshold", len)));
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).map_err(|e| AnnoraError::InferenceError(format!("Read string bytes error: {}", e)))?;
        String::from_utf8(buf).map_err(|e| AnnoraError::InferenceError(format!("Invalid UTF-8 in GGUF string: {}", e)))
    }

    fn read_value<R: Read + Seek>(reader: &mut R, value_type: u32) -> Result<GgufValue> {
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

                // If array is large (e.g. tokenizer vocab with 150,000 items), skip seeking to avoid allocating huge memory
                if array_len > 256 {
                    let fixed_elem_size = match elem_type {
                        0 | 1 | 7 => Some(1u64),
                        2 | 3 => Some(2u64),
                        4 | 5 | 6 => Some(4u64),
                        10 | 11 | 12 => Some(8u64),
                        _ => None,
                    };

                    if let Some(elem_size) = fixed_elem_size {
                        let total_bytes = elem_size * array_len;
                        reader.seek(SeekFrom::Current(total_bytes as i64))
                            .map_err(|e| AnnoraError::InferenceError(format!("Seek error skipping array: {}", e)))?;
                    } else if elem_type == 8 {
                        // String array: skip each string efficiently
                        for _ in 0..array_len {
                            let str_len = Self::read_u64(reader)?;
                            reader.seek(SeekFrom::Current(str_len as i64))
                                .map_err(|e| AnnoraError::InferenceError(format!("Seek error skipping string in array: {}", e)))?;
                        }
                    } else {
                        return Err(AnnoraError::InferenceError(format!("Unsupported nested large array element type: {}", elem_type)));
                    }

                    Ok(GgufValue::LargeArray { element_type: elem_type, len: array_len })
                } else {
                    let mut items = Vec::with_capacity(array_len as usize);
                    for _ in 0..array_len {
                        items.push(Self::read_value(reader, elem_type)?);
                    }
                    Ok(GgufValue::Array(items))
                }
            }
            10 => Ok(GgufValue::Uint64(Self::read_u64(reader)?)),
            11 => Ok(GgufValue::Int64(Self::read_i64(reader)?)),
            12 => Ok(GgufValue::Float64(Self::read_f64(reader)?)),
            other => Err(AnnoraError::InferenceError(format!("Unknown GGUF value type: {}", other))),
        }
    }
}

pub struct GgufRuntime;

impl GgufRuntime {
    pub fn parse_header(bytes: &[u8]) -> Result<GgufHeader> {
        if bytes.len() < 24 {
            return Err(AnnoraError::InferenceError("Buffer too small for GGUF header".into()));
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != GGUF_MAGIC {
            return Err(AnnoraError::InferenceError(format!("Invalid GGUF magic bytes: 0x{:08X}", magic)));
        }

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let tensor_count = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let metadata_kv_count = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19],
            bytes[20], bytes[21], bytes[22], bytes[23],
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
        metadata_raw.insert("general.architecture".to_string(), GgufValue::String("qwen2".to_string()));
        metadata.insert("general.name".to_string(), model_name.to_string());
        metadata_raw.insert("general.name".to_string(), GgufValue::String(model_name.to_string()));
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
