//! Intégration réelle Silero → Whisper → activation sur PCM synthétique.
//!
//! Les fixtures proviennent d'eSpeak NG (texte de synthèse dans manifest.json),
//! pas d'un microphone ni d'un locuteur humain. Ce test prouve le passage entre
//! les moteurs et le contrôleur ; il ne mesure pas la précision sur les accents.
//! Aucun résultat ASR n'est remplacé par le texte du manifeste.
//!
//! Exécution explicite :
//! HAKILLE_VOICE_FIXTURES=/chemin/fixtures HAKILLE_MODELS_DIR=/chemin/models \
//! cargo test -p hakille-voice --test activation_real -- --ignored --nocapture

use hakille_inference::ModelProvisioner;
use hakille_perception::audio::{
    ActivationIgnoreReason, ActivationOutcome, PipelineState, VoiceEvent, VoicePipeline,
    VoicePipelineConfig,
};
use hakille_voice::input::{VoiceInputConfig, VoiceRecognizer};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn fixture_pcm(fixtures_dir: &Path, manifest: &Value, name: &str) -> Vec<f32> {
    let fixture = manifest["fixtures"]
        .as_array()
        .expect("manifest.fixtures doit être une liste")
        .iter()
        .find(|row| row["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("fixture requise absente du manifeste : {name}"));
    assert_eq!(fixture["synthetic"], true, "provenance synthétique requise");
    let source_path = Path::new(fixture["pcm"].as_str().expect("chemin PCM requis"));
    // Le manifeste d'origine contient des chemins absolus. L'environnement
    // fournit le répertoire réel pour que les mêmes fixtures soient déplaçables.
    let path = fixtures_dir.join(source_path.file_name().expect("nom PCM requis"));
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("fixture PCM requise {} : {error}", path.display()));
    let samples = usize::try_from(
        fixture["samples"]
            .as_u64()
            .expect("nombre de samples requis"),
    )
    .expect("nombre de samples représentable");
    assert!(samples > 0 && samples <= 16_000 * 12);
    assert_eq!(bytes.len(), samples * 4, "PCM f32 LE complet : {name}");
    let pcm: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("sample de 4 octets")))
        .collect();
    assert!(
        pcm.iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
    );
    pcm
}

fn segment_with_real_silero(models_dir: &Path, pcm: &[f32], name: &str) -> Vec<f32> {
    let mut pipeline = VoicePipeline::new(VoicePipelineConfig {
        models_dir: Some(models_dir.to_path_buf()),
        wake_word: None,
        enable_asr: false,
        vad_hangover_windows: 25,
        pre_roll_ms: 300,
        max_utterance_seconds: 12.0,
        ..VoicePipelineConfig::default()
    })
    .expect("le vrai modèle Silero doit être disponible et compatible");
    let mut stream = pcm.to_vec();
    stream.extend(std::iter::repeat_n(0.0, 24_000)); // 1,5 s de silence de fin.
    let mut starts = 0;
    let mut ends = 0;
    for chunk in stream.chunks(160) {
        for event in pipeline.push_audio(chunk) {
            match event {
                VoiceEvent::SpeechStart => starts += 1,
                VoiceEvent::SpeechEnd => ends += 1,
                VoiceEvent::Error(error) => panic!("{name}: erreur du pipeline réel : {error}"),
                VoiceEvent::Wake { .. } => panic!("le VAD seul ne doit pas détecter Hakille"),
                VoiceEvent::Transcript { .. } => panic!("ASR du pipeline désactivé dans ce test"),
            }
        }
    }
    // Pas de finish_stream forcé : la fin doit venir des fenêtres Silero et
    // du hangover, comme pour un segment reçu par le worker microphone.
    assert_eq!(starts, 1, "{name}: début de parole réel requis");
    assert_eq!(
        ends, 1,
        "{name}: fin de parole réelle requise après silence"
    );
    assert_eq!(pipeline.state(), PipelineState::Transcribing);
    assert!(
        !pipeline.last_utterance_truncated(),
        "{name}: segment tronqué"
    );
    let utterance = pipeline.take_utterance().expect("segment VAD disponible");
    assert!((400..=stream.len()).contains(&utterance.len()));
    eprintln!(
        "{name}: PCM source={} samples, segment Silero={} samples ({} s)",
        pcm.len(),
        utterance.len(),
        utterance.len() as f32 / 16_000.0
    );
    utterance
}

