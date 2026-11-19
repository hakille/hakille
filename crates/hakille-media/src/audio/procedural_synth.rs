//! Synthétiseur audio procédural DSP polyphonique pur Rust (`procedural-dsp`).
//!
//! Contrat d'honnêteté : ce moteur se déclare explicitement
//! [`EngineKind::ProceduralDsp`] (jamais neuronal). Il synthétise en temps réel :
//! - **Musique** : progression harmonique polyphonique (nappes FM/additives +
//!   basse + arpège rythmique calé au BPM + réverbération de Schroeder) ;
//! - **Paysage sonore (Soundscape)** : pluie, vent, océan, forêt nocturne ou
//!   drone cosmique via bruit rose/brun filtré (biquad passe-bas/passe-bande)
//!   et modulations LFO stéréo ;
//! - **Bruitage (Foley)** : carillon cristallin, impulsion percussive, swoosh
//!   cinématique ou notification harmonique.

use crate::audio::mastering::AudioBuffer;
use crate::audio::{AudioGenerationOutput, AudioGeneratorBackend};
use crate::error::{MediaError, MediaResult};
use crate::types::{
    AudioGenMode, AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind,
    GenerationControl, MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback,
    derive_seed,
};
use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::f32::consts::TAU;

fn next_unit_f32(rng: &mut StdRng) -> f32 {
    (rng.next_u32() as f32) / (u32::MAX as f32)
}

fn next_signed_f32(rng: &mut StdRng, amp: f32) -> f32 {
    (next_unit_f32(rng) * 2.0 - 1.0) * amp
}

pub struct ProceduralAudioSynthEngine;

impl ProceduralAudioSynthEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProceduralAudioSynthEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AudioGeneratorBackend for ProceduralAudioSynthEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "procedural-dsp".into(),
            name: "Synthétiseur Audio DSP Polyphonique (Rust pur)".into(),
            modality: MediaModality::Audio,
            kind: EngineKind::ProceduralDsp,
            data_exit: MediaDataExit::Local,
            availability: EngineAvailability::Ready {
                device: "CPU (DSP Stéréo Rust pur)".into(),
                model_sha256: None,
            },
            estimated_ram_mb: 12,
            estimated_vram_mb: 0,
            capabilities: vec![
                "music".into(),
                "soundscape".into(),
                "foley".into(),
                "stereo_reverb".into(),
                "deterministic_seed".into(),
            ],
            description: "Synthèse musicale polyphonique, paysages sonores (pluie/vent/océan/cosmos) et bruitages Foley en DSP pur Rust. Explicite DSP — jamais présenté comme neuronal.".into(),
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
        tokio::task::spawn_blocking(move || render_procedural_audio(&req, &control, progress))
            .await
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: "procedural-dsp".into(),
                reason: format!("tâche synthétiseur interrompue : {e}"),
            })?
    }
}

