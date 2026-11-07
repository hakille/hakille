use hakille_perception::audio::capture::{
    ContinuousAudioRingBuffer, SAMPLES_PER_2S, SAMPLES_PER_10MS, SAMPLES_PER_100MS,
};
use hakille_perception::audio::pipeline::{EmotionHook, VoiceEvent};
use hakille_perception::vision::capture::FrameBufferPool;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ============================================================================
// 1. ADVERSARIAL TEST: ContinuousAudioRingBuffer Concurrent Multi-thread Stress
// ============================================================================

#[test]
fn test_ring_buffer_concurrency_fast_producer_readers_wrap_around() {
    // Capacité : 2 secondes @ 16 kHz = 32 000 échantillons
    let rb = Arc::new(ContinuousAudioRingBuffer::new(2, 16_000));
    assert_eq!(rb.capacity(), 32_000);

    let running = Arc::new(AtomicBool::new(true));
    let total_written_samples = Arc::new(AtomicU64::new(0));

    // Producteur rapide : pousse des blocs de 160 échantillons (10 ms)
    // utilisant une rampe séquentielle continue pour vérifier l'exactitude
    let producer_rb = Arc::clone(&rb);
    let producer_running = Arc::clone(&running);
    let producer_total = Arc::clone(&total_written_samples);

    let producer = thread::spawn(move || {
        let mut sample_counter: u64 = 0;
        let blocks = 2000; // 2000 * 160 = 320 000 échantillons (10x la capacité du ring buffer !)
        for _ in 0..blocks {
            let mut block = [0.0f32; SAMPLES_PER_10MS];
            for sample in block.iter_mut() {
                *sample = sample_counter as f32;
                sample_counter += 1;
            }
            producer_rb.push_samples(&block);
            producer_total.store(sample_counter, Ordering::Relaxed);
            // Très courte pause (ou yield) pour émuler un producteur haute fréquence
            thread::yield_now();
        }
        producer_running.store(false, Ordering::Relaxed);
        sample_counter
    });

    // Lecteur YIN (100 ms = 1 600 échantillons) : lit en continu et vérifie la stricte monotonie
    let yin_rb = Arc::clone(&rb);
    let yin_running = Arc::clone(&running);
    let yin_reader = thread::spawn(move || {
        let mut dest = [0.0f32; SAMPLES_PER_100MS];
        let mut reads = 0;
        let mut discontinuities = 0;

        while yin_running.load(Ordering::Relaxed) || reads < 10 {
            if yin_rb.read_prosody_100ms(&mut dest).is_ok() {
                reads += 1;
                // Vérifier que dans la fenêtre de 1 600 échantillons, chaque échantillon est égal au précédent + 1.0
                for i in 1..dest.len() {
                    let diff = dest[i] - dest[i - 1];
                    if (diff - 1.0).abs() > 1e-4 {
                        discontinuities += 1;
                        break;
                    }
                }
            }
            thread::yield_now();
        }
        (reads, discontinuities)
    });

    // Lecteur SenseVoice (2,0 s = 32 000 échantillons) : lit la fenêtre complète après wrap-around
    let sv_rb = Arc::clone(&rb);
    let sv_running = Arc::clone(&running);
    let sv_reader = thread::spawn(move || {
        let mut dest = [0.0f32; SAMPLES_PER_2S];
        let mut reads = 0;
        let mut discontinuities = 0;

        while sv_running.load(Ordering::Relaxed) || reads < 5 {
            if sv_rb.read_sensevoice_2s(&mut dest).is_ok() {
                reads += 1;
                // Vérifier la continuité interne
                for i in 1..dest.len() {
                    let diff = dest[i] - dest[i - 1];
                    if (diff - 1.0).abs() > 1e-4 {
                        discontinuities += 1;
                        break;
                    }
                }
            }
            thread::yield_now();
        }
        (reads, discontinuities)
    });

    let total_produced = producer.join().expect("producer failed");
    let (yin_reads, yin_discont) = yin_reader.join().expect("yin reader failed");
    let (sv_reads, sv_discont) = sv_reader.join().expect("sv reader failed");

    assert_eq!(total_produced, 320_000);
    assert_eq!(rb.total_written(), 320_000);
    assert_eq!(rb.evicted_total(), 320_000 - 32_000);
    assert_eq!(rb.current_len(), 32_000);

    assert!(
        yin_reads > 0,
        "YIN doit avoir effectué des lectures concurrentes"
    );
    assert_eq!(
        yin_discont, 0,
        "Aucune discontinuité ou déchirure dans les fenêtres YIN (zero torn reads)"
    );

    assert!(
        sv_reads > 0,
        "SenseVoice doit avoir effectué des lectures concurrentes"
    );
    assert_eq!(
        sv_discont, 0,
        "Aucune discontinuité ou déchirure dans les fenêtres SenseVoice (zero torn reads)"
    );

    // Vérification finale après arrêt du producteur : la dernière fenêtre 100ms doit être exactement les 1600 derniers
    let mut final_yin = [0.0f32; SAMPLES_PER_100MS];
    rb.read_prosody_100ms(&mut final_yin)
        .expect("lecture finale YIN");
    let expected_first_yin = (320_000 - 1_600) as f32;
    assert_eq!(final_yin[0], expected_first_yin);
    assert_eq!(final_yin[1_599], 319_999.0);

    // Et la fenêtre SenseVoice doit contenir les 32 000 derniers échantillons [288 000 .. 320 000]
    let mut final_sv = [0.0f32; SAMPLES_PER_2S];
    rb.read_sensevoice_2s(&mut final_sv)
        .expect("lecture finale SenseVoice");
    assert_eq!(final_sv[0], (320_000 - 32_000) as f32);
    assert_eq!(final_sv[31_999], 319_999.0);
}

