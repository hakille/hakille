//! Multidimensional registry storage and index structures.

use super::lifecycle::CapabilityLifecycle;
use crate::types::{CapabilityDescriptor, CapabilityManifest, CapabilityScope, CapabilityType};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

/// Stored entry in the registry retaining original manifest, descriptor, and active lifecycle state.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    /// Declarative source manifest.
    pub manifest: CapabilityManifest,
    /// Evaluated runtime descriptor.
    pub descriptor: CapabilityDescriptor,
    /// Current lifecycle state.
    pub lifecycle: CapabilityLifecycle,
    /// Initial registration timestamp.
    pub registered_at: DateTime<Utc>,
    /// Last lifecycle update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Precomputed 384-dimensional vector embedding for semantic discovery (MiniLM).
    pub semantic_embedding: Option<Vec<f32>>,
    /// Normalized search tags (lowercase).
    pub tags: HashSet<String>,
}

/// Multidimensional indexing inner store.
#[derive(Default, Debug)]
pub struct RegistryInner {
    /// Primary storage: Capability ID -> (Version -> RegistryEntry)
    pub entries: HashMap<String, HashMap<semver::Version, RegistryEntry>>,

    /// Active version pointer: Capability ID -> currently active Version
    pub active_versions: HashMap<String, semver::Version>,

    /// Tool Name Index: tool_name -> (CapabilityId, semver::Version)
    pub tool_to_capability: HashMap<String, (String, semver::Version)>,

    /// Secondary Index: CapabilityType -> Set<CapabilityId>
    pub type_index: HashMap<CapabilityType, HashSet<String>>,

    /// Secondary Index: CapabilityScope -> Set<CapabilityId>
    pub scope_index: HashMap<CapabilityScope, HashSet<String>>,

    /// Secondary Index: Tag (lowercase) -> Set<CapabilityId>
    pub tag_index: HashMap<String, HashSet<String>>,
}
