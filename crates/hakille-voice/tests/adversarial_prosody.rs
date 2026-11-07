//! Adversarial stress test suite for `crates/hakille-voice` (Milestone 10 Challenger 2)

use hakille_core::trust::ConfidenceLevel;
use hakille_voice::FormantVoice;
use hakille_voice::prosody::{
    AdaptiveProsodyEngine, PROSODY_MAX_BREATHINESS, PROSODY_MAX_F0, PROSODY_MAX_RATE,
    PROSODY_MIN_BREATHINESS, PROSODY_MIN_F0, PROSODY_MIN_RATE, ProsodyContext,
};
use hakille_voice::tts::{FormantTts, TtsError};

#[test]
fn test_adv_prosody_non_finite_and_out_of_bounds_inputs() {
    let engine = AdaptiveProsodyEngine::new();
    let base_voice = FormantVoice::default();

    let c_adversarial = [
        -100.0f32,
        -10.0,
        -1.0,
        -0.0001,
        0.0,
        0.5,
        1.0,
        1.0001,
        2.0,
        50.0,
        1000.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];

    let u_adversarial = [
        -50.0f32,
        -5.0,
        -1.0,
        -0.0001,
        0.0,
        0.5,
        1.0,
        1.0001,
        2.0,
        10.0,
        500.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];

    for &c in &c_adversarial {
        for &u in &u_adversarial {
            // Direct formulas
            let rate = AdaptiveProsodyEngine::compute_rate(base_voice.rate, u, c);
            assert!(
                rate.is_finite(),
                "Rate must be finite for C={c}, U={u}; got {rate}"
            );
            assert!(
                rate >= PROSODY_MIN_RATE && rate <= PROSODY_MAX_RATE,
                "Rate out of bounds [{PROSODY_MIN_RATE}, {PROSODY_MAX_RATE}]: got {rate} for C={c}, U={u}"
            );

            let pitch = AdaptiveProsodyEngine::compute_pitch(base_voice.f0_base_hz, u, c);
            assert!(
                pitch.is_finite(),
                "Pitch must be finite for C={c}, U={u}; got {pitch}"
            );
            assert!(
                pitch >= PROSODY_MIN_F0 && pitch <= PROSODY_MAX_F0,
                "Pitch out of bounds [{PROSODY_MIN_F0}, {PROSODY_MAX_F0}]: got {pitch} for C={c}, U={u}"
            );

            let breath = AdaptiveProsodyEngine::compute_breathiness(base_voice.breathiness, u, c);
            assert!(
                breath.is_finite(),
                "Breathiness must be finite for C={c}, U={u}; got {breath}"
            );
            assert!(
                breath >= PROSODY_MIN_BREATHINESS && breath <= PROSODY_MAX_BREATHINESS,
                "Breathiness out of bounds [{PROSODY_MIN_BREATHINESS}, {PROSODY_MAX_BREATHINESS}]: got {breath} for C={c}, U={u}"
            );

            // Context evaluation
            let ctx = ProsodyContext {
                confidence: ConfidenceLevel {
                    model_confidence: c as f64,
                    graph_corroboration: c as f64,
                    combined: c as f64,
                },
                urgency: u,
            };
            let adapted = engine.evaluate(&base_voice, &ctx);
            assert!(
                adapted.voice.rate >= PROSODY_MIN_RATE && adapted.voice.rate <= PROSODY_MAX_RATE,
                "Adapted voice rate out of bounds: {}",
                adapted.voice.rate
            );
            assert!(
                adapted.voice.f0_base_hz >= PROSODY_MIN_F0
                    && adapted.voice.f0_base_hz <= PROSODY_MAX_F0,
                "Adapted voice F0 out of bounds: {}",
                adapted.voice.f0_base_hz
            );
            assert!(
                adapted.voice.breathiness >= PROSODY_MIN_BREATHINESS
                    && adapted.voice.breathiness <= PROSODY_MAX_BREATHINESS,
                "Adapted voice breathiness out of bounds: {}",
                adapted.voice.breathiness
            );
        }
    }
}

