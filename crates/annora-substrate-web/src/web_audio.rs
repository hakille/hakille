use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAudioNodeConfig {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub is_muted: bool,
}

impl Default for WebAudioNodeConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 1024,
            is_muted: false,
        }
    }
}

pub struct WebAudioBridge {
    config: WebAudioNodeConfig,
}

impl WebAudioBridge {
    pub fn new(config: WebAudioNodeConfig) -> Self {
        Self { config }
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.config.is_muted = muted;
    }

    pub fn config(&self) -> &WebAudioNodeConfig {
        &self.config
    }
}

impl Default for WebAudioBridge {
    fn default() -> Self {
        Self::new(WebAudioNodeConfig::default())
    }
}
