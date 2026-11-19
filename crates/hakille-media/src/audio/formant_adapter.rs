//! Adaptateur de synthèse vocale formantique réelle (`formant-dsp`).
//!
//! Enveloppe `hakille_voice::FormantTts` / `FormantStream` (Klatt 24 kHz pur Rust)
//! et exporte la timeline synchronisée de visèmes phonémiques (`VisemeFrame`).
//! Se déclare explicitement [`EngineKind::FormantDsp`] (jamais neuronal).

use crate::audio::mastering::AudioBuffer;
use crate::audio::{AudioGenerationOutput, AudioGeneratorBackend};
use crate::error::{MediaError, MediaResult};
use crate::types::{
    AudioGenMode, AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind,
    GenerationControl, MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback,
    derive_seed,
};
use async_trait::async_trait;
use hakille_voice::{FORMANT_SAMPLE_RATE, FormantStream, FormantTts, TtsRequest};

pub struct FormantSpeechEngine;

impl FormantSpeechEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FormantSpeechEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AudioGeneratorBackend for FormantSpeechEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "formant-dsp".into(),
            name: "Synthèse Vocale Formantique Klatt (hakille-voice)".into(),
            modality: MediaModality::Audio,
            kind: EngineKind::FormantDsp,
            data_exit: MediaDataExit::Local,
            availability: EngineAvailability::Ready {
                device: "CPU (Formants Klatt 24 kHz)".into(),
                model_sha256: None,
            },
            estimated_ram_mb: 8,
            estimated_vram_mb: 0,
            capabilities: vec![
                "speech".into(),
                "viseme_timeline".into(),
                "fr_en_phonemizer".into(),
                "zero_model_required".into(),
            ],
            description: "Synthèse vocale formantique réelle 24 kHz (phonémiseur FR/EN + source glottique + 5 formants + export visèmes JSON). Explicite Formant DSP.".into(),
        }
    }

    async fn generate(
        &self,
        req: &AudioGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<AudioGenerationOutput> {
        let req = req.clone();
        let control = control.clone();
        tokio::task::spawn_blocking(move || synthesize_formant(&req, &control, progress))
            .await
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: "formant-dsp".into(),
                reason: format!("tâche formantique interrompue : {e}"),
            })?
    }
}

fn synthesize_formant(
    req: &AudioGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<AudioGenerationOutput> {
    let (voice_id, speed_rate, emotional_tone) = match &req.mode {
        AudioGenMode::Speech {
            voice,
            speed,
            emotion,
            ..
        } => (voice.clone(), *speed, emotion.clone()),
        _ => ("hakille-neutre".to_string(), 1.0f32, "calm".to_string()),
    };

    let tts = FormantTts::new();
    let tts_req = TtsRequest {
        text: req.prompt.clone(),
        voice_id,
        speed_rate: speed_rate.clamp(0.5, 2.0),
        emotional_tone,
    };
    let voice = tts.voice_for_request(&tts_req);
    let mut stream = FormantStream::new(&req.prompt, &voice);
    let total_samples = stream.total_samples();
    if total_samples == 0 {
        return Err(MediaError::InvalidRequest(
            "aucun phonème synthétisable dans le texte fourni".into(),
        ));
    }

    let visemes_json = if req.emit_visemes {
        let timeline = stream.viseme_timeline();
        let payload = serde_json::json!({
            "sample_rate": FORMANT_SAMPLE_RATE,
            "total_samples": total_samples,
            "frames": timeline,
        });
        Some(serde_json::to_vec_pretty(&payload)?)
    } else {
        None
    };

    let chunk_size = 4_800usize; // 200 ms par étape
    let total_chunks = total_samples.div_ceil(chunk_size).max(1) as u32;
    let mut all_samples = Vec::with_capacity(total_samples);
    let mut step = 0u32;

    while !stream.is_done() {
        control.check()?;
        let chunk = stream.next_chunk(chunk_size);
        if chunk.is_empty() {
            break;
        }
        all_samples.extend_from_slice(&chunk);
        step = step.saturating_add(1);
        if let Some(cb) = &progress {
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Audio,
                stage: "synthèse_formantique".into(),
                step,
                total_steps: total_chunks,
                fraction: (step as f32 / total_chunks as f32).min(1.0),
                elapsed_ms: control.elapsed_ms(),
                message: format!("Synthèse vocale Klatt {step}/{total_chunks}"),
                preview: None,
            });
        }
    }

    let mut buffer = AudioBuffer::new(FORMANT_SAMPLE_RATE, 1, all_samples)?
        .resample(req.sample_rate)
        .to_channels(req.channels);
    buffer.apply_fades(8, 30);
    buffer.normalize_peak(0.92);

    let preview = buffer.render_waveform_preview(200, 96);
    let wav_bytes = buffer.encode_wav(req.format)?;
    let seed = derive_seed(&req.prompt, req.seed);

    Ok(AudioGenerationOutput {
        wav_bytes,
        buffer,
        preview,
        visemes_json,
        seed,
        engine_id: "formant-dsp".into(),
        engine_kind: EngineKind::FormantDsp,
        data_exit: MediaDataExit::Local,
        model_id: None,
        model_sha256: None,
    })
}
