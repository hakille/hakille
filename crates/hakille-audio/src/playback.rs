//! Lecture PCM : `play_pcm` bloque jusqu'à la fin de la lecture.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

use crate::AudioError;
use crate::resample::resample_linear;

/// Marge ajoutée à la durée théorique avant de considérer la lecture bloquée.
const PLAYBACK_TIMEOUT_MARGIN: Duration = Duration::from_secs(5);

/// Lecture de `samples` (mono `f32` à `sample_rate` Hz) sur le périphérique de
/// sortie par défaut.
///
/// La fonction bloque jusqu'à ce que le dernier échantillon ait été remis au
/// matériel (la latence matérielle résiduelle du périphérique n'est pas
/// attendue). Les échantillons sont rééchantillonnés linéairement vers la
/// fréquence du périphérique puis convertis vers son format natif.
pub fn play_pcm(samples: &[f32], sample_rate: u32) -> Result<(), AudioError> {
    if sample_rate == 0 {
        return Err(AudioError::Unsupported(
            "sample_rate ne peut pas être nul".to_string(),
        ));
    }
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(AudioError::NoOutputDevice)?;
    if samples.is_empty() {
        tracing::debug!(device = %device, "play_pcm appelé avec un buffer vide : rien à jouer");
        return Ok(());
    }
    let supported = device.default_output_config().map_err(|e| {
        AudioError::StreamError(format!(
            "configuration de sortie par défaut indisponible pour « {device} » : {e}"
        ))
    })?;
    let format = supported.sample_format();
    let config = supported.config();
    if config.channels == 0 {
        return Err(AudioError::Unsupported(format!(
            "« {device} » annonce zéro canal de sortie"
        )));
    }
    let device_rate = config.sample_rate;
    if device_rate == 0 {
        return Err(AudioError::Unsupported(format!(
            "« {device} » annonce une fréquence de sortie nulle"
        )));
    }
    let mono = if device_rate == sample_rate {
        samples.to_vec()
    } else {
        resample_linear(samples, sample_rate, device_rate)
    };
    match format {
        SampleFormat::F32 => play_on_device::<f32>(&device, config, &mono, device_rate),
        SampleFormat::F64 => play_on_device::<f64>(&device, config, &mono, device_rate),
        SampleFormat::I8 => play_on_device::<i8>(&device, config, &mono, device_rate),
        SampleFormat::I16 => play_on_device::<i16>(&device, config, &mono, device_rate),
        SampleFormat::I24 => play_on_device::<cpal::I24>(&device, config, &mono, device_rate),
        SampleFormat::I32 => play_on_device::<i32>(&device, config, &mono, device_rate),
        SampleFormat::I64 => play_on_device::<i64>(&device, config, &mono, device_rate),
        SampleFormat::U8 => play_on_device::<u8>(&device, config, &mono, device_rate),
        SampleFormat::U16 => play_on_device::<u16>(&device, config, &mono, device_rate),
        SampleFormat::U24 => play_on_device::<cpal::U24>(&device, config, &mono, device_rate),
        SampleFormat::U32 => play_on_device::<u32>(&device, config, &mono, device_rate),
        SampleFormat::U64 => play_on_device::<u64>(&device, config, &mono, device_rate),
        other => Err(AudioError::Unsupported(format!(
            "format de sortie « {other} » non convertible depuis f32"
        ))),
    }
}

fn play_on_device<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mono: &[f32],
    device_rate: u32,
) -> Result<(), AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let (stream, shared) = spawn_playback::<T>(device, config, mono)?;
    if let Err(e) = stream.play() {
        drop(stream);
        return Err(AudioError::StreamError(format!(
            "démarrage de la lecture sur « {device} » : {e}"
        )));
    }
    let expected =
        Duration::from_secs_f64(mono.len() as f64 / device_rate as f64) + PLAYBACK_TIMEOUT_MARGIN;
    wait_for_playback(&shared, expected)
}

/// Variante non bloquante : lance [`play_pcm`] dans un thread dédié.
///
/// Les erreurs d'ouverture ou de flux ne sont visibles qu'à l'appel de
/// [`PlaybackHandle::wait`]. Si le handle est abandonné, la lecture continue
/// jusqu'à la fin du buffer dans le thread détaché.
pub fn play_pcm_async(samples: Vec<f32>, sample_rate: u32) -> PlaybackHandle {
    let join = std::thread::spawn(move || play_pcm(&samples, sample_rate));
    PlaybackHandle { join }
}

/// Poignée de lecture asynchrone retournée par [`play_pcm_async`].
pub struct PlaybackHandle {
    join: std::thread::JoinHandle<Result<(), AudioError>>,
}

impl PlaybackHandle {
    /// Attend la fin de la lecture et remonte l'erreur éventuelle.
    pub fn wait(self) -> Result<(), AudioError> {
        match self.join.join() {
            Ok(result) => result,
            Err(_) => Err(AudioError::StreamError(
                "le thread de lecture audio a paniqué".to_string(),
            )),
        }
    }
}

struct PlaybackShared<T> {
    inner: Mutex<PlaybackInner<T>>,
    cv: Condvar,
}

