//! Adversarial stress test suite for `SovereignVoiceprintGate` (Milestone 10 Challenger 2)

use hakille_core::Result;
use hakille_perception::audio::capture::AudioChunk;
use hakille_perception::audio::speaker_id::{
    SOVEREIGN_VOICEPRINT_THRESHOLD, SovereignVoiceprintGate, SpeakerIdentifier,
    VoiceBiometricEmbedding,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Helper: generate a deterministic, L2-normalized test vector of length `dim`.
fn generate_normalized_vector(seed: f32, dim: usize) -> Vec<f32> {
    let mut v: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.137 + seed).sin()).collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(norm > 1e-6);
    for x in &mut v {
        *x /= norm;
    }
    v
}

/// Helper: construct a unit vector with exact cosine similarity `target_cos` relative to unit vector `u`.
fn construct_vector_with_cosine(target_cos: f32, u: &[f32]) -> Vec<f32> {
    let dim = u.len();
    // Choose a non-collinear vector to orthogonalize via Gram-Schmidt
    let v0: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.31 + 1.7).cos()).collect();
    let dot: f32 = v0.iter().zip(u).map(|(a, b)| a * b).sum();
    let mut w: Vec<f32> = v0.iter().zip(u).map(|(a, b)| a - dot * b).collect();
    let w_norm: f32 = w.iter().map(|x| x * x).sum::<f32>().sqrt();
    for x in &mut w {
        *x /= w_norm;
    }
    let sin_val = (1.0f32 - target_cos * target_cos).max(0.0).sqrt();
    u.iter()
        .zip(&w)
        .map(|(ui, wi)| target_cos * ui + sin_val * wi)
        .collect()
}

#[test]
fn test_adv_collinear_inverted_vectors_rejection() {
    let dim = 192;
    let enrolled_vec = generate_normalized_vector(1.0, dim);
    let enrolled = VoiceBiometricEmbedding {
        dimension: dim,
        vector: enrolled_vec.clone(),
    };
    let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

    // 1. Exact inverted vector: v_inv = -v (collinear, S = -1.0)
    let inv_vec: Vec<f32> = enrolled_vec.iter().map(|&x| -x).collect();
    let candidate_inv = VoiceBiometricEmbedding {
        dimension: dim,
        vector: inv_vec,
    };
    let res = gate.verify(&candidate_inv).expect("verify inverted vector");
    assert!(
        !res.is_verified,
        "Collinear inverted vector MUST NOT be verified"
    );
    assert!(
        (res.similarity - (-1.0)).abs() < 1e-4,
        "Inverted similarity must be -1.0, got {}",
        res.similarity
    );

    // 2. Scaled inverted vector: v_inv_scaled = -0.5 * v (still S = -1.0)
    let inv_scaled: Vec<f32> = enrolled_vec.iter().map(|&x| -0.5 * x).collect();
    let candidate_inv_scaled = VoiceBiometricEmbedding {
        dimension: dim,
        vector: inv_scaled,
    };
    let res_scaled = gate
        .verify(&candidate_inv_scaled)
        .expect("verify scaled inverted");
    assert!(!res_scaled.is_verified);
    assert!((res_scaled.similarity - (-1.0)).abs() < 1e-4);
}

#[test]
fn test_adv_boundary_nearly_aligned_vectors() {
    let dim = 192;
    // Canonical coordinate frame: enrolled = (1, 0, 0, ..., 0)
    let mut enrolled_vec = vec![0.0f32; dim];
    enrolled_vec[0] = 1.0;
    let enrolled = VoiceBiometricEmbedding {
        dimension: dim,
        vector: enrolled_vec.clone(),
    };
    let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");
    assert_eq!(gate.threshold(), SOVEREIGN_VOICEPRINT_THRESHOLD); // 0.72

    // 1. Target S = 0.719999 (strictly below 0.72) -> must reject (fail-closed)
    let s_below = 0.719999f32;
    let cand_below = construct_vector_with_cosine(s_below, &enrolled_vec);
    let candidate_below = VoiceBiometricEmbedding {
        dimension: dim,
        vector: cand_below,
    };
    let res_below = gate.verify(&candidate_below).expect("verify below");
    assert!(
        !res_below.is_verified,
        "S = 0.719999 < 0.72 MUST NOT be verified (got similarity {})",
        res_below.similarity
    );
    assert!(
        res_below.similarity < SOVEREIGN_VOICEPRINT_THRESHOLD,
        "Measured similarity {} must be < 0.72",
        res_below.similarity
    );

    // 2. Target S = 0.720001 (strictly above 0.72) -> must accept
    let s_above = 0.720001f32;
    let cand_above = construct_vector_with_cosine(s_above, &enrolled_vec);
    let candidate_above = VoiceBiometricEmbedding {
        dimension: dim,
        vector: cand_above,
    };
    let res_above = gate.verify(&candidate_above).expect("verify above");
    assert!(
        res_above.is_verified,
        "S = 0.720001 >= 0.72 MUST be verified (got similarity {})",
        res_above.similarity
    );
    assert!(
        res_above.similarity >= SOVEREIGN_VOICEPRINT_THRESHOLD,
        "Measured similarity {} must be >= 0.72",
        res_above.similarity
    );

    // 3. Ultra-fine sub-boundary: S = 0.7199999 vs 0.7200001
    let s_ultra_below = 0.7199999f32;
    let cand_u_below = construct_vector_with_cosine(s_ultra_below, &enrolled_vec);
    let res_u_below = gate
        .verify(&VoiceBiometricEmbedding {
            dimension: dim,
            vector: cand_u_below,
        })
        .expect("verify ultra-below");
    assert!(
        !res_u_below.is_verified,
        "S = 0.7199999 must not be verified"
    );

    let s_ultra_above = 0.7200001f32;
    let cand_u_above = construct_vector_with_cosine(s_ultra_above, &enrolled_vec);
    let res_u_above = gate
        .verify(&VoiceBiometricEmbedding {
            dimension: dim,
            vector: cand_u_above,
        })
        .expect("verify ultra-above");
    assert!(res_u_above.is_verified, "S = 0.7200001 must be verified");
}