#[test]
fn test_adv_prosody_boundary_epistemic_contradiction() {
    // Requirements:
    // |C_model - C_graph| = 0.39999 (no rise) vs 0.40001 (terminal rise triggers)
    // To isolate epistemic contradiction condition from general uncertainty (c < 0.60),
    // combined confidence must be >= 0.60.

    // Case 1: Model higher than Graph
    // Let C_model = 0.85000.
    // Graph = 0.85000 - 0.39999 = 0.45001.
    // Discrepancy = 0.39999.
    // Combined = 0.85000 * 0.6 + 0.45001 * 0.4 = 0.51000 + 0.180004 = 0.690004 >= 0.60.
    let ctx_sub_boundary = ProsodyContext {
        confidence: ConfidenceLevel {
            model_confidence: 0.85000,
            graph_corroboration: 0.45001,
            combined: 0.85000 * 0.6 + 0.45001 * 0.4,
        },
        urgency: 0.0,
    };
    assert!(
        !AdaptiveProsodyEngine::should_terminal_rise(&ctx_sub_boundary),
        "Discrepancy 0.39999 with C=0.69 must NOT trigger terminal rise"
    );

    // Super-boundary: Discrepancy = 0.40001
    // Graph = 0.85000 - 0.40001 = 0.44999.
    // Discrepancy = 0.40001.
    // Combined = 0.85000 * 0.6 + 0.44999 * 0.4 = 0.51000 + 0.179996 = 0.689996 >= 0.60.
    let ctx_super_boundary = ProsodyContext {
        confidence: ConfidenceLevel {
            model_confidence: 0.85000,
            graph_corroboration: 0.44999,
            combined: 0.85000 * 0.6 + 0.44999 * 0.4,
        },
        urgency: 0.0,
    };
    assert!(
        AdaptiveProsodyEngine::should_terminal_rise(&ctx_super_boundary),
        "Discrepancy 0.40001 with C=0.69 MUST trigger terminal rise"
    );

    // Case 2: Inverted (Graph higher than Model)
    // C_graph = 0.85000, C_model = 0.45001 -> Discrepancy = 0.39999
    // Combined = 0.45001 * 0.6 + 0.85000 * 0.4 = 0.270006 + 0.34 = 0.610006 >= 0.60.
    let ctx_inv_sub = ProsodyContext {
        confidence: ConfidenceLevel {
            model_confidence: 0.45001,
            graph_corroboration: 0.85000,
            combined: 0.45001 * 0.6 + 0.85000 * 0.4,
        },
        urgency: 0.0,
    };
    assert!(
        !AdaptiveProsodyEngine::should_terminal_rise(&ctx_inv_sub),
        "Inverted discrepancy 0.39999 with C=0.61 must NOT trigger terminal rise"
    );

    // Inverted Super-boundary: C_graph = 0.85000, C_model = 0.44999 -> Discrepancy = 0.40001
    // Combined = 0.44999 * 0.6 + 0.85000 * 0.4 = 0.269994 + 0.34 = 0.609994 >= 0.60.
    let ctx_inv_super = ProsodyContext {
        confidence: ConfidenceLevel {
            model_confidence: 0.44999,
            graph_corroboration: 0.85000,
            combined: 0.44999 * 0.6 + 0.85000 * 0.4,
        },
        urgency: 0.0,
    };
    assert!(
        AdaptiveProsodyEngine::should_terminal_rise(&ctx_inv_super),
        "Inverted discrepancy 0.40001 with C=0.61 MUST trigger terminal rise"
    );
}

