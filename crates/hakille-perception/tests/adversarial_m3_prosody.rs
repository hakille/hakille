use hakille_audio::{YinPitchDetector, YinWorkspace};
use hakille_perception::audio::capture::{
    ContinuousAudioRingBuffer, SAMPLES_PER_10MS, SAMPLES_PER_100MS,
};
use hakille_perception::audio::prosody::VocalProsodyAnalyzer;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Instant;

/// Helper: Génère une onde sinusoïdale pure.
fn generate_sine(freq_hz: f32, sample_rate: u32, duration_secs: f32, amplitude: f32) -> Vec<f32> {
    let n = (sample_rate as f32 * duration_secs) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * freq_hz * t).sin() * amplitude
        })
        .collect()
}

/// Helper: Génère du bruit blanc uniforme pseudo-aléatoire dans [-amplitude, amplitude].
fn generate_white_noise(n: usize, seed: u64, amplitude: f32) -> Vec<f32> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let unit = ((state >> 32) as u32 as f32) / (u32::MAX as f32);
            (unit * 2.0 - 1.0) * amplitude
        })
        .collect()
}

// ============================================================================
// 1. ACOUSTIC EDGE CASES: Silence, Extreme Clipping, Saturated Rails
// ============================================================================

#[test]
fn test_acoustic_silence_absolute_and_neg120db() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);
    let mut ws = YinWorkspace::new();

    // 1. Silence absolu (0.0 f32)
    let silence_zeros = [0.0f32; SAMPLES_PER_100MS];
    let f_zero = detector.analyze_with_workspace(&silence_zeros, &mut ws);

    assert!(!f_zero.is_voiced, "Silence absolu ne doit pas être voisé");
    assert_eq!(f_zero.f0_hz, None, "Pitch doit être None");
    assert_eq!(f_zero.periodicity, 0.0);
    assert_eq!(f_zero.rms_energy, 0.0);
    assert!(f_zero.hnr_db.is_finite(), "HNR doit être un nombre fini");
    assert_eq!(f_zero.jitter_local, None);
    assert_eq!(f_zero.shimmer_local, None);

    // 2. Silence extrême (-120 dB, amplitude = 1e-6)
    let silence_neg120 = generate_sine(200.0, sample_rate, 0.1, 1e-6);
    let f_neg120 = detector.analyze(&silence_neg120);

    assert!(!f_neg120.is_voiced, "-120 dB doit être non voisé");
    assert_eq!(f_neg120.f0_hz, None);
    assert!(f_neg120.rms_energy < 1e-4, "RMS doit être sous le plancher");
    assert!(f_neg120.hnr_db.is_finite());
    assert!(!f_neg120.hnr_db.is_nan());
    assert_eq!(f_neg120.jitter_local, None);
    assert_eq!(f_neg120.shimmer_local, None);

    // 3. Silence à -80 dB (amplitude = 1e-4)
    let silence_neg80 = generate_sine(300.0, sample_rate, 0.1, 1e-4);
    let f_neg80 = detector.analyze(&silence_neg80);
    assert!(!f_neg80.is_voiced);
    assert_eq!(f_neg80.f0_hz, None);
}

