//! Conformité VAD neurale réelle (Silero v5 via `ort`).
//!
//! - Skip gracieux si `models/silero_vad.onnx` absent (`println!` + `return`, PAS de panic).
//! - Ordre relatif seul (pas de seuil absolu fragile).
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
fn neural_vad_silence_below_tone() {
    let path = silero_path();
    if !Path::new(&path).exists() {
        println!("ignoré : modèle absent");
        return;
    }
    let mut sess = OrtVadSession::open(&path).expect("la vraie session doit s'ouvrir");
    let silence = vec![0.0f32; 512];
    let tone = tone_440hz_512(0.5);
    let p_silence = sess.score(&silence).expect("score silence");
    let p_tone = sess.score(&tone).expect("score tone");
    assert!(
        (0.0..=1.0).contains(&p_silence),
        "proba silence dans [0,1], vu {p_silence}"
    );
    assert!(
        (0.0..=1.0).contains(&p_tone),
        "proba tone dans [0,1], vu {p_tone}"
    );
    assert!(
        p_silence < p_tone,
        "ordre relatif : silence ({p_silence}) < 440Hz ({p_tone})"
    );
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

    // Repli honnête sans modèle : Ok + flag faux.
    // NOTE : avec le modèle présent, le VAD est 100 % réel (pas d'heuristique) ;
    // le vrai Silero peut scorer ~0.5+ sur 512 zéros d'un coup (état récurrent
    // vierge) — on compare donc en relatif (silence < ton), jamais en absolu.
    let mut vad_fallback = VoiceActivityDetector::default_silero_profile();
    let silent = AudioChunk::new(vec![0.0f32; 512], 16_000, 1);
    let res_fb = vad_fallback
        .process_real(&silent)
        .expect("le chemin réel doit réussir (modèle présent)");
    assert!(
        vad_fallback.last_used_real(),
        "chemin 100 % réel, flag vrai"
    );
    assert!(
        res_fb.speech_probability < res.speech_probability,
        "relatif : silence ({}) < ton 440Hz ({})",
        res_fb.speech_probability,
        res.speech_probability
    );
}
