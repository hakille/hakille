//! ASR Whisper tiny/base multilingue local, Candle CPU, sans traduction implicite.
//!
//! Le décodage greedy suit l'exemple Whisper de Candle (MIT/Apache-2.0).
//! Le PCM doit être mono, normalisé, à 16 kHz, entre 100 ms et 30 secondes.
//! Les 99 langues du tokenizer multilingue sont détectables, sans garantie de
//! qualité identique. Le silence quasi nul est éliminé avant tout forward ; le
//! score neuronal « no speech » filtre ensuite les sorties peu probables.
//! Sur un énoncé très court, la langue automatique peut être erronée : une
//! préférence explicite permet de la fixer. Ce moteur n'est pas un détecteur
//! spécialisé de mot d'activation et ne garantit pas l'absence d'hallucinations.
//! L'échéance est coopérative : un forward Candle en cours reste non préemptible.

use candle_core::{Device, IndexOp, Tensor};
use candle_transformers::models::whisper::{self as whisper, Config, model::Whisper};
use hakille_core::{HakilleError, Result};
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use tokenizers::Tokenizer;

pub const WHISPER_MODEL_FILE: &str = "whisper-tiny.safetensors";
pub const WHISPER_CONFIG_FILE: &str = "whisper-tiny-config.json";
pub const WHISPER_TOKENIZER_FILE: &str = "whisper-tiny-tokenizer.json";
pub const WHISPER_SAMPLE_RATE: usize = 16_000;
pub const WHISPER_MAX_SAMPLES: usize = 30 * WHISPER_SAMPLE_RATE;
pub const WHISPER_MIN_SAMPLES: usize = WHISPER_SAMPLE_RATE / 10;
const MAX_GENERATED_TOKENS: usize = 224;
const SILENCE_RMS: f64 = 0.0001;

/// Codes Whisper officiels, dans l'ordre des tokens 50259..50357.
pub const WHISPER_LANGUAGES: [&str; 99] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su",
];

/// Résultat mesuré. Les probabilités absentes n'ont pas été calculées par le modèle.
#[derive(Debug, Clone)]
pub struct WhisperTranscription {
    pub text: String,
    pub language: Option<String>,
    pub language_probability: Option<f32>,
    pub no_speech_probability: Option<f32>,
    /// Silence énergétique ou « no speech » neuronal associé à une faible vraisemblance.
    pub suppressed_silence: bool,
    pub generated_tokens: usize,
    pub elapsed_ms: u128,
}

pub struct WhisperEngine {
    model: Whisper,
    tokenizer: Tokenizer,
    filters: Vec<f32>,
    language_tokens: Vec<u32>,
    suppression: Vec<bool>,
    sot: u32,
    eot: u32,
    transcribe: u32,
    no_timestamps: u32,
    no_speech: u32,
    blank: u32,
    previous: u32,
}

fn error(message: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("Whisper local : {message}"))
}

