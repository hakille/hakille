use annora_core::{AnnoraError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const GGUF_MAGIC: u32 = 0x46554747; // "GGUF" in little endian

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
pub struct GgufModel {
    pub header: GgufHeader,
    pub metadata: HashMap<String, String>,
    pub tensors: HashMap<String, GgufTensorInfo>,
}

pub struct GgufRuntime;

impl GgufRuntime {
    pub fn parse_header(bytes: &[u8]) -> Result<GgufHeader> {
        if bytes.len() < 24 {
            return Err(AnnoraError::InferenceError("Buffer too small for GGUF header".into()));
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != GGUF_MAGIC {
            return Err(AnnoraError::InferenceError("Invalid GGUF magic bytes".into()));
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
        metadata.insert("general.architecture".to_string(), "qwen2".to_string());
        metadata.insert("general.name".to_string(), model_name.to_string());
        metadata.insert("qwen2.context_length".to_string(), "32768".to_string());

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
            tensors,
        }
    }
}
