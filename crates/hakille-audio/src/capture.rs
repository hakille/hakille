//! Capture micro : flux cpal -> downmix mono -> 16 kHz f32 -> blocs de 10 ms.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

use crate::resample::LinearStreamResampler;
use crate::{AudioError, CAPTURE_BLOCK_SAMPLES, TARGET_SAMPLE_RATE};

/// Capture audio temps réel depuis un périphérique d'entrée.
///
/// Le callback fourni à [`Capture::start`] reçoit des blocs mono `f32` à
/// [`TARGET_SAMPLE_RATE`] (16 kHz) de [`CAPTURE_BLOCK_SAMPLES`] échantillons
/// (10 ms). La conversion (downmix + rééchantillonnage) est faite à la volée.
pub struct Capture {
    device: cpal::Device,
    device_name: String,
    format: SampleFormat,
    config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("device_name", &self.device_name)
            .field("sample_rate", &self.config.sample_rate)
            .field("channels", &self.config.channels)
            .field("format", &self.format)
            .field("running", &self.stream.is_some())
            .finish()
    }
}

impl Capture {
    /// Ouvre le périphérique d'entrée par défaut.
    ///
    /// Renvoie [`AudioError::NoInputDevice`] s'il n'existe aucun périphérique
    /// d'entrée : aucun succès n'est simulé.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        Self::from_device(device)
    }

    /// Ouvre le périphérique d'entrée dont le nom correspond exactement à `name`
    /// (tel que renvoyé par [`crate::input_devices`]).
    pub fn with_device_name(name: &str) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let devices = host.input_devices().map_err(|e| {
            AudioError::StreamError(format!("énumération des entrées impossible : {e}"))
        })?;
        let device = devices
            .into_iter()
            .find(|d| d.to_string() == name)
            .ok_or_else(|| AudioError::DeviceNotFound(name.to_string()))?;
        Self::from_device(device)
    }

    fn from_device(device: cpal::Device) -> Result<Self, AudioError> {
        let device_name = device.to_string();
        let (format, config) = select_input_config(&device)?;
        ensure_convertible(format, &device_name)?;
        Ok(Self {
            device,
            device_name,
            format,
            config,
            stream: None,
        })
    }

    /// Nom du périphérique ouvert.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Fréquence d'échantillonnage native du flux cpal (avant rééchantillonnage).
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Nombre de canaux natifs du flux cpal (avant downmix).
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    /// `true` si une capture est en cours.
    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    /// Démarre la capture et livre les blocs au callback.
    ///
    /// Si une capture était déjà active, elle est d'abord arrêtée puis remplacée.
    pub fn start<F>(&mut self, callback: F) -> Result<(), AudioError>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.stop()?;
        let stream = match self.format {
            SampleFormat::F32 => self.build::<f32, F>(callback)?,
            SampleFormat::F64 => self.build::<f64, F>(callback)?,
            SampleFormat::I8 => self.build::<i8, F>(callback)?,
            SampleFormat::I16 => self.build::<i16, F>(callback)?,
            SampleFormat::I24 => self.build::<cpal::I24, F>(callback)?,
            SampleFormat::I32 => self.build::<i32, F>(callback)?,
            SampleFormat::I64 => self.build::<i64, F>(callback)?,
            SampleFormat::U8 => self.build::<u8, F>(callback)?,
            SampleFormat::U16 => self.build::<u16, F>(callback)?,
            SampleFormat::U24 => self.build::<cpal::U24, F>(callback)?,
            SampleFormat::U32 => self.build::<u32, F>(callback)?,
            SampleFormat::U64 => self.build::<u64, F>(callback)?,
            other => {
                return Err(AudioError::Unsupported(format!(
                    "format de capture « {other} » non convertible vers f32"
                )));
            }
        };
        self.stream = Some(stream);
        Ok(())
    }

    /// Arrête la capture. Idempotent : renvoie `Ok(())` si rien n'est actif.
    pub fn stop(&mut self) -> Result<(), AudioError> {
        if let Some(stream) = self.stream.take() {
            if let Err(e) = stream.pause() {
                tracing::warn!(
                    error = %e,
                    device = %self.device_name,
                    "pause du flux de capture impossible, arrêt par destruction du flux"
                );
            }
            drop(stream);
        }
        Ok(())
    }

    fn build<T, F>(&self, mut callback: F) -> Result<cpal::Stream, AudioError>
    where
        T: SizedSample + Send + 'static,
        f32: FromSample<T>,
        F: FnMut(&[f32]) + Send + 'static,
    {
        let channels = self.config.channels as usize;
        let mut resampler = LinearStreamResampler::new(self.config.sample_rate, TARGET_SAMPLE_RATE);
        let mut mono: Vec<f32> = Vec::new();
        let mut resampled: Vec<f32> = Vec::new();
        let mut pending: Vec<f32> = Vec::new();
        let error_device_name = self.device_name.clone();
        let stream = self
            .device
            .build_input_stream(
                self.config,
                move |data: &[T], _info: &cpal::InputCallbackInfo| {
                    mono.clear();
                    for frame in data.chunks_exact(channels) {
                        let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                        mono.push(sum / channels as f32);
                    }
                    resampled.clear();
                    resampler.push(&mono, &mut resampled);
                    pending.extend_from_slice(&resampled);
                    while pending.len() >= CAPTURE_BLOCK_SAMPLES {
                        let block: Vec<f32> = pending.drain(..CAPTURE_BLOCK_SAMPLES).collect();
                        callback(&block);
                    }
                },
                move |err: cpal::Error| {
                    tracing::error!(
                        error = %err,
                        device = %error_device_name,
                        "erreur du flux de capture audio"
                    );
                },
                None,
            )
            .map_err(|e| {
                AudioError::StreamError(format!(
                    "construction du flux d'entrée « {} » : {e}",
                    self.device_name
                ))
            })?;
        if let Err(e) = stream.play() {
            drop(stream);
            return Err(AudioError::StreamError(format!(
                "démarrage de la capture « {} » : {e}",
                self.device_name
            )));
        }
        tracing::info!(
            device = %self.device_name,
            rate = self.config.sample_rate,
            channels = self.config.channels,
            format = %self.format,
            "capture audio démarrée"
        );
        Ok(stream)
    }
}

