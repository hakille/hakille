//! État observable de la capture ; aucune ouverture de flux lors de l'inventaire.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[cfg(feature = "cpal")]
use cpal::traits::HostTrait;

#[cfg(feature = "cpal")]
use crate::capture::select_input_config;
use crate::AudioError;

/// Configuration native retenue, avant downmix et conversion vers 16 kHz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
}

/// Configuration annoncée par le pilote, sans preuve de permission ni de signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceDiagnostic {
    pub name: String,
    pub is_default: bool,
    pub config: Option<CaptureConfig>,
    pub error: Option<String>,
}

/// État depuis le dernier démarrage. Une capture démarrée peut ne recevoir aucun
/// callback : l'appelant doit imposer son délai de détection de flux bloqué.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureStatus {
    pub callbacks_received: u64,
    pub blocks_delivered: u64,
    pub errors_received: u64,
    pub last_callback_age_ms: Option<u64>,
    /// Reste vrai après `take_error()` ; seul un nouveau démarrage le réinitialise.
    pub failed: bool,
}

/// Inventorie les entrées et leur configuration utilisable. N'enregistre pas
/// d'audio et ne prétend pas tester le micro, son niveau ou ses permissions.
#[cfg(feature = "cpal")]
pub fn input_device_diagnostics() -> Result<Vec<InputDeviceDiagnostic>, AudioError> {
    let host = cpal::default_host();
    let default = host.default_input_device();
    let devices = host.input_devices().map_err(|e| {
        AudioError::StreamError(format!("énumération des entrées impossible : {e}"))
    })?;
    Ok(devices
        .map(|device| {
            let (config, error) = match select_input_config(&device) {
                Ok((format, config)) => (
                    Some(CaptureConfig {
                        sample_rate: config.sample_rate,
                        channels: config.channels,
                        sample_format: format.to_string(),
                    }),
                    None,
                ),
                Err(error) => (None, Some(error.to_string())),
            };
            InputDeviceDiagnostic {
                name: device.to_string(),
                is_default: default.as_ref().is_some_and(|d| d == &device),
                config,
                error,
            }
        })
        .collect())
}

#[cfg(not(feature = "cpal"))]
pub fn input_device_diagnostics() -> Result<Vec<InputDeviceDiagnostic>, AudioError> {
    #[cfg(feature = "mock-audio")]
    {
        Ok(vec![InputDeviceDiagnostic {
            name: "Hakille Mock Input".to_string(),
            is_default: true,
            config: Some(CaptureConfig {
                sample_rate: 16_000,
                channels: 1,
                sample_format: "F32".to_string(),
            }),
            error: None,
        }])
    }
    #[cfg(not(feature = "mock-audio"))]
    {
        Ok(Vec::new())
    }
}

pub(crate) struct CaptureHealth {
    started: Instant,
    callbacks: AtomicU64,
    blocks: AtomicU64,
    errors: AtomicU64,
    last_callback_us: AtomicU64,
    failed: AtomicBool,
    // Une seule erreur en attente : un pilote défaillant ne remplit pas la RAM.
    first_error: Mutex<Option<AudioError>>,
}

impl CaptureHealth {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            callbacks: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_callback_us: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            first_error: Mutex::new(None),
        }
    }

    pub(crate) fn callback_received(&self) {
        self.last_callback_us
            .store(self.elapsed_us().max(1), Ordering::Relaxed);
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn block_delivered(&self) {
        self.blocks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_error(&self, error: AudioError) {
        let mut pending = self.first_error.lock().unwrap_or_else(|e| e.into_inner());
        if pending.is_none() {
            *pending = Some(error);
        }
        self.errors.fetch_add(1, Ordering::Relaxed);
        self.failed.store(true, Ordering::Release);
    }

    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub(crate) fn take_error(&self) -> Option<AudioError> {
        self.first_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    fn elapsed_us(&self) -> u64 {
        self.started.elapsed().as_micros().min(u64::MAX as u128) as u64
    }

    pub(crate) fn status(&self) -> CaptureStatus {
        let last = self.last_callback_us.load(Ordering::Relaxed);
        CaptureStatus {
            callbacks_received: self.callbacks.load(Ordering::Relaxed),
            blocks_delivered: self.blocks.load(Ordering::Relaxed),
            errors_received: self.errors.load(Ordering::Relaxed),
            last_callback_age_ms: (last != 0)
                .then(|| self.elapsed_us().saturating_sub(last) / 1000),
            failed: self.failed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_status_does_not_invent_callbacks_or_signal() {
        let health = CaptureHealth::new();
        assert_eq!(
            health.status(),
            CaptureStatus {
                callbacks_received: 0,
                blocks_delivered: 0,
                errors_received: 0,
                last_callback_age_ms: None,
                failed: false,
            }
        );
        assert!(health.take_error().is_none());
    }

    #[test]
    fn callback_and_delivery_are_distinct_observations() {
        let health = CaptureHealth::new();
        health.callback_received();
        assert_eq!(health.status().callbacks_received, 1);
        assert_eq!(health.status().blocks_delivered, 0);
        assert!(health.status().last_callback_age_ms.is_some());
        health.block_delivered();
        assert_eq!(health.status().blocks_delivered, 1);
    }

    #[test]
    fn async_error_is_bounded_consumable_and_failure_stays_latched() {
        let health = CaptureHealth::new();
        health.record_error(AudioError::StreamError("micro débranché".into()));
        for _ in 0..1000 {
            health.record_error(AudioError::StreamError("erreur suivante".into()));
        }
        assert_eq!(health.status().errors_received, 1001);
        assert!(
            health
                .take_error()
                .unwrap()
                .to_string()
                .contains("micro débranché")
        );
        assert!(health.take_error().is_none());
        assert!(health.status().failed);
        let new_stream = CaptureHealth::new();
        assert!(!new_stream.status().failed);
        assert!(new_stream.take_error().is_none());
    }
}
