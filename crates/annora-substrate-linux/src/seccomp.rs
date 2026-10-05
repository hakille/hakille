use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompProfile {
    pub default_action: String, // "kill", "trap", "errno", "allow"
    pub allowed_syscalls: Vec<String>,
    pub isolated_namespaces: Vec<String>,
}

impl Default for SeccompProfile {
    fn default() -> Self {
        Self {
            default_action: "errno".to_string(),
            allowed_syscalls: vec![
                "read".to_string(),
                "write".to_string(),
                "close".to_string(),
                "fstat".to_string(),
                "mmap".to_string(),
                "munmap".to_string(),
                "brk".to_string(),
                "exit_group".to_string(),
                "futex".to_string(),
                "clock_gettime".to_string(),
            ],
            isolated_namespaces: vec![
                "pid".to_string(),
                "net".to_string(),
                "ipc".to_string(),
                "uts".to_string(),
                "mount".to_string(),
            ],
        }
    }
}
