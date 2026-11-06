//! Capture micro : flux cpal -> downmix mono -> 16 kHz f32 -> blocs de 10 ms.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

use crate::diagnostics::{CaptureConfig, CaptureHealth, CaptureStatus};
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
    health: Arc<CaptureHealth>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("device_name", &self.device_name)
            .field("sample_rate", &self.config.sample_rate)
            .field("channels", &self.config.channels)
            .field("format", &self.format)
            .field("running", &self.is_running())
            .field("status", &self.status())
            .finish()
    }
}

impl Capture {
    /// Ouvre le périphérique d'entrée par défaut.
    ///
    /// Renvoie [`AudioError::NoInputDevice`] s'il n'existe aucun périphérique
    /// d'entrée : aucun succès n'est simulé. Aucun autre périphérique n'est
    /// choisi automatiquement si le micro par défaut est inutilisable.
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
        let mut matching = devices.filter(|d| d.to_string() == name);
        let device = matching
            .next()
            .ok_or_else(|| AudioError::DeviceNotFound(name.to_string()))?;
        if matching.next().is_some() {
            return Err(AudioError::Unsupported(format!(
                "nom de micro ambigu « {name} » : plusieurs entrées portent ce nom ; choisissez le périphérique par défaut dans le système"
            )));
        }
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
            health: Arc::new(CaptureHealth::new()),
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

    /// Configuration choisie ; ne constitue pas une preuve de réception audio.
    pub fn config_info(&self) -> CaptureConfig {
        CaptureConfig {
            sample_rate: self.config.sample_rate,
            channels: self.config.channels,
            sample_format: self.format.to_string(),
        }
    }

    /// Observations depuis le dernier démarrage, sans bloquer le callback audio.
    pub fn status(&self) -> CaptureStatus {
        self.health.status()
    }

    /// Consomme la première erreur de flux en attente. Le statut `failed` reste
    /// vrai jusqu'au prochain démarrage ; l'appelant doit arrêter/recréer le flux.
    pub fn take_error(&self) -> Option<AudioError> {
        self.health.take_error()
    }

    /// `true` si le flux a démarré sans erreur signalée. Cela ne prouve pas qu'un
    /// callback a été reçu, ni que le micro contient un signal non silencieux.
    pub fn is_running(&self) -> bool {
        self.stream.is_some() && !self.health.failed()
    }

