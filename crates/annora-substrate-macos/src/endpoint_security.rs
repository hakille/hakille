use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsEventSubscription {
    pub event_types: Vec<String>,
    pub mute_path_prefixes: Vec<String>,
}

pub struct EndpointSecurityClient {
    subscription: EsEventSubscription,
    connected: bool,
}

impl EndpointSecurityClient {
    pub fn new(subscription: EsEventSubscription) -> Self {
        Self {
            subscription,
            connected: false,
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
}
