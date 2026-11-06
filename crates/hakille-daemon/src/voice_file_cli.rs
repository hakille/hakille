//! Diagnostic WAV du même ASR et contrôleur d'activation que le microphone.
//! Les commandes reconnues sont affichées uniquement, jamais soumises au noyau.

use hakille_inference::whisper_engine::{
    WHISPER_LANGUAGES, WHISPER_MAX_SAMPLES, WHISPER_MIN_SAMPLES,
};
use hakille_perception::audio::{ActivationIgnoreReason, ActivationOutcome, WakeActivation};
use hakille_voice::input::{VoiceInputConfig, VoiceRecognition, VoiceRecognizer};
use serde_json::{Value, json};
use std::{collections::HashSet, path::PathBuf};

struct Options {
    config: VoiceInputConfig,
    audio: PathBuf,
    next_audio: Option<PathBuf>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.first().map(String::as_str) != Some("test") {
            return Err("Diagnostic vocal : voice test --audio <WAV> [--next-audio <WAV>] [--language auto|fr|en|…] [--models-dir <dossier>] [--alias <nom>]".into());
        }
        let mut config = VoiceInputConfig::default();
        let mut audio = None;
        let mut next_audio = None;
        let mut seen = HashSet::new();
        let mut index = 1;
        while index < args.len() {
            let key = args[index].as_str();
            if !matches!(
                key,
                "--audio" | "--next-audio" | "--language" | "--models-dir" | "--alias"
            ) {
                return Err(format!("Option de diagnostic vocal inconnue : {key}"));
            }
            if key != "--alias" && !seen.insert(key) {
                return Err(format!("Option répétée : {key}"));
            }
            let value = args
                .get(index + 1)
                .filter(|value| !value.is_empty() && !value.starts_with("--"))
                .ok_or_else(|| format!("Valeur requise pour {key}"))?;
            match key {
                "--audio" => audio = Some(PathBuf::from(value)),
                "--next-audio" => next_audio = Some(PathBuf::from(value)),
                "--models-dir" => config.models_dir = PathBuf::from(value),
                "--alias" => config.activation.aliases.push(value.clone()),
                "--language" if value == "auto" => config.language = None,
                "--language" => {
                    if !WHISPER_LANGUAGES.contains(&value.as_str()) {
                        return Err(format!("Code langue Whisper inconnu : {value}"));
                    }
                    config.language = Some(value.clone());
                }
                _ => unreachable!(),
            }
            index += 2;
        }
        let audio = audio.ok_or("voice test exige --audio <WAV>")?;
        // Les alias invalides sont rejetés avant tout accès aux modèles ou à l'audio.
        WakeActivation::new(config.activation.clone()).map_err(|error| error.to_string())?;
        Ok(Self {
            config,
            audio,
            next_audio,
        })
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args)?;
    let paths: Vec<_> = std::iter::once(options.audio)
        .chain(options.next_audio)
        .collect();
    let mut audio = Vec::new();
    for path in &paths {
        let samples =
            super::models_cli::read_audio(path, 16_000).map_err(|error| error.to_string())?;
        if !(WHISPER_MIN_SAMPLES..=WHISPER_MAX_SAMPLES).contains(&samples.len()) {
            return Err(format!(
                "{} : Whisper accepte de 0,1 à 30 secondes d'audio",
                path.display()
            ));
        }
        audio.push(samples);
    }
    let mut recognizer = VoiceRecognizer::new(&options.config)?;
    let mut rows = Vec::new();
    for (path, samples) in paths.iter().zip(&audio) {
        let result = recognizer.recognize(samples)?;
        rows.push(json!({
            "audio":path,
            "samples":samples.len(),
            "sample_rate":16000,
            "result":recognition_json(&result),
        }));
        if matches!(result.activation, ActivationOutcome::Command { .. }) {
            // Fin immédiate du diagnostic : aucune action n'est exécutée.
            // Le cooldown réel est conservé si un second fichier est fourni.
            recognizer.finish_command();
        }
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok":true,
            "mode":"wav_diagnostic",
            "microphone_tested":false,
            "actions_executed":false,
            "lexical_hint":hakille_voice::input::VOICE_LEXICAL_HINT,
            "requested_language":options.config.language,
            "aliases":options.config.activation.aliases,
            "segments":rows,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn recognition_json(result: &VoiceRecognition) -> Value {
    let activation = match &result.activation {
        ActivationOutcome::Armed { expires_at_ms } => {
            json!({"outcome":"armed","expires_at_ms":expires_at_ms})
        }
        ActivationOutcome::Command { text, language } => {
            json!({"outcome":"command","text":text,"language":language,"executed":false})
        }
        ActivationOutcome::Ignored { reason } => {
            let reason = match reason {
                ActivationIgnoreReason::Empty => "empty",
                ActivationIgnoreReason::NoWakeWord => "no_wake_word",
                ActivationIgnoreReason::Busy => "busy",
                ActivationIgnoreReason::Cooldown => "cooldown",
                ActivationIgnoreReason::StaleTimestamp => "stale_timestamp",
                ActivationIgnoreReason::TextTooLong => "text_too_long",
            };
            json!({"outcome":"ignored","reason":reason})
        }
    };
    let transcription = &result.transcription;
    json!({
        "text":transcription.text,
        "language":transcription.language,
        "language_probability":transcription.language_probability,
        "no_speech_probability":transcription.no_speech_probability,
        "suppressed_silence":transcription.suppressed_silence,
        "generated_tokens":transcription.generated_tokens,
        "inference_elapsed_ms":transcription.elapsed_ms,
        "real_forward":transcription.no_speech_probability.is_some(),
        "activation":activation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_inference::whisper_engine::WhisperTranscription;
    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(&args.iter().map(|value| (*value).into()).collect::<Vec<_>>())
    }

    #[test]
    fn voice_file_cli_rejects_ignored_or_ambiguous_options() {
        for args in [
            vec!["test"],
            vec!["test", "--audio"],
            vec!["test", "--audio", "first.wav", "--audio", "second.wav"],
            vec!["test", "--audio", "first.wav", "--language", "xx"],
            vec!["test", "--audio", "first.wav", "--alias", "two words"],
            vec![
                "test",
                "--audio",
                "first.wav",
                "--text",
                "fabricated transcript",
            ],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
        let options = parse(&[
            "test",
            "--audio",
            "first.wav",
            "--next-audio",
            "second.wav",
            "--language",
            "auto",
            "--alias",
            "Akille",
            "--models-dir",
            "custom",
        ])
        .unwrap();
        assert!(options.config.language.is_none());
        assert_eq!(options.config.models_dir, PathBuf::from("custom"));
        assert_eq!(options.next_audio, Some(PathBuf::from("second.wav")));
        assert!(
            options
                .config
                .activation
                .aliases
                .iter()
                .any(|alias| alias == "Akille")
        );
    }

    #[test]
    fn voice_file_cli_reports_actual_activation_without_executing() {
        let mut activation = WakeActivation::default();
        let mut result = VoiceRecognition {
            transcription: WhisperTranscription {
                text: "Hakille".into(),
                language: Some("fr".into()),
                language_probability: None,
                no_speech_probability: Some(0.01),
                suppressed_silence: false,
                generated_tokens: 3,
                elapsed_ms: 10,
            },
            activation: activation.on_transcript("Hakille", "fr", 0),
        };
        assert_eq!(recognition_json(&result)["activation"]["outcome"], "armed");
        result.transcription.text = "Ouvre le calendrier.".into();
        result.activation = activation.on_transcript(&result.transcription.text, "fr", 1);
        let row = recognition_json(&result);
        assert_eq!(row["text"], "Ouvre le calendrier.");
        assert_eq!(row["activation"]["outcome"], "command");
        assert_eq!(row["activation"]["executed"], false);
        activation.finish_command(2);
        result.activation = activation.on_transcript("Hakille ouvre", "fr", 3);
        assert_eq!(
            recognition_json(&result)["activation"]["reason"],
            "cooldown"
        );
    }
}
