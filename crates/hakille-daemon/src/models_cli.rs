//! Inférence locale explicite, indépendante du démarrage du démon.
//! Le contrôle d'intégrité précède le chargement ; les résultats précisent le forward réel.

use hakille_inference::{ModelCatalog, ModelProvisioner, ModelTier};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type CliResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
struct Options {
    command: String,
    values: HashMap<String, String>,
    all: bool,
    json: bool,
    directory: PathBuf,
}

impl Options {
    fn parse(args: &[String], directory: PathBuf) -> CliResult<Self> {
        let command = args.first().map(String::as_str).unwrap_or("list");
        if matches!(command, "--help" | "-h" | "help") {
            return Ok(Self {
                command: "help".into(),
                values: HashMap::new(),
                all: false,
                json: false,
                directory,
            });
        }
        if !matches!(command, "list" | "download" | "verify" | "test" | "infer") {
            return Err(format!("Commande modèles inconnue : {command}").into());
        }
        let mut options = Self {
            command: command.into(),
            values: HashMap::new(),
            all: false,
            json: false,
            directory,
        };
        let mut i = 1;
        while i < args.len() {
            let name = args[i].as_str();
            match name {
                "--json" if !options.json => options.json = true,
                "--all" if !options.all => options.all = true,
                "--models-dir" | "--model" | "--tier" | "--text" | "--audio" | "--image"
                | "--output" | "--endpoint" | "--max-tokens" | "--timeout-seconds"
                | "--language" => {
                    let value = args
                        .get(i + 1)
                        .filter(|v| !v.starts_with("--") && !v.is_empty())
                        .ok_or_else(|| format!("Valeur requise pour {name}"))?;
                    if options.values.insert(name.into(), value.clone()).is_some() {
                        return Err(format!("Option répétée : {name}").into());
                    }
                    if name == "--models-dir" {
                        options.directory = PathBuf::from(value);
                    }
                    i += 1;
                }
                _ => return Err(format!("Option inconnue ou répétée : {name}").into()),
            }
            i += 1;
        }
        let selectors = usize::from(options.all)
            + usize::from(options.values.contains_key("--model"))
            + usize::from(options.values.contains_key("--tier"));
        if selectors > 1 {
            return Err("Choisir un seul sélecteur : --model, --tier ou --all".into());
        }
        for key in options.values.keys() {
            let allowed = matches!(key.as_str(), "--models-dir" | "--model" | "--tier")
                || (matches!(command, "test" | "infer")
                    && matches!(
                        key.as_str(),
                        "--text"
                            | "--audio"
                            | "--image"
                            | "--output"
                            | "--endpoint"
                            | "--max-tokens"
                            | "--timeout-seconds"
                            | "--language"
                    ));
            if !allowed {
                return Err(format!("{key} n'est pas applicable à {command}").into());
            }
        }
        if command == "infer" && !options.values.contains_key("--model") {
            return Err("models infer exige --model <identifiant>".into());
        }
        for (key, max) in [("--max-tokens", 4096), ("--timeout-seconds", 3600)] {
            if let Some(value) = options.values.get(key) {
                let n: usize = value
                    .parse()
                    .map_err(|_| format!("{key} doit être un entier positif"))?;
                if n == 0 || n > max {
                    return Err(format!("{key} doit être compris entre 1 et {max}").into());
                }
            }
        }
        if options
            .values
            .get("--text")
            .is_some_and(|s| s.len() > 32_768)
        {
            return Err("Texte limité à 32768 octets".into());
        }
        if let Some(language) = options.get("--language") {
            if options.get("--model") != Some("whisper-tiny-multilingual") {
                return Err("--language exige --model whisper-tiny-multilingual".into());
            }
            if language != "auto"
                && !hakille_inference::whisper_engine::WHISPER_LANGUAGES.contains(&language)
            {
                return Err(format!("Code langue Whisper inconnu : {language}; utiliser auto ou un code tel que fr, en, ar").into());
            }
        }
        options.validate_inference_options(&options.model_ids()?)?;
        Ok(options)
    }