    /// Démarre la capture et livre les blocs au callback.
    ///
    /// Si une capture était déjà active, elle est d'abord arrêtée puis remplacée.
    /// Le callback s'exécute sur le thread audio : utiliser une file bornée avec
    /// `try_send`, jamais une inférence ou un envoi bloquant dans ce callback.
    pub fn start<F>(&mut self, callback: F) -> Result<(), AudioError>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.stop()?;
        self.health = Arc::new(CaptureHealth::new());
        let result = match self.format {
            SampleFormat::F32 => self.build::<f32, F>(callback),
            SampleFormat::F64 => self.build::<f64, F>(callback),
            SampleFormat::I8 => self.build::<i8, F>(callback),
            SampleFormat::I16 => self.build::<i16, F>(callback),
            SampleFormat::I24 => self.build::<cpal::I24, F>(callback),
            SampleFormat::I32 => self.build::<i32, F>(callback),
            SampleFormat::I64 => self.build::<i64, F>(callback),
            SampleFormat::U8 => self.build::<u8, F>(callback),
            SampleFormat::U16 => self.build::<u16, F>(callback),
            SampleFormat::U24 => self.build::<cpal::U24, F>(callback),
            SampleFormat::U32 => self.build::<u32, F>(callback),
            SampleFormat::U64 => self.build::<u64, F>(callback),
            other => {
                return Err(AudioError::Unsupported(format!(
                    "format de capture « {other} » non convertible vers f32"
                )));
            }
        };
        let stream = result.inspect_err(|error| {
            self.health
                .record_error(AudioError::StreamError(error.to_string()));
        })?;
        if self.health.failed() {
            drop(stream);
            return Err(self.take_error().unwrap_or_else(|| {
                AudioError::StreamError("erreur de capture pendant le démarrage".into())
            }));
        }
        self.stream = Some(stream);
        Ok(())
    }

    /// Arrête la capture en détruisant le flux CPAL. Idempotent.
    /// On ne demande pas une pause : certains pilotes ALSA ne disposent pas
    /// de pause matérielle alors que la fermeture du flux fonctionne bien.
    pub fn stop(&mut self) -> Result<(), AudioError> {
        drop(self.stream.take());
        Ok(())
    }

    fn build<T, F>(&self, mut callback: F) -> Result<cpal::Stream, AudioError>
    where
        T: SizedSample + Send + 'static,
        f32: FromSample<T>,
        F: FnMut(&[f32]) + Send + 'static,
    {
        let mut processor = CaptureProcessor::new(self.config.channels, self.config.sample_rate);
        let data_health = Arc::clone(&self.health);
        let error_health = Arc::clone(&self.health);
        let error_device_name = self.device_name.clone();
        let stream = self
            .device
            .build_input_stream(
                self.config,
                move |data: &[T], _info: &cpal::InputCallbackInfo| {
                    if data_health.failed() {
                        return;
                    }
                    data_health.callback_received();
                    if let Err(error) = processor.push(data, &mut |block| {
                        callback(block);
                        data_health.block_delivered();
                    }) {
                        data_health.record_error(error);
                    }
                },
                move |err: cpal::Error| {
                    error_health.record_error(AudioError::StreamError(format!(
                        "capture « {error_device_name} » : {err}"
                    )));
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

/// Convertisseur persistant : aucun buffer de 10 ms alloué ou décalé à chaque
/// livraison. Les vecteurs de travail sont réutilisés entre callbacks CPAL.
struct CaptureProcessor {
    channels: usize,
    resampler: LinearStreamResampler,
    mono: Vec<f32>,
    resampled: Vec<f32>,
    block: [f32; CAPTURE_BLOCK_SAMPLES],
    block_len: usize,
}

impl CaptureProcessor {
    fn new(channels: u16, sample_rate: u32) -> Self {
        Self {
            channels: channels as usize,
            resampler: LinearStreamResampler::new(sample_rate, TARGET_SAMPLE_RATE),
            mono: Vec::new(),
            resampled: Vec::new(),
            block: [0.0; CAPTURE_BLOCK_SAMPLES],
            block_len: 0,
        }
    }

    fn push<T: Sample>(
        &mut self,
        data: &[T],
        callback: &mut impl FnMut(&[f32]),
    ) -> Result<(), AudioError>
    where
        f32: FromSample<T>,
    {
        if self.channels == 0 || !data.len().is_multiple_of(self.channels) {
            return Err(AudioError::StreamError(
                "trame micro incomplète : nombre d'échantillons incompatible avec les canaux"
                    .into(),
            ));
        }
        self.mono.clear();
        for frame in data.chunks_exact(self.channels) {
            let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
            let sample = sum / self.channels as f32;
            if !sample.is_finite() {
                return Err(AudioError::StreamError(
                    "échantillon micro non fini : capture interrompue".into(),
                ));
            }
            self.mono.push(sample);
        }
        self.resampled.clear();
        self.resampler.push(&self.mono, &mut self.resampled);
        if self.resampled.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioError::StreamError(
                "conversion du micro non finie : capture interrompue".into(),
            ));
        }
        for &sample in &self.resampled {
            self.block[self.block_len] = sample;
            self.block_len += 1;
            if self.block_len == CAPTURE_BLOCK_SAMPLES {
                callback(&self.block);
                self.block_len = 0;
            }
        }
        Ok(())
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

/// Classe les plages utilisables indépendamment de l'ordre d'énumération CPAL.
/// f32 est préféré, puis mono, puis 16 / 48 / 44,1 kHz. Aucun changement de micro.
fn preferred_config(
    ranges: impl IntoIterator<Item = cpal::SupportedStreamConfigRange>,
) -> Option<cpal::SupportedStreamConfig> {
    ranges
        .into_iter()
        .filter_map(|range| {
            if range.channels() == 0
                || range.min_sample_rate() == 0
                || range.max_sample_rate() < range.min_sample_rate()
                || ensure_convertible(range.sample_format(), "entrée").is_err()
            {
                return None;
            }
            let rate = [TARGET_SAMPLE_RATE, 48_000, 44_100]
                .into_iter()
                .find(|&rate| range.contains_rate(rate))
                .unwrap_or(range.max_sample_rate());
            range.try_with_sample_rate(rate)
        })
        .min_by_key(|config| {
            let rate = config.sample_rate();
            let rate_rank = match rate {
                TARGET_SAMPLE_RATE => 0,
                48_000 => 1,
                44_100 => 2,
                _ => 3,
            };
            (
                config.sample_format() != SampleFormat::F32,
                config.channels(),
                rate_rank,
                rate,
                config.sample_format().to_string(),
            )
        })
}

pub(crate) fn select_input_config(
    device: &cpal::Device,
) -> Result<(SampleFormat, cpal::StreamConfig), AudioError> {
    let name = device.to_string();
    // Certains pilotes ne savent pas énumérer leurs plages mais fournissent
    // bien une configuration par défaut. On conserve ce chemin réel.
    let supported = device
        .supported_input_configs()
        .ok()
        .and_then(preferred_config)
        .map(Ok)
        .unwrap_or_else(|| {
            device.default_input_config().map_err(|e| {
                AudioError::StreamError(format!(
                    "configuration d'entrée indisponible pour « {name} » : {e}"
                ))
            })
        })?;
    ensure_convertible(supported.sample_format(), &name)?;
    let config = supported.config();
    if config.channels == 0 || config.sample_rate == 0 {
        return Err(AudioError::Unsupported(format!(
            "« {name} » annonce une configuration nulle ({} canaux, {} Hz)",
            config.channels, config.sample_rate
        )));
    }
    Ok((supported.sample_format(), config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn range(
        channels: u16,
        min: u32,
        max: u32,
        format: SampleFormat,
    ) -> cpal::SupportedStreamConfigRange {
        cpal::SupportedStreamConfigRange::new(
            channels,
            min,
            max,
            cpal::SupportedBufferSize::Unknown,
            format,
        )
    }

    #[test]
    fn config_selection_prefers_convertible_f32_mono_independent_of_driver_order() {
        let ranges = vec![
            range(2, 48_000, 48_000, SampleFormat::F32),
            range(1, 16_000, 48_000, SampleFormat::I16),
            range(1, 16_000, 48_000, SampleFormat::F32),
        ];
        let first = preferred_config(ranges.clone()).unwrap();
        let reversed = preferred_config(ranges.into_iter().rev()).unwrap();
        assert_eq!(first.sample_format(), SampleFormat::F32);
        assert_eq!(first.channels(), 1);
        assert_eq!(first.sample_rate(), TARGET_SAMPLE_RATE);
        assert_eq!(first.config(), reversed.config());
    }

    #[test]
    fn config_selection_rejects_invalid_ranges_and_accepts_integer_only_devices() {
        assert!(
            preferred_config([
                range(0, 16_000, 48_000, SampleFormat::F32),
                range(1, 0, 48_000, SampleFormat::F32),
                range(1, 48_000, 16_000, SampleFormat::F32),
            ])
            .is_none()
        );
        let integer = preferred_config([range(2, 44_100, 48_000, SampleFormat::I16)]).unwrap();
        assert_eq!(integer.sample_format(), SampleFormat::I16);
        assert_eq!(integer.sample_rate(), 48_000);
        let unusual = preferred_config([range(1, 22_050, 22_050, SampleFormat::F32)]).unwrap();
        assert_eq!(unusual.sample_rate(), 22_050);
    }

    #[test]
    fn capture_processing_delivers_exact_10ms_without_identity_startup_delay() {
        let mut processor = CaptureProcessor::new(1, 16_000);
        let input: Vec<f32> = (0..320).map(|i| i as f32 / 320.0).collect();
        let mut blocks = Vec::new();
        processor
            .push(&input[..160], &mut |b| blocks.push(b.to_vec()))
            .unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], input[..160]);
        processor
            .push(&input[160..199], &mut |b| blocks.push(b.to_vec()))
            .unwrap();
        assert_eq!(blocks.len(), 1);
        processor
            .push(&input[199..], &mut |b| blocks.push(b.to_vec()))
            .unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks.concat(), input);
    }

    #[test]
    fn capture_processing_downmixes_integer_48k_and_keeps_partial_blocks() {
        let mut processor = CaptureProcessor::new(2, 48_000);
        let input: Vec<i16> = (0..960).flat_map(|_| [16_384, 0]).collect();
        let mut blocks = Vec::new();
        for chunk in input.chunks(192) {
            processor
                .push(chunk, &mut |b| blocks.push(b.to_vec()))
                .unwrap();
        }
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|b| b.len() == CAPTURE_BLOCK_SAMPLES));
        assert!(
            blocks
                .concat()
                .iter()
                .all(|&sample| (sample - 0.25).abs() < 1e-6)
        );
    }

    #[test]
    fn capture_processing_rejects_incomplete_and_nonfinite_frames() {
        let mut stereo = CaptureProcessor::new(2, 16_000);
        assert!(
            stereo
                .push(&[0.0_f32], &mut |_| panic!("bloc invalide livré"))
                .is_err()
        );
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(
                stereo
                    .push(&[invalid, 0.0], &mut |_| panic!("bloc invalide livré"))
                    .is_err()
            );
        }
        let mut resampled = CaptureProcessor::new(1, 48_000);
        assert!(
            resampled
                .push(&[f32::MAX, -f32::MAX], &mut |_| {
                    panic!("résultat de conversion non fini livré")
                })
                .is_err()
        );
    }

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
    #[ignore = "nécessite un micro physique autorisé ; démarrage et réception doivent réussir"]
    fn capture_start_delivers_10ms_blocks_when_device_available() {
        let mut capture = Capture::new().expect("micro par défaut utilisable requis");
        assert!(!capture.is_running());
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(4);
        capture
            .start(move |block| {
                let _ = tx.try_send(block.to_vec());
            })
            .expect("autorisation et démarrage du micro requis");
        assert!(capture.is_running());
        let received = rx.recv_timeout(Duration::from_secs(3));
        capture.stop().expect("arrêt de la capture");
        assert!(!capture.is_running());
        assert!(
            capture.take_error().is_none(),
            "erreur asynchrone de capture"
        );
        let block = received.expect("aucun bloc reçu du microphone dans les 3 secondes");
        assert_eq!(block.len(), CAPTURE_BLOCK_SAMPLES);
        assert!(block.iter().all(|s| s.is_finite()));
        assert!(capture.status().callbacks_received > 0);
        assert!(capture.status().blocks_delivered > 0);
        eprintln!(
            "micro={} configuration={:?} état={:?}",
            capture.device_name(),
            capture.config_info(),
            capture.status()
        );
    }
}