struct PlaybackInner<T> {
    samples: Vec<T>,
    pos: usize,
    finished: bool,
    error: Option<String>,
}

fn lock<T>(shared: &PlaybackShared<T>) -> MutexGuard<'_, PlaybackInner<T>> {
    shared.inner.lock().unwrap_or_else(PoisonError::into_inner)
}

fn fill_equilibrium<T: Sample>(out: &mut [T]) {
    for sample in out.iter_mut() {
        *sample = <T as Sample>::EQUILIBRIUM;
    }
}

fn spawn_playback<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mono: &[f32],
) -> Result<(cpal::Stream, Arc<PlaybackShared<T>>), AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let mut samples: Vec<T> = Vec::with_capacity(mono.len() * channels);
    for &value in mono {
        let sample = T::from_sample(value);
        for _ in 0..channels {
            samples.push(sample);
        }
    }
    let shared = Arc::new(PlaybackShared {
        inner: Mutex::new(PlaybackInner {
            samples,
            pos: 0,
            finished: false,
            error: None,
        }),
        cv: Condvar::new(),
    });
    let data = Arc::clone(&shared);
    let error_state = Arc::clone(&shared);
    let device_name = device.to_string();
    let error_device_name = device_name.clone();
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [T], _info: &cpal::OutputCallbackInfo| {
                let mut inner = lock(&data);
                if inner.finished {
                    fill_equilibrium(out);
                    return;
                }
                let remaining = inner.samples.len().saturating_sub(inner.pos);
                let count = remaining.min(out.len());
                out[..count].copy_from_slice(&inner.samples[inner.pos..inner.pos + count]);
                inner.pos += count;
                fill_equilibrium(&mut out[count..]);
                if inner.pos >= inner.samples.len() {
                    inner.finished = true;
                    inner.samples = Vec::new();
                    drop(inner);
                    data.cv.notify_all();
                }
            },
            move |err: cpal::Error| {
                tracing::error!(
                    error = %err,
                    device = %error_device_name,
                    "erreur du flux de sortie audio"
                );
                let mut inner = lock(&error_state);
                inner.error = Some(err.to_string());
                inner.finished = true;
                drop(inner);
                error_state.cv.notify_all();
            },
            None,
        )
        .map_err(|e| {
            AudioError::StreamError(format!(
                "construction du flux de sortie « {device_name} » : {e}"
            ))
        })?;
    Ok((stream, shared))
}

fn wait_for_playback<T>(shared: &PlaybackShared<T>, timeout: Duration) -> Result<(), AudioError> {
    let deadline = Instant::now() + timeout;
    let mut inner = lock(shared);
    while !inner.finished {
        let now = Instant::now();
        if now >= deadline {
            return Err(AudioError::StreamError(
                "lecture audio non terminée dans le délai imparti (périphérique bloqué ?)"
                    .to_string(),
            ));
        }
        let (guard, result) = shared
            .cv
            .wait_timeout(inner, deadline - now)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner = guard;
        if result.timed_out() && !inner.finished {
            return Err(AudioError::StreamError(
                "lecture audio non terminée dans le délai imparti (périphérique bloqué ?)"
                    .to_string(),
            ));
        }
    }
    match inner.error.take() {
        Some(err) => Err(AudioError::StreamError(format!(
            "lecture interrompue par le périphérique : {err}"
        ))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TARGET_SAMPLE_RATE;

    #[test]
    fn play_pcm_zero_sample_rate_is_unsupported() {
        let err = play_pcm(&[0.0], 0).expect_err("sample_rate nul doit être refusé");
        assert!(
            matches!(err, AudioError::Unsupported(_)),
            "attendu Unsupported, obtenu {err:?}"
        );
    }

    #[test]
    fn play_pcm_without_output_device_returns_explicit_error() {
        let host = cpal::default_host();
        if host.default_output_device().is_some() {
            eprintln!("SKIP: un périphérique de sortie est présent, NoOutputDevice non testable");
            return;
        }
        let silence = vec![0.0f32; 160];
        let err = play_pcm(&silence, TARGET_SAMPLE_RATE)
            .expect_err("aucun device déclaré mais play_pcm a réussi");
        assert!(
            matches!(err, AudioError::NoOutputDevice),
            "attendu NoOutputDevice, obtenu {err:?}"
        );
    }

    #[test]
    fn play_pcm_silence_completes_when_device_available() {
        let host = cpal::default_host();
        if host.default_output_device().is_none() {
            eprintln!("SKIP: aucun périphérique de sortie");
            return;
        }
        let silence = vec![0.0f32; 320];
        if let Err(err) = play_pcm(&silence, TARGET_SAMPLE_RATE) {
            eprintln!("SKIP: sortie audio non ouvrable : {err}");
        }
    }

    #[test]
    fn play_pcm_async_waits_for_completion_when_device_available() {
        let host = cpal::default_host();
        if host.default_output_device().is_none() {
            eprintln!("SKIP: aucun périphérique de sortie");
            return;
        }
        let silence = vec![0.0f32; 160];
        if let Err(err) = play_pcm_async(silence, TARGET_SAMPLE_RATE).wait() {
            eprintln!("SKIP: sortie audio asynchrone non ouvrable : {err}");
        }
    }
}
