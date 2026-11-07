//! Universal Capability Registry facade and indexing implementation.

pub mod error;
pub mod index;
pub mod lifecycle;
pub mod version;

use chrono::Utc;
use semver::{Version, VersionReq};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{
    CanonicalToolDefinition, CapabilityDescriptor, CapabilityManifest, CapabilityScope,
    CapabilityType,
};
pub use error::RegistryError;
pub use index::{RegistryEntry, RegistryInner};
pub use lifecycle::CapabilityLifecycle;
pub use version::resolve_version;

/// Thread-safe universal registry managing capabilities, lifecycle states,
/// SemVer versioning, and multidimensional indices.
#[derive(Clone, Debug, Default)]
pub struct UniversalCapabilityRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

impl UniversalCapabilityRegistry {
    /// Creates a new empty capability registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner::default())),
        }
    }

    /// Registers a new capability manifest. Initializes in `Registered` state.
    pub async fn register(&self, manifest: CapabilityManifest) -> Result<(), RegistryError> {
        manifest.validate()?;
        let id = manifest.id.clone();
        let version = manifest.version.clone();
        let descriptor = manifest.to_descriptor();
        let now = Utc::now();

        let mut inner = self.inner.write().await;
        let versions_map = inner.entries.entry(id.clone()).or_default();
        if versions_map.contains_key(&version) {
            return Err(RegistryError::AlreadyExists { id, version });
        }

        let tags: HashSet<String> = manifest
            .metadata
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_ascii_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        let entry = RegistryEntry {
            manifest,
            descriptor,
            lifecycle: CapabilityLifecycle::Registered { registered_at: now },
            registered_at: now,
            updated_at: now,
            semantic_embedding: None,
            tags: tags.clone(),
        };

        versions_map.insert(version, entry.clone());
        inner
            .type_index
            .entry(entry.descriptor.capability_type)
            .or_default()
            .insert(id.clone());
        for scope in &entry.descriptor.scopes_required {
            inner
                .scope_index
                .entry(scope.clone())
                .or_default()
                .insert(id.clone());
        }
        for tag in tags {
            inner.tag_index.entry(tag).or_default().insert(id.clone());
        }

        Ok(())
    }

    /// Activates a specific version (or latest satisfying requirement) of a registered capability.
    pub async fn activate(
        &self,
        id: &str,
        req: Option<&VersionReq>,
    ) -> Result<Version, RegistryError> {
        let mut inner = self.inner.write().await;
        let versions = inner
            .entries
            .get(id)
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

        let target_version = match req {
            Some(requirement) => versions
                .keys()
                .filter(|v| requirement.matches(v))
                .max()
                .cloned()
                .ok_or_else(|| RegistryError::VersionMismatch {
                    id: id.to_string(),
                    requirement: requirement.to_string(),
                })?,
            None => versions
                .keys()
                .max()
                .cloned()
                .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?,
        };

        // Check for tool name collisions across currently active capabilities
        let target_tool_names: Vec<String> = {
            let entry = versions.get(&target_version).unwrap();
            for tool in &entry.descriptor.tools {
                if let Some((existing_id, existing_ver)) = inner.tool_to_capability.get(&tool.name)
                    && existing_id != id
                {
                    return Err(RegistryError::ToolCollision {
                        tool_name: tool.name.clone(),
                        existing_capability_id: format!("{existing_id}@{existing_ver}"),
                    });
                }
            }
            entry
                .descriptor
                .tools
                .iter()
                .map(|t| t.name.clone())
                .collect()
        };

        // Deactivate previously active version of this capability if different
        if let Some(prev_ver) = inner.active_versions.get(id).cloned()
            && prev_ver != target_version
        {
            let prev_tool_names: Vec<String> = if let Some(prev_entry) =
                inner.entries.get_mut(id).and_then(|m| m.get_mut(&prev_ver))
            {
                prev_entry.lifecycle = CapabilityLifecycle::Disabled {
                    registered_at: prev_entry.registered_at,
                    disabled_at: Utc::now(),
                    reason: Some(format!("Superseded by active version {target_version}")),
                };
                prev_entry.updated_at = Utc::now();
                prev_entry
                    .descriptor
                    .tools
                    .iter()
                    .map(|t| t.name.clone())
                    .collect()
            } else {
                Vec::new()
            };

            for tool_name in prev_tool_names {
                inner.tool_to_capability.remove(&tool_name);
            }
        }

        // Mark target version active
        let entry_mut = inner
            .entries
            .get_mut(id)
            .unwrap()
            .get_mut(&target_version)
            .unwrap();
        entry_mut.lifecycle = CapabilityLifecycle::Active {
            registered_at: entry_mut.registered_at,
            activated_at: Utc::now(),
        };
        entry_mut.updated_at = Utc::now();

        // Index tools
        for tool_name in target_tool_names {
            inner
                .tool_to_capability
                .insert(tool_name, (id.to_string(), target_version.clone()));
        }

        inner
            .active_versions
            .insert(id.to_string(), target_version.clone());
        Ok(target_version)
    }

    /// Disables an active capability.
    pub async fn disable(&self, id: &str) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;
        let active_ver = inner
            .active_versions
            .remove(id)
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

        let tool_names: Vec<String> = inner
            .entries
            .get(id)
            .and_then(|m| m.get(&active_ver))
            .map(|e| e.descriptor.tools.iter().map(|t| t.name.clone()).collect())
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

        for tool_name in tool_names {
            inner.tool_to_capability.remove(&tool_name);
        }

        let entry_mut = inner
            .entries
            .get_mut(id)
            .and_then(|m| m.get_mut(&active_ver))
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

        entry_mut.lifecycle = CapabilityLifecycle::Disabled {
            registered_at: entry_mut.registered_at,
            disabled_at: Utc::now(),
            reason: Some("Explicitly disabled via registry API".to_string()),
        };
        entry_mut.updated_at = Utc::now();
        Ok(())
    }

    /// Transitions an active capability to deprecated state.
    pub async fn deprecate(
        &self,
        id: &str,
        message: Option<String>,
        alternative_capability_id: Option<String>,
    ) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;
        let active_ver = inner
            .active_versions
            .get(id)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;
        let entry_mut = inner
            .entries
            .get_mut(id)
            .and_then(|m| m.get_mut(&active_ver))
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

        entry_mut.lifecycle = CapabilityLifecycle::Deprecated {
            registered_at: entry_mut.registered_at,
            deprecated_at: Utc::now(),
            message,
            alternative_capability_id,
        };
        entry_mut.updated_at = Utc::now();
        Ok(())
    }

    /// Unregisters a specific version of a capability.
    /// Active capabilities cannot be unregistered directly; they must be disabled or deprecated first.
    pub async fn unregister(&self, id: &str, version: &Version) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;

        if let Some(active_ver) = inner.active_versions.get(id)
            && active_ver == version
        {
            return Err(RegistryError::ActiveUnregisterBlocked { id: id.to_string() });
        }

        let removed_entry = {
            let versions_map = inner
                .entries
                .get_mut(id)
                .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;

            versions_map
                .remove(version)
                .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?
        };

        for tool in &removed_entry.descriptor.tools {
            if let Some((cap_id, ver)) = inner.tool_to_capability.get(&tool.name)
                && cap_id == id
                && ver == version
            {
                inner.tool_to_capability.remove(&tool.name);
            }
        }

        let is_empty = inner.entries.get(id).map(|m| m.is_empty()).unwrap_or(false);
        if is_empty {
            inner.entries.remove(id);
            inner.active_versions.remove(id);

            // Clean up secondary indices
            inner
                .type_index
                .entry(removed_entry.descriptor.capability_type)
                .or_default()
                .remove(id);
            for scope in &removed_entry.descriptor.scopes_required {
                inner
                    .scope_index
                    .entry(scope.clone())
                    .or_default()
                    .remove(id);
            }
            for tag in &removed_entry.tags {
                inner.tag_index.entry(tag.clone()).or_default().remove(id);
            }
        }

        Ok(())
    }

    /// Resolves an active or deprecated tool name to its parent CapabilityDescriptor and Tool Definition.
    pub async fn find_tool(
        &self,
        tool_name: &str,
    ) -> Option<(CapabilityDescriptor, CanonicalToolDefinition)> {
        let inner = self.inner.read().await;
        let (cap_id, version) = inner.tool_to_capability.get(tool_name)?;
        let entry = inner.entries.get(cap_id)?.get(version)?;
        if !entry.lifecycle.is_active() && !entry.lifecycle.is_deprecated() {
            return None;
        }
        let tool = entry.descriptor.find_tool(tool_name)?.clone();
        Some((entry.descriptor.clone(), tool))
    }

    /// Lists all active capability descriptors.
    pub async fn list_active(&self) -> Vec<CapabilityDescriptor> {
        let inner = self.inner.read().await;
        let mut result = Vec::new();
        for (id, ver) in &inner.active_versions {
            if let Some(entry) = inner.entries.get(id).and_then(|m| m.get(ver))
                && entry.lifecycle.is_active()
            {
                result.push(entry.descriptor.clone());
            }
        }
        result
    }

    /// Lists active capabilities requiring a specific Biscuit capability scope.
    pub async fn list_by_scope(&self, scope: &CapabilityScope) -> Vec<CapabilityDescriptor> {
        let inner = self.inner.read().await;
        let mut result = Vec::new();
        if let Some(ids) = inner.scope_index.get(scope) {
            for id in ids {
                if let Some(ver) = inner.active_versions.get(id)
                    && let Some(entry) = inner.entries.get(id).and_then(|m| m.get(ver))
                    && entry.lifecycle.is_active()
                {
                    result.push(entry.descriptor.clone());
                }
            }
        }
        result
    }

    /// Lists active capabilities by their execution capability type.
    pub async fn list_by_type(&self, cap_type: CapabilityType) -> Vec<CapabilityDescriptor> {
        let inner = self.inner.read().await;
        let mut result = Vec::new();
        if let Some(ids) = inner.type_index.get(&cap_type) {
            for id in ids {
                if let Some(ver) = inner.active_versions.get(id)
                    && let Some(entry) = inner.entries.get(id).and_then(|m| m.get(ver))
                    && entry.lifecycle.is_active()
                {
                    result.push(entry.descriptor.clone());
                }
            }
        }
        result
    }

    /// Lists active capabilities with a specific tag (case-insensitive).
    pub async fn list_by_tag(&self, tag: &str) -> Vec<CapabilityDescriptor> {
        let normalized = tag.to_ascii_lowercase();
        let inner = self.inner.read().await;
        let mut result = Vec::new();
        if let Some(ids) = inner.tag_index.get(&normalized) {
            for id in ids {
                if let Some(ver) = inner.active_versions.get(id)
                    && let Some(entry) = inner.entries.get(id).and_then(|m| m.get(ver))
                    && entry.lifecycle.is_active()
                {
                    result.push(entry.descriptor.clone());
                }
            }
        }
        result
    }

    /// Retrieves descriptor for a specific capability ID and optional version.
    /// If version is None, returns the active version's descriptor.
    pub async fn get_descriptor(
        &self,
        id: &str,
        version: Option<&Version>,
    ) -> Option<CapabilityDescriptor> {
        let inner = self.inner.read().await;
        let target_ver = match version {
            Some(v) => v,
            None => inner.active_versions.get(id)?,
        };
        inner
            .entries
            .get(id)
            .and_then(|m| m.get(target_ver))
            .map(|e| e.descriptor.clone())
    }

    /// Retrieves lifecycle state of a capability version.
    pub async fn get_lifecycle(&self, id: &str, version: &Version) -> Option<CapabilityLifecycle> {
        let inner = self.inner.read().await;
        inner
            .entries
            .get(id)
            .and_then(|m| m.get(version))
            .map(|e| e.lifecycle.clone())
    }

    /// Associates a 384-dimensional semantic embedding vector with a registered capability.
    pub async fn set_embedding(
        &self,
        id: &str,
        version: &Version,
        embedding: Vec<f32>,
    ) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;
        let entry = inner
            .entries
            .get_mut(id)
            .and_then(|m| m.get_mut(version))
            .ok_or_else(|| RegistryError::NotFound { id: id.to_string() })?;
        entry.semantic_embedding = Some(embedding);
        entry.updated_at = Utc::now();
        Ok(())
    }

    /// Retrieves the semantic embedding vector if configured.
    pub async fn get_embedding(&self, id: &str, version: &Version) -> Option<Vec<f32>> {
        let inner = self.inner.read().await;
        inner
            .entries
            .get(id)
            .and_then(|m| m.get(version))
            .and_then(|e| e.semantic_embedding.clone())
    }
}
