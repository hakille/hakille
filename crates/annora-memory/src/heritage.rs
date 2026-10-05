use serde::{Deserialize, Serialize};

pub struct EncryptionKey(pub Vec<u8>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeritageItemType {
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeritageItem {
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeneficiaryConfig {
    pub user_id: String,
    pub public_key: Vec<u8>,
    pub allowed_items: Vec<HeritageItemType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeritageConfig {
    pub beneficiaries: Vec<BeneficiaryConfig>,
    pub items: Vec<HeritageItem>,
}

pub struct HeritageManager;

impl HeritageManager {
    pub fn crypto_erase(_key: &EncryptionKey) {}
    pub fn configure_heritage(_config: HeritageConfig) {}
}
