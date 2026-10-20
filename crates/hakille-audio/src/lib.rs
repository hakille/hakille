//! I/O audio réel pour Hakille : capture micro et lecture haut-parleur via `cpal`.
//!
//! Ce crate ne simule jamais l'audio : si aucun périphérique n'est disponible ou si
//! un format n'est pas convertible, une erreur explicite est renvoyée. La capture
//! livre des blocs mono `f32` à 16 kHz (format attendu par le pipeline voix/STT).

pub mod capture;
pub mod playback;
pub mod resample;

use cpal::traits::HostTrait;

pub use capture::Capture;
pub use playback::{PlaybackHandle, play_pcm, play_pcm_async};
pub use resample::{downmix_to_mono, resample_linear};

/// Fréquence d'échantillonnage cible du pipeline Hakille (16 kHz mono f32).
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Nombre d'échantillons par bloc livré par [`Capture`] (10 ms à 16 kHz).
pub const CAPTURE_BLOCK_SAMPLES: usize = 160;

/// Erreurs audio explicites. Aucune variante ne représente un succès simulé.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// Aucun périphérique d'entrée n'est disponible sur le système.
    #[error("aucun périphérique d'entrée audio disponible (micro absent ou inaccessible)")]
    NoInputDevice,
    /// Aucun périphérique de sortie n'est disponible sur le système.
    #[error("aucun périphérique de sortie audio disponible (haut-parleur absent ou inaccessible)")]
    NoOutputDevice,
    /// Le périphérique demandé par nom n'existe pas.
    #[error("périphérique audio introuvable : {0}")]
    DeviceNotFound(String),
    /// Échec cpal à la construction, au démarrage ou pendant l'exécution d'un flux.
    #[error("erreur de flux audio : {0}")]
    StreamError(String),
    /// Format, fréquence ou paramètre non supporté par le périphérique.
    #[error("format ou paramètre audio non supporté : {0}")]
    Unsupported(String),
}

/// Périphérique audio tel qu'énuméré par le système.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Nom lisible du périphérique.
    pub name: String,
    /// `true` si c'est le périphérique par défaut pour sa direction.
    pub is_default: bool,
}

/// Liste les périphériques d'entrée disponibles, périphérique par défaut marqué.
pub fn input_devices() -> Result<Vec<AudioDevice>, AudioError> {
    let host = cpal::default_host();
    let default = host.default_input_device();
    let devices = host.input_devices().map_err(|e| {
        AudioError::StreamError(format!("énumération des entrées impossible : {e}"))
    })?;
    Ok(devices
        .map(|device| AudioDevice {
            name: device.to_string(),
            is_default: default.as_ref().is_some_and(|d| d == &device),
        })
        .collect())
}

/// Liste les périphériques de sortie disponibles, périphérique par défaut marqué.
pub fn output_devices() -> Result<Vec<AudioDevice>, AudioError> {
    let host = cpal::default_host();
    let default = host.default_output_device();
    let devices = host.output_devices().map_err(|e| {
        AudioError::StreamError(format!("énumération des sorties impossible : {e}"))
    })?;
    Ok(devices
        .map(|device| AudioDevice {
            name: device.to_string(),
            is_default: default.as_ref().is_some_and(|d| d == &device),
        })
        .collect())
}

/// Nom du périphérique d'entrée par défaut, s'il existe.
pub fn default_input_device_name() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .map(|d| d.to_string())
}

/// Nom du périphérique de sortie par défaut, s'il existe.
pub fn default_output_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .map(|d| d.to_string())
}

impl From<AudioError> for hakille_core::HakilleError {
    fn from(err: AudioError) -> Self {
        hakille_core::HakilleError::SubstrateError(format!("audio : {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_input_devices_is_coherent() {
        match input_devices() {
            Ok(devices) => {
                if devices.is_empty() {
                    eprintln!("SKIP: aucun périphérique d'entrée détecté (attendu sans micro)");
                    assert_eq!(default_input_device_name(), None);
                } else {
                    assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
                    assert!(devices.iter().all(|d| !d.name.is_empty()));
                }
            }
            Err(err) => eprintln!("SKIP: énumération des entrées indisponible : {err}"),
        }
    }

    #[test]
    fn enumeration_output_devices_is_coherent() {
        match output_devices() {
            Ok(devices) => {
                if devices.is_empty() {
                    eprintln!("SKIP: aucun périphérique de sortie détecté (attendu sans audio)");
                    assert_eq!(default_output_device_name(), None);
                } else {
                    assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
                    assert!(devices.iter().all(|d| !d.name.is_empty()));
                }
            }
            Err(err) => eprintln!("SKIP: énumération des sorties indisponible : {err}"),
        }
    }

    #[test]
    fn audio_error_maps_to_hakille_error() {
        let hakille: hakille_core::HakilleError = AudioError::NoInputDevice.into();
        assert!(matches!(
            hakille,
            hakille_core::HakilleError::SubstrateError(_)
        ));
    }
}
