use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Erreurs spécifiques au tampon circulaire audio.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RingBufferError {
    #[error("Échantillons insuffisants dans le buffer (disponibles: {available}, demandés: {requested})")]
    InsufficientSamples {
        available: usize,
        requested: usize,
    },
    #[error("Capacité du buffer dépassée ou paramètres invalides")]
    InvalidParameters,
}

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

/// Constantes standardisées pour les fenêtres temporelles d'inférence (@16 kHz mono).
pub const SAMPLES_PER_10MS: usize = 160;     // CPAL capture block (10 ms)
pub const SAMPLES_PER_100MS: usize = 1_600;  // Fenêtre prosodie YIN/pYIN (100 ms)
pub const SAMPLES_PER_2S: usize = 32_000;    // Fenêtre acoustique SenseVoice (2,0 s)

/// Tampon circulaire continu thread-safe garantissant :
/// 1. Zéro allocation dynamique sur le chemin chaud temps réel (mémoire pré-allouée fixe).
/// 2. Écritures rapides et non bloquantes pour le callback audio CPAL (copie mémoire directe).
/// 3. Lectures fenêtrées concurrentes sans destruction (non-destructives) pour YIN (100 ms) et SenseVoice (2 s).
/// 4. Compatibilité intégrale avec l'API existante (`push_samples`, `pop_chunk`, `evicted_total`).
pub struct ContinuousAudioRingBuffer {
    inner: RwLock<RingBufferInner>,
    sample_rate: u32,
    channels: u16,
    capacity_samples: usize,
    total_written: AtomicU64,
    evicted_total: AtomicU64,
}

struct RingBufferInner {
    buffer: Box<[f32]>,
    write_head: usize,
    current_len: usize,
}

impl ContinuousAudioRingBuffer {
    /// Construit un buffer circulaire continu pré-alloué pour `capacity_seconds` à `sample_rate`.
    pub fn new(capacity_seconds: usize, sample_rate: u32) -> Self {
        let capacity_samples = capacity_seconds * sample_rate as usize;
        Self {
            inner: RwLock::new(RingBufferInner {
                buffer: vec![0.0f32; capacity_samples].into_boxed_slice(),
                write_head: 0,
                current_len: 0,
            }),
            sample_rate,
            channels: 1,
            capacity_samples,
            total_written: AtomicU64::new(0),
            evicted_total: AtomicU64::new(0),
        }
    }

    /// Capacité maximale en nombre d'échantillons.
    pub fn capacity(&self) -> usize {
        self.capacity_samples
    }

    /// Fréquence d'échantillonnage en Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Nombre total d'échantillons écrits depuis la création.
    pub fn total_written(&self) -> u64 {
        self.total_written.load(Ordering::Relaxed)
    }

    /// Nombre total d'échantillons évincés par dépassement de capacité (audit de perte).
    pub fn evicted_total(&self) -> u64 {
        self.evicted_total.load(Ordering::Relaxed)
    }

    /// Indique si le buffer ne contient aucun échantillon.
    pub fn is_empty(&self) -> bool {
        self.current_len() == 0
    }

    /// Nombre actuel d'échantillons stockés dans le buffer.
    pub fn current_len(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.current_len
    }

