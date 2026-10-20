use serde::{Deserialize, Serialize};

pub const CLONE_NEWNS: u32 = 0x0002_0000;
pub const CLONE_NEWCGROUP: u32 = 0x0200_0000;
pub const CLONE_NEWUTS: u32 = 0x0400_0000;
pub const CLONE_NEWIPC: u32 = 0x0800_0000;
pub const CLONE_NEWUSER: u32 = 0x1000_0000;
pub const CLONE_NEWPID: u32 = 0x2000_0000;
pub const CLONE_NEWNET: u32 = 0x4000_0000;

pub const MS_RDONLY: u64 = 1;
pub const MS_NOSUID: u64 = 2;
pub const MS_NODEV: u64 = 4;
pub const MS_NOEXEC: u64 = 8;
pub const MS_BIND: u64 = 4096;
pub const MS_REC: u64 = 16384;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub flags: u64,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdMapping {
    pub inside_id: u32,
    pub outside_id: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    pub flags: u32,
    pub hostname: Option<String>,
    pub uid_mappings: Vec<IdMapping>,
    pub gid_mappings: Vec<IdMapping>,
    pub mounts: Vec<MountSpec>,
    pub rootfs: Option<String>,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            flags: CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWNET | CLONE_NEWIPC | CLONE_NEWUTS,
            hostname: Some("hakille-sandbox".to_string()),
            uid_mappings: vec![IdMapping {
                inside_id: 0,
                outside_id: 1000,
                count: 1,
            }],
            gid_mappings: vec![IdMapping {
                inside_id: 0,
                outside_id: 1000,
                count: 1,
            }],
            mounts: vec![
                MountSpec {
                    source: "none".to_string(),
                    target: "/proc".to_string(),
                    fstype: "proc".to_string(),
                    flags: MS_NOSUID | MS_NODEV | MS_NOEXEC,
                    data: None,
                },
                MountSpec {
                    source: "tmpfs".to_string(),
                    target: "/tmp".to_string(),
                    fstype: "tmpfs".to_string(),
                    flags: MS_NOSUID | MS_NODEV,
                    data: Some("size=64M".to_string()),
                },
            ],
            rootfs: Some("/var/lib/hakille/sandbox/rootfs".to_string()),
        }
    }
}

impl NamespaceConfig {
    pub fn is_network_isolated(&self) -> bool {
        (self.flags & CLONE_NEWNET) != 0
    }

    pub fn is_pid_isolated(&self) -> bool {
        (self.flags & CLONE_NEWPID) != 0
    }

    pub fn is_mount_isolated(&self) -> bool {
        (self.flags & CLONE_NEWNS) != 0
    }

    pub fn is_ipc_isolated(&self) -> bool {
        (self.flags & CLONE_NEWIPC) != 0
    }

    pub fn add_bind_mount(&mut self, source: &str, target: &str, readonly: bool) {
        let mut flags = MS_BIND | MS_REC;
        if readonly {
            flags |= MS_RDONLY;
        }
        self.mounts.push(MountSpec {
            source: source.to_string(),
            target: target.to_string(),
            fstype: "none".to_string(),
            flags,
            data: None,
        });
    }
}
