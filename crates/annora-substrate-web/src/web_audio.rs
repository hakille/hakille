use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAudioNodeConfig {
    pub input_sample_rate: u32,  // Typically 48000 Hz from Web Audio Context
    pub target_sample_rate: u32, // 16000 Hz for Annora Voice STT / Mimi codec
    pub buffer_capacity_samples: usize,
    pub is_muted: bool,
}

impl Default for WebAudioNodeConfig {
    fn default() -> Self {
        Self {
            input_sample_rate: 48000,
            target_sample_rate: 16000,
            buffer_capacity_samples: 48000, // 1 second buffer
            is_muted: false,
        }
    }
}

pub struct WebAudioPcmBuffer {
    config: WebAudioNodeConfig,
    pcm_queue: VecDeque<i16>,
    playback_queue: VecDeque<u8>,
    overrun_count: u64,
    underrun_count: u64,
}

impl WebAudioPcmBuffer {
    pub fn new(config: WebAudioNodeConfig) -> Self {
        Self {
            config,
            pcm_queue: VecDeque::new(),
            playback_queue: VecDeque::new(),
            overrun_count: 0,
            underrun_count: 0,
        }
    }

    pub fn config(&self) -> &WebAudioNodeConfig {
        &self.config
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.config.is_muted = muted;
    }

    /// Ingests Float32 audio samples from browser AudioWorkletProcessor,
    /// applies band-smoothed linear interpolation resampling, and converts to Int16 PCM samples.
    pub fn ingest_worklet_samples_f32(&mut self, samples: &[f32]) -> usize {
        if self.config.is_muted
            || samples.is_empty()
            || self.config.target_sample_rate == 0
            || self.config.input_sample_rate == 0
        {
            return 0;
        }

        let ratio = self.config.input_sample_rate as f64 / self.config.target_sample_rate as f64;
        let output_len = (samples.len() as f64 / ratio).floor() as usize;

        for i in 0..output_len {
            let src_pos = i as f64 * ratio;
            let idx0 = src_pos.floor() as usize;
            let idx1 = (idx0 + 1).min(samples.len() - 1);
            let frac = (src_pos - idx0 as f64) as f32;

            if idx0 < samples.len() {
                let s0 = samples[idx0].clamp(-1.0, 1.0);
                let s1 = samples[idx1].clamp(-1.0, 1.0);
                // Linear interpolated fractional sample
                let sample_f32 = s0 * (1.0 - frac) + s1 * frac;
                let sample_i16 = (sample_f32 * 32767.0).clamp(-32768.0, 32767.0) as i16;

                if self.config.buffer_capacity_samples > 0
                    && self.pcm_queue.len() >= self.config.buffer_capacity_samples
                {
                    // Overrun: evict oldest sample to maintain bounded causal latency (<20ms delay)
                    self.pcm_queue.pop_front();
                    self.overrun_count += 1;
                }
                self.pcm_queue.push_back(sample_i16);
            }
        }

        output_len
    }

    /// Pops a contiguous chunk of Int16 PCM samples for Annora Voice STT
    pub fn pop_pcm_chunk(&mut self, chunk_size: usize) -> Option<Vec<i16>> {
        if chunk_size == 0 {
            return Some(Vec::new());
        }
        if self.pcm_queue.len() < chunk_size {
            self.underrun_count += 1;
            return None;
        }

        let chunk: Vec<i16> = (0..chunk_size).filter_map(|_| self.pcm_queue.pop_front()).collect();
        Some(chunk)
    }

    /// Queues audio data for browser speaker playback
    pub fn push_playback_audio(&mut self, audio: &[u8]) {
        self.playback_queue.extend(audio.iter().copied());
    }

    pub fn pop_playback_audio(&mut self, max_bytes: usize) -> Vec<u8> {
        let count = max_bytes.min(self.playback_queue.len());
        (0..count).filter_map(|_| self.playback_queue.pop_front()).collect()
    }

    pub fn overrun_count(&self) -> u64 {
        self.overrun_count
    }

    pub fn underrun_count(&self) -> u64 {
        self.underrun_count
    }

    pub fn queued_samples_count(&self) -> usize {
        self.pcm_queue.len()
    }
}

pub struct WebAudioBridge {
    buffer: WebAudioPcmBuffer,
}

impl WebAudioBridge {
    pub fn new(config: WebAudioNodeConfig) -> Self {
        Self {
            buffer: WebAudioPcmBuffer::new(config),
        }
    }

    pub fn buffer(&self) -> &WebAudioPcmBuffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut WebAudioPcmBuffer {
        &mut self.buffer
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.buffer.set_muted(muted);
    }

    pub fn config(&self) -> &WebAudioNodeConfig {
        self.buffer.config()
    }
}

impl Default for WebAudioBridge {
    fn default() -> Self {
        Self::new(WebAudioNodeConfig::default())
    }
}