fn ensure_convertible(format: SampleFormat, device_name: &str) -> Result<(), AudioError> {
    match format {
        SampleFormat::I8
        | SampleFormat::I16
        | SampleFormat::I24
        | SampleFormat::I32
        | SampleFormat::I64
        | SampleFormat::U8
        | SampleFormat::U16
        | SampleFormat::U24
        | SampleFormat::U32
        | SampleFormat::U64
        | SampleFormat::F32
        | SampleFormat::F64 => Ok(()),
        other => Err(AudioError::Unsupported(format!(
            "« {device_name} » ne propose que le format « {other} », non convertible vers f32"
        ))),
    }
}

/// Choisit la configuration d'entrée : f32 natif si disponible (16 kHz si supporté,
/// sinon 48 kHz / 44,1 kHz, sinon fréquence max), sinon la config par défaut du
/// périphérique (convertie ensuite i16/u16/etc. -> f32).
fn select_input_config(
    device: &cpal::Device,
) -> Result<(SampleFormat, cpal::StreamConfig), AudioError> {
    let name = device.to_string();
    let mut native_f32 = None;
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            if range.sample_format() == SampleFormat::F32 {
                native_f32 = Some(if range.contains_rate(TARGET_SAMPLE_RATE) {
                    range.with_sample_rate(TARGET_SAMPLE_RATE)
                } else if let Some(config) = range.try_with_standard_sample_rate() {
                    config
                } else {
                    range.with_sample_rate(range.max_sample_rate())
                });
                break;
            }
        }
    }
    let supported = match native_f32 {
        Some(config) => config,
        None => device.default_input_config().map_err(|e| {
            AudioError::StreamError(format!(
                "configuration d'entrée par défaut indisponible pour « {name} » : {e}"
            ))
        })?,
    };
    let config = supported.config();
    if config.channels == 0 {
        return Err(AudioError::Unsupported(format!(
            "« {name} » annonce zéro canal d'entrée"
        )));
    }
    if config.sample_rate == 0 {
        return Err(AudioError::Unsupported(format!(
            "« {name} » annonce une fréquence d'entrée nulle"
        )));
    }
    Ok((supported.sample_format(), config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn capture_without_input_device_returns_explicit_error() {
        let host = cpal::default_host();
        if host.default_input_device().is_some() {
            eprintln!("SKIP: un périphérique d'entrée est présent, NoInputDevice non testable");
            return;
        }
        let err = Capture::new().expect_err("aucun device déclaré mais Capture::new a réussi");
        assert!(
            matches!(err, AudioError::NoInputDevice),
            "attendu NoInputDevice, obtenu {err:?}"
        );
    }

    #[test]
    fn capture_with_unknown_device_name_returns_device_not_found() {
        let host = cpal::default_host();
        if host.default_input_device().is_none() {
            eprintln!("SKIP: aucun périphérique d'entrée");
            return;
        }
        let err = Capture::with_device_name("__hakille_inexistant__")
            .expect_err("un device inexistant ne peut pas être ouvert");
        assert!(
            matches!(err, AudioError::DeviceNotFound(_)),
            "attendu DeviceNotFound, obtenu {err:?}"
        );
    }

    #[test]
    fn capture_start_delivers_10ms_blocks_when_device_available() {
        let host = cpal::default_host();
        if host.default_input_device().is_none() {
            eprintln!("SKIP: aucun périphérique d'entrée");
            return;
        }
        let mut capture = match Capture::new() {
            Ok(capture) => capture,
            Err(err) => {
                eprintln!("SKIP: micro par défaut non ouvrable : {err}");
                return;
            }
        };
        assert!(!capture.is_running());
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        if let Err(err) = capture.start(move |block| {
            let _ = tx.send(block.to_vec());
        }) {
            eprintln!("SKIP: démarrage de la capture indisponible : {err}");
            return;
        }
        assert!(capture.is_running());
        let received = rx.recv_timeout(Duration::from_secs(3));
        capture.stop().expect("arrêt de la capture");
        assert!(!capture.is_running());
        match received {
            Ok(block) => {
                assert_eq!(block.len(), CAPTURE_BLOCK_SAMPLES);
                assert!(block.iter().all(|s| s.is_finite()));
            }
            Err(_) => eprintln!(
                "SKIP partiel: aucun bloc reçu en 3 s (micro muet, confidentialité Windows ou périphérique occupé)"
            ),
        }
    }
}
