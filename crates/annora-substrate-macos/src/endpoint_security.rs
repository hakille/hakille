use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const ES_EVENT_TYPE_AUTH_EXEC: &str = "ES_EVENT_TYPE_AUTH_EXEC";
pub const ES_EVENT_TYPE_AUTH_OPEN: &str = "ES_EVENT_TYPE_AUTH_OPEN";
pub const ES_EVENT_TYPE_AUTH_UNLINK: &str = "ES_EVENT_TYPE_AUTH_UNLINK";
pub const ES_EVENT_TYPE_NOTIFY_EXEC: &str = "ES_EVENT_TYPE_NOTIFY_EXEC";
pub const ES_EVENT_TYPE_NOTIFY_FORK: &str = "ES_EVENT_TYPE_NOTIFY_FORK";
pub const ES_EVENT_TYPE_NOTIFY_EXIT: &str = "ES_EVENT_TYPE_NOTIFY_EXIT";
pub const ES_EVENT_TYPE_NOTIFY_WRITE: &str = "ES_EVENT_TYPE_NOTIFY_WRITE";
pub const ES_EVENT_TYPE_NOTIFY_CLOSE: &str = "ES_EVENT_TYPE_NOTIFY_CLOSE";
pub const ES_EVENT_TYPE_NOTIFY_MMAP: &str = "ES_EVENT_TYPE_NOTIFY_MMAP";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EsAuthResult {
    Allow = 1,
    Deny = 0,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EsProcess {
    pub pid: i32,
    pub ppid: i32,
    pub signing_id: String,
    pub team_id: Option<String>,
    pub cdhash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EsFile {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EsEvent {
    AuthExec {
        target_path: String,
        process: EsProcess,
        args: Vec<String>,
    },
    AuthOpen {
        file: EsFile,
        flags: u32,
        process: EsProcess,
    },
    AuthUnlink {
        target_path: String,
        process: EsProcess,
    },
    NotifyExec {
        process: EsProcess,
        executable_path: String,
    },
    NotifyFork {
        parent: EsProcess,
        child_pid: i32,
    },
    NotifyExit {
        process: EsProcess,
        stat_loc: i32,
    },
    NotifyWrite {
        file: EsFile,
        process: EsProcess,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsEventSubscription {
    pub event_types: Vec<String>,
    pub mute_path_prefixes: Vec<String>,
    pub muted_pids: Vec<i32>,
}

impl Default for EsEventSubscription {
    fn default() -> Self {
        Self {
            event_types: vec![
                ES_EVENT_TYPE_AUTH_EXEC.to_string(),
                ES_EVENT_TYPE_AUTH_OPEN.to_string(),
                ES_EVENT_TYPE_NOTIFY_FORK.to_string(),
                ES_EVENT_TYPE_NOTIFY_EXIT.to_string(),
            ],
            mute_path_prefixes: vec![
                "/System/Library".to_string(),
                "/usr/lib".to_string(),
                "/Applications/Annora.app/Contents".to_string(),
            ],
            muted_pids: Vec::new(),
        }
    }
}

pub struct EndpointSecurityClient {
    subscription: EsEventSubscription,
    connected: bool,
    handled_events: Mutex<Vec<EsEvent>>,
}

impl EndpointSecurityClient {
    pub fn new(subscription: EsEventSubscription) -> Self {
        Self {
            subscription,
            connected: false,
            handled_events: Mutex::new(Vec::new()),
        }
    }

    pub fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn subscription(&self) -> &EsEventSubscription {
        &self.subscription
    }

    pub fn add_mute_path(&mut self, prefix: impl Into<String>) {
        self.subscription.mute_path_prefixes.push(prefix.into());
    }

    pub fn add_mute_pid(&mut self, pid: i32) {
        if !self.subscription.muted_pids.contains(&pid) {
            self.subscription.muted_pids.push(pid);
        }
    }

    pub fn is_pid_muted(&self, pid: i32) -> bool {
        self.subscription.muted_pids.contains(&pid)
    }

    pub fn is_path_muted(&self, path: &str) -> bool {
        self.subscription
            .mute_path_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }

    pub fn evaluate_auth(&self, event: &EsEvent) -> EsAuthResult {
        match event {
            EsEvent::AuthExec {
                target_path,
                process,
                ..
            } => {
                if self.is_pid_muted(process.pid) {
                    return EsAuthResult::Allow;
                }
                // Deny unauthorized binaries outside known secure prefixes
                if self.is_path_muted(target_path) {
                    EsAuthResult::Allow
                } else if target_path.contains("/malicious/")
                    || target_path.ends_with(".trojan")
                    || target_path.contains("..")
                {
                    EsAuthResult::Deny
                } else {
                    EsAuthResult::Allow
                }
            }
            EsEvent::AuthOpen { file, .. } => {
                if file.path.contains("/etc/shadow") || file.path.contains("/etc/master.passwd") {
                    EsAuthResult::Deny
                } else {
                    EsAuthResult::Allow
                }
            }
            EsEvent::AuthUnlink { target_path, .. } => {
                if target_path.contains("/System/") || target_path.contains("/Library/") {
                    EsAuthResult::Deny
                } else {
                    EsAuthResult::Allow
                }
            }
            _ => EsAuthResult::Allow,
        }
    }

    pub fn handle_event(&self, event: EsEvent) -> Option<EsAuthResult> {
        let auth_res = match &event {
            EsEvent::AuthExec { .. } | EsEvent::AuthOpen { .. } | EsEvent::AuthUnlink { .. } => {
                Some(self.evaluate_auth(&event))
            }
            _ => None,
        };

        let mut events = self.handled_events.lock().unwrap();
        events.push(event);
        auth_res
    }

    pub fn recorded_events(&self) -> Vec<EsEvent> {
        self.handled_events.lock().unwrap().clone()
    }
}

impl Default for EndpointSecurityClient {
    fn default() -> Self {
        Self::new(EsEventSubscription::default())
    }
}