fn check_deadline(deadline: Option<Instant>) -> Result<()> {
    if deadline.is_some_and(|end| Instant::now() >= end) {
        return Err(error("échéance de transcription dépassée"));
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)
        .map_err(|e| error(format!("lecture {} : {e}", path.display())))?;
    if !file.metadata().map_err(error)?.is_file() {
        return Err(error("l'artefact doit être un fichier ordinaire"));
    }
    if file.metadata().map_err(error)?.len() > max_bytes {
        return Err(error("artefact trop volumineux"));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 > max_bytes {
        return Err(error("artefact trop volumineux"));
    }
    Ok(bytes)
}

fn validate_config(config: &Config) -> Result<()> {
    // Limiter les formes évite les divisions nulles et les allocations arbitraires
    // dans le constructeur amont. Les modèles anglais seuls sont refusés.
    let tiny = (384, 6, 4);
    let base = (512, 8, 6);
    let shape = (
        config.d_model,
        config.encoder_attention_heads,
        config.encoder_layers,
    );
    if ![tiny, base].contains(&shape)
        || config.decoder_attention_heads != config.encoder_attention_heads
        || config.decoder_layers != config.encoder_layers
        || config.num_mel_bins != 80
        || config.max_source_positions != 1500
        || config.max_target_positions != 448
        || config.vocab_size != 51865
        || config
            .suppress_tokens
            .iter()
            .any(|&id| id as usize >= config.vocab_size)
    {
        return Err(error(
            "configuration incompatible : Whisper tiny/base multilingue attendu",
        ));
    }
    Ok(())
}

/// Validation avant allocation et calcul ; renvoie le RMS de la composante AC.
fn validate_pcm(pcm: &[f32]) -> Result<f64> {
    if !(WHISPER_MIN_SAMPLES..=WHISPER_MAX_SAMPLES).contains(&pcm.len()) {
        return Err(error("PCM mono 16 kHz requis, entre 100 ms et 30 secondes"));
    }
    if pcm.iter().any(|v| !v.is_finite() || v.abs() > 1.0) {
        return Err(error("PCM non fini ou hors de [-1, 1]"));
    }
    let mean = pcm.iter().map(|&v| f64::from(v)).sum::<f64>() / pcm.len() as f64;
    let variance = pcm
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / pcm.len() as f64;
    Ok(variance.sqrt())
}

fn language_index(language: Option<&str>) -> Result<Option<usize>> {
    language
        .map(|code| {
            WHISPER_LANGUAGES
                .iter()
                .position(|&candidate| candidate == code)
                .ok_or_else(|| {
                    error(format!(
                        "code de langue Whisper non pris en charge : {code}"
                    ))
                })
        })
        .transpose()
}

/// Banque Slaney de librosa/OpenAI : FFT 400, 80 bandes, 0..8000 Hz, norm="slaney".
/// Les constantes de référence sont testées contre melfilters.bytes de Candle.
fn mel_filters() -> Vec<f32> {
    let mel_max = 15.0 + 8.0_f64.ln() * 27.0 / 6.4_f64.ln();
    let points: Vec<f64> = (0..82)
        .map(|i| {
            let mel = mel_max * i as f64 / 81.0;
            if mel < 15.0 {
                mel * 200.0 / 3.0
            } else {
                1000.0 * ((mel - 15.0) * 6.4_f64.ln() / 27.0).exp()
            }
        })
        .collect();
    let mut filters = vec![0.0; 80 * 201];
    for band in 0..80 {
        let (left, center, right) = (points[band], points[band + 1], points[band + 2]);
        for bin in 0..201 {
            let hz = bin as f64 * 40.0;
            filters[band * 201 + bin] = (((hz - left) / (center - left))
                .min((right - hz) / (right - center))
                .max(0.0)
                * 2.0
                / (right - left)) as f32;
        }
    }
    filters
}

fn finite_argmax(values: &[f32]) -> Result<usize> {
    if values.iter().any(|v| v.is_nan() || *v == f32::INFINITY) {
        return Err(error("logits non finis"));
    }
    values
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i)
        .ok_or_else(|| error("aucun token décodable"))
}

fn probability(logits: &[f32], index: usize) -> Result<f32> {
    let max = logits[finite_argmax(logits)?];
    let normalizer = logits
        .iter()
        .map(|&v| f64::from(v - max).exp())
        .sum::<f64>();
    let value = logits
        .get(index)
        .ok_or_else(|| error("token hors vocabulaire"))?;
    Ok((f64::from(*value - max).exp() / normalizer) as f32)
}

impl WhisperEngine {
    /// Aucun téléchargement implicite : les trois artefacts doivent être provisionnés.
    pub fn load_from_dir(models_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = models_dir.as_ref();
        Self::load_with_paths(
            dir.join(WHISPER_MODEL_FILE),
            dir.join(WHISPER_CONFIG_FILE),
            dir.join(WHISPER_TOKENIZER_FILE),
        )
    }