#[test]
fn test_acoustic_saturated_signals_and_extreme_clipping() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);

    // 1. Rail DC plat saturé (+1.0 continu)
    let dc_rail_pos = [1.0f32; SAMPLES_PER_100MS];
    let f_dc_pos = detector.analyze(&dc_rail_pos);
    assert!(
        !f_dc_pos.is_voiced,
        "Un signal DC constant +1.0 n'est pas voisé"
    );
    assert_eq!(f_dc_pos.f0_hz, None);
    assert_eq!(f_dc_pos.periodicity, 0.0);
    assert!(
        f_dc_pos.hnr_db.is_finite(),
        "HNR ne doit pas être Inf ou NaN sur DC"
    );
    assert_eq!(f_dc_pos.jitter_local, None);
    assert_eq!(f_dc_pos.shimmer_local, None);

    // 2. Rail DC négatif saturé (-1.0 continu)
    let dc_rail_neg = [-1.0f32; SAMPLES_PER_100MS];
    let f_dc_neg = detector.analyze(&dc_rail_neg);
    assert!(!f_dc_neg.is_voiced);
    assert_eq!(f_dc_neg.f0_hz, None);
    assert!(f_dc_neg.hnr_db.is_finite());

    // 3. Rail DC extrême (+1000.0 f32, hors normes)
    let dc_extreme = [1000.0f32; SAMPLES_PER_100MS];
    let f_dc_ext = detector.analyze(&dc_extreme);
    assert!(!f_dc_ext.is_voiced);
    assert_eq!(f_dc_ext.f0_hz, None);
    assert!(f_dc_ext.hnr_db.is_finite());

    // 4. Signal carré saturé dur (clipping extrême d'une voix à 200 Hz)
    // Onde carrée alternant +1.0 et -1.0 avec période = 80 échantillons (200 Hz @ 16 kHz)
    let period_samples = 80;
    let mut square_wave = [0.0f32; SAMPLES_PER_100MS];
    for (i, sample) in square_wave.iter_mut().enumerate() {
        *sample = if (i % period_samples) < (period_samples / 2) {
            1.0
        } else {
            -1.0
        };
    }
    let f_square = detector.analyze(&square_wave);
    assert!(
        f_square.is_voiced,
        "Onde carrée harmonique 200 Hz doit être détectée"
    );
    assert!(
        f_square.f0_hz.is_some(),
        "F0 doit être extrait pour onde carrée périodique"
    );
    let f0 = f_square.f0_hz.unwrap();
    assert!(
        (f0 - 200.0).abs() < 5.0,
        "F0 onde carrée attendu ~200 Hz, obtenu: {f0}"
    );
    assert!(f_square.hnr_db.is_finite());
    assert!(!f_square.hnr_db.is_nan());
    assert!(!f_square.periodicity.is_nan());

    // 5. Sinusoïde suramplifiée et hard-clamped à [-1.0, 1.0] (saturation d'ampli / micro)
    let raw_loud_sine = generate_sine(250.0, sample_rate, 0.1, 50.0);
    let clipped_sine: Vec<f32> = raw_loud_sine.iter().map(|&s| s.clamp(-1.0, 1.0)).collect();
    let f_clipped = detector.analyze(&clipped_sine);
    assert!(f_clipped.is_voiced);
    assert!(f_clipped.f0_hz.is_some());
    let f0_c = f_clipped.f0_hz.unwrap();
    assert!(
        (f0_c - 250.0).abs() < 5.0,
        "F0 sinusoïde saturée attendu ~250 Hz, obtenu: {f0_c}"
    );
    assert!(f_clipped.hnr_db.is_finite());
}

// ============================================================================
// 2. ACOUSTIC EDGE CASES: White Noise, Frequency Jumps, Voiced/Unvoiced
// ============================================================================

#[test]
fn test_acoustic_pure_white_noise_unvoiced_across_seeds() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);

    for seed in [1u64, 42, 1337, 99999, 0xDEADBEEF] {
        let noise = generate_white_noise(SAMPLES_PER_100MS, seed, 0.8);
        let f = detector.analyze(&noise);

        assert!(
            !f.is_voiced,
            "Bruit blanc pur (graine {seed}) ne doit JAMAIS être classé voisé"
        );
        assert_eq!(f.f0_hz, None, "Pitch doit être None pour bruit blanc");
        assert!(
            f.periodicity < 0.65,
            "Périodicité du bruit blanc doit être basse (obtenu: {})",
            f.periodicity
        );
        assert!(f.hnr_db.is_finite(), "HNR bruit blanc doit être fini");
        assert!(!f.hnr_db.is_nan());
        assert_eq!(f.jitter_local, None);
        assert_eq!(f.shimmer_local, None);
    }
}