#[test]
fn test_ring_buffer_evicted_total_false_positive_on_pop_or_clear() {
    let rb = ContinuousAudioRingBuffer::new(1, 1000);
    // Pousser 1000 échantillons
    rb.push_samples(&vec![1.0; 1000]);
    assert_eq!(rb.current_len(), 1000);
    assert_eq!(rb.evicted_total(), 0);

    // Consommer 500 échantillons via pop_chunk (libère 500 places dans le buffer)
    let chunk = rb.pop_chunk(500).expect("pop 500");
    assert_eq!(chunk.samples.len(), 500);
    assert_eq!(rb.current_len(), 500);

    // Pousser 200 échantillons. La capacité est 1000, le buffer passe de 500 à 700 échantillons.
    // ZÉRO échantillon n'a été écrasé ou évincé !
    rb.push_samples(&vec![2.0; 200]);
    assert_eq!(rb.current_len(), 700);

    // Après remédiation : ZÉRO échantillon évincé !
    let observed_evicted = rb.evicted_total();
    println!(
        "evicted_total() après pop de 500 et push de 200 : {}",
        observed_evicted
    );
    assert_eq!(
        observed_evicted, 0,
        "Validation de la correction : evicted_total rapporte 0 échantillon évincé (buffer à 700/1000)"
    );
}

// ============================================================================
// 2. ADVERSARIAL TEST: FrameBufferPool Recycling & Capacity Bounds
// ============================================================================

#[test]
fn test_frame_buffer_pool_capacity_bounds_and_recycling() {
    let max_buffers = 3;
    let pool = Arc::new(FrameBufferPool::new(max_buffers, 1024));

    // Acquérir 5 tampons (dépasse max_buffers)
    let mut buffers: Vec<Vec<u8>> = (0..5).map(|_| pool.acquire(2048)).collect();
    assert_eq!(pool.stats().total_acquired, 5);
    assert_eq!(pool.stats().allocations_prevented, 0);
    assert_eq!(pool.stats().current_available, 0);

    // Remplir les données avec des octets sensibles
    for b in &mut buffers {
        b.extend_from_slice(&[0xFF; 2048]);
    }

    // Recycler les 5 tampons dans un pool de max 3
    for b in buffers {
        pool.recycle(b);
    }

    // Le pool ne doit pas dépasser max_buffers = 3
    assert_eq!(pool.stats().total_recycled, 5);
    assert_eq!(pool.stats().current_available, 3);

    // Acquérir à nouveau : les 3 premiers doivent provenir du pool (zéro allocation)
    let b1 = pool.acquire(2048);
    let _b2 = pool.acquire(2048);
    let _b3 = pool.acquire(2048);
    assert_eq!(pool.stats().allocations_prevented, 3);
    assert_eq!(pool.stats().current_available, 0);

    // Vérifier que la Règle 8 (secure_zeroize) a bien été appliquée au recyclage
    assert!(b1.is_empty(), "le buffer recyclé doit être clear()");
    assert!(b1.capacity() >= 2048);

    // Le 4ème acquisition doit être une nouvelle allocation car pool vide
    let _b4 = pool.acquire(2048);
    assert_eq!(pool.stats().allocations_prevented, 3);
    assert_eq!(pool.stats().total_acquired, 9);
}