    /// Réinitialise le contenu du buffer sans réallouer la mémoire.
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.write_head = 0;
        inner.current_len = 0;
    }

    /// Injection d'échantillons depuis le thread audio CPAL.
    /// ZÉRO allocation dynamique.
    /// Verrou exclusif court (quelques dizaines de nanosecondes pour 160 échantillons).
    pub fn push_samples(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let n = samples.len();
        let cap = self.capacity_samples;

        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let head = inner.write_head;
        let old_len = inner.current_len;
        let evicted = if n + old_len > cap {
            (n + old_len - cap) as u64
        } else {
            0
        };

        if n >= cap {
            // Le bloc dépasse la capacité totale : on ne retient que les `cap` plus récents.
            let start = n - cap;
            inner.buffer.copy_from_slice(&samples[start..]);
            inner.write_head = 0;
            inner.current_len = cap;
        } else {
            let space_to_end = cap - head;
            if n <= space_to_end {
                inner.buffer[head..head + n].copy_from_slice(samples);
                inner.write_head = (head + n) % cap;
            } else {
                inner.buffer[head..cap].copy_from_slice(&samples[..space_to_end]);
                let remainder = n - space_to_end;
                inner.buffer[..remainder].copy_from_slice(&samples[space_to_end..]);
                inner.write_head = remainder;
            }
            inner.current_len = (old_len + n).min(cap);
        }

        self.total_written.fetch_add(n as u64, Ordering::Relaxed);
        if evicted > 0 {
            self.evicted_total.fetch_add(evicted, Ordering::Relaxed);
        }
    }

    /// Extraction d'une fenêtre temporelle récente dans un buffer fourni par l'appelant.
    /// ZÉRO allocation dynamique sur le chemin chaud.
    /// Lecture non-destructive : les échantillons restent disponibles pour les autres consommateurs.
    pub fn read_latest_into(&self, dest: &mut [f32]) -> Result<usize, RingBufferError> {
        let want = dest.len();
        if want == 0 {
            return Ok(0);
        }
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let available = inner.current_len;
        if available < want {
            return Err(RingBufferError::InsufficientSamples {
                available,
                requested: want,
            });
        }
        let cap = self.capacity_samples;
        let head = inner.write_head;

        // Le début de la fenêtre se situe `want` échantillons avant `head`.
        let start = if head >= want {
            head - want
        } else {
            cap - (want - head)
        };

        if start + want <= cap {
            dest.copy_from_slice(&inner.buffer[start..start + want]);
        } else {
            let first_chunk = cap - start;
            dest[..first_chunk].copy_from_slice(&inner.buffer[start..cap]);
            let second_chunk = want - first_chunk;
            dest[first_chunk..].copy_from_slice(&inner.buffer[..second_chunk]);
        }
        Ok(want)
    }

    /// Récupère la dernière fenêtre de 100 ms (1 600 échantillons @ 16 kHz) pour YIN/pYIN.
    /// ZÉRO allocation.
    pub fn read_prosody_100ms(&self, dest: &mut [f32; SAMPLES_PER_100MS]) -> Result<(), RingBufferError> {
        self.read_latest_into(dest).map(|_| ())
    }

    /// Récupère la dernière fenêtre de 2,0 secondes (32 000 échantillons @ 16 kHz) pour SenseVoice.
    /// ZÉRO allocation.
    pub fn read_sensevoice_2s(&self, dest: &mut [f32; SAMPLES_PER_2S]) -> Result<(), RingBufferError> {
        self.read_latest_into(dest).map(|_| ())
    }

    /// Récupère une copie de la dernière fenêtre sous forme d'`AudioChunk` (lecture non-destructive).
    pub fn get_latest_window(&self, samples_count: usize) -> Option<AudioChunk> {
        let mut buf = vec![0.0f32; samples_count];
        if self.read_latest_into(&mut buf).is_ok() {
            Some(AudioChunk::new(buf, self.sample_rate, self.channels))
        } else {
            None
        }
    }

    /// Consommation séquentielle destructive (maintien de la compatibilité ascendante avec pop_chunk).
    pub fn pop_chunk(&self, chunk_size: usize) -> Option<AudioChunk> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.current_len < chunk_size {
            return None;
        }

        let cap = self.capacity_samples;
        let head = inner.write_head;
        let start = if head >= inner.current_len {
            head - inner.current_len
        } else {
            cap - (inner.current_len - head)
        };

        let mut chunk_samples = vec![0.0f32; chunk_size];
        if start + chunk_size <= cap {
            chunk_samples.copy_from_slice(&inner.buffer[start..start + chunk_size]);
        } else {
            let first = cap - start;
            chunk_samples[..first].copy_from_slice(&inner.buffer[start..cap]);
            let second = chunk_size - first;
            chunk_samples[first..].copy_from_slice(&inner.buffer[..second]);
        }

        inner.current_len -= chunk_size;
        Some(AudioChunk::new(chunk_samples, self.sample_rate, self.channels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32 * 0.5).collect()
    }

    #[test]
    fn ring_buffer_zero_alloc_window_extraction_100ms_and_2s() {
        let rb = ContinuousAudioRingBuffer::new(5, 16_000); // 5 secondes de capacité = 80 000 échantillons
        assert_eq!(rb.capacity(), 80_000);
        assert!(rb.is_empty());

        // Pousse 48 000 échantillons (3 secondes de signal)
        let samples: Vec<f32> = (0..48_000).map(|i| (i as f32) * 0.1).collect();
        rb.push_samples(&samples);
        assert_eq!(rb.current_len(), 48_000);
        assert_eq!(rb.evicted_total(), 0);

        // 1. Extraction non-destructive d'une fenêtre de 100 ms (1 600 échantillons) pour YIN
        let mut yin_window = [0.0f32; SAMPLES_PER_100MS];
        rb.read_prosody_100ms(&mut yin_window).expect("extraction YIN 100ms réussie");

        // Vérification des valeurs : les 1 600 derniers échantillons sont 46 400..48 000
        for (i, &val) in yin_window.iter().enumerate() {
            let expected = (46_400 + i) as f32 * 0.1;
            assert!((val - expected).abs() < 1e-5, "YIN mismatch at {i}: {val} vs {expected}");
        }

        // Le buffer n'a PAS été consommé (lecture non-destructive)
        assert_eq!(rb.current_len(), 48_000);

        // 2. Extraction non-destructive d'une fenêtre de 2,0 s (32 000 échantillons) pour SenseVoice
        let mut sensevoice_window = [0.0f32; SAMPLES_PER_2S];
        rb.read_sensevoice_2s(&mut sensevoice_window).expect("extraction SenseVoice 2s réussie");

        // Vérification des valeurs : les 32 000 derniers échantillons sont 16 000..48 000
        for (i, &val) in sensevoice_window.iter().enumerate() {
            let expected = (16_000 + i) as f32 * 0.1;
            assert!((val - expected).abs() < 1e-4, "SenseVoice mismatch at {i}: {val} vs {expected}");
        }

        // 3. get_latest_window retourne bien un AudioChunk avec la bonne durée
        let chunk = rb.get_latest_window(SAMPLES_PER_100MS).expect("chunk 100ms");
        assert_eq!(chunk.samples.len(), 1_600);
        assert!((chunk.duration_seconds() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn ring_buffer_wrap_around_boundary_exactness() {
        // Capacité 1 000 échantillons
        let rb = ContinuousAudioRingBuffer::new(1, 1_000);

        // Écriture initiale de 800 échantillons [0.0 .. 799.0]
        let first: Vec<f32> = (0..800).map(|i| i as f32).collect();
        rb.push_samples(&first);
        assert_eq!(rb.current_len(), 800);
        assert_eq!(rb.evicted_total(), 0);

        // Écriture suivante chevauchant la frontière circulaire : 500 échantillons [1000.0 .. 1499.0]
        // 800 + 500 = 1300 -> 300 évincés, write_head à (800 + 500) % 1000 = 300
        let second: Vec<f32> = (0..500).map(|i| 1000.0 + i as f32).collect();
        rb.push_samples(&second);
        assert_eq!(rb.current_len(), 1000);
        assert_eq!(rb.evicted_total(), 300);

        // Lecture des 400 plus récents (chevauche la frontière circulaire : 300 au début, 100 à la fin)
        let mut dest = [0.0f32; 400];
        rb.read_latest_into(&mut dest).expect("lecture circulaire réussie");

        // Les 400 plus récents sont les 400 derniers de `second` (indices 100..500, soit valeurs 1100.0 .. 1499.0)
        for (i, &val) in dest.iter().enumerate() {
            let expected = 1100.0 + i as f32;
            assert!((val - expected).abs() < 1e-6, "Wrap mismatch at {i}: {val} vs {expected}");
        }
    }

    #[test]
    fn ring_buffer_insufficient_samples_rejected() {
        let rb = ContinuousAudioRingBuffer::new(1, 1_000);
        rb.push_samples(&[1.0, 2.0, 3.0]);
        let mut dest = [0.0f32; 10];
        let err = rb.read_latest_into(&mut dest).unwrap_err();
        assert_eq!(
            err,
            RingBufferError::InsufficientSamples {
                available: 3,
                requested: 10
            }
        );
    }

    #[test]
    fn ring_buffer_multithreaded_concurrent_rw_stress() {
        let rb = Arc::new(ContinuousAudioRingBuffer::new(2, 16_000)); // 32 000 échantillons
        let writer_rb = Arc::clone(&rb);
        let reader_yin_rb = Arc::clone(&rb);
        let reader_sv_rb = Arc::clone(&rb);

        // Thread Producteur : simule l'arrivée continue de blocs audio CPAL de 10 ms (160 samples)
        let writer = thread::spawn(move || {
            let block = [1.0f32; SAMPLES_PER_10MS];
            for _ in 0..200 {
                writer_rb.push_samples(&block);
                thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        // Thread Consommateur 1 : analyse périodique YIN (100 ms = 1 600 samples)
        let reader_yin = thread::spawn(move || {
            let mut buf = [0.0f32; SAMPLES_PER_100MS];
            let mut successful_reads = 0;
            for _ in 0..50 {
                if reader_yin_rb.read_prosody_100ms(&mut buf).is_ok() {
                    successful_reads += 1;
                    assert!(buf.iter().all(|&s| s == 1.0));
                }
                thread::sleep(std::time::Duration::from_millis(3));
            }
            successful_reads
        });

        // Thread Consommateur 2 : analyse SenseVoice (500 ms = 8 000 samples)
        let reader_sv = thread::spawn(move || {
            let mut buf = [0.0f32; 8_000];
            let mut successful_reads = 0;
            for _ in 0..25 {
                if reader_sv_rb.read_latest_into(&mut buf).is_ok() {
                    successful_reads += 1;
                    assert!(buf.iter().all(|&s| s == 1.0));
                }
                thread::sleep(std::time::Duration::from_millis(7));
            }
            successful_reads
        });

        writer.join().expect("writer finished");
        let yin_reads = reader_yin.join().expect("yin reader finished");
        let sv_reads = reader_sv.join().expect("sv reader finished");

        assert!(yin_reads > 0, "YIN reader must have performed concurrent reads");
        assert!(sv_reads > 0, "SenseVoice reader must have performed concurrent reads");
        assert_eq!(rb.total_written(), 200 * 160);
    }

    #[test]
    fn ring_buffer_pop_chunk_backward_compatibility() {
        let rb = ContinuousAudioRingBuffer::new(1, 1_000);
        rb.push_samples(&ramp(500));
        assert_eq!(rb.current_len(), 500);

        let chunk1 = rb.pop_chunk(200).expect("chunk 200");
        assert_eq!(chunk1.samples.len(), 200);
        assert_eq!(rb.current_len(), 300);
        assert!((chunk1.samples[0] - 0.0).abs() < 1e-6);
        assert!((chunk1.samples[199] - 99.5).abs() < 1e-6);

        let chunk2 = rb.pop_chunk(300).expect("chunk 300");
        assert_eq!(chunk2.samples.len(), 300);
        assert_eq!(rb.current_len(), 0);
        assert!(rb.is_empty());
        assert!(rb.pop_chunk(1).is_none());
    }

    #[test]
    fn ring_buffer_lossless_ordered_roundtrip() {
        let rb = ContinuousAudioRingBuffer::new(10, 16_000);
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
        let rb = ContinuousAudioRingBuffer::new(1, 1000);
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
        let rb = ContinuousAudioRingBuffer::new(1, 16_000);
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

    #[test]
    fn test_ring_buffer_evicted_total_false_positive_on_pop_or_clear() {
        let rb = ContinuousAudioRingBuffer::new(1, 1000);
        let data = vec![1.0f32; 1000];
        rb.push_samples(&data);
        assert_eq!(rb.current_len(), 1000);
        assert_eq!(rb.evicted_total(), 0);

        // Consommation de 500 échantillons
        let popped = rb.pop_chunk(500).expect("pop 500");
        assert_eq!(popped.samples.len(), 500);
        assert_eq!(rb.current_len(), 500);
        assert_eq!(rb.evicted_total(), 0);

        // Écriture de 200 échantillons : 500 + 200 = 700 <= 1000 -> 0 évincés
        let extra200 = vec![2.0f32; 200];
        rb.push_samples(&extra200);
        assert_eq!(rb.current_len(), 700);
        assert_eq!(rb.evicted_total(), 0);

        // Écriture de 400 échantillons : 700 + 400 = 1100 > 1000 -> 100 évincés
        let extra400 = vec![3.0f32; 400];
        rb.push_samples(&extra400);
        assert_eq!(rb.current_len(), 1000);
        assert_eq!(rb.evicted_total(), 100);
    }
}