#[test]
fn test_acoustic_rapid_frequency_jumps_and_transitions() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);

    // Séquence de sauts de fréquences brutaux trame par trame :
    // 100 Hz -> 500 Hz -> 150 Hz -> 450 Hz -> 200 Hz
    let freqs = [100.0, 500.0, 150.0, 450.0, 200.0];
    for &freq in &freqs {
        let wave = generate_sine(freq, sample_rate, 0.1, 0.7);
        let f = detector.analyze(&wave);
        assert!(f.is_voiced, "Onde {freq} Hz doit être voisée");
        let detected = f.f0_hz.expect("F0 doit être présent");
        assert!(
            (detected - freq).abs() < 5.0,
            "Saut à {freq} Hz détecté comme {detected} Hz"
        );
        assert!(f.periodicity > 0.8);
        assert!(f.hnr_db.is_finite() && !f.hnr_db.is_nan());

        // Jitter et Shimmer doivent être finis et dans [0.0, 1.0]
        if let Some(j) = f.jitter_local {
            assert!(j.is_finite() && !j.is_nan());
            assert!((0.0..=1.0).contains(&j), "Jitter {j} doit être dans [0, 1]");
        }
        if let Some(s) = f.shimmer_local {
            assert!(s.is_finite() && !s.is_nan());
            assert!(
                (0.0..=1.0).contains(&s),
                "Shimmer {s} doit être dans [0, 1]"
            );
        }
    }

    // Transition intra-trame brutale : première moitié à 150 Hz, seconde moitié à 450 Hz
    let half_n = SAMPLES_PER_100MS / 2;
    let wave1 = generate_sine(150.0, sample_rate, 0.05, 0.7);
    let wave2 = generate_sine(450.0, sample_rate, 0.05, 0.7);
    let mut mixed = Vec::with_capacity(SAMPLES_PER_100MS);
    mixed.extend_from_slice(&wave1[..half_n]);
    mixed.extend_from_slice(&wave2[..half_n]);

    let f_mixed = detector.analyze(&mixed);
    // Vérifier l'absence de panique, de NaN et de division par zéro
    assert!(f_mixed.hnr_db.is_finite());
    assert!(!f_mixed.hnr_db.is_nan());
    assert!(!f_mixed.periodicity.is_nan());
    if let Some(j) = f_mixed.jitter_local {
        assert!(j.is_finite() && !j.is_nan());
    }
}

#[test]
fn test_acoustic_rapid_alternating_voiced_unvoiced() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);

    let voiced_wave = generate_sine(220.0, sample_rate, 0.1, 0.8);
    let silence = [0.0f32; SAMPLES_PER_100MS];

    // Alternance rapide : voisé, silence, voisé, silence sur 10 cycles
    for cycle in 0..10 {
        let f_v = detector.analyze(&voiced_wave);
        assert!(f_v.is_voiced, "Cycle {cycle}: voisé attendu");
        assert!(f_v.f0_hz.is_some());

        let f_s = detector.analyze(&silence);
        assert!(!f_s.is_voiced, "Cycle {cycle}: silence attendu non voisé");
        assert_eq!(f_s.f0_hz, None);
        assert_eq!(f_s.jitter_local, None);
        assert_eq!(f_s.shimmer_local, None);
    }
}

// ============================================================================
// 3. ACOUSTIC EDGE CASES: Degenerate Inputs (NaN, Inf, Subnormal, Empty)
// ============================================================================

#[test]
fn test_acoustic_degenerate_empty_and_subnormal() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);

    // Tranche vide
    let empty = detector.analyze(&[]);
    assert!(!empty.is_voiced);
    assert_eq!(empty.f0_hz, None);
    assert_eq!(empty.rms_energy, 0.0);

    // Tranche très courte (inférieure à min_period)
    let tiny = [0.5f32; 10];
    let f_tiny = detector.analyze(&tiny);
    assert!(!f_tiny.is_voiced);
    assert_eq!(f_tiny.f0_hz, None);

    // Nombres sous-normaux (denormals: ~1e-40)
    let denormals = [1e-40f32; SAMPLES_PER_100MS];
    let f_denorm = detector.analyze(&denormals);
    assert!(!f_denorm.is_voiced);
    assert_eq!(f_denorm.f0_hz, None);
    assert!(f_denorm.hnr_db.is_finite());
}

// ============================================================================
// 4. VOCAL PROSODY ANALYZER: Temporal Smoothing & Silence Eviction
// ============================================================================

