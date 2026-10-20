use hakille_core::Timestamp;
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

/// Tampon circulaire continu : aucun échantillon n'est perdu ni réordonné
/// tant que la capacité suffit ; en dépassement, les plus anciens sont
/// évincés en premier (FIFO documenté, `evicted_total()` pour l'audit).
pub struct ContinuousAudioRingBuffer {
    buffer: VecDeque<f32>,
    capacity: usize,
    sample_rate: u32,
    evicted_total: u64,
}

impl ContinuousAudioRingBuffer {
    pub fn new(capacity_seconds: usize, sample_rate: u32) -> Self {
        let capacity = capacity_seconds * sample_rate as usize;
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            sample_rate,
            evicted_total: 0,
        }
    }

    /// Capacité maximale (échantillons).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Échantillons évincés par dépassement depuis la création (audit de perte).
    pub fn evicted_total(&self) -> u64 {
        self.evicted_total
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        for &s in samples {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
                self.evicted_total += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32 * 0.5).collect()
    }

    #[test]
    fn ring_buffer_lossless_ordered_roundtrip() {
        let mut rb = ContinuousAudioRingBuffer::new(10, 16_000);
        assert!(rb.is_empty());
        let src = ramp(5000);
        // Poussées morcelées de tailles impaires (aucune perte, aucun réordre).
        let mut off = 0;
        for step in [100usize, 7, 1024, 513, 255, 999].iter().cycle().take(20) {
            if off >= src.len() {
                break;
            }
            let end = (off + step).min(src.len());
            rb.push_samples(&src[off..end]);
            off = end;
        }
        assert_eq!(rb.current_len(), 5000);
        assert_eq!(rb.evicted_total(), 0);
        // Lectures en chunks 512 : 9×512 = 4608, reste 392.
        let mut collected = Vec::new();
        for _ in 0..9 {
            let c = rb.pop_chunk(512).expect("chunk disponible");
            assert_eq!(c.samples.len(), 512);
            assert_eq!(c.sample_rate, 16_000);
            collected.extend_from_slice(&c.samples);
        }
        assert_eq!(rb.current_len(), 392);
        assert!(rb.pop_chunk(512).is_none(), "pas de chunk partiel");
        let tail = rb.pop_chunk(392).expect("reste exact");
        collected.extend_from_slice(&tail.samples);
        assert!(rb.is_empty());
        assert_eq!(collected.len(), src.len());
        for (i, (a, b)) in collected.iter().zip(src.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-6,
                "échantillon {i} perdu ou réordonné: {a} vs {b}"
            );
        }
    }

    #[test]
    fn ring_buffer_overflow_evicts_oldest_first() {
        // Capacité 1000 échantillons.
        let mut rb = ContinuousAudioRingBuffer::new(1, 1000);
        assert_eq!(rb.capacity(), 1000);
        rb.push_samples(&ramp(1000));
        assert_eq!(rb.evicted_total(), 0);
        // +300 -> 300 plus anciens évincés, ordre conservé sur les 1000 restants.
        let extra: Vec<f32> = (0..300).map(|i| 100_000.0 + i as f32).collect();
        rb.push_samples(&extra);
        assert_eq!(rb.evicted_total(), 300);
        assert_eq!(rb.current_len(), 1000);
        let c = rb.pop_chunk(1000).expect("chunk plein");
        // Les 700 derniers du ramp (150.0..=499.5 par pas de 0.5) + extra.
        assert!((c.samples[0] - 150.0).abs() < 1e-6);
        assert!((c.samples[699] - 499.5).abs() < 1e-6);
        assert!((c.samples[700] - 100_000.0).abs() < 1e-6);
        assert!((c.samples[999] - 100_299.0).abs() < 1e-6);
    }

    #[test]
    fn ring_buffer_clear_and_empty_pop() {
        let mut rb = ContinuousAudioRingBuffer::new(1, 16_000);
        assert!(rb.pop_chunk(1).is_none());
        rb.push_samples(&[1.0, 2.0]);
        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.current_len(), 0);
        assert!(rb.pop_chunk(1).is_none());
    }

    #[test]
    fn audio_chunk_duration_seconds() {
        let c = AudioChunk::new(vec![0.0; 1600], 16_000, 1);
        assert!((c.duration_seconds() - 0.1).abs() < 1e-6);
        let bad = AudioChunk::new(vec![0.0; 10], 0, 0);
        assert_eq!(bad.duration_seconds(), 0.0);
    }
}
