use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wrapper around chrono::DateTime<Utc>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(Utc::now())
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

/// Wrapper around uuid::Uuid for user identities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(pub Uuid);

/// Wrapper around uuid::Uuid for node IDs in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub Uuid);

/// Wrapper around uuid::Uuid for triplet IDs in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TripletId(pub Uuid);

/// Wrapper around uuid::Uuid for predicate IDs in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PredicateId(pub Uuid);

/// Audit entry ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(pub Uuid);

/// String-based model identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelReference(pub String);

/// Platform-agnostic path
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualPath(pub String);

impl VirtualPath {
    /// Validate and normalize a virtual path.
    ///
    /// Rejects:
    /// - NUL bytes (`\0`)
    /// - Windows separators (`\`)
    /// - parent traversal (`..` as a path segment)
    /// - empty segments (`//`)
    ///
    /// Returns the normalized path on success.
    pub fn normalized(&self) -> crate::error::Result<String> {
        let s = &self.0;
        if s.contains('\0') {
            return Err(crate::error::AnnoraError::CapabilityDenied(
                "NUL byte in virtual path".into(),
            ));
        }
        if s.contains('\\') {
            return Err(crate::error::AnnoraError::CapabilityDenied(
                "windows separator in virtual path".into(),
            ));
        }
        if s.contains("//") {
            return Err(crate::error::AnnoraError::CapabilityDenied(
                "empty segment (//) in virtual path".into(),
            ));
        }
        // Reject `..` as a full path segment (split on `/`).
        for seg in s.split('/') {
            if seg == ".." {
                return Err(crate::error::AnnoraError::CapabilityDenied(
                    "parent traversal (..) rejected".into(),
                ));
            }
        }
        Ok(s.clone())
    }
}

impl std::fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Represents a financial cost
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Cost {
    pub amount_cents: u64,
    pub currency: String,
    pub description: String,
}