#[test]
fn test_prosody_analyzer_prolonged_silence_behavior() {
    let mut analyzer = VocalProsodyAnalyzer::default_16k();

    let wave_220 = generate_sine(220.0, 16_000, 0.1, 0.8);
    let mut buf = [0.0f32; SAMPLES_PER_100MS];
    buf.copy_from_slice(&wave_220[..SAMPLES_PER_100MS]);

    // 5 trames de son voisé à 220 Hz
    for _ in 0..5 {
        let p = analyzer.process_window(&buf);
        assert!(p.smoothed_f0_hz.is_some());
        assert!(p.voiced_ratio > 0.0);
    }

    // Maintenant 15 trames consécutives de silence absolu (dépasse history_capacity = 10)
    let silence = [0.0f32; SAMPLES_PER_100MS];
    let mut last_p = None;
    for _ in 0..15 {
        last_p = Some(analyzer.process_window(&silence));
    }

    let p = last_p.unwrap();
    assert_eq!(
        p.voiced_ratio, 0.0,
        "Après 15 trames de silence, voiced_ratio doit être 0.0"
    );
    assert_eq!(
        p.smoothed_f0_hz, None,
        "F0 lissé doit être None après silence prolongé"
    );
    assert_eq!(p.f0_range_hz, None, "Plage de F0 doit être None");
    assert_eq!(p.mean_rms_energy, 0.0, "RMS moyen doit être 0.0");
    assert!(p.smoothed_hnr_db.is_finite());
}

// ============================================================================
// 5. PERFORMANCE: Latency Benchmark (< 100 µs on 1600 samples)
// ============================================================================