#[test]
fn test_adv_subnormal_and_all_zeros_rejection() {
    let dim = 192;
    let enrolled_vec = generate_normalized_vector(2.0, dim);
    let enrolled = VoiceBiometricEmbedding {
        dimension: dim,
        vector: enrolled_vec,
    };
    let gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

    // 1. All-zeros candidate -> norm_sq = 0.0 < 1e-9 -> fail-closed Err
    let all_zeros = VoiceBiometricEmbedding {
        dimension: dim,
        vector: vec![0.0f32; dim],
    };
    assert!(
        gate.verify(&all_zeros).is_err(),
        "All-zeros candidate must be rejected fail-closed"
    );

    // 2. Subnormal / denormal floats: f32::from_bits(1) is min positive subnormal (~1.4e-45)
    let min_subnormal = f32::from_bits(1);
    assert!(min_subnormal.is_subnormal());
    let subnormal_cand = VoiceBiometricEmbedding {
        dimension: dim,
        vector: vec![min_subnormal; dim],
    };
    assert!(
        gate.verify(&subnormal_cand).is_err(),
        "Subnormal embedding vector must be rejected due to degenerate norm"
    );

    // 3. Largest subnormal: f32::from_bits(0x007f_ffff) (~1.175e-38)
    let max_subnormal = f32::from_bits(0x007f_ffff);
    assert!(max_subnormal.is_subnormal());
    let max_subnormal_cand = VoiceBiometricEmbedding {
        dimension: dim,
        vector: vec![max_subnormal; dim],
    };
    assert!(
        gate.verify(&max_subnormal_cand).is_err(),
        "Max subnormal vector must be rejected due to degenerate norm"
    );

    // 4. Infiltration of single subnormal into enrolled creation
    assert!(SovereignVoiceprintGate::new(subnormal_cand.clone()).is_err());
    assert!(SovereignVoiceprintGate::new(all_zeros.clone()).is_err());
}

#[test]
fn test_adv_nan_inf_fail_closed_infiltration() {
    let dim = 192;
    let valid_vec = generate_normalized_vector(3.0, dim);
    let enrolled = VoiceBiometricEmbedding {
        dimension: dim,
        vector: valid_vec.clone(),
    };
    let mut gate = SovereignVoiceprintGate::new(enrolled).expect("gate creation");

    let bad_values = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];

    // Infiltration at every position in candidate: start, middle, end
    for &bad in &bad_values {
        for &pos in &[0, dim / 2, dim - 1] {
            let mut corrupted = valid_vec.clone();
            corrupted[pos] = bad;
            let cand = VoiceBiometricEmbedding {
                dimension: dim,
                vector: corrupted,
            };
            assert!(
                gate.verify(&cand).is_err(),
                "Non-finite value {bad} at position {pos} must be rejected with Err"
            );
        }

        // Entire vector non-finite
        let all_bad = VoiceBiometricEmbedding {
            dimension: dim,
            vector: vec![bad; dim],
        };
        assert!(gate.verify(&all_bad).is_err());

        // Enrolled update rejected
        assert!(gate.update_enrolled(all_bad).is_err());
    }

    // Gate threshold non-finite or out-of-bounds
    for bad_thresh in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.5,
        0.0,
        1.0001,
        2.0,
    ] {
        let res = SovereignVoiceprintGate::with_threshold(
            VoiceBiometricEmbedding {
                dimension: dim,
                vector: valid_vec.clone(),
            },
            bad_thresh,
        );
        assert!(
            res.is_err(),
            "Invalid threshold {bad_thresh} must be rejected during gate construction"
        );
    }
}

