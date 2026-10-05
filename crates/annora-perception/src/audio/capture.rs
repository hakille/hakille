use annora_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp: Timestamp,
    pub samples: Vec<f32>,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            timestamp: Timestamp::now(),
            samples,
        }
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            0.0
        } else {
            self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
        }
    }
}

pub struct ContinuousAudioRingBuffer {
    buffer: VecDeque<f32>,
    capacity: usize,
    sample_rate: u32,
}

impl ContinuousAudioRingBuffer {
    pub fn new(capacity_seconds: usize, sample_rate: u32) -> Self {
        let capacity = capacity_seconds * sample_rate as usize;
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            sample_rate,
        }
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        for &s in samples {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(s);
        }
    }

    pub fn pop_chunk(&mut self, chunk_size: usize) -> Option<AudioChunk> {
        if self.buffer.len() < chunk_size {
            return None;
        }
        let chunk_samples: Vec<f32> = self.buffer.drain(..chunk_size).collect();
        Some(AudioChunk::new(chunk_samples, self.sample_rate, 1))
    }

    pub fn current_len(&self) -> usize {
        self.buffer.len()
    }
}