#[test]
fn test_adv_prosody_render_extreme_texts() {
    let tts = FormantTts::new();
    let ctx_neutral = ProsodyContext::confident_neutral();
    let ctx_contra = ProsodyContext::epistemic_contradiction();

    // 1. Whitespace only -> must return Err(TtsError::EmptyText)
    for ws in ["", "   ", "\t\t\n\r  ", " \n "] {
        let res = tts.render_pcm_with_prosody(ws, &ctx_neutral);
        assert!(
            matches!(res, Err(TtsError::EmptyText)),
            "Whitespace-only text '{ws}' must fail with EmptyText"
        );
    }

    // 2. Pure emojis -> no pronounceable phonemes -> Err(TtsError::EmptyText)
    let emojis = "😀🎉🔥🚀🤖✨💡🏆🎮🌍";
    let res_emojis = tts.render_pcm_with_prosody(emojis, &ctx_neutral);
    assert!(
        matches!(res_emojis, Err(TtsError::EmptyText)),
        "Pure emojis text must fail-closed with EmptyText"
    );

    // 3. Emojis mixed with text -> should synthesize the phonemes without panic
    let mixed = "Bonjour le monde ! 😀🎉🚀";
    let res_mixed = tts.render_pcm_with_prosody(mixed, &ctx_neutral);
    assert!(res_mixed.is_ok(), "Mixed text + emojis should synthesize");
    let pcm_mixed = res_mixed.unwrap();
    assert!(!pcm_mixed.samples.is_empty());
    assert!(pcm_mixed.samples.iter().all(|s| s.is_finite()));

    // 4. Purely unvoiced consonant strings ("pst! ssshh!")
    let unvoiced = "pst! ssshh!";
    let res_unvoiced = tts.render_pcm_with_prosody(unvoiced, &ctx_contra);
    assert!(
        res_unvoiced.is_ok(),
        "Purely unvoiced consonant string must synthesize"
    );
    let pcm_unvoiced = res_unvoiced.unwrap();
    assert!(!pcm_unvoiced.samples.is_empty());
    assert!(pcm_unvoiced.samples.iter().all(|s| s.is_finite()));
    assert!(
        pcm_unvoiced.samples.iter().all(|&s| s >= -1.0 && s <= 1.0),
        "Samples must be in [-1, 1]"
    );

    // 5. Massive text (10k chars)
    let sentence =
        "Le systeme cognitif souverain garantit l'integrite de la parole et de la pensee. ";
    let repeat_count = (10_000 / sentence.len()) + 1;
    let mut massive_text = sentence.repeat(repeat_count);
    massive_text.truncate(10_000);
    assert_eq!(massive_text.len(), 10_000);

    let start = std::time::Instant::now();
    let res_massive = tts.render_pcm_with_prosody(&massive_text, &ctx_neutral);
    let elapsed = start.elapsed();
    eprintln!("10k chars synthesis elapsed: {elapsed:?}");
    assert!(
        res_massive.is_ok(),
        "10k characters synthesis should succeed without stack overflow or panic"
    );
    let pcm_massive = res_massive.unwrap();
    assert!(!pcm_massive.samples.is_empty());
    assert!(pcm_massive.samples.iter().all(|s| s.is_finite()));
    assert!(
        pcm_massive.samples.iter().all(|&s| s >= -1.0 && s <= 1.0),
        "Samples must be clamped in [-1.0, 1.0]"
    );
}

#[test]
fn test_adv_prosody_non_finite_base_voice() {
    let engine = AdaptiveProsodyEngine::new();
    let ctx = ProsodyContext::confident_neutral();

    let adversarial_floats = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -1000.0,
        -1.0,
        0.0,
        10000.0,
    ];

    for &bad in &adversarial_floats {
        let bad_voice = FormantVoice {
            f0_base_hz: bad,
            rate: bad,
            breathiness: bad,
            gain: 0.8,
        };

        let adapted = engine.evaluate(&bad_voice, &ctx);
        assert!(
            adapted.voice.rate.is_finite(),
            "Rate must be finite for base {bad}, got {}",
            adapted.voice.rate
        );
        assert!(
            adapted.voice.rate >= PROSODY_MIN_RATE && adapted.voice.rate <= PROSODY_MAX_RATE,
            "Rate must be bounded in [{}, {}]: got {}",
            PROSODY_MIN_RATE,
            PROSODY_MAX_RATE,
            adapted.voice.rate
        );

        assert!(
            adapted.voice.f0_base_hz.is_finite(),
            "F0 must be finite for base {bad}, got {}",
            adapted.voice.f0_base_hz
        );
        assert!(
            adapted.voice.f0_base_hz >= PROSODY_MIN_F0
                && adapted.voice.f0_base_hz <= PROSODY_MAX_F0,
            "F0 must be bounded in [{}, {}]: got {}",
            PROSODY_MIN_F0,
            PROSODY_MAX_F0,
            adapted.voice.f0_base_hz
        );

        assert!(
            adapted.voice.breathiness.is_finite(),
            "Breathiness must be finite for base {bad}, got {}",
            adapted.voice.breathiness
        );
        assert!(
            adapted.voice.breathiness >= PROSODY_MIN_BREATHINESS
                && adapted.voice.breathiness <= PROSODY_MAX_BREATHINESS,
            "Breathiness must be bounded in [{}, {}]: got {}",
            PROSODY_MIN_BREATHINESS,
            PROSODY_MAX_BREATHINESS,
            adapted.voice.breathiness
        );
    }
}

#[test]
fn test_adv_prosody_nan_inf_confidence_terminal_rise() {
    // Fail-closed requirement: if epistemic confidence is corrupt (NaN or Inf),
    // terminal rise MUST trigger as precautionary hesitation / question inflection.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let ctx_bad = ProsodyContext {
            confidence: ConfidenceLevel {
                model_confidence: bad,
                graph_corroboration: bad,
                combined: bad,
            },
            urgency: 0.0,
        };
        assert!(
            AdaptiveProsodyEngine::should_terminal_rise(&ctx_bad),
            "Non-finite confidence ({bad}) must trigger terminal rise fail-closed"
        );
    }
}