#[test]
fn test_yin_dsp_latency_under_100_micros() {
    let sample_rate = 16_000;
    let mut detector = YinPitchDetector::new(sample_rate);
    let mut ws = YinWorkspace::new();

    let sine_wave = generate_sine(220.0, sample_rate, 0.1, 0.7);
    assert_eq!(sine_wave.len(), SAMPLES_PER_100MS);

    // Warmup (20 itérations)
    for _ in 0..20 {
        let _ = detector.analyze_with_workspace(&sine_wave, &mut ws);
    }

    // Benchmark sur 200 itérations consécutives
    let iters = 200;
    let start = Instant::now();
    for _ in 0..iters {
        let _ = detector.analyze_with_workspace(&sine_wave, &mut ws);
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_micros() as f64 / iters as f64;

    println!(
        "\n======================================================\n\
         YIN DSP LATENCY BENCHMARK: {:.2} µs par trame (sur {} itérations)\n\
         ======================================================",
        avg_us, iters
    );

    // En compilation release (--release), la latence doit impérativement être < 100 µs.
    #[cfg(not(debug_assertions))]
    assert!(
        avg_us < 100.0,
        "VIOLATION PERFORMANCE: latence mesurée = {:.2} µs (> 100 µs)",
        avg_us
    );

    // En compilation debug, tolérance raisonnable pour tests non-optimisés (< 5000 µs)
    #[cfg(debug_assertions)]
    assert!(
        avg_us < 5000.0,
        "VIOLATION PERFORMANCE DEBUG: latence mesurée = {:.2} µs (> 5000 µs)",
        avg_us
    );
}

// ============================================================================
// 6. CONCURRENCY STRESS: Multi-Thread Access to RingBuffer & Prosody
// ============================================================================

#[test]
fn test_ring_buffer_and_prosody_concurrency_stress() {
    // Tampon de 2 secondes à 16 kHz = 32 000 échantillons
    let rb = Arc::new(ContinuousAudioRingBuffer::new(2, 16_000));
    let running = Arc::new(AtomicBool::new(true));
    let total_written = Arc::new(AtomicU64::new(0));

    // Barrière pour démarrage parfaitement synchrone des threads
    let num_readers = 4;
    let barrier = Arc::new(Barrier::new(num_readers + 1));

    // Thread Producteur : écrit une rampe continue par blocs de 160 échantillons (10 ms)
    let prod_rb = Arc::clone(&rb);
    let prod_running = Arc::clone(&running);
    let prod_written = Arc::clone(&total_written);
    let prod_barrier = Arc::clone(&barrier);

    let producer = thread::spawn(move || {
        prod_barrier.wait();
        let mut sample_idx = 0u64;
        let blocks = 1500; // 1500 * 160 = 240 000 échantillons (~7.5x wrap-around de capacité)
        for _ in 0..blocks {
            let mut block = [0.0f32; SAMPLES_PER_10MS];
            for s in block.iter_mut() {
                *s = sample_idx as f32;
                sample_idx += 1;
            }
            prod_rb.push_samples(&block);
            prod_written.store(sample_idx, Ordering::Relaxed);
            thread::yield_now();
        }
        prod_running.store(false, Ordering::Relaxed);
        sample_idx
    });

    // 4 Threads Consommateurs concurrents : lisent read_prosody_100ms et analysent avec VocalProsodyAnalyzer
    let mut reader_handles = Vec::new();
    let reader_reads_count = Arc::new(AtomicUsize::new(0));

    for reader_id in 0..num_readers {
        let r_rb = Arc::clone(&rb);
        let r_running = Arc::clone(&running);
        let r_barrier = Arc::clone(&barrier);
        let r_count = Arc::clone(&reader_reads_count);

        reader_handles.push(thread::spawn(move || {
            let mut analyzer = VocalProsodyAnalyzer::default_16k();
            let mut window = [0.0f32; SAMPLES_PER_100MS];
            let mut local_reads = 0;
            let mut torn_reads = 0;

            r_barrier.wait();

            while r_running.load(Ordering::Relaxed) || local_reads < 20 {
                if r_rb.read_prosody_100ms(&mut window).is_ok() {
                    local_reads += 1;

                    // 1. Vérification de cohérence atomique : aucune déchirure (zero torn read)
                    // Chaque échantillon consécutif doit être exactement précédent + 1.0
                    for i in 1..window.len() {
                        let diff = window[i] - window[i - 1];
                        if (diff - 1.0).abs() > 1e-4 {
                            torn_reads += 1;
                            break;
                        }
                    }

                    // 2. Traitement immédiat par VocalProsodyAnalyzer
                    let prosody = analyzer.process_window(&window);
                    assert!(prosody.smoothed_hnr_db.is_finite());
                }
                thread::yield_now();
            }

            r_count.fetch_add(local_reads, Ordering::Relaxed);
            (reader_id, local_reads, torn_reads)
        }));
    }

    let produced = producer.join().expect("producer ok");
    assert_eq!(produced, 240_000);

    let mut total_torn = 0;
    let mut total_reads = 0;
    for handle in reader_handles {
        let (_id, reads, torn) = handle.join().expect("reader ok");
        total_reads += reads;
        total_torn += torn;
    }

    println!(
        "\nCONCURRENCY PROSODY STRESS: {} lectures concurrentes effectuées, {} déchirures (torn reads)",
        total_reads, total_torn
    );

    assert!(
        total_reads > 50,
        "Doit avoir effectué au moins 50 lectures concurrentes"
    );
    assert_eq!(
        total_torn, 0,
        "VIOLATION INTÉGRITÉ AUDIO : lectures concurrentes avec déchirures de données !"
    );

    // Vérification finale de lecture après l'arrêt
    let mut final_window = [0.0f32; SAMPLES_PER_100MS];
    rb.read_prosody_100ms(&mut final_window)
        .expect("lecture finale réussie");
    assert_eq!(final_window[0], (240_000 - 1_600) as f32);
    assert_eq!(final_window[1_599], 239_999.0);
}

#[test]
fn test_shared_mutex_prosody_analyzer_concurrency() {
    // Test de robustesse : analyseur prosodique protégé par Mutex partagé entre 4 threads
    let shared_analyzer = Arc::new(Mutex::new(VocalProsodyAnalyzer::default_16k()));
    let rb = Arc::new(ContinuousAudioRingBuffer::new_default());

    // Remplir le ring buffer avec une sinusoïde à 220 Hz
    let wave = generate_sine(220.0, 16_000, 0.5, 0.8);
    rb.push_samples(&wave);

    let mut handles = Vec::new();
    for thread_idx in 0..4 {
        let an_clone = Arc::clone(&shared_analyzer);
        let rb_clone = Arc::clone(&rb);

        handles.push(thread::spawn(move || {
            let mut window = [0.0f32; SAMPLES_PER_100MS];
            let mut results = Vec::new();

            for _ in 0..50 {
                if rb_clone.read_prosody_100ms(&mut window).is_ok() {
                    let mut locked = an_clone.lock().unwrap();
                    let prosody = locked.process_window(&window);
                    results.push(prosody.smoothed_f0_hz);
                }
                thread::yield_now();
            }
            (thread_idx, results.len())
        }));
    }

    for h in handles {
        let (id, count) = h.join().expect("thread ok");
        assert_eq!(
            count, 50,
            "Thread {id} doit avoir effectué 50 analyses sans blocage"
        );
    }

    let mut final_state = shared_analyzer.lock().unwrap();
    let p = final_state.process_slice(&wave[..1600]);
    assert!(p.smoothed_f0_hz.is_some());
    assert!((p.smoothed_f0_hz.unwrap() - 220.0).abs() < 3.0);
}