    fn validate_inference_options(&self, ids: &[String]) -> CliResult<()> {
        if !matches!(self.command.as_str(), "infer" | "test") {
            return Ok(());
        }
        for key in self.values.keys() {
            let applicable = ids.iter().any(|id| match key.as_str() {
                "--text" => matches!(
                    id.as_str(),
                    "qwen2.5-0.5b-instruct-q4km" | "all-minilm-l6-v2" | "ui-tars-2b-sft-q4km"
                ),
                "--audio" => matches!(
                    id.as_str(),
                    "silero-vad-v5"
                        | "campplus-speaker-id"
                        | "sensevoice-small-int8"
                        | "moshi-mimi-s2s"
                        | "whisper-tiny-multilingual"
                ),
                "--output" => id == "moshi-mimi-s2s",
                "--image" | "--endpoint" => id == "ui-tars-2b-sft-q4km",
                "--max-tokens" => id == "qwen2.5-0.5b-instruct-q4km",
                "--timeout-seconds" => matches!(
                    id.as_str(),
                    "qwen2.5-0.5b-instruct-q4km" | "whisper-tiny-multilingual"
                ),
                "--language" => id == "whisper-tiny-multilingual",
                _ => true,
            });
            if !applicable {
                return Err(format!("Option {key} non applicable aux modèles sélectionnés").into());
            }
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    fn model_ids(&self) -> CliResult<Vec<String>> {
        let catalog = ModelCatalog::official_catalog();
        if let Some(id) = self.get("--model") {
            if !catalog.iter().any(|m| m.id == id) {
                return Err(
                    format!("Identifiant modèle inconnu : {id}; consulter models list").into(),
                );
            }
            return Ok(vec![id.into()]);
        }
        if let Some(tier) = self.get("--tier") {
            let tier = match tier {
                "guest" => ModelTier::Guest,
                "native" => ModelTier::Native,
                "sovereign" => ModelTier::Sovereign,
                _ => return Err(format!("Tier inconnu : {tier}").into()),
            };
            return Ok(ModelCatalog::models_for_tier(tier)
                .into_iter()
                .map(|m| m.id)
                .collect());
        }
        Ok(catalog.into_iter().map(|m| m.id).collect())
    }
}

pub async fn run(args: &[String], directory: PathBuf) -> Result<(), Box<dyn Error>> {
    run_inner(args, directory)
        .await
        .map_err(|e| e as Box<dyn Error>)
}

async fn run_inner(args: &[String], directory: PathBuf) -> CliResult<()> {
    let options = Options::parse(args, directory)?;
    if options.command == "help" {
        println!(
            "Modèles locaux : list | download | verify | test | infer\n\
Sélection : --model <id> | --tier guest|native|sovereign | --all\n\
Stockage : --models-dir <dossier> ; sortie : --json\n\
Inférence : --text <texte>, --audio <WAV mono/stéréo>, --image <PNG/JPEG>\n\
Mimi : --audio <WAV 24 kHz> --output <nouveau WAV>\n\
UI-TARS : --endpoint http://127.0.0.1:8080 --image <fichier> --text <instruction>\n\
Qwen : --max-tokens <1..4096> ; Qwen/Whisper : --timeout-seconds <1..3600>\n\
Whisper : --model whisper-tiny-multilingual --audio <WAV 0,1..30 s> --language auto|fr|en|…\n\
test vérifie le fonctionnement, pas la qualité métier ; Whisper sans --audio teste le rejet du silence sans forward.\n\
verify/test échouent si un modèle sélectionné est absent, corrompu ou inexécutable."
        );
        return Ok(());
    }
    let ids = options.model_ids()?;
    let provisioner = ModelProvisioner::new(options.directory.clone());
    let mut rows = Vec::new();
    let mut failed = false;
    for id in ids {
        let start = Instant::now();
        let outcome: CliResult<Value> = async {
            let entry = ModelCatalog::get_by_id(&id).ok_or("Entrée catalogue absente")?;
            match options.command.as_str() {
                "list" => {
                    let status = provisioner.status(&entry);
                    Ok(json!({"name":entry.name,"tier":entry.tier,"installed":status.is_installed,"disk_bytes":status.total_disk_bytes,"missing":status.missing_artifacts,"invalid":status.invalid_artifacts,"inference_tested":false}))
                }
                "download" => {
                    let path = provisioner.provision_by_id(&id, None).await?;
                    Ok(json!({"path":path,"integrity":"sha256_verified"}))
                }
                "verify" => {
                    let reports = provisioner.verify_model_by_id(&id)?;
                    require_integrity(&reports)?;
                    Ok(serde_json::to_value(reports)?)
                }
                "infer" | "test" => {
                    require_integrity(&provisioner.verify_model_by_id(&id)?)?;
                    if id == "ui-tars-2b-sft-q4km" {
                        let endpoint = options.get("--endpoint").ok_or("UI-TARS nécessite --endpoint vers llama-server local avec GGUF et projecteur vérifiés")?;
                        let image_path = options.get("--image").ok_or("UI-TARS nécessite --image PNG/JPEG")?;
                        let metadata = std::fs::metadata(image_path)?;
                        if metadata.len() > 10 * 1024 * 1024 { return Err("Image limitée à 10 Mio".into()); }
                        let image = std::fs::read(image_path)?;
                        let client = hakille_perception::vision::ui_tars_local::UiTarsLocalClient::new(endpoint)?;
                        let task = options.get("--text").unwrap_or("Identify the visible button and propose a click.");
                        Ok(serde_json::to_value(client.ground(&image, task).await?)?)
                    } else {
                        infer(&id, &options)
                    }
                }
                _ => unreachable!(),
            }
        }.await;
        let row = match outcome {
            Ok(value) => {
                json!({"model":id,"operation":options.command,"ok":true,"elapsed_ms":start.elapsed().as_millis(),"result":value})
            }
            Err(error) => {
                failed = true;
                json!({"model":id,"operation":options.command,"ok":false,"elapsed_ms":start.elapsed().as_millis(),"error":error.to_string()})
            }
        };
        rows.push(row);
    }
    let report = json!({"models_directory":options.directory,"ok":!failed,"models":rows});
    if options.json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    if failed {
        return Err(
            "Au moins un modèle sélectionné a échoué ; consulter le résultat par modèle".into(),
        );
    }
    Ok(())
}

fn require_integrity(reports: &[hakille_inference::IntegrityReport]) -> CliResult<()> {
    let invalid: Vec<_> = reports
        .iter()
        .filter(|r| !r.verified)
        .map(|r| format!("{} : {}", r.filename, r.note))
        .collect();
    if reports.is_empty() || !invalid.is_empty() {
        return Err(format!("Intégrité non établie : {}", invalid.join(" ; ")).into());
    }
    Ok(())
}

fn infer(id: &str, options: &Options) -> CliResult<Value> {
    use hakille_inference::{
        CampplusOnnx, MimiEngine, MiniLmOnnx, OrtVadSession, QwenEngine, SenseVoiceOnnx,
    };
    let path = |filename: &str| options.directory.join(filename);
    let smoke = options.command == "test";
    match id {
        "qwen2.5-0.5b-instruct-q4km" => {
            let text = options
                .get("--text")
                .or(smoke.then_some("What is 2 + 2? Answer with the number only."))
                .ok_or("Qwen nécessite --text")?;
            let max_tokens = options
                .get("--max-tokens")
                .unwrap_or(if smoke { "16" } else { "128" })
                .parse()?;
            let timeout = options.get("--timeout-seconds").unwrap_or("120").parse()?;
            let mut engine = QwenEngine::load_with_paths(
                path("qwen2.5-0.5b-instruct-q4_k_m.gguf"),
                path("tokenizer.json"),
            )?;
            let result = engine.generate_with_deadline(
                text,
                max_tokens,
                0.0,
                1.0,
                Some(Instant::now() + Duration::from_secs(timeout)),
            )?;
            if result.text.trim().is_empty() {
                return Err("Qwen n'a produit aucun texte".into());
            }
            Ok(
                json!({"text":result.text,"tokens_generated":result.tokens_generated,"prompt_tokens":result.prompt_tokens,"backend":"candle_cpu","real_forward":true}),
            )
        }
        "all-minilm-l6-v2" => {
            let text = options
                .get("--text")
                .or(smoke.then_some("Local semantic embedding validation."))
                .ok_or("MiniLM nécessite --text")?;
            let engine =
                MiniLmOnnx::open(path("all-minilm-l6-v2.onnx"), path("minilm_tokenizer.json"))?;
            let embedding = engine.embed(text)?;
            Ok(json!({"dimensions":embedding.len(),"embedding":embedding,"real_forward":true}))
        }
        "silero-vad-v5" | "campplus-speaker-id" | "sensevoice-small-int8" => {
            let samples = if let Some(audio) = options.get("--audio") {
                read_audio(Path::new(audio), 16_000)?
            } else if smoke {
                vec![0.0; 32_000]
            } else {
                return Err("Ce modèle nécessite --audio <WAV>".into());
            };
            match id {
                "silero-vad-v5" => {
                    let mut engine = OrtVadSession::open(path("silero_vad.onnx"))?;
                    let mut scores = engine.process_stream(&samples)?;
                    if let Some(last) = engine.finish_stream()? {
                        scores.push(last);
                    }
                    Ok(
                        json!({"sample_rate":16000,"samples":samples.len(),"window_samples":512,"scores":scores,"real_forward":true}),
                    )
                }
                "campplus-speaker-id" => {
                    let engine = CampplusOnnx::open(path("campplus_zh_en_16k_advanced.onnx"))?;
                    let embedding = engine.embed(&samples)?;
                    Ok(
                        json!({"dimensions":embedding.len(),"embedding":embedding,"real_forward":true}),
                    )
                }
                _ => {
                    let mut engine = SenseVoiceOnnx::open(
                        path("sensevoice_small.int8.onnx"),
                        path("tokens.txt"),
                    )?;
                    let transcript = engine.transcribe(&samples)?;
                    Ok(
                        json!({"text":transcript.text,"language":transcript.language,"confidence":transcript.confidence,"emotion":transcript.emotion_event.emotion,"events":transcript.emotion_event.events,"real_forward":true}),
                    )
                }
            }
        }
        "whisper-tiny-multilingual" => {
            use hakille_inference::whisper_engine::{
                WHISPER_MAX_SAMPLES, WHISPER_MIN_SAMPLES, WhisperEngine,
            };
            let synthetic_silence = smoke && options.get("--audio").is_none();
            let samples = if let Some(audio) = options.get("--audio") {
                read_audio(Path::new(audio), 16_000)?
            } else if synthetic_silence {
                vec![0.0; 16_000]
            } else {
                return Err("Whisper nécessite --audio <WAV de 0,1 à 30 secondes>".into());
            };
            if !(WHISPER_MIN_SAMPLES..=WHISPER_MAX_SAMPLES).contains(&samples.len()) {
                return Err("Whisper accepte de 0,1 à 30 secondes d'audio".into());
            }
            let language = options
                .get("--language")
                .filter(|language| *language != "auto");
            let timeout: u64 = options.get("--timeout-seconds").unwrap_or("120").parse()?;
            let mut engine = WhisperEngine::load_from_dir(&options.directory)?;
            let result = engine.transcribe(
                &samples,
                language,
                Some(Instant::now() + Duration::from_secs(timeout)),
            )?;
            Ok(whisper_result_json(
                &result,
                samples.len(),
                synthetic_silence,
            ))
        }
        "moshi-mimi-s2s" => {
            let samples = if let Some(audio) = options.get("--audio") {
                read_audio(Path::new(audio), 24_000)?
            } else if smoke {
                (0..3840)
                    .map(|n| 0.1 * (n as f32 * 440.0 * std::f32::consts::TAU / 24000.0).sin())
                    .collect()
            } else {
                return Err("Mimi nécessite --audio <WAV 24 kHz> et --output <WAV>".into());
            };
            if !smoke && options.get("--output").is_none() {
                return Err("Mimi nécessite --output pour le PCM reconstruit".into());
            }
            let mut encoder = MimiEngine::load(path("mimi_candle.safetensors"))?;
            let mut decoder = encoder.fresh_stream();
            let mut decoded = Vec::new();
            let mut tokens = Vec::new();
            for chunk in samples.chunks(1920) {
                let mut frame = [0.0; 1920];
                frame[..chunk.len()].copy_from_slice(chunk);
                let codes = encoder.encode_frame(&frame)?;
                decoded.extend(decoder.decode_frame(&codes)?);
                tokens.push(codes);
            }
            let padded_samples = decoded.len().saturating_sub(samples.len());
            decoded.truncate(samples.len());
            if let Some(output) = options.get("--output") {
                write_audio(Path::new(output), &decoded, 24000)?;
            }
            Ok(
                json!({"sample_rate":24000,"samples":decoded.len(),"padding_removed":padded_samples,"frames":tokens.len(),"codes":tokens,"output":options.get("--output"),"real_forward":true,"model_role":"neural_audio_codec_not_moshi_dialogue"}),
            )
        }
        _ => Err(format!("Moteur local inconnu : {id}").into()),
    }
}

/// Un score neuronal absent signifie que le silence a été rejeté avant forward.
fn whisper_result_json(
    result: &hakille_inference::whisper_engine::WhisperTranscription,
    samples: usize,
    synthetic_silence: bool,
) -> Value {
    let real_forward = result.no_speech_probability.is_some();
    json!({
        "text":result.text,
        "language":result.language,
        "language_probability":result.language_probability,
        "no_speech_probability":result.no_speech_probability,
        "suppressed_silence":result.suppressed_silence,
        "generated_tokens":result.generated_tokens,
        "inference_elapsed_ms":result.elapsed_ms,
        "sample_rate":16000,
        "samples":samples,
        "backend":"candle_cpu",
        "real_forward":real_forward,
        "synthetic_silence":synthetic_silence,
        "acoustic_quality_evaluated":false,
        "validation":if real_forward {"transcription"} else {"silence_rejected_before_forward"},
    })
}

/// WAV borné et validé avant toute allocation dépendant du contenu audio.
pub(super) fn read_audio(path: &Path, target_rate: u32) -> CliResult<Vec<f32>> {
    if std::fs::metadata(path)?.len() > 64 * 1024 * 1024 {
        return Err("WAV limité à 64 Mio".into());
    }
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels)
        || !(8000..=192000).contains(&spec.sample_rate)
        || reader.duration() == 0
        || reader.duration() > spec.sample_rate * 60
    {
        return Err(
            "WAV requis : mono/stéréo, 8..192 kHz, durée non nulle limitée à 60 secondes".into(),
        );
    }
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>()?,
        (hound::SampleFormat::Int, bits @ (8 | 16 | 24 | 32)) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 2f32.powi(i32::from(bits) - 1)))
            .collect::<Result<_, _>>()?,
        _ => return Err("Format WAV non pris en charge : PCM 8/16/24/32 ou float32 requis".into()),
    };
    if samples.iter().any(|v| !v.is_finite() || v.abs() > 1.0)
        || !samples.len().is_multiple_of(usize::from(spec.channels))
    {
        return Err("Échantillons WAV invalides ou trame incomplète".into());
    }
    if target_rate == 16_000 {
        return Ok(hakille_inference::try_to_mono_16k(
            &samples,
            spec.channels,
            spec.sample_rate,
        )?);
    }
    // Le codec Mimi accepte un flux natif 24 kHz ; pas de conversion silencieuse.
    if spec.sample_rate != target_rate {
        return Err(format!("Mimi nécessite un WAV à {target_rate} Hz").into());
    }
    Ok(samples
        .chunks_exact(spec.channels as usize)
        .map(|frame| frame.iter().sum::<f32>() / f32::from(spec.channels))
        .collect())
}

