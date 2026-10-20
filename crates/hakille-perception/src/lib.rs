pub mod audio;
pub mod vision;
pub mod world_model;

pub use audio::{
    asr::*, capture::AudioChunk, capture::ContinuousAudioRingBuffer, diarization::*, pipeline::*,
    speaker_id::*, vad::*, wake_word::*,
};
pub use vision::*;
pub use world_model::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Syllabe voisée formantée (recette du golden `ort_vad` : harmoniques +
    /// résonateurs 2 pôles + transients 15 ms). Le vrai Silero VAD v5 REJETTE
    /// les tons purs stationnaires (sine 150 Hz -> ~0.0005) et ne décolle que
    /// sur structure syllabique avec onsets/offsets.
    fn voiced_syllable(f1: f32, f2: f32, f3: f32, f0: f32, dur_s: f32, amp: f32) -> Vec<f32> {
        use std::f32::consts::PI;
        const SR: f32 = 16_000.0;
        fn reson(x: Vec<f32>, fc: f32, r: f32) -> Vec<f32> {
            let b = 2.0 * r * (2.0 * PI * fc / 16_000.0).cos();
            let c = -(r * r);
            let (mut y1, mut y2) = (0.0, 0.0);
            x.into_iter()
                .map(|xn| {
                    let yn = xn + b * y1 + c * y2;
                    y2 = y1;
                    y1 = yn;
                    yn
                })
                .collect()
        }
        let m = (dur_s * SR) as usize;
        let edge = (0.015 * SR).max(8.0);
        let g: Vec<f32> = (0..m)
            .map(|t| {
                let tt = t as f32 / SR;
                let env = ((t as f32 / edge).min(1.0)) * (((m - t) as f32 / edge).min(1.0));
                let mut s = 0.0;
                for k in 1..=16 {
                    s += (tt * f0 * k as f32 * 2.0 * PI).sin() * amp / k as f32;
                }
                s * env
            })
            .collect();
        let (a, b, c) = (
            reson(g.clone(), f1, 0.985),
            reson(g.clone(), f2, 0.98),
            reson(g, f3, 0.97),
        );
        a.into_iter()
            .zip(b)
            .zip(c)
            .map(|((x, y), z)| x * 0.5 + y * 0.35 + z * 0.25)
            .collect()
    }

    #[test]
    fn test_vad_silence_vs_speech() {
        let mut vad = VoiceActivityDetector::default_silero_profile();
        if !vad.model_path().is_some_and(|path| path.is_file()) {
            eprintln!("SKIP: models/silero_vad.onnx absent — test VAD réel ignoré");
            return;
        }

        // 1. Silent chunk
        let silent_chunk = AudioChunk {
            samples: vec![0.0f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        let res_silence = vad.process(&silent_chunk);
        assert!(!res_silence.is_speech);
        assert_eq!(res_silence.energy_rms, 0.0);
        assert!(
            vad.last_used_real(),
            "le chemin doit être le forward réel, jamais l'heuristique"
        );

        // 2. Ton pur stationnaire : le VRAI Silero le rejette (non-parole).
        // Fait empirique du golden `ort_vad` (prob ~0.0005), pas une régression.
        let mut vad_tone = VoiceActivityDetector::default_silero_profile();
        let tone: Vec<f32> = (0..16_000)
            .map(|t| (t as f32 / 16000.0 * 150.0 * 2.0 * std::f32::consts::PI).sin() * 0.4)
            .collect();
        let res_tone = vad_tone.process(&AudioChunk {
            samples: tone,
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        });
        assert!(
            res_tone.speech_probability < 0.05,
            "ton pur rejeté par le vrai modèle: {}",
            res_tone.speech_probability
        );

        // 3. Parole voisée synthétique (/ba/ + /si/ formantés) : contraste
        // strict vs silence, chemin 100 % réel.
        let mut voiced = voiced_syllable(730.0, 1090.0, 2440.0, 118.0, 0.22, 0.5);
        voiced.extend_from_slice(&vec![0.0; 1600]);
        voiced.extend_from_slice(&voiced_syllable(270.0, 2290.0, 3010.0, 132.0, 0.18, 0.45));
        voiced.extend_from_slice(&vec![0.0; 1600]);
        let voiced = {
            let mut v = voiced.clone();
            v.extend_from_slice(&voiced);
            // Normalisation peak 0.5 (recette du golden) : les résonateurs
            // 2 pôles amplifient ~60× ; sans normalisation le signal écrêté
            // sort de la distribution d'entraînement du modèle.
            let peak = v.iter().map(|s| s.abs()).fold(0.0f32, f32::max).max(1e-6);
            for s in v.iter_mut() {
                *s = (*s / peak * 0.5).clamp(-1.0, 1.0);
            }
            v
        };
        let res_speech = vad.process(&AudioChunk {
            samples: voiced,
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        });
        assert!(vad.last_used_real());
        assert!(
            vad.real_forward_calls().unwrap_or(0) > 0,
            "au moins un forward ort réel"
        );
        assert!(
            res_speech.speech_probability > res_silence.speech_probability + 0.005,
            "contraste parole voisée vs silence insuffisant: {} vs {}",
            res_speech.speech_probability,
            res_silence.speech_probability
        );
    }

    #[test]
    fn test_wake_word_trigger_and_cooldown() {
        let mut detector = WakeWordDetector::default_hakille();

        // Silence: not detected
        let silent_chunk = AudioChunk {
            samples: vec![0.0f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        assert!(!detector.process(&silent_chunk).detected);

        // High-energy voice chunk: detected
        let energetic_chunk = AudioChunk {
            samples: vec![0.3f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        let det = detector.process(&energetic_chunk);
        assert!(det.detected);
        assert_eq!(det.keyword, "Dis Hakille");

        // Immediately following chunk: must be blocked by cooldown
        let follow_up = detector.process(&energetic_chunk);
        assert!(
            !follow_up.detected,
            "Cooldown must prevent consecutive wake word triggers"
        );
    }

    #[test]
    fn test_vad_nan_and_inf_resilience() {
        let mut vad = VoiceActivityDetector::default_silero_profile();

        let corrupted_chunk = AudioChunk {
            samples: vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.2, 0.3],
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };

        let res = vad.process(&corrupted_chunk);
        assert!(!res.energy_rms.is_nan(), "RMS must never be NaN");
        assert!(
            !res.speech_probability.is_nan(),
            "Speech probability must never be NaN"
        );
        assert!(res.speech_probability >= 0.0 && res.speech_probability <= 1.0);
    }

    #[test]
    fn test_camera_pipeline_and_world_model() {
        let config = hakille_substrate::CameraConfig {
            width: 640,
            height: 480,
            fps: 30,
            facing: hakille_substrate::CameraFacing::Front,
        };
        let mut pipeline = CameraPipeline::new(config.clone());
        assert_eq!(pipeline.frames_processed(), 0);

        // 1. Process dark empty frame
        let dark_frame = hakille_substrate::CameraFrame {
            width: 640,
            height: 480,
            data: vec![5u8; 640 * 480 * 3],
            format: hakille_substrate::ImageFormat::Rgb8,
            timestamp_ms: 1000,
        };
        let ctx_dark = pipeline.process_frame(&dark_frame);
        assert_eq!(ctx_dark.lighting, LightingCondition::Dark);
        assert_eq!(ctx_dark.user_presence, UserPresence::Absent);
        assert_eq!(pipeline.frames_processed(), 1);

        // 2. Process daylight frame with centered subject
        let mut bright_data = vec![140u8; 640 * 480 * 3];
        // Inject high center contrast (representing a face/silhouette)
        for y in 160..320 {
            for x in 200..440 {
                let idx = (y * 640 + x) * 3;
                bright_data[idx] = 230;
                bright_data[idx + 1] = 190;
                bright_data[idx + 2] = 170;
            }
        }
        let bright_frame = hakille_substrate::CameraFrame {
            width: 640,
            height: 480,
            data: bright_data,
            format: hakille_substrate::ImageFormat::Rgb8,
            timestamp_ms: 2000,
        };
        let ctx_bright = pipeline.process_frame(&bright_frame);
        assert_eq!(ctx_bright.lighting, LightingCondition::IndoorAdequate);
        assert_eq!(ctx_bright.user_presence, UserPresence::Present);
        assert!(ctx_bright.user_gaze_focused_on_device);
        assert_eq!(pipeline.frames_processed(), 2);

        // 3. Update WorldModel with camera perception
        let mut world_model = WorldModel::new("HomeOffice");
        pipeline.update_world_model(&ctx_bright, &mut world_model);

        assert!(world_model.state().user_present);
        assert!(world_model.state().user_attentive);
        assert!(!world_model.state().third_parties_present);
        assert!(world_model.is_private_environment());
    }

    #[test]
    fn test_environmental_privacy_arbitration() {
        let mut world_model = WorldModel::new("ConferenceRoom");
        assert!(world_model.is_private_environment());

        // Introduce multiple people in room
        world_model.update_visual_presence(true, false, 3, 2.5, "Daylight");
        assert!(world_model.state().third_parties_present);
        assert!(!world_model.is_private_environment());
        assert!(world_model.should_suppress_loud_audio());

        // In meeting
        world_model.set_meeting_status(true);
        assert!(world_model.should_suppress_loud_audio());
    }
}
