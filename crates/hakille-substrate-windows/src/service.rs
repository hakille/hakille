use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsServiceState {
    Stopped,
    StartPending,
    Running,
    PausePending,
    Paused,
    StopPending,
}

pub struct WindowsServiceManager {
    service_name: String,
    state: WindowsServiceState,
}

impl WindowsServiceManager {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            state: WindowsServiceState::Stopped,
        }
    }

    pub fn start(&mut self) {
        self.state = WindowsServiceState::Running;
    }

    pub fn stop(&mut self) {
        self.state = WindowsServiceState::Stopped;
    }

    pub fn pause(&mut self) {
        self.state = WindowsServiceState::Paused;
    }

    pub fn state(&self) -> WindowsServiceState {
        self.state
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
