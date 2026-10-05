use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinuxDaemonConfig {
    pub socket_path: String,
    pub uid: u32,
    pub gid: u32,
    pub drop_privileges: bool,
}

pub struct LinuxDaemon {
    config: LinuxDaemonConfig,
    is_running: bool,
}

impl LinuxDaemon {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            config: LinuxDaemonConfig {
                socket_path: socket_path.into(),
                uid: 1000,
                gid: 1000,
                drop_privileges: true,
            },
            is_running: false,
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn config(&self) -> &LinuxDaemonConfig {
        &self.config
    }
}
