use annora_core::{AnnoraError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

pub const CAP_READ: u32 = 0x01;
pub const CAP_WRITE: u32 = 0x02;
pub const CAP_EXEC: u32 = 0x04;
pub const CAP_SCHEME_REGISTER: u32 = 0x08;
pub const CAP_DMA_BUFFER: u32 = 0x10;
pub const CAP_PORT_IO: u32 = 0x20;
pub const CAP_IRQS: u32 = 0x40;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedoxHandleEntry {
    pub fd: usize,
    pub scheme_name: String,
    pub resource_id: String,
    pub capabilities: u32,
}

pub struct RedoxCapabilityTable {
    handles: Mutex<HashMap<usize, RedoxHandleEntry>>,
    next_fd: Mutex<usize>,
}

impl RedoxCapabilityTable {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
            next_fd: Mutex::new(10), // Standard descriptors reserved
        }
    }

    pub fn allocate_handle(
        &self,
        scheme_name: impl Into<String>,
        resource_id: impl Into<String>,
        capabilities: u32,
    ) -> usize {
        let mut next = self.next_fd.lock().unwrap();
        let fd = *next;
        *next += 1;

        let entry = RedoxHandleEntry {
            fd,
            scheme_name: scheme_name.into(),
            resource_id: resource_id.into(),
            capabilities,
        };

        let mut map = self.handles.lock().unwrap();
        map.insert(fd, entry);
        fd
    }

    pub fn verify_access(&self, fd: usize, required_cap: u32) -> Result<RedoxHandleEntry> {
        let map = self.handles.lock().unwrap();
        if let Some(entry) = map.get(&fd) {
            if (entry.capabilities & required_cap) == required_cap {
                Ok(entry.clone())
            } else {
                Err(AnnoraError::SubstrateError(format!(
                    "Redox capability violation on fd {}: missing required capability 0x{:x}",
                    fd, required_cap
                )))
            }
        } else {
            Err(AnnoraError::SubstrateError(format!(
                "Bad file descriptor fd: {}",
                fd
            )))
        }
    }

    pub fn derive_restricted_capability(
        &self,
        parent_fd: usize,
        mask_capabilities: u32,
    ) -> Result<usize> {
        let parent = self.verify_access(parent_fd, mask_capabilities)?;
        // Attenuated capabilities can only be a subset of parent capabilities
        let new_caps = parent.capabilities & mask_capabilities;
        let child_fd = self.allocate_handle(
            parent.scheme_name,
            format!("{}-derived", parent.resource_id),
            new_caps,
        );
        Ok(child_fd)
    }

    pub fn transfer_capability(&self, fd: usize, new_resource_id: &str) -> Result<usize> {
        let parent = self.verify_access(fd, 0)?;
        let child_fd =
            self.allocate_handle(parent.scheme_name, new_resource_id, parent.capabilities);
        Ok(child_fd)
    }

    pub fn revoke_handle(&self, fd: usize) -> Result<()> {
        let mut map = self.handles.lock().unwrap();
        if map.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(AnnoraError::SubstrateError(format!(
                "Handle fd {} not found",
                fd
            )))
        }
    }

    pub fn list_handles(&self) -> Vec<RedoxHandleEntry> {
        self.handles.lock().unwrap().values().cloned().collect()
    }
}

impl Default for RedoxCapabilityTable {
    fn default() -> Self {
        Self::new()
    }
}