#[test]
fn test_adv_dimension_mismatch_and_coherence() {
    let dim = 192;
    let valid_vec = generate_normalized_vector(4.0, dim);
    let gate = SovereignVoiceprintGate::new(VoiceBiometricEmbedding {
        dimension: dim,
        vector: valid_vec.clone(),
    })
    .expect("gate creation");

    // 1. Dimension field declared != vector length
    let incoherent = VoiceBiometricEmbedding {
        dimension: 192,
        vector: vec![0.1f32; 128],
    };
    assert!(gate.verify(&incoherent).is_err());

    // 2. Dimension 0
    let zero_dim = VoiceBiometricEmbedding {
        dimension: 0,
        vector: Vec::new(),
    };
    assert!(gate.verify(&zero_dim).is_err());

    // 3. Different valid dimensions (e.g. 128 or 256 vs 192)
    for other_dim in [64, 128, 256, 512] {
        let other_vec = generate_normalized_vector(4.0, other_dim);
        let other_cand = VoiceBiometricEmbedding {
            dimension: other_dim,
            vector: other_vec,
        };
        assert!(
            gate.verify(&other_cand).is_err(),
            "Candidate with dimension {other_dim} != 192 must be rejected"
        );
    }
}

struct AdversarialMockIdentifier {
    output: VoiceBiometricEmbedding,
}