fn write_audio(path: &Path, samples: &[f32], sample_rate: u32) -> CliResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::new(temporary.as_file_mut(), spec)?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> CliResult<Options> {
        Options::parse(
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            PathBuf::from("models"),
        )
    }
    #[test]
    fn model_cli_rejects_ambiguous_or_ignored_options() {
        for args in [
            vec!["download", "--tier", "typo"],
            vec!["test", "--model", "missing"],
            vec!["infer"],
            vec!["test", "--all", "--model", "silero-vad-v5"],
            vec!["download", "--audio", "voice.wav"],
            vec![
                "infer",
                "--model",
                "silero-vad-v5",
                "--timeout-seconds",
                "0",
            ],
            vec!["list", "--models-dir"],
        ] {
            assert!(
                parse(&args).and_then(|o| o.model_ids()).is_err(),
                "{args:?}"
            );
        }
    }
    #[test]
    fn model_cli_uses_explicit_directory_and_exact_model() {
        let o = parse(&[
            "test",
            "--models-dir",
            "custom",
            "--model",
            "silero-vad-v5",
            "--json",
        ])
        .unwrap();
        assert_eq!(o.directory, PathBuf::from("custom"));
        assert_eq!(o.model_ids().unwrap(), ["silero-vad-v5"]);
    }
    #[test]
    fn model_cli_whisper_language_and_model_flags_are_strict() {
        for command in ["test", "infer"] {
            for language in ["auto", "fr", "en", "ar"] {
                let options = parse(&[
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--audio",
                    "voice.wav",
                    "--language",
                    language,
                    "--timeout-seconds",
                    "30",
                ])
                .unwrap();
                assert_eq!(options.get("--language"), Some(language));
            }
            for arguments in [
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--language",
                    "xx",
                ],
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--language",
                    "FR",
                ],
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--language",
                ],
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--language",
                    "fr",
                    "--language",
                    "en",
                ],
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--text",
                    "ignored",
                ],
                vec![
                    command,
                    "--model",
                    "whisper-tiny-multilingual",
                    "--max-tokens",
                    "12",
                ],
                vec![
                    command,
                    "--model",
                    "sensevoice-small-int8",
                    "--language",
                    "fr",
                ],
            ] {
                assert!(parse(&arguments).is_err(), "{arguments:?}");
            }
        }
        assert!(
            parse(&[
                "list",
                "--model",
                "whisper-tiny-multilingual",
                "--language",
                "fr"
            ])
            .is_err()
        );
        assert!(parse(&["test", "--all", "--language", "fr"]).is_err());
        let options = parse(&["test", "--model", "whisper-tiny-multilingual"]).unwrap();
        assert!(options.get("--language").is_none());
    }

    #[test]
    fn model_cli_whisper_rejects_missing_or_out_of_bounds_audio_before_loading() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = parse(&["infer", "--model", "whisper-tiny-multilingual"]).unwrap();
        options.directory = dir.path().join("absent-models");
        assert!(
            infer("whisper-tiny-multilingual", &options)
                .unwrap_err()
                .to_string()
                .contains("--audio")
        );
        for sample_count in [800, 480_001] {
            let path = dir.path().join(format!("audio-{sample_count}.wav"));
            write_audio(&path, &vec![0.0; sample_count], 16_000).unwrap();
            options
                .values
                .insert("--audio".into(), path.to_str().unwrap().into());
            let error = infer("whisper-tiny-multilingual", &options)
                .unwrap_err()
                .to_string();
            assert!(error.contains("0,1 à 30 secondes"), "{error}");
        }
    }

    #[test]
    fn model_cli_whisper_silence_never_claims_neural_forward_or_probability() {
        use hakille_inference::whisper_engine::WhisperTranscription;
        let mut result = WhisperTranscription {
            text: String::new(),
            language: None,
            language_probability: None,
            no_speech_probability: None,
            suppressed_silence: true,
            generated_tokens: 0,
            elapsed_ms: 0,
        };
        let silence = whisper_result_json(&result, 16_000, true);
        assert_eq!(silence["real_forward"], false);
        assert_eq!(silence["synthetic_silence"], true);
        assert_eq!(silence["acoustic_quality_evaluated"], false);
        assert!(silence["language_probability"].is_null());
        assert!(silence["no_speech_probability"].is_null());
        // Un silence rejeté par le réseau, en revanche, a réellement exécuté le forward.
        result.no_speech_probability = Some(0.99);
        let neural_silence = whisper_result_json(&result, 16_000, false);
        assert_eq!(neural_silence["real_forward"], true);
        assert_eq!(neural_silence["suppressed_silence"], true);
    }

    #[test]
    fn model_cli_requires_verified_nonempty_integrity_reports() {
        assert!(require_integrity(&[]).is_err());
        let mut report = hakille_inference::IntegrityReport {
            model_id: "fixture".into(),
            filename: "weights".into(),
            verified: false,
            note: "checksum invalide".into(),
        };
        assert!(require_integrity(&[report.clone()]).is_err());
        report.verified = true;
        assert!(require_integrity(&[report]).is_ok());
    }
    #[test]
    fn wav_reader_rejects_nonfinite_and_preserves_existing_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.wav");
        write_audio(&path, &[0.25, -0.5], 24000).unwrap();
        assert_eq!(read_audio(&path, 24000).unwrap(), [0.25, -0.5]);
        assert!(write_audio(&path, &[0.0], 24000).is_err());
        let invalid = dir.path().join("invalid.wav");
        write_audio(&invalid, &[f32::NAN], 24000).unwrap();
        assert!(read_audio(&invalid, 24000).is_err());
    }
}