// ============================================================================
// 3. ADVERSARIAL TEST: FrameBufferPool Concurrency & Deadlock Detection
// ============================================================================

#[test]
fn test_frame_buffer_pool_concurrent_deadlock_detection() {
    let pool = Arc::new(FrameBufferPool::new(8, 1024));
    let iterations = 50_000;

    let pool_acq = Arc::clone(&pool);
    let pool_rec = Arc::clone(&pool);

    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        let mut handles = Vec::new();
        for _ in 0..4 {
            let p = Arc::clone(&pool_acq);
            handles.push(thread::spawn(move || {
                for _ in 0..iterations {
                    let buf = p.acquire(1024);
                    assert!(buf.capacity() >= 1024);
                    p.recycle(buf);
                }
            }));
            let p2 = Arc::clone(&pool_rec);
            handles.push(thread::spawn(move || {
                for _ in 0..iterations {
                    let buf = p2.acquire(512);
                    p2.recycle(buf);
                }
            }));
            let p3 = Arc::clone(&pool_acq);
            handles.push(thread::spawn(move || {
                for _ in 0..iterations {
                    let _ = p3.stats();
                }
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }
        let _ = tx.send(());
    });

    match rx.recv_timeout(Duration::from_millis(3000)) {
        Ok(()) => {
            // Did not deadlock in this run
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "CRITICAL BUG CONFIRMED: DEADLOCK DETECTED in FrameBufferPool!\n\
                 Lock inversion between:\n\
                 1) `FrameBufferPool::acquire` (locks stats THEN buffers)\n\
                 2) `FrameBufferPool::recycle` (locks buffers THEN stats)"
            );
        }
        Err(e) => panic!("channel error: {e:?}"),
    }
}

// ============================================================================
// 4. ADVERSARIAL TEST: VoiceEvent::Emotion Transmission & Hook
// ============================================================================

#[test]
fn test_voice_pipeline_emotion_event_and_hook_integration() {
    let recorded_emotions = Arc::new(Mutex::new(Vec::<(String, Vec<String>, f32)>::new()));
    let rec_clone = Arc::clone(&recorded_emotions);

    let hook: Arc<dyn EmotionHook> =
        Arc::new(move |emotion: &str, events: &[String], conf: f32| {
            rec_clone
                .lock()
                .unwrap()
                .push((emotion.to_string(), events.to_vec(), conf));
        });

    // Simuler l'appel hook
    hook.on_emotion("HAPPY", &["laughter".to_string(), "sigh".to_string()], 0.92);

    let rec = recorded_emotions.lock().unwrap().clone();
    assert_eq!(rec.len(), 1);
    assert_eq!(rec[0].0, "HAPPY");
    assert_eq!(rec[0].1, vec!["laughter", "sigh"]);
    assert_eq!(rec[0].2, 0.92);

    // Test de la sérialisation / désérialisation de l'événement VoiceEvent::Emotion
    let evt = VoiceEvent::Emotion {
        emotion: "ANGRY".to_string(),
        acoustic_events: vec!["gasp".to_string()],
        confidence: 0.88,
    };

    let serialized = serde_json::to_string(&evt).expect("serialization ok");
    let deserialized: VoiceEvent = serde_json::from_str(&serialized).expect("deserialization ok");
    assert_eq!(evt, deserialized);
}