    pub fn load_with_paths(
        weights: impl AsRef<Path>,
        config: impl AsRef<Path>,
        tokenizer: impl AsRef<Path>,
    ) -> Result<Self> {
        let value: serde_json::Value =
            serde_json::from_slice(&read_bounded(config.as_ref(), 1024 * 1024)?).map_err(error)?;
        if value.get("model_type").and_then(|v| v.as_str()) != Some("whisper") {
            return Err(error("model_type doit être whisper"));
        }
        let config: Config = serde_json::from_value(value).map_err(error)?;
        validate_config(&config)?;
        let tokenizer = Tokenizer::from_bytes(read_bounded(tokenizer.as_ref(), 16 * 1024 * 1024)?)
            .map_err(error)?;
        if tokenizer.get_truncation().is_some()
            || tokenizer.get_padding().is_some()
            || tokenizer
                .get_vocab(true)
                .values()
                .any(|&id| id as usize >= config.vocab_size)
        {
            return Err(error("tokenizer incompatible ou tronqué implicitement"));
        }
        let token = |name: &str, expected: u32| -> Result<u32> {
            let actual = tokenizer
                .token_to_id(name)
                .ok_or_else(|| error(format!("token requis absent : {name}")))?;
            if actual != expected {
                return Err(error(format!("identifiant incorrect pour {name}")));
            }
            Ok(actual)
        };
        let sot = token(whisper::SOT_TOKEN, 50258)?;
        let eot = token(whisper::EOT_TOKEN, 50257)?;
        let transcribe = token(whisper::TRANSCRIBE_TOKEN, 50359)?;
        let no_timestamps = token(whisper::NO_TIMESTAMPS_TOKEN, 50363)?;
        let no_speech = whisper::NO_SPEECH_TOKENS
            .iter()
            .find_map(|name| token(name, 50362).ok())
            .ok_or_else(|| error("token no-speech multilingue absent/incompatible"))?;
        let previous = token("<|startofprev|>", 50361)?;
        let language_tokens = WHISPER_LANGUAGES
            .iter()
            .enumerate()
            .map(|(i, code)| token(&format!("<|{code}|>"), 50259 + i as u32))
            .collect::<Result<Vec<_>>>()?;
        let blank = token("Ġ", 220)?;
        let mut suppression: Vec<bool> =
            (0..config.vocab_size).map(|id| id > eot as usize).collect();
        for &id in &config.suppress_tokens {
            suppression[id as usize] = true;
        }
        if suppression[eot as usize] {
            return Err(error("la configuration interdit le token de fin"));
        }
        let bytes = read_bounded(weights.as_ref(), 384 * 1024 * 1024)?;
        let vb =
            candle_nn::VarBuilder::from_buffered_safetensors(bytes, whisper::DTYPE, &Device::Cpu)
                .map_err(error)?;
        let model = Whisper::load(&vb, config).map_err(error)?;
        Ok(Self {
            model,
            tokenizer,
            filters: mel_filters(),
            language_tokens,
            suppression,
            sot,
            eot,
            transcribe,
            no_timestamps,
            no_speech,
            blank,
            previous,
        })
    }

    /// `None` détecte la langue ; `Some("fr")` force le français sans traduction.
    /// Le verrou de l'appelant doit sérialiser les accès au moteur mutable.
    pub fn transcribe(
        &mut self,
        pcm_16khz: &[f32],
        language: Option<&str>,
        deadline: Option<Instant>,
    ) -> Result<WhisperTranscription> {
        self.transcribe_with_prompt(pcm_16khz, language, None, deadline)
    }

    /// Indice lexical natif Whisper, par exemple « Hakille », sans remplacement
    /// de texte après décodage. Au plus 512 octets/64 tokens ordinaires ; aucune
    /// influence sur la passe de détection de langue, exécutée séparément.
    pub fn transcribe_with_prompt(
        &mut self,
        pcm_16khz: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
        deadline: Option<Instant>,
    ) -> Result<WhisperTranscription> {
        let started = Instant::now();
        self.model.reset_kv_cache();
        let result = self.transcribe_inner(pcm_16khz, language, initial_prompt, deadline);
        self.model.reset_kv_cache();
        result.map(|mut result| {
            result.elapsed_ms = started.elapsed().as_millis();
            result
        })
    }

