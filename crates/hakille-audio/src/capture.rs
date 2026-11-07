//! Capture micro : flux cpal ou fallback headless.

#[cfg(feature = "cpal")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(feature = "cpal")]
use cpal::{FromSample, SampleFormat, SizedSample};

#[cfg(feature = "cpal")]
use crate::resample::LinearStreamResampler;
#[allow(unused_imports)]
use crate::{AudioError, CAPTURE_BLOCK_SAMPLES, TARGET_SAMPLE_RATE};

/// Capture audio temps réel depuis un périphérique d'entrée.
pub struct Capture {
    #[cfg(feature = "cpal")]
    device: cpal::Device,
    device_name: String,
    #[cfg(feature = "cpal")]
    format: SampleFormat,
    #[cfg(feature = "cpal")]
    config: cpal::StreamConfig,
    #[cfg(feature = "cpal")]
    stream: Option<cpal::Stream>,

    #[cfg(not(feature = "cpal"))]
    sample_rate: u32,
    #[cfg(not(feature = "cpal"))]
    channels: u16,
    #[cfg(not(feature = "cpal"))]
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(all(not(feature = "cpal"), feature = "mock-audio"))]
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    health: std::sync::Arc<crate::diagnostics::CaptureHealth>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("device_name", &self.device_name)
            .field("sample_rate", &self.sample_rate())
            .field("channels", &self.channels())
            .field("running", &self.is_running())
            .finish()
    }
}

impl Capture {
    /// Ouvre le périphérique d'entrée par défaut.
    pub fn new() -> Result<Self, AudioError> {
        #[cfg(feature = "cpal")]
        {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or(AudioError::NoInputDevice)?;
            Self::from_device(device)
        }
        #[cfg(not(feature = "cpal"))]
        {
            #[cfg(feature = "mock-audio")]
            {
                Self::with_device_name("Hakille Mock Input")
            }
            #[cfg(not(feature = "mock-audio"))]
            {
                Err(AudioError::NoInputDevice)
            }
        }
    }