#[async_trait::async_trait]
impl SpeakerIdentifier for AdversarialMockIdentifier {
    async fn extract_embedding(&self, _chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding> {
        Ok(self.output.clone())
    }
}

#[tokio::test]
async fn test_adv_verify_audio_fail_closed_pipeline() {
    let dim = 192;
    let valid_vec = generate_normalized_vector(5.0, dim);
    let gate = SovereignVoiceprintGate::new(VoiceBiometricEmbedding {
        dimension: dim,
        vector: valid_vec.clone(),
    })
    .expect("gate creation");

    let mock_id = AdversarialMockIdentifier {
        output: VoiceBiometricEmbedding {
            dimension: dim,
            vector: valid_vec.clone(),
        },
    };

    // 1. Empty samples buffer
    let empty_chunk = AudioChunk {
        samples: Vec::new(),
        sample_rate: 16000,
        channels: 1,
        timestamp: hakille_core::Timestamp::now(),
    };
    assert!(gate.verify_audio(&mock_id, &empty_chunk).await.is_err());

    // 2. Audio buffer shorter than CAMP_WIN (400 samples)
    for short_len in [1, 10, 100, 250, 399] {
        let short_chunk = AudioChunk {
            samples: vec![0.05f32; short_len],
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(
            gate.verify_audio(&mock_id, &short_chunk).await.is_err(),
            "Audio chunk of len {short_len} < 400 must be rejected fail-closed"
        );
    }

    // 3. Audio buffer containing non-finite samples (NaN/Inf)
    for bad_sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut corrupted_samples = vec![0.1f32; 800];
        corrupted_samples[400] = bad_sample;
        let bad_chunk = AudioChunk {
            samples: corrupted_samples,
            sample_rate: 16000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(
            gate.verify_audio(&mock_id, &bad_chunk).await.is_err(),
            "Audio chunk with sample {bad_sample} must be rejected"
        );
    }

    // 4. Legitimate chunk (>= 400 finite samples) passes and verifies
    let legit_chunk = AudioChunk {
        samples: vec![0.05f32; 1600],
        sample_rate: 16000,
        channels: 1,
        timestamp: hakille_core::Timestamp::now(),
    };
    let res = gate
        .verify_audio(&mock_id, &legit_chunk)
        .await
        .expect("legit chunk");
    assert!(res.is_verified);
    assert_eq!(res.threshold, SOVEREIGN_VOICEPRINT_THRESHOLD);
}

#[test]
fn test_adv_high_concurrency_stress_verification() {
    let dim = 192;
    let enrolled_vec = generate_normalized_vector(42.0, dim);
    let gate = Arc::new(
        SovereignVoiceprintGate::new(VoiceBiometricEmbedding {
            dimension: dim,
            vector: enrolled_vec.clone(),
        })
        .expect("gate creation"),
    );

    // Pre-create different test candidates
    // A: Exact sovereign (S = 1.0) -> verified: true
    let cand_sovereign = VoiceBiometricEmbedding {
        dimension: dim,
        vector: enrolled_vec.clone(),
    };

    // B: Impostor (S << 0.72) -> verified: false
    let cand_impostor = VoiceBiometricEmbedding {
        dimension: dim,
        vector: generate_normalized_vector(999.0, dim),
    };

    // C: Inverted (S = -1.0) -> verified: false
    let cand_inverted = VoiceBiometricEmbedding {
        dimension: dim,
        vector: enrolled_vec.iter().map(|&x| -x).collect(),
    };

    // D: Boundary slightly above (S = 0.72005) -> verified: true
    let cand_above = VoiceBiometricEmbedding {
        dimension: dim,
        vector: construct_vector_with_cosine(0.72005, &enrolled_vec),
    };

    // E: Boundary slightly below (S = 0.71995) -> verified: false
    let cand_below = VoiceBiometricEmbedding {
        dimension: dim,
        vector: construct_vector_with_cosine(0.71995, &enrolled_vec),
    };

    // F: Pathological NaN vector -> Err
    let mut v_nan = enrolled_vec.clone();
    v_nan[42] = f32::NAN;
    let cand_nan = VoiceBiometricEmbedding {
        dimension: dim,
        vector: v_nan,
    };

    let num_threads = 16;
    let iterations_per_thread = 500;

    let verified_sovereign_count = Arc::new(AtomicUsize::new(0));
    let rejected_impostor_count = Arc::new(AtomicUsize::new(0));
    let rejected_inverted_count = Arc::new(AtomicUsize::new(0));
    let verified_boundary_count = Arc::new(AtomicUsize::new(0));
    let rejected_boundary_count = Arc::new(AtomicUsize::new(0));
    let rejected_nan_error_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    let start_time = std::time::Instant::now();

    for _ in 0..num_threads {
        let gate = Arc::clone(&gate);
        let c_sov = cand_sovereign.clone();
        let c_imp = cand_impostor.clone();
        let c_inv = cand_inverted.clone();
        let c_abv = cand_above.clone();
        let c_blw = cand_below.clone();
        let c_nan = cand_nan.clone();

        let v_sov_cnt = Arc::clone(&verified_sovereign_count);
        let r_imp_cnt = Arc::clone(&rejected_impostor_count);
        let r_inv_cnt = Arc::clone(&rejected_inverted_count);
        let v_abv_cnt = Arc::clone(&verified_boundary_count);
        let r_blw_cnt = Arc::clone(&rejected_boundary_count);
        let r_nan_cnt = Arc::clone(&rejected_nan_error_count);

        handles.push(thread::spawn(move || {
            for _ in 0..iterations_per_thread {
                // 1. Sovereign candidate
                let res_sov = gate.verify(&c_sov).expect("verify sovereign");
                if res_sov.is_verified {
                    v_sov_cnt.fetch_add(1, Ordering::Relaxed);
                }

                // 2. Impostor candidate
                let res_imp = gate.verify(&c_imp).expect("verify impostor");
                if !res_imp.is_verified {
                    r_imp_cnt.fetch_add(1, Ordering::Relaxed);
                }

                // 3. Inverted candidate
                let res_inv = gate.verify(&c_inv).expect("verify inverted");
                if !res_inv.is_verified {
                    r_inv_cnt.fetch_add(1, Ordering::Relaxed);
                }

                // 4. Boundary above
                let res_abv = gate.verify(&c_abv).expect("verify boundary above");
                if res_abv.is_verified {
                    v_abv_cnt.fetch_add(1, Ordering::Relaxed);
                }

                // 5. Boundary below
                let res_blw = gate.verify(&c_blw).expect("verify boundary below");
                if !res_blw.is_verified {
                    r_blw_cnt.fetch_add(1, Ordering::Relaxed);
                }

                // 6. Pathological NaN
                if gate.verify(&c_nan).is_err() {
                    r_nan_cnt.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    let elapsed = start_time.elapsed();
    let total_ops = num_threads * iterations_per_thread * 6;
    eprintln!(
        "High concurrency stress test: {total_ops} operations across {num_threads} threads took {elapsed:?} ({:.1} ops/sec)",
        total_ops as f64 / elapsed.as_secs_f64()
    );

    let expected_per_category = num_threads * iterations_per_thread;
    assert_eq!(
        verified_sovereign_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every sovereign verification must succeed"
    );
    assert_eq!(
        rejected_impostor_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every impostor verification must be rejected"
    );
    assert_eq!(
        rejected_inverted_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every inverted verification must be rejected"
    );
    assert_eq!(
        verified_boundary_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every boundary-above verification must succeed"
    );
    assert_eq!(
        rejected_boundary_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every boundary-below verification must be rejected"
    );
    assert_eq!(
        rejected_nan_error_count.load(Ordering::SeqCst),
        expected_per_category,
        "Every NaN pathological input must return Err fail-closed"
    );
}