    fn transcribe_inner(
        &mut self,
        pcm: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
        deadline: Option<Instant>,
    ) -> Result<WhisperTranscription> {
        check_deadline(deadline)?;
        let selected_language = language_index(language)?;
        let rms = validate_pcm(pcm)?;
        let mut prefix = Vec::new();
        if let Some(prompt) = initial_prompt {
            if prompt.len() > 512 {
                return Err(error("indice lexical supérieur à 512 octets"));
            }
            if !prompt.trim().is_empty() {
                let encoded = self
                    .tokenizer
                    .encode(format!(" {}", prompt.trim()), false)
                    .map_err(error)?;
                if encoded.len() > 64 || encoded.get_ids().iter().any(|&id| id >= self.eot) {
                    return Err(error(
                        "indice lexical supérieur à 64 tokens ou contenant un token de contrôle",
                    ));
                }
                prefix.push(self.previous);
                prefix.extend_from_slice(encoded.get_ids());
            }
        }
        check_deadline(deadline)?;
        if rms <= SILENCE_RMS {
            return Ok(WhisperTranscription {
                text: String::new(),
                language: None,
                language_probability: None,
                no_speech_probability: None,
                suppressed_silence: true,
                generated_tokens: 0,
                elapsed_ms: 0,
            });
        }
        let mel = whisper::audio::pcm_to_mel(&self.model.config, pcm, &self.filters);
        check_deadline(deadline)?;
        let frames = mel.len() / self.model.config.num_mel_bins;
        let mel = Tensor::from_vec(mel, (1, 80, frames), &Device::Cpu).map_err(error)?;
        let mel = mel
            .narrow(2, 0, frames.min(whisper::N_FRAMES))
            .map_err(error)?;
        let features = self.model.encoder.forward(&mel, true).map_err(error)?;
        check_deadline(deadline)?;
        // Un forward SOT fournit les scores de langue et « no speech » sans
        // réencoder le même son une seconde fois.
        let sot = Tensor::new(&[[self.sot]], &Device::Cpu).map_err(error)?;
        let ys = self
            .model
            .decoder
            .forward(&sot, &features, true)
            .map_err(error)?;
        let logits = self
            .model
            .decoder
            .final_linear(&ys)
            .map_err(error)?
            .i((0, 0))
            .map_err(error)?
            .to_vec1::<f32>()
            .map_err(error)?;
        let no_speech_probability = probability(&logits, self.no_speech as usize)?;
        let (language_index, language_probability) = match selected_language {
            Some(index) => (index, None),
            None => {
                let language_logits: Vec<f32> = self
                    .language_tokens
                    .iter()
                    .map(|&id| logits[id as usize])
                    .collect();
                let index = finite_argmax(&language_logits)?;
                (index, Some(probability(&language_logits, index)?))
            }
        };
        check_deadline(deadline)?;
        let mut tokens = prefix;
        tokens.extend_from_slice(&[
            self.sot,
            self.language_tokens[language_index],
            self.transcribe,
            self.no_timestamps,
        ]);
        let prompt_len = tokens.len();
        let mut sum_logprob = 0.0_f64;
        for step in 0..MAX_GENERATED_TOKENS {
            check_deadline(deadline)?;
            // Candle recalcule la séquence texte, mais conserve les clés/valeurs
            // de l'attention croisée audio. Le premier pas purge la détection.
            let input = Tensor::new(tokens.as_slice(), &Device::Cpu)
                .map_err(error)?
                .unsqueeze(0)
                .map_err(error)?;
            let ys = self
                .model
                .decoder
                .forward(&input, &features, step == 0)
                .map_err(error)?;
            let last = ys.narrow(1, tokens.len() - 1, 1).map_err(error)?;
            let mut logits = self
                .model
                .decoder
                .final_linear(&last)
                .map_err(error)?
                .i((0, 0))
                .map_err(error)?
                .to_vec1::<f32>()
                .map_err(error)?;
            check_deadline(deadline)?;
            for (value, &suppress) in logits.iter_mut().zip(&self.suppression) {
                if suppress {
                    *value = f32::NEG_INFINITY;
                }
            }
            if step == 0 {
                logits[self.blank as usize] = f32::NEG_INFINITY;
                logits[self.eot as usize] = f32::NEG_INFINITY;
            }
            let next = finite_argmax(&logits)? as u32;
            sum_logprob += f64::from(probability(&logits, next as usize)?).ln();
            if next == self.eot {
                let generated_tokens = tokens.len() - prompt_len;
                let avg_logprob = sum_logprob / (generated_tokens + 1) as f64;
                let suppressed = no_speech_probability > whisper::NO_SPEECH_THRESHOLD as f32
                    && avg_logprob < whisper::LOGPROB_THRESHOLD;
                let text = if suppressed {
                    String::new()
                } else {
                    self.tokenizer
                        .decode(&tokens[prompt_len..], true)
                        .map_err(error)?
                        .trim()
                        .to_string()
                };
                check_deadline(deadline)?;
                return Ok(WhisperTranscription {
                    text,
                    language: (!suppressed).then(|| WHISPER_LANGUAGES[language_index].to_string()),
                    language_probability: if suppressed {
                        None
                    } else {
                        language_probability
                    },
                    no_speech_probability: Some(no_speech_probability),
                    suppressed_silence: suppressed,
                    generated_tokens,
                    elapsed_ms: 0,
                });
            }
            tokens.push(next);
        }
        Err(error(
            "limite de 224 tokens atteinte sans fin ; transcription partielle refusée",
        ))
    }
}