    /// Ouvre le périphérique d'entrée dont le nom correspond exactement à `name`.
    pub fn with_device_name(name: &str) -> Result<Self, AudioError> {
        #[cfg(feature = "cpal")]
        {
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
        #[cfg(not(feature = "cpal"))]
        {
            #[cfg(feature = "mock-audio")]
            {
                if name == "Hakille Mock Input" || name == "mock" {
                    Ok(Self {
                        device_name: name.to_string(),
                        sample_rate: TARGET_SAMPLE_RATE,
                        channels: 1,
                        running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                        stop_tx: None,
                        health: std::sync::Arc::new(crate::diagnostics::CaptureHealth::new()),
                    })
                } else {
                    Err(AudioError::DeviceNotFound(name.to_string()))
                }
            }
            #[cfg(not(feature = "mock-audio"))]
            {
                Err(AudioError::DeviceNotFound(name.to_string()))
            }
        }
    }

    #[cfg(feature = "cpal")]
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
            health: std::sync::Arc::new(crate::diagnostics::CaptureHealth::new()),
        })
    }

    /// Nom du périphérique ouvert.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Fréquence d'échantillonnage native du flux cpal (avant rééchantillonnage).
    pub fn sample_rate(&self) -> u32 {
        #[cfg(feature = "cpal")]
        {
            self.config.sample_rate
        }
        #[cfg(not(feature = "cpal"))]
        {
            self.sample_rate
        }
    }

    /// Nombre de canaux natifs du flux cpal (avant downmix).
    pub fn channels(&self) -> u16 {
        #[cfg(feature = "cpal")]
        {
            self.config.channels
        }
        #[cfg(not(feature = "cpal"))]
        {
            self.channels
        }
    }

    /// `true` si une capture est en cours.
    pub fn is_running(&self) -> bool {
        #[cfg(feature = "cpal")]
        {
            self.stream.is_some()
        }
        #[cfg(not(feature = "cpal"))]
        {
            self.running.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// Démarre la capture et livre les blocs au callback.
    pub fn start<F>(&mut self, callback: F) -> Result<(), AudioError>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.stop()?;
        #[cfg(feature = "cpal")]
        {
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
        #[cfg(not(feature = "cpal"))]
        {
            #[cfg(feature = "mock-audio")]
            {
                let running = std::sync::Arc::clone(&self.running);
                let health = std::sync::Arc::clone(&self.health);
                running.store(true, std::sync::atomic::Ordering::SeqCst);
                let (tx, rx) = std::sync::mpsc::channel();
                self.stop_tx = Some(tx);
                let mut cb = callback;
                std::thread::spawn(move || {
                    let silence = vec![0.0f32; CAPTURE_BLOCK_SAMPLES];
                    while running.load(std::sync::atomic::Ordering::SeqCst) {
                        if rx.try_recv().is_ok() {
                            break;
                        }
                        health.callback_received();
                        health.block_delivered();
                        cb(&silence);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                });
                Ok(())
            }
            #[cfg(not(feature = "mock-audio"))]
            {
                let _ = callback;
                Err(AudioError::NoInputDevice)
            }
        }
    }

    /// Récupère et efface l'erreur survenue lors de la capture, le cas échéant.
    pub fn take_error(&self) -> Option<AudioError> {
        self.health.take_error()
    }

    /// État de santé de la capture depuis son démarrage.
    pub fn status(&self) -> crate::diagnostics::CaptureStatus {
        self.health.status()
    }

    /// Arrête la capture. Idempotent : renvoie `Ok(())` si rien n'est actif.
    pub fn stop(&mut self) -> Result<(), AudioError> {
        #[cfg(feature = "cpal")]
        {
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
        #[cfg(not(feature = "cpal"))]
        {
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            #[cfg(feature = "mock-audio")]
            {
                if let Some(tx) = self.stop_tx.take() {
                    let _ = tx.send(());
                }
            }
            Ok(())
        }
    }

    #[cfg(feature = "cpal")]
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
        let health = std::sync::Arc::clone(&self.health);
        let error_health = std::sync::Arc::clone(&self.health);
        let stream = self
            .device
            .build_input_stream(
                &self.config,
                move |data: &[T], _info: &cpal::InputCallbackInfo| {
                    health.callback_received();
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
                        health.block_delivered();
                        callback(&block);
                    }
                },
                move |err: cpal::Error| {
                    error_health.record_error(AudioError::StreamError(err.to_string()));
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

#[cfg(feature = "cpal")]
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

#[cfg(feature = "cpal")]
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

    #[test]
    #[cfg(feature = "cpal")]
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
    #[cfg(not(feature = "cpal"))]
    fn capture_headless_behavior() {
        #[cfg(not(feature = "mock-audio"))]
        {
            let err =
                Capture::new().expect_err("Capture::new must fail without audio hardware or mock");
            assert!(matches!(err, AudioError::NoInputDevice));
            let err_named = Capture::with_device_name("non-existent")
                .expect_err("device not found without mock");
            assert!(matches!(err_named, AudioError::DeviceNotFound(_)));
        }
        #[cfg(feature = "mock-audio")]
        {
            let mut cap = Capture::new().expect("mock capture can be created");
            assert_eq!(cap.device_name(), "Hakille Mock Input");
            assert_eq!(cap.sample_rate(), TARGET_SAMPLE_RATE);
            assert!(!cap.is_running());
            let (tx, rx) = std::sync::mpsc::channel();
            cap.start(move |block| {
                let _ = tx.send(block.to_vec());
            })
            .expect("start mock capture");
            assert!(cap.is_running());
            let block = rx
                .recv_timeout(std::time::Duration::from_millis(500))
                .expect("receive block");
            assert_eq!(block.len(), CAPTURE_BLOCK_SAMPLES);
            cap.stop().expect("stop mock capture");
            assert!(!cap.is_running());
        }
    }
}
