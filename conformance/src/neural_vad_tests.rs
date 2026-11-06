//! Conformité VAD neurale réelle (Silero v5 via `ort`).
//!
//! - Skip gracieux si `models/silero_vad.onnx` absent (`println!` + `return`, PAS de panic).
//! - Silence et tons stationnaires restent sous le seuil de parole (0.5).
//! - Les compteurs attestent les forwards réels, sans imposer un classement
//!   arbitraire entre deux entrées qui ne sont pas de la parole.
//! - Les autres moteurs réels (MiniLM/SenseVoice/CAM++/Qwen/RVQ/formant/CV)
//!   sont golden-testés dans leurs crates respectives (non testés ici).

use hakille_inference::{OrtVadSession, to_mono_16k};
use hakille_perception::audio::{AudioChunk, VoiceActivityDetector};
use std::path::{Path, PathBuf};

fn models_dir() -> PathBuf {
    if Path::new("./models").exists() {
        PathBuf::from("./models")
    } else if Path::new("../models").exists() {
        PathBuf::from("../models")
    } else if Path::new("../../models").exists() {
        PathBuf::from("../../models")
    } else {
        PathBuf::from("./models")
    }
}

fn silero_path() -> PathBuf {
    models_dir().join("silero_vad.onnx")
}

fn tone_440hz_512(amplitude: f32) -> Vec<f32> {
    (0..512)
        .map(|t| (t as f32 / 16_000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * amplitude)
        .collect()
}

#[test]
fn neural_vad_real_session_opens() {
    let path = silero_path();
    if !Path::new(&path).exists() {
        println!("ignoré : modèle absent");
        return;
    }
    let sess = OrtVadSession::open(&path).expect("la vraie session doit s'ouvrir");
    let inputs = sess.real_input_names();
    let outputs = sess.real_output_names();
    assert!(
        inputs.iter().any(|n| n == "input"),
        "le graphe réel doit exposer 'input', vu {inputs:?}"
    );
    assert!(
        outputs.iter().any(|n| n == "output"),
        "le graphe réel doit exposer 'output', vu {outputs:?}"
    );
}

#[test]
fn neural_vad_silence_and_tone_are_not_speech() {
    let path = silero_path();
    if !Path::new(&path).exists() {
        println!("ignoré : modèle absent");
        return;
    }
    let mut sess = OrtVadSession::open(&path).expect("la vraie session doit s'ouvrir");
    let silence = vec![0.0f32; 512];
    let tone = tone_440hz_512(0.5);
    let p_silence = sess.score(&silence).expect("score silence");
    assert_eq!(sess.forward_calls(), 1);
    sess.reset_state(); // Deux signaux indépendants, aucun contexte partagé.
    let p_tone = sess.score(&tone).expect("score tone");
    assert_eq!(sess.forward_calls(), 2);
    assert!(
        (0.0..=1.0).contains(&p_silence),
        "proba silence dans [0,1], vu {p_silence}"
    );
    assert!(
        (0.0..=1.0).contains(&p_tone),
        "proba tone dans [0,1], vu {p_tone}"
    );
    assert!(p_silence < 0.5, "silence non-parole: {p_silence}");
    assert!(p_tone < 0.5, "ton stationnaire non-parole: {p_tone}");
}

#[test]
fn neural_vad_resample_preserves_energy() {
    // 100 ms de 440Hz @44100Hz -> 1600 @16kHz ; RMS préservée à ±10%.
    let sr_in = 44_100u32;
    let n_in = 4410usize;
    let input: Vec<f32> = (0..n_in)
        .map(|t| (t as f32 / sr_in as f32 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5)
        .collect();
    let rms_before = (input.iter().map(|s| s * s).sum::<f32>() / input.len() as f32).sqrt();
    let out = to_mono_16k(&input, 1, sr_in);
    assert!(!out.is_empty(), "resample ne doit pas être vide");
    assert_eq!(out.len(), 1600, "4410 @44100Hz -> 1600 @16kHz");
    let rms_after = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
    let ratio = rms_after / rms_before;
    assert!(
        (0.9..=1.1).contains(&ratio),
        "énergie préservée à ±10%, ratio={ratio} ({rms_before} -> {rms_after})"
    );
}

#[test]
fn neural_vad_last_used_real_flag() {
    let path = silero_path();
    if !Path::new(&path).exists() {
        println!("ignoré : modèle absent");
        return;
    }
    let tone: Vec<f32> = (0..1600)
        .map(|t| (t as f32 / 16_000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5)
        .collect();
    let chunk = AudioChunk::new(tone, 16_000, 1);
    let mut vad = VoiceActivityDetector::with_onnx_model(&path);
    assert!(!vad.last_used_real(), "faux avant tout forward réel");
    let res = vad
        .process_real(&chunk)
        .expect("process_real doit réussir quand le modèle est présent");
    assert!(
        vad.last_used_real(),
        "last_used_real doit être true quand le modèle est présent"
    );
    assert!(
        (0.0..=1.0).contains(&res.speech_probability),
        "proba dans [0,1]"
    );

    assert_eq!(
        vad.real_forward_calls(),
        Some(3),
        "1600 samples: trois fenêtres complètes"
    );
    assert!(!res.is_speech);
    assert!(res.speech_probability < 0.5);

    // Un détecteur indépendant sur silence utilise lui aussi le vrai graphe.
    // Le silence et un ton pur sont tous deux non-parole; leur classement
    // relatif ne constitue pas un contrat de ce classifieur.
    let mut silent_vad = VoiceActivityDetector::with_onnx_model(&path);
    let silent = AudioChunk::new(vec![0.0f32; 512], 16_000, 1);
    let result = silent_vad
        .process_real(&silent)
        .expect("forward silence réel");
    assert!(silent_vad.last_used_real());
    assert_eq!(silent_vad.real_forward_calls(), Some(1));
    assert!((0.0..0.5).contains(&result.speech_probability));
    assert!(!result.is_speech);

    let empty = AudioChunk::new(vec![], 16_000, 1);
    silent_vad.process_real(&empty).unwrap();
    assert!(
        !silent_vad.last_used_real(),
        "aucun forward pour une entrée vide"
    );
    assert_eq!(
        silent_vad.real_forward_calls(),
        Some(1),
        "compteur inchangé"
    );
}