#[cfg(test)]
mod whisper_engine_tests {
    use super::*;

    #[test]
    fn pcm_bounds_silence_and_language_are_checked() {
        assert!(validate_pcm(&[]).is_err());
        assert!(validate_pcm(&vec![0.0; WHISPER_MIN_SAMPLES - 1]).is_err());
        assert!(validate_pcm(&vec![0.0; WHISPER_MAX_SAMPLES + 1]).is_err());
        assert_eq!(validate_pcm(&vec![0.0; WHISPER_MIN_SAMPLES]).unwrap(), 0.0);
        assert_eq!(validate_pcm(&vec![0.25; WHISPER_MIN_SAMPLES]).unwrap(), 0.0);
        for value in [f32::NAN, f32::INFINITY, -1.001, 1.001] {
            let mut pcm = vec![0.0; WHISPER_MIN_SAMPLES];
            pcm[42] = value;
            assert!(validate_pcm(&pcm).is_err());
        }
        assert_eq!(language_index(Some("fr")).unwrap(), Some(6));
        assert_eq!(language_index(Some("en")).unwrap(), Some(0));
        assert_eq!(language_index(None).unwrap(), None);
        assert!(language_index(Some("auto")).is_err());
        assert_eq!(
            WHISPER_LANGUAGES
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            99
        );
    }

    #[test]
    fn mel_matches_official_slaney_filter_values() {
        let filters = mel_filters();
        assert_eq!(filters.len(), 16080);
        assert!(filters.iter().all(|&x| x.is_finite() && x >= 0.0));
        for (index, expected) in [
            (1, 0.024862595_f32),
            (202, 0.001990822),
            (203, 0.022871772),
            (16069, 0.002232055),
        ] {
            assert!((filters[index] - expected).abs() < 2e-9);
        }
        assert_eq!(filters[0], 0.0);
        assert_eq!(filters[2], 0.0);
    }

    #[test]
    fn invalid_logits_and_expired_deadline_fail() {
        assert!(finite_argmax(&[f32::NEG_INFINITY]).is_err());
        assert!(finite_argmax(&[0.0, f32::NAN]).is_err());
        assert!(finite_argmax(&[f32::INFINITY]).is_err());
        assert_eq!(finite_argmax(&[-2.0, 1.0, f32::NEG_INFINITY]).unwrap(), 1);
        assert!((probability(&[1.0, 1.0], 1).unwrap() - 0.5).abs() < 1e-6);
        assert!(check_deadline(Some(Instant::now())).is_err());
        assert!(check_deadline(None).is_ok());
    }