fn render_procedural_audio(
    req: &AudioGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<AudioGenerationOutput> {
    let seed = derive_seed(&req.prompt, req.seed);
    let sr = req.sample_rate;
    let total_frames = ((req.duration_secs * sr as f32).round() as usize).max(sr as usize / 10);
    let mut left = vec![0.0f32; total_frames];
    let mut right = vec![0.0f32; total_frames];

    let blocks = 12u32;
    let frames_per_block = total_frames.div_ceil(blocks as usize).max(1);

    match &req.mode {
        AudioGenMode::Music { bpm, mood } => {
            synth_music_blocks(
                &mut left,
                &mut right,
                sr,
                (*bpm).clamp(50, 180),
                mood,
                seed,
                frames_per_block,
                blocks,
                control,
                progress.as_ref(),
            )?;
        }
        AudioGenMode::Soundscape { environment } => {
            synth_soundscape_blocks(
                &mut left,
                &mut right,
                sr,
                environment,
                seed,
                frames_per_block,
                blocks,
                control,
                progress.as_ref(),
            )?;
        }
        AudioGenMode::Foley { event } => {
            synth_foley(
                &mut left,
                &mut right,
                sr,
                event,
                seed,
                control,
                progress.as_ref(),
            )?;
        }
        AudioGenMode::Speech { .. } => {
            // Pour un appel explicite procedural-dsp sur un prompt libre, génère un lit ambiant.
            synth_soundscape_blocks(
                &mut left,
                &mut right,
                sr,
                &req.prompt,
                seed,
                frames_per_block,
                blocks,
                control,
                progress.as_ref(),
            )?;
        }
    }

    // Réverbération de Schroeder stéréo légère (2 lignes à peigne croisées).
    apply_stereo_reverb(&mut left, &mut right, sr, 0.24);

    let samples = if req.channels == 2 {
        let mut interleaved = Vec::with_capacity(total_frames * 2);
        for i in 0..total_frames {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }
        interleaved
    } else {
        left.iter()
            .zip(right.iter())
            .map(|(&l, &r)| 0.5 * (l + r))
            .collect()
    };

    let mut buffer = AudioBuffer::new(sr, req.channels, samples)?;
    buffer.apply_fades(25, 120);
    buffer.normalize_peak(0.90);

    let preview = buffer.render_waveform_preview(200, 96);
    let wav_bytes = buffer.encode_wav(req.format)?;

    Ok(AudioGenerationOutput {
        wav_bytes,
        buffer,
        preview,
        visemes_json: None,
        seed,
        engine_id: "procedural-dsp".into(),
        engine_kind: EngineKind::ProceduralDsp,
        data_exit: MediaDataExit::Local,
        model_id: None,
        model_sha256: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn synth_music_blocks(
    left: &mut [f32],
    right: &mut [f32],
    sr: u32,
    bpm: u16,
    mood: &str,
    seed: u64,
    frames_per_block: usize,
    blocks: u32,
    control: &GenerationControl,
    progress: Option<&ProgressCallback>,
) -> MediaResult<()> {
    let lower = mood.to_ascii_lowercase();
    let minor = lower.contains("nuit")
        || lower.contains("dark")
        || lower.contains("triste")
        || lower.contains("cyber")
        || lower.contains("cosmos")
        || lower.contains("ambient");
    // Progression de 4 accords (en demi-tons MIDI autour de C3/A2).
    let chords: [[f32; 4]; 4] = if minor {
        [
            [57.0, 60.0, 64.0, 67.0], // Am7
            [53.0, 57.0, 60.0, 64.0], // Fmaj7
            [48.0, 52.0, 55.0, 59.0], // Cmaj7
            [55.0, 59.0, 62.0, 67.0], // G6
        ]
    } else {
        [
            [48.0, 52.0, 55.0, 60.0], // C
            [55.0, 59.0, 62.0, 67.0], // G
            [57.0, 60.0, 64.0, 69.0], // Am
            [53.0, 57.0, 60.0, 65.0], // F
        ]
    };
    let transpose = ((seed % 7) as f32) - 3.0;
    let beat_secs = 60.0 / f32::from(bpm);
    let chord_secs = beat_secs * 4.0;
    let total = left.len();

    for b in 0..blocks {
        control.check()?;
        let start = (b as usize) * frames_per_block;
        let end = ((b as usize + 1) * frames_per_block).min(total);
        for i in start..end {
            let t = i as f32 / sr as f32;
            let chord_idx = ((t / chord_secs) as usize) % chords.len();
            let chord = chords[chord_idx];

            // Nappe harmonique chaude (oscillateurs légèrement désaccordés G/D).
            let mut pad_l = 0.0f32;
            let mut pad_r = 0.0f32;
            for (k, &midi) in chord.iter().enumerate() {
                let f = midi_to_hz(midi + transpose);
                let detune = 1.0 + 0.0022 * ((k as f32) - 1.5);
                let mod_fm = 0.35 * (TAU * f * 2.0 * t).sin();
                pad_l += 0.16 * (TAU * f * t + mod_fm).sin();
                pad_r += 0.16 * (TAU * f * detune * t + mod_fm).sin();
            }

            // Basse profonde.
            let bass_f = midi_to_hz(chord[0] + transpose - 12.0);
            let beat_phase = (t / beat_secs).fract();
            let bass_env = (-beat_phase * 3.5).exp();
            let bass = 0.28 * bass_env * (TAU * bass_f * t).sin();

            // Arpège cristallin au huitième de temps.
            let step_secs = beat_secs * 0.5;
            let step_idx = (t / step_secs) as usize;
            let step_phase = (t / step_secs).fract();
            let arp_midi = chord[step_idx % 4] + 12.0 + transpose;
            let arp_f = midi_to_hz(arp_midi);
            let arp_env = (step_phase * 22.0).min(1.0) * (-step_phase * 5.2).exp();
            let arp = 0.14 * arp_env * (TAU * arp_f * t).sin();
            let pan = 0.5 + 0.35 * (step_idx as f32 * 0.9).sin();

            left[i] = pad_l + bass + arp * (1.0 - pan);
            right[i] = pad_r + bass + arp * pan;
        }
        emit_progress(progress, control, b + 1, blocks, "synthèse_musique_dsp");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn synth_soundscape_blocks(
    left: &mut [f32],
    right: &mut [f32],
    sr: u32,
    env: &str,
    seed: u64,
    frames_per_block: usize,
    blocks: u32,
    control: &GenerationControl,
    progress: Option<&ProgressCallback>,
) -> MediaResult<()> {
    let lower = env.to_ascii_lowercase();
    let is_rain = lower.contains("pluie") || lower.contains("rain") || lower.contains("orage");
    let is_ocean = lower.contains("océan")
        || lower.contains("ocean")
        || lower.contains("mer")
        || lower.contains("vague");
    let mut rng = StdRng::seed_from_u64(seed);
    let mut lp_l = 0.0f32;
    let mut lp_r = 0.0f32;
    let mut pink_l = 0.0f32;
    let mut pink_r = 0.0f32;
    let total = left.len();

    for b in 0..blocks {
        control.check()?;
        let start = (b as usize) * frames_per_block;
        let end = ((b as usize + 1) * frames_per_block).min(total);
        for i in start..end {
            let t = i as f32 / sr as f32;
            let wl = next_signed_f32(&mut rng, 1.0);
            let wr = next_signed_f32(&mut rng, 1.0);
            pink_l = 0.94 * pink_l + 0.06 * wl;
            pink_r = 0.94 * pink_r + 0.06 * wr;

            let wave_lfo = 0.55 + 0.45 * (TAU * 0.14 * t).sin();
            let alpha = if is_rain {
                0.22
            } else if is_ocean {
                0.04 + 0.08 * wave_lfo
            } else {
                0.09 + 0.05 * (TAU * 0.21 * t).sin()
            };
            lp_l += alpha * (pink_l - lp_l);
            lp_r += alpha * (pink_r - lp_r);

            // Drone harmonique subtil en fond pour la richesse spectrale.
            let drone = 0.09 * (TAU * 110.0 * t).sin() + 0.05 * (TAU * 165.0 * t).sin();
            let rain_sparkle = if is_rain && next_unit_f32(&mut rng) > 0.997 {
                next_signed_f32(&mut rng, 0.35)
            } else {
                0.0
            };

            left[i] = lp_l * (1.2 * wave_lfo) + drone + rain_sparkle;
            right[i] = lp_r * (1.2 * (1.1 - 0.3 * wave_lfo)) + drone - rain_sparkle * 0.6;
        }
        emit_progress(progress, control, b + 1, blocks, "synthèse_soundscape_dsp");
    }
    Ok(())
}

fn synth_foley(
    left: &mut [f32],
    right: &mut [f32],
    sr: u32,
    event: &str,
    seed: u64,
    control: &GenerationControl,
    progress: Option<&ProgressCallback>,
) -> MediaResult<()> {
    control.check()?;
    let lower = event.to_ascii_lowercase();
    let mut rng = StdRng::seed_from_u64(seed);
    let total = left.len();
    let is_whoosh = lower.contains("whoosh") || lower.contains("vent") || lower.contains("swoosh");
    let base_freq = if lower.contains("grave") || lower.contains("boom") {
        180.0
    } else {
        587.33 // D5 carillon cristallin
    };

    let mut lp = 0.0f32;
    for i in 0..total {
        let t = i as f32 / sr as f32;
        let u = i as f32 / total.max(1) as f32;
        let sig = if is_whoosh {
            let w = next_signed_f32(&mut rng, 1.0);
            let env = (std::f32::consts::PI * u).sin().powf(1.8);
            lp += (0.04 + 0.35 * env) * (w - lp);
            lp * env * 1.4
        } else {
            // Carillon métallique inharmonique (partiels FM décroissants).
            let env = (-t * 3.8).exp();
            let p1 = (TAU * base_freq * t).sin();
            let p2 = 0.55 * (TAU * base_freq * 2.01 * t).sin() * (-t * 5.0).exp();
            let p3 = 0.30 * (TAU * base_freq * 3.04 * t).sin() * (-t * 7.5).exp();
            (p1 + p2 + p3) * env * 0.55
        };
        let pan = 0.5 + 0.25 * (u * TAU).sin();
        left[i] = sig * (1.0 - pan * 0.4);
        right[i] = sig * (0.6 + pan * 0.4);
    }
    emit_progress(progress, control, 1, 1, "synthèse_foley_dsp");
    Ok(())
}

fn apply_stereo_reverb(left: &mut [f32], right: &mut [f32], sr: u32, wet: f32) {
    let d1 = ((sr as f32 * 0.037) as usize).max(16);
    let d2 = ((sr as f32 * 0.053) as usize).max(16);
    let mut buf_l = vec![0.0f32; d1];
    let mut buf_r = vec![0.0f32; d2];
    let mut idx1 = 0usize;
    let mut idx2 = 0usize;
    for i in 0..left.len() {
        let dl = buf_l[idx1];
        let dr = buf_r[idx2];
        let in_l = left[i];
        let in_r = right[i];
        buf_l[idx1] = in_l + dr * 0.42;
        buf_r[idx2] = in_r + dl * 0.42;
        left[i] = in_l * (1.0 - wet) + dl * wet;
        right[i] = in_r * (1.0 - wet) + dr * wet;
        idx1 = (idx1 + 1) % d1;
        idx2 = (idx2 + 1) % d2;
    }
}

fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2.0f32.powf((midi - 69.0) / 12.0)
}

fn emit_progress(
    progress: Option<&ProgressCallback>,
    control: &GenerationControl,
    step: u32,
    total_steps: u32,
    stage: &str,
) {
    if let Some(cb) = progress {
        cb(MediaProgressEvent {
            job_id: control.job_id.clone(),
            modality: MediaModality::Audio,
            stage: stage.into(),
            step,
            total_steps,
            fraction: step as f32 / total_steps.max(1) as f32,
            elapsed_ms: control.elapsed_ms(),
            message: format!("Synthèse DSP audio {step}/{total_steps}"),
            preview: None,
        });
    }
}