fn recognizer(models_dir: &Path, language: &str) -> VoiceRecognizer {
    VoiceRecognizer::new(&VoiceInputConfig {
        models_dir: models_dir.to_path_buf(),
        language: Some(language.to_owned()),
        ..VoiceInputConfig::default()
    })
    .expect("Whisper réel, vérifié et avec indice lexical Hakille, doit être disponible")
}

#[test]
#[ignore = "exige HAKILLE_VOICE_FIXTURES (manifest + PCM eSpeak) et HAKILLE_MODELS_DIR (Silero + Whisper réels)"]
fn vad_whisper_activation_synthetic_fixtures() {
    let fixtures_dir = PathBuf::from(
        std::env::var_os("HAKILLE_VOICE_FIXTURES").expect("HAKILLE_VOICE_FIXTURES obligatoire"),
    );
    let models_dir = PathBuf::from(
        std::env::var_os("HAKILLE_MODELS_DIR").expect("HAKILLE_MODELS_DIR obligatoire"),
    );
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(fixtures_dir.join("manifest.json")).expect("manifest.json obligatoire"),
    )
    .expect("manifeste JSON valide");
    assert_eq!(manifest["synthetic"], true);
    assert_eq!(manifest["sample_rate"], 16_000);
    assert_eq!(manifest["channels"], 1);
    eprintln!("Provenance : {}", manifest["source"]);
    let reports = ModelProvisioner::new(models_dir.clone())
        .verify_model_by_id("silero-vad-v5")
        .expect("vérification du modèle Silero");
    assert!(!reports.is_empty() && reports.iter().all(|report| report.verified));

    for (name, language) in [
        ("fr_hakille_only", "fr"),
        ("en_hakille_command", "en"),
        ("en_greeting_negative", "en"),
    ] {
        let pcm = fixture_pcm(&fixtures_dir, &manifest, name);
        let utterance = segment_with_real_silero(&models_dir, &pcm, name);
        // Nouveau contrôleur par cas : le négatif anglais ne bénéficie jamais
        // d'un armement antérieur ni d'un cooldown qui masquerait un faux positif.
        let mut recognizer = recognizer(&models_dir, language);
        let started = Instant::now();
        let result = recognizer
            .recognize(&utterance)
            .unwrap_or_else(|error| panic!("{name}: reconnaissance réelle échouée : {error}"));
        eprintln!("{name}: {:?} ; durée={:?}", result, started.elapsed());
        assert!(!result.transcription.suppressed_silence);
        assert!(!result.transcription.text.trim().is_empty());
        assert_eq!(result.transcription.language.as_deref(), Some(language));
        match name {
            "fr_hakille_only" => assert!(
                matches!(result.activation, ActivationOutcome::Armed { .. }),
                "mot seul : armement attendu, résultat réel {:?}",
                result.activation
            ),
            "en_hakille_command" => match result.activation {
                ActivationOutcome::Command { text, language } => {
                    assert_eq!(language, "en");
                    let words = text
                        .split(|character: char| !character.is_alphabetic())
                        .filter(|word| !word.is_empty())
                        .map(str::to_lowercase)
                        .collect::<Vec<_>>();
                    assert_eq!(words, ["what", "time", "is", "it"]);
                }
                other => panic!("commande anglaise attendue, résultat réel {other:?}"),
            },
            "en_greeting_negative" => assert_eq!(
                result.activation,
                ActivationOutcome::Ignored {
                    reason: ActivationIgnoreReason::NoWakeWord
                },
                "salutation sans Hakille : aucune activation autorisée"
            ),
            _ => unreachable!(),
        }
    }
}