    #[test]
    fn malformed_config_is_rejected_before_constructing_model() {
        let mut config = Config {
            num_mel_bins: 80,
            max_source_positions: 1500,
            d_model: 384,
            encoder_attention_heads: 6,
            encoder_layers: 4,
            vocab_size: 51865,
            max_target_positions: 448,
            decoder_attention_heads: 6,
            decoder_layers: 4,
            suppress_tokens: vec![],
        };
        assert!(validate_config(&config).is_ok());
        config.vocab_size = 51864;
        assert!(validate_config(&config).is_err());
        config.vocab_size = 51865;
        config.decoder_attention_heads = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn oversized_local_artifact_is_refused() {
        let path =
            std::env::temp_dir().join(format!("hakille-whisper-{}.bin", uuid::Uuid::new_v4()));
        std::fs::write(&path, [1, 2, 3, 4]).unwrap();
        assert_eq!(read_bounded(&path, 4).unwrap(), [1, 2, 3, 4]);
        assert!(read_bounded(&path, 3).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(read_bounded(&path, 4).is_err());
    }

    /// Fixtures externes publiques/provisionnées, PCM f32 LE mono 16 kHz.
    /// Exige une vraie transcription et détection de langue FR puis EN.
    #[test]
    #[ignore = "nécessite Whisper tiny et HAKILLE_WHISPER_TEST_FR_PCM / HAKILLE_WHISPER_TEST_EN_PCM"]
    fn real_multilingual_fixtures_and_silence() {
        let dir = std::env::var_os("HAKILLE_MODELS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        let mut engine = WhisperEngine::load_from_dir(dir).expect("modèle Whisper requis");
        let silence = engine.transcribe(&vec![0.0; 16_000], None, None).unwrap();
        assert!(silence.text.is_empty() && silence.suppressed_silence);
        assert!(silence.language.is_none() && silence.no_speech_probability.is_none());
        assert!(
            engine
                .transcribe_with_prompt(&vec![0.0; 16_000], None, Some("<|fr|>"), None)
                .is_err()
        );
        assert!(
            engine
                .transcribe_with_prompt(&vec![0.0; 16_000], None, Some(&"a".repeat(513)), None)
                .is_err()
        );
        let lexical_prompt = std::env::var("HAKILLE_WHISPER_TEST_PROMPT").ok();
        let forbid_wake_name =
            std::env::var("HAKILLE_WHISPER_FORBID_WAKE_NAME").as_deref() == Ok("1");
        for (language, variable, expected_var) in [
            (
                "fr",
                "HAKILLE_WHISPER_TEST_FR_PCM",
                "HAKILLE_WHISPER_TEST_FR_EXPECTED",
            ),
            (
                "en",
                "HAKILLE_WHISPER_TEST_EN_PCM",
                "HAKILLE_WHISPER_TEST_EN_EXPECTED",
            ),
        ] {
            let path = std::env::var(variable).expect("fixture PCM requise");
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(bytes.len() % 4, 0);
            let pcm: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            assert!(engine.transcribe(&pcm, None, Some(Instant::now())).is_err());
            let result = engine
                .transcribe_with_prompt(
                    &pcm,
                    None,
                    lexical_prompt.as_deref(),
                    Some(Instant::now() + std::time::Duration::from_secs(180)),
                )
                .unwrap();
            eprintln!("Whisper {language} : {result:?}");
            assert_eq!(result.language.as_deref(), Some(language));
            assert!(!result.text.is_empty() && !result.suppressed_silence);
            let expected = std::env::var(expected_var).expect("fragment attendu requis");
            assert!(!expected.is_empty());
            assert!(
                result
                    .text
                    .to_lowercase()
                    .contains(&expected.to_lowercase()),
                "transcription sans le fragment attendu {expected:?}"
            );
            if forbid_wake_name {
                assert!(
                    !result
                        .text
                        .split(|c: char| !c.is_alphabetic())
                        .any(|word| word.eq_ignore_ascii_case("hakille")),
                    "indice lexical halluciné dans une fixture sans nom d'activation"
                );
            }
        }
    }

    #[test]
    #[ignore = "nécessite Whisper tiny local ; essais de bruit synthétique, pas une garantie anti-hallucination"]
    fn nonverbal_noise_with_lexical_prompt() {
        let dir = std::env::var_os("HAKILLE_MODELS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        let mut engine = WhisperEngine::load_from_dir(dir).unwrap();
        let silence = engine
            .transcribe_with_prompt(&vec![0.0; 16_000], Some("fr"), Some("Hakille"), None)
            .unwrap();
        assert!(silence.suppressed_silence && silence.text.is_empty());
        let mut seed = 42_u32;
        let noise: Vec<f32> = (0..48_000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed as f64 / u32::MAX as f64 * 0.04 - 0.02) as f32
            })
            .collect();
        let tone: Vec<f32> = (0..48_000)
            .map(|index| (index as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.1)
            .collect();
        for (label, pcm) in [("white_noise", noise), ("tone_440hz", tone)] {
            for language in [None, Some("fr")] {
                let result = engine
                    .transcribe_with_prompt(
                        &pcm,
                        language,
                        Some("Hakille"),
                        Some(Instant::now() + std::time::Duration::from_secs(120)),
                    )
                    .unwrap();
                eprintln!(
                    "WHISPER_NONVERBAL {}",
                    serde_json::json!({
                        "fixture": label, "forced_language": language, "text": result.text,
                        "language": result.language, "no_speech_probability": result.no_speech_probability,
                        "suppressed_silence": result.suppressed_silence, "elapsed_ms": result.elapsed_ms,
                    })
                );
                assert!(
                    !result
                        .text
                        .split(|c: char| !c.is_alphabetic())
                        .any(|word| word.eq_ignore_ascii_case("hakille")),
                    "faux réveil non verbal"
                );
            }
        }
    }

    /// Diagnostic explicitement synthétique : le texte eSpeak n'est jamais
    /// utilisé comme transcription. Les positifs sont mesurés, les faux réveils
    /// sur négatifs font échouer le test après production du rapport complet.
    #[test]
    #[ignore = "nécessite HAKILLE_WHISPER_WAKE_MANIFEST et Whisper tiny local ; 16 inférences CPU"]
    fn synthetic_wake_comparison_report() {
        let manifest =
            std::env::var("HAKILLE_WHISPER_WAKE_MANIFEST").expect("manifest fixtures requis");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        assert_eq!(manifest["synthetic"], true);
        let fixtures = manifest["fixtures"].as_array().unwrap();
        assert!(!fixtures.is_empty() && fixtures.len() <= 32);
        let dir = std::env::var_os("HAKILLE_MODELS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        let mut engine = WhisperEngine::load_from_dir(dir).unwrap();
        let mut false_activations = Vec::new();
        let forced_language = std::env::var("HAKILLE_WHISPER_WAKE_LANGUAGE").ok();
        for fixture in fixtures {
            let bytes = std::fs::read(fixture["pcm"].as_str().unwrap()).unwrap();
            assert_eq!(bytes.len() % 4, 0);
            let pcm: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            let language = match forced_language.as_deref() {
                Some("fixture") => fixture["voice"].as_str().unwrap().split('-').next(),
                other => other,
            };
            for prompt in [None, Some("Hakille")] {
                let result = engine
                    .transcribe_with_prompt(
                        &pcm,
                        language,
                        prompt,
                        Some(Instant::now() + std::time::Duration::from_secs(180)),
                    )
                    .unwrap();
                let name_present = result
                    .text
                    .split(|c: char| !c.is_alphabetic())
                    .any(|word| word.eq_ignore_ascii_case("hakille"));
                eprintln!(
                    "WHISPER_SYNTHETIC {}",
                    serde_json::json!({
                        "fixture": fixture["name"], "prompt": prompt,
                    "forced_language": language,
                        "text": result.text, "language": result.language,
                        "language_probability": result.language_probability,
                        "no_speech_probability": result.no_speech_probability,
                        "elapsed_ms": result.elapsed_ms, "hakille_literal": name_present,
                        "intended_wake": fixture["contains_intended_wake_name"],
                    })
                );
                if name_present && fixture["contains_intended_wake_name"] == false {
                    false_activations.push((fixture["name"].clone(), prompt));
                }
            }
        }
        assert!(
            false_activations.is_empty(),
            "faux réveils synthétiques : {false_activations:?}"
        );
    }
}
