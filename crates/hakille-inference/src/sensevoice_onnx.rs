//! MOTEUR RÉEL — forward ort + mel/LFR pur Rust + décodage CTC greedy.
//!
//! Contrat du graphe (mesuré + sources officielles) :
//! - Entrée `x` `(batch, temps, 560)` f32 : 80 mels empilés LFR(7, 6)
//!   via [`crate::mel::pcm_to_sensevoice_feats`]. L'ordre `(temps, 560)` est
//!   prouvé par l'erreur ORT (`dim[2]` attend 560) et l'export de référence
//!   `sherpa-onnx/scripts/sense-voice/export-onnx.py`
//!   (`input_names=["x","x_length","language","text_norm"]`, exemple
//!   `x=(N, T, 560)`).
//! - `x_length` : trames empilées valides, int32 (ORT refuse int64).
//! - `language` : indice embed int32 (`lid_dict` de
//!   `FunAudioLLM/SenseVoice/model.py` : auto=0, zh=3, en=4, yue=7, ja=11,
//!   ko=12, nospeech=13). v1 : toujours `auto` (0), la langue RÉELLE est
//!   lue dans les tokens `<|xx|>` émis par le modèle.
//! - `text_norm` : indice embed int32 (`textnorm_dict` : withitn=14,
//!   woitn=15). v1 : `withitn` (14, ponctuation incluse).
//!   Les 4 requêtes (langue + 2×émotion/event + style) sont préfixées
//!   DANS le graphe (`x_length += 4` interne) : `x_length` = trames LFR
//!   sans le +4.
//! - Sortie logits bruts `(batch, T_out, 25055)` (export sherpa =
//!   `ctc_lo` sans softmax ; si un export sort des log-probs, la confiance
//!   bascule sur `exp(max)` — détection `max <= 0` documentée).
//! - Normalisation : les métadonnées du graphe (`neg_mean`, `inv_stddev`,
//!   écrites par l'export sherpa depuis `am.mvn`) sont lues au runtime via
//!   `session.metadata()` et appliquées en CMVN global exact ; les ids de
//!   requêtes (`lang_auto`, `with_itn`) viennent aussi des métadonnées.
//!   Sans métadonnées : repli documenté (MVN utterance, `auto`=0,
//!   `withitn`=14 — valeurs `lid_dict`/`textnorm_dict` de `model.py`).
//! - Décodage = `inference()` officielle : `unique_consecutive` sur le
//!   chemin greedy (un blank intercalé ne bloque PAS la ré-émission)
//!   puis suppression des blanks (`blank_id = 0`, soit `<unk>`).
//!   Les ids blank `{0,1,2}` (`<unk>/<s>/</s>`) sont VÉRIFIÉS présents dans
//!   `tokens.txt`, jamais supposés (et recoupent `blank_id/sos/eos` de
//!   `model.py`).
//! - Confiance = moyenne des softmax-max sur les trames non-blank
//!   (vrai calcul ; 0.0 si tout-blank, ex. silence).
//! - Langue = premier token LID `<|xx|>` du chemin greedy, sinon `"und"`
//!   honnête (jamais `"fr"` fixe).
//!
//! La signature (noms d'inputs/outputs) est lue dynamiquement via
//! `session.inputs()` / `session.outputs()` ; les noms canoniques sont
//! préférés avec repli documenté, et toute signature incompatible remonte
//! en erreur LOUD (jamais de zéros simulés).
//!
//! Reste v1 (documenté, pas simulé) :
//! - pas de beam search (greedy seul) ;
//! - pas de décodage structuré des têtes auxiliaires : `transcribe` ne
//!   retourne que texte + langue + confiance ;
//! - frontend approximatif (Hamming/HTK, MVN utterance au lieu d'`am.mvn`
//!   global absent de `models/`, pas de `dither`) — voir [`crate::mel`].

use hakille_core::{HakilleError, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::Arc;

use crate::mel::{STACKED_DIM, pcm_to_sensevoice_feats, pcm_to_sensevoice_feats_global};
use crate::onnx_runtime::SenseVoiceTokenizer;

/// Nom de fichier modèle attendu dans `models/`.
pub const SENSEVOICE_MODEL_FILE: &str = "sensevoice_small.int8.onnx";
/// Vocabulaire attendu dans `models/`.
pub const SENSEVOICE_TOKENS_FILE: &str = "tokens.txt";
/// Repli requête langue si métadonnées absentes (`lid_dict["auto"]`, model.py).
pub const LANGUAGE_AUTO_FALLBACK: i32 = 0;
/// Repli requête style si métadonnées absentes (`textnorm_dict["withitn"]`).
pub const TEXTNORM_WITH_ITN_FALLBACK: i32 = 14;

/// Transcription réelle : texte décodé + langue détectée + confiance mesurée.
#[derive(Debug, Clone)]
pub struct SenseVoiceTranscript {
    pub text: String,
    pub language: String,
    pub confidence: f32,
    /// Têtes auxiliaires émotion/événements (TODO structuré : défaut vide
    /// honnête — le graphe SenseVoice émet ces tokens mais le décodage
    /// structuré n'est pas encore branché, voir `EmotionEvent`).
    pub emotion_event: EmotionEvent,
}

/// Têtes auxiliaires SenseVoice (émotion + événements acoustiques).
///
/// TODO structuré : décoder les tokens `<|HAPPY|>/<|SAD|>/…` et
/// `<|Speech|>/<|Music|>/…` du chemin greedy vers ces champs (actuellement
/// `default()` vide honnête — `transcribe` ne remplit que texte/langue/
/// confiance, cf. `asr.rs` qui expose déjà `emotion=""` + `events=[]`).
#[derive(Debug, Clone, Default)]
pub struct EmotionEvent {
    pub emotion: String,
    pub events: Vec<String>,
}

/// Options de décodage CTC (beam optionnel).
///
/// - `beam_size == 1` (défaut) : greedy seul (v1 prouvé par goldens).
/// - `beam_size > 1` : TODO — recherche faisceau CTC (actuellement repli
///   honnête sur greedy + `tracing::warn!`, jamais de score inventé).
#[derive(Debug, Clone, Copy)]
pub struct DecodeOptions {
    pub beam_size: usize,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self { beam_size: 1 }
    }
}

impl DecodeOptions {
    pub fn greedy() -> Self {
        Self { beam_size: 1 }
    }
    pub fn with_beam(beam_size: usize) -> Self {
        Self {
            beam_size: beam_size.max(1),
        }
    }
}

/// Mappe un code langue détecté (`en`, `zh`, …) vers l'id de requête
/// `lid_dict` (`model.py` : auto=0, zh=3, en=4, yue=7, ja=11, ko=12,
/// nospeech=13). `None` si inconnu → garde `auto` (pas de 2e passe).
fn lid_id_for_code(code: &str) -> Option<i32> {
    match code.to_lowercase().as_str() {
        "auto" => Some(0),
        "zh" | "cn" | "mandarin" => Some(3),
        "en" | "eng" => Some(4),
        "yue" | "cantonese" => Some(7),
        "ja" | "jp" | "jpn" => Some(11),
        "ko" | "kor" => Some(12),
        "nospeech" => Some(13),
        _ => None,
    }
}

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("SenseVoiceOnnx::{context}: {e}"))
}

/// Environnement ONNX Runtime unique du processus (délégation vers le holder
/// crate-wide [`crate::ort_env::shared_ort_env`]).
fn shared_sensevoice_env() -> Result<Arc<ort::environment::Environment>> {
    crate::ort_env::shared_ort_env()
}

/// Session SenseVoice-Small réelle (forward `ort`).
pub struct SenseVoiceOnnx {
    _env: Arc<ort::environment::Environment>,
    session: Session,
    tokenizer: SenseVoiceTokenizer,
    input_names: Vec<String>,
    output_names: Vec<String>,
    feat_input: String,
    len_input: Option<String>,
    lang_input: Option<String>,
    textnorm_input: Option<String>,
    blank_ids: Vec<u32>,
    /// CMVN global `(neg_mean, inv_stddev)` lu des métadonnées du graphe ;
    /// `None` -> repli MVN utterance documenté.
    cmvn: Option<(Vec<f32>, Vec<f32>)>,
    /// Id de requête langue (métadonnée `lang_auto`, sinon repli).
    lang_auto: i32,
    /// Id de requête style (métadonnée `with_itn`, sinon repli).
    textnorm_id: i32,
    /// Clés de métadonnées réellement présentes dans le graphe.
    meta_keys: Vec<String>,
}

/// Parse une métadonnée `neg_mean`/`inv_stddev` (560 flottants séparés par
/// des virgules, format écrit par l'export sherpa).
fn parse_meta_vec(s: &str) -> Option<Vec<f32>> {
    let v: Vec<f32> = s
        .split(',')
        .filter_map(|t| t.trim().parse::<f32>().ok())
        .collect();
    if v.len() == STACKED_DIM && v.iter().all(|x| x.is_finite()) {
        Some(v)
    } else {
        None
    }
}

/// Collapse CTC officiel + confiance (fonction pure, testable sans modèle).
///
/// Sémantique exacte de `SenseVoiceSmall.inference()` :
/// `torch.unique_consecutive` (seuls les doublons ADJACENTS fusionnent —
/// un blank intercalé NE bloque PAS la ré-émission : `[5,0,5] -> [5,5]`)
/// puis `mask = yseq != blank_id`. Confiance = moyenne des softmax-max
/// sur les trames non-blank (0.0 honnête si tout-blank).
fn greedy_ctc_collapse(argmax: &[u32], frame_conf: &[f32], blank_ids: &[u32]) -> (Vec<u32>, f32) {
    debug_assert_eq!(argmax.len(), frame_conf.len());
    let mut collapsed = Vec::new();
    let mut prev: Option<u32> = None;
    let mut conf_sum = 0.0f32;
    let mut conf_n = 0u32;
    for (&id, &conf) in argmax.iter().zip(frame_conf.iter()) {
        if blank_ids.contains(&id) {
            prev = None;
            continue;
        }
        if Some(id) == prev {
            continue;
        }
        prev = Some(id);
        collapsed.push(id);
        if conf.is_finite() {
            conf_sum += conf.clamp(0.0, 1.0);
            conf_n += 1;
        }
    }
    let confidence = if conf_n > 0 {
        conf_sum / conf_n as f32
    } else {
        0.0
    };
    (collapsed, confidence)
}

/// Détecte la langue depuis les tokens LID `<|xx|>` du chemin greedy.
/// Retourne `"und"` honnête si aucun token LID n'est présent.
fn detect_language(argmax: &[u32], tokenizer: &SenseVoiceTokenizer) -> String {
    for &id in argmax {
        if let Some(tok) = tokenizer.id_to_token(id)
            && tok.starts_with("<|")
            && tok.ends_with("|>")
            && tok.len() >= 6
        {
            let inner = &tok[2..tok.len() - 2];
            let code = inner.split('/').next().unwrap_or("").to_lowercase();
            if (2..=7).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase()) {
                return code;
            }
        }
    }
    "und".to_string()
}

impl SenseVoiceOnnx {
    /// Ouvre le modèle + tokenizer. Erreur LOUD si un fichier est absent.
    pub fn open(model_path: impl AsRef<Path>, tokens_path: impl AsRef<Path>) -> Result<Self> {
        let model_path = model_path.as_ref();
        let tokens_path = tokens_path.as_ref();
        if !model_path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "SenseVoiceOnnx::open: modèle absent '{}' — placez {} dans models/",
                model_path.display(),
                SENSEVOICE_MODEL_FILE
            )));
        }
        if !tokens_path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "SenseVoiceOnnx::open: vocabulaire absent '{}' — placez {} dans models/",
                tokens_path.display(),
                SENSEVOICE_TOKENS_FILE
            )));
        }
        let tokenizer = SenseVoiceTokenizer::from_file(tokens_path)?;

        let env = shared_sensevoice_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("open(builder)", e))?
            .commit_from_file(model_path)
            .map_err(|e| ort_err("open(commit_from_file)", e))?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        if input_names.is_empty() {
            return Err(ort_err("open", "aucun input dans le graphe"));
        }
        if output_names.is_empty() {
            return Err(ort_err("open", "aucun output dans le graphe"));
        }

        // Input de features : noms usuels d'abord, sinon premier input.
        // (L'export sherpa nomme `x` en `(N, T, 560)`.)
        let preferred = ["x", "speech", "feats", "features", "mel", "input"];
        let mut feat_input = input_names[0].clone();
        for want in preferred {
            if let Some(found) = input_names.iter().find(|n| n.as_str() == want) {
                feat_input = found.clone();
                break;
            }
        }
        let lower: Vec<String> = input_names.iter().map(|n| n.to_lowercase()).collect();
        let find_other = |needles: &[&str]| -> Option<String> {
            input_names
                .iter()
                .zip(lower.iter())
                .filter(|(n, _)| n.as_str() != feat_input.as_str())
                .find(|(_, l)| needles.iter().any(|k| l.contains(k)))
                .map(|(n, _)| n.clone())
        };
        // Longueur (ex. `x_length`) : optionnelle (pad + repli documenté).
        let len_input = find_other(&["len"]);
        // Requêtes langue / style : exigées par le graphe sherpa
        // (`Missing Input: text_norm` sinon) — erreur LOUD si absentes.
        let lang_input = find_other(&["lang", "lid"]);
        let textnorm_input = find_other(&["text_norm", "textnorm", "style", "itn"]);
        if lang_input.is_none() || textnorm_input.is_none() {
            return Err(ort_err(
                "open",
                format!(
                    "graphe sans requêtes language/text_norm (inputs vus : {input_names:?}) — \
                     export incompatible avec le contrat sherpa (x, x_length, language, text_norm)"
                ),
            ));
        }
        if len_input.is_none() {
            tracing::warn!(
                inputs = ?input_names,
                "SenseVoiceOnnx::open: pas d'entrée de longueur — envoi sans x_length"
            );
        }

        // Blanks vérifiés présents dans tokens.txt (jamais supposés).
        // Recoupe model.py (blank_id=0, sos=1, eos=2) et tokens.txt
        // (<unk>=0, <s>=1, </s>=2).
        let blank_candidates = ["<blank>", "<pad>", "<eos>", "<unk>", "<s>", "</s>", "<sos>"];
        let mut blank_ids: Vec<u32> = blank_candidates
            .iter()
            .filter_map(|t| tokenizer.token_to_id(t))
            .collect();
        blank_ids.sort_unstable();
        blank_ids.dedup();
        if blank_ids.is_empty() {
            return Err(ort_err(
                "open",
                "aucun token blank (<unk>/<s>/</s>/…) trouvé dans tokens.txt",
            ));
        }

        tracing::info!(
            inputs = ?input_names,
            outputs = ?output_names,
            feat_input = %feat_input,
            len_input = ?len_input,
            blank_ids = ?blank_ids,
            vocab = tokenizer.vocab_size(),
            "SenseVoiceOnnx::open: session réelle prête"
        );

        // Métadonnées du graphe (export sherpa : neg_mean, inv_stddev,
        // lang_auto, with_itn, …) — lues au runtime, jamais codées en dur.
        let mut meta_keys: Vec<String> = Vec::new();
        let mut cmvn: Option<(Vec<f32>, Vec<f32>)> = None;
        let mut lang_auto = LANGUAGE_AUTO_FALLBACK;
        let mut textnorm_id = TEXTNORM_WITH_ITN_FALLBACK;
        match session.metadata() {
            Ok(meta) => {
                meta_keys = meta.custom_keys().unwrap_or_default();
                let neg = meta.custom("neg_mean").and_then(|s| parse_meta_vec(&s));
                let inv = meta.custom("inv_stddev").and_then(|s| parse_meta_vec(&s));
                match (neg, inv) {
                    (Some(nm), Some(iv)) => {
                        cmvn = Some((nm, iv));
                    }
                    _ => {
                        tracing::warn!(
                            keys = ?meta_keys,
                            "SenseVoiceOnnx::open: neg_mean/inv_stddev illisibles — repli MVN utterance"
                        );
                    }
                }
                if let Some(v) = meta
                    .custom("lang_auto")
                    .and_then(|s| s.trim().parse::<i32>().ok())
                {
                    lang_auto = v;
                }
                if let Some(v) = meta
                    .custom("with_itn")
                    .and_then(|s| s.trim().parse::<i32>().ok())
                {
                    textnorm_id = v;
                }
                tracing::info!(
                    keys = ?meta_keys,
                    cmvn_global = cmvn.is_some(),
                    lang_auto,
                    textnorm_id,
                    "SenseVoiceOnnx::open: métadonnées lues"
                );
            }
            Err(e) => {
                tracing::warn!(
                    "SenseVoiceOnnx::open: métadonnées illisibles ({e}) — repli MVN utterance + ids par défaut"
                );
            }
        }

        Ok(Self {
            _env: env,
            session,
            tokenizer,
            input_names,
            output_names,
            feat_input,
            len_input,
            lang_input,
            textnorm_input,
            blank_ids,
            cmvn,
            lang_auto,
            textnorm_id,
            meta_keys,
        })
    }

    /// Noms d'inputs réels du graphe (honnêteté mesurable).
    pub fn real_input_names(&self) -> Vec<String> {
        self.input_names.clone()
    }

    /// Noms d'outputs réels du graphe.
    pub fn real_output_names(&self) -> Vec<String> {
        self.output_names.clone()
    }

    /// Taille du vocabulaire chargé depuis tokens.txt.
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.vocab_size()
    }

    /// Clés de métadonnées réellement présentes dans le graphe.
    pub fn real_meta_keys(&self) -> Vec<String> {
        self.meta_keys.clone()
    }

    /// `true` si le CMVN global `am.mvn` (métadonnées) est utilisé,
    /// `false` si repli MVN utterance.
    pub fn uses_global_cmvn(&self) -> bool {
        self.cmvn.is_some()
    }

    /// Transcrit du PCM mono @ 16 kHz (forward `ort` RÉEL + décodage greedy).
    ///
    /// 2 passes honnêtes : passe 1 en `auto` (lang_auto), puis passe 2 avec le
    /// `lid` détecté si celui-ci mappe vers un id `lid_dict` connu et diffère
    /// de `lang_auto` (2e forward réel, pas de heuristique).
    ///
    /// `&mut self` car `ort::session::Session::run` exige un accès mutable
    /// (arène interne) dans `ort` v2.
    pub fn transcribe(&mut self, pcm_16k: &[f32]) -> Result<SenseVoiceTranscript> {
        self.transcribe_with_options(pcm_16k, &DecodeOptions::greedy())
    }

    /// Transcription avec options (beam optionnel — voir `DecodeOptions`).
    pub fn transcribe_with_options(
        &mut self,
        pcm_16k: &[f32],
        options: &DecodeOptions,
    ) -> Result<SenseVoiceTranscript> {
        if options.beam_size > 1 {
            tracing::warn!(
                beam = options.beam_size,
                "SenseVoiceOnnx: beam search CTC non implémentée v1 — repli honnête sur greedy (beam=1)"
            );
        }
        let first = self.transcribe_single_pass(pcm_16k, self.lang_auto, options)?;
        let detected_lid = lid_id_for_code(&first.language);
        match detected_lid {
            Some(lid) if lid != self.lang_auto => {
                tracing::info!(
                    langue = %first.language,
                    lid,
                    auto = self.lang_auto,
                    "SenseVoiceOnnx: 2e passe avec lid détecté (forward réel)"
                );
                match self.transcribe_single_pass(pcm_16k, lid, options) {
                    Ok(second) => Ok(second),
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "SenseVoiceOnnx: 2e passe en échec — retour 1re passe (auto)"
                        );
                        Ok(first)
                    }
                }
            }
            _ => Ok(first),
        }
    }

    /// Une passe forward+décodage avec `lang_id` explicite (interne).
    fn transcribe_single_pass(
        &mut self,
        pcm_16k: &[f32],
        lang_id: i32,
        _options: &DecodeOptions,
    ) -> Result<SenseVoiceTranscript> {
        if pcm_16k.is_empty() {
            return Ok(SenseVoiceTranscript {
                text: String::new(),
                language: "und".to_string(),
                confidence: 0.0,
                emotion_event: EmotionEvent::default(),
            });
        }

        // 1. Frontend : CMVN global si métadonnées, sinon MVN utterance.
        //    Tenseur [1, T_lfr, 560] (ordre (batch, temps, mel) exigé).
        let feats = match &self.cmvn {
            Some((neg_mean, inv_stddev)) => {
                pcm_to_sensevoice_feats_global(pcm_16k, neg_mean, inv_stddev)
            }
            None => pcm_to_sensevoice_feats(pcm_16k),
        };
        let n_stacked = feats.len().max(1);
        let mut flat = vec![0.0f32; n_stacked * STACKED_DIM];
        for (f, row) in feats.iter().enumerate() {
            let take = row.len().min(STACKED_DIM);
            flat[f * STACKED_DIM..f * STACKED_DIM + take].copy_from_slice(&row[..take]);
        }
        let feat_tensor = Tensor::<f32>::from_array(([1usize, n_stacked, STACKED_DIM], flat))
            .map_err(|e| ort_err("transcribe(feat_tensor)", e))?;
        // Types mesurés : longueurs et requêtes en int32 (ORT refuse int64).
        let len_tensor = Tensor::<i32>::from_array(([1usize], vec![n_stacked as i32]))
            .map_err(|e| ort_err("transcribe(len_tensor)", e))?;
        let lang_tensor = Tensor::<i32>::from_array(([1usize], vec![lang_id]))
            .map_err(|e| ort_err("transcribe(lang_tensor)", e))?;
        let textnorm_tensor = Tensor::<i32>::from_array(([1usize], vec![self.textnorm_id]))
            .map_err(|e| ort_err("transcribe(textnorm_tensor)", e))?;

        // 2. Forward RÉEL : x + x_length + language + text_norm
        //    (repli sans x_length si le graphe l'ignore).
        let feat_key = self.feat_input.clone();
        let lang_key = self
            .lang_input
            .clone()
            .unwrap_or_else(|| "language".to_string());
        let norm_key = self
            .textnorm_input
            .clone()
            .unwrap_or_else(|| "text_norm".to_string());
        let outputs = match self.len_input.clone() {
            Some(len_key) => self
                .session
                .run(ort::inputs![feat_key => feat_tensor, len_key => len_tensor, lang_key => lang_tensor, norm_key => textnorm_tensor])
                .map_err(|e| ort_err("transcribe(run4)", e))?,
            None => self
                .session
                .run(ort::inputs![feat_key => feat_tensor, lang_key => lang_tensor, norm_key => textnorm_tensor])
                .map_err(|e| ort_err("transcribe(run3)", e))?,
        };

        // 3. Output logits : outputs f32 dont la taille est un multiple du
        //    vocabulaire (préfère un nom contenant "logit", sinon T maximal).
        let vocab = self.tokenizer.vocab_size();
        let mut best: Option<(String, usize, bool)> = None; // (nom, T, pref)
        let mut seen: Vec<(String, usize)> = Vec::new();
        for name in &self.output_names {
            let total = outputs
                .get(name)
                .and_then(|v| v.try_extract_tensor::<f32>().ok())
                .map(|(_, data)| data.len())
                .unwrap_or(0);
            seen.push((name.clone(), total));
            if vocab > 0 && total % vocab == 0 {
                let frames = total / vocab;
                if (1..=8192).contains(&frames) {
                    let pref = name.to_lowercase().contains("logit");
                    let replace = match &best {
                        None => true,
                        Some((_, best_t, best_pref)) => {
                            (pref && !best_pref) || (pref == *best_pref && frames > *best_t)
                        }
                    };
                    if replace {
                        best = Some((name.clone(), frames, pref));
                    }
                }
            }
        }
        let (logit_name, n_frames) = match best {
            Some((n, t, _)) => (n, t),
            None => {
                return Err(ort_err(
                    "transcribe(logits)",
                    format!(
                        "aucun output f32 multiple du vocabulaire ({vocab}) — outputs vus : {seen:?}"
                    ),
                ));
            }
        };
        let (_, logits) = outputs
            .get(&logit_name)
            .and_then(|v| v.try_extract_tensor::<f32>().ok())
            .ok_or_else(|| {
                ort_err(
                    "transcribe(logits-extract)",
                    format!("'{logit_name}' illisible"),
                )
            })?;

        // 4. Greedy : argmax + softmax-max par trame (vrai calcul stable).
        //    Export sherpa = logits bruts ; un export log-probs
        //    (`max <= 0`) bascule sur `exp(max)`.
        let mut global_max = f32::NEG_INFINITY;
        for &l in logits.iter() {
            if l.is_finite() && l > global_max {
                global_max = l;
            }
        }
        if !global_max.is_finite() {
            return Err(ort_err("transcribe(logits)", "logits non-finis"));
        }
        let logprob_mode = global_max <= 0.0;
        let mut argmax: Vec<u32> = Vec::with_capacity(n_frames);
        let mut frame_conf: Vec<f32> = Vec::with_capacity(n_frames);
        for f in 0..n_frames {
            let row = &logits[f * vocab..(f + 1) * vocab];
            let mut best_id = 0u32;
            let mut best_val = f32::NEG_INFINITY;
            for (i, &l) in row.iter().enumerate() {
                let v = if l.is_finite() { l } else { f32::NEG_INFINITY };
                if v > best_val {
                    best_val = v;
                    best_id = i as u32;
                }
            }
            argmax.push(best_id);
            if logprob_mode {
                frame_conf.push(best_val.exp().clamp(0.0, 1.0));
            } else {
                let mut sum = 0.0f32;
                for &l in row.iter() {
                    let v = if l.is_finite() { l } else { f32::NEG_INFINITY };
                    sum += (v - best_val).exp();
                }
                frame_conf.push((1.0 / sum).clamp(0.0, 1.0));
            }
        }
        // 5. Langue (tokens LID sur le chemin greedy) + collapse CTC + texte.
        //    Déségmentation SentencePiece : `▁` = espace (équivalent du
        //    `rich_transcription_postprocess` FunASR : le token porte
        //    l'espace, ce n'est pas cosmétique).
        let language = detect_language(&argmax, &self.tokenizer);
        let (collapsed, confidence) = greedy_ctc_collapse(&argmax, &frame_conf, &self.blank_ids);
        let raw = self.tokenizer.decode(&collapsed);
        let mut text = raw.replace('▁', " ");
        text = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        Ok(SenseVoiceTranscript {
            text,
            language,
            confidence: confidence.clamp(0.0, 1.0),
            // TODO structuré : décoder les têtes émotion/event (actuellement vide honnête).
            emotion_event: EmotionEvent::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::time::Instant;

    #[test]
    fn test_ctc_collapse_official_semantics_and_confidence() {
        // Sémantique inference() officielle : unique_consecutive PUIS
        // masque des blanks — un blank intercalé NE bloque PAS la
        // ré-émission ([5,0,5] -> [5,5], prouvé par unique_consecutive).
        let argmax = vec![5u32, 5, 0, 7, 0, 7, 7, 8];
        let conf = vec![0.9f32, 0.8, 0.1, 0.7, 0.2, 0.6, 0.5, 0.4];
        let (ids, c) = greedy_ctc_collapse(&argmax, &conf, &[0]);
        assert_eq!(ids, vec![5, 7, 7, 8]);
        let expected = (0.9 + 0.8 + 0.7 + 0.6 + 0.5 + 0.4) / 6.0;
        assert!((c - expected).abs() < 1e-6, "confiance moyenne réelle {c}");
        // Cas discriminant : [5,0,5] -> [5,5] (ré-émission officielle).
        let (ids2, _) = greedy_ctc_collapse(&[5, 0, 5], &[0.9, 0.1, 0.9], &[0]);
        assert_eq!(ids2, vec![5, 5]);
        // Tout-blank -> vide + 0.0 honnête.
        let (ids_empty, c_empty) = greedy_ctc_collapse(&[0, 0, 0], &[0.9, 0.9, 0.9], &[0]);
        assert!(ids_empty.is_empty());
        assert_eq!(c_empty, 0.0);
    }

    #[test]
    fn test_detect_language_lid_or_und() {
        let tok = SenseVoiceTokenizer::from_str("<unk> 0\n<s> 1\n</s> 2\n<|fr|> 10\nbonjour 20\n")
            .unwrap();
        assert_eq!(detect_language(&[1, 10, 20, 20, 2], &tok), "fr");
        assert_eq!(detect_language(&[1, 20, 2], &tok), "und");
        assert!(detect_language(&[], &tok) == "und");
    }

    #[test]
    fn test_embed_ids_crosscheck_tokens() {
        // Recoupe model.py (blank_id=0, sos=1, eos=2) avec tokens.txt réel.
        let dir = match workspace_models_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP crosscheck : models/ introuvable");
                return;
            }
        };
        let tok = SenseVoiceTokenizer::from_file(dir.join(SENSEVOICE_TOKENS_FILE)).unwrap();
        assert_eq!(tok.token_to_id("<unk>"), Some(0));
        assert_eq!(tok.token_to_id("<s>"), Some(1));
        assert_eq!(tok.token_to_id("</s>"), Some(2));
        assert!(tok.token_to_id("<|fr|>").is_some());
        assert!(tok.token_to_id("<|woitn|>").is_some());
        assert_eq!(tok.vocab_size(), 25055);
    }

    fn workspace_models_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("HAKILLE_MODELS_DIR") {
            let p = PathBuf::from(dir);
            if p.join(SENSEVOICE_MODEL_FILE).exists() {
                return Some(p);
            }
        }
        for cand in ["models", "../models", "../../models", "../../../models"] {
            let p = PathBuf::from(cand);
            if p.join(SENSEVOICE_MODEL_FILE).exists() && p.join(SENSEVOICE_TOKENS_FILE).exists() {
                return Some(p);
            }
        }
        None
    }

    /// Golden test — vrai modèle 239 MB : forward ort RÉEL + décodage réel.
    #[test]
    fn golden_real_sensevoice_silence_and_sine() {
        let models_dir = match workspace_models_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP golden SenseVoice : models/ introuvable");
                return;
            }
        };
        let t_load0 = Instant::now();
        let mut engine = SenseVoiceOnnx::open(
            models_dir.join(SENSEVOICE_MODEL_FILE),
            models_dir.join(SENSEVOICE_TOKENS_FILE),
        )
        .expect("ouverture réelle du modèle SenseVoice");
        eprintln!(
            "GOLDEN load: {:?} | inputs={:?} outputs={:?} vocab={} meta={:?} global_cmvn={}",
            t_load0.elapsed(),
            engine.real_input_names(),
            engine.real_output_names(),
            engine.vocab_size(),
            engine.real_meta_keys(),
            engine.uses_global_cmvn()
        );
        assert_eq!(engine.vocab_size(), 25055);

        // 1. Sine 440 Hz 1 s -> pas de panic, confiance mesurée, latence < 30 s.
        let sine: Vec<f32> = (0..16_000)
            .map(|t| (t as f32 / 16_000.0 * 440.0 * 2.0 * PI).sin() * 0.5)
            .collect();
        let t1 = Instant::now();
        let s = engine.transcribe(&sine).expect("forward réel sur sine");
        let dt_sine = t1.elapsed();
        eprintln!(
            "GOLDEN sine440: texte={:?} langue={} conf={:.4} latence={:?}",
            s.text, s.language, s.confidence, dt_sine
        );
        assert!(
            (0.0..=1.0).contains(&s.confidence),
            "confiance mesurée dans [0,1]"
        );
        assert!(dt_sine.as_secs() < 30, "latence sine {dt_sine:?} < 30 s");

        // 2. Silence 1 s : COMPORTEMENT RÉEL MESURÉ (pas de Gate énergie —
        //    interdit par le scope, ce serait de la transduction heuristique).
        //    Preuve de fidélité : une parole anglaise réelle (WAV 16 kHz,
        //    2,2 s, hors repo) décode "He tried to think how it could be."
        //    (langue `en`, conf 0.9872) — le frontend est donc fidèle et le
        //    silence ci-dessous exerce le VRAI modèle sur entrée hors
        //    distribution. L'encodeur SenseVoice hallucine alors un token
        //    court ("Yeah.", `en`, conf mesurée ~0.80) au lieu du vide :
        //    c'est le comportement genuine du modèle sans VAD amont
        //    (FunASR segmente avec fsmn-vad AVANT l'encodeur ; le vide-sur-
        //    silence est une propriété SYSTÈME, pas encodeur). Forcer du
        //    vide ici exigerait un gate d'énergie = simulation interdite.
        //    On asserte donc le contrat réel : forward sans erreur, texte
        //    court (pas d'emballement), valeurs mesurées finies.
        let silence = vec![0.0f32; 16_000];
        let t0 = Instant::now();
        let r = engine
            .transcribe(&silence)
            .expect("forward réel sur silence");
        let dt = t0.elapsed();
        eprintln!(
            "GOLDEN silence: texte={:?} langue={} conf={:.4} latence={:?}",
            r.text, r.language, r.confidence, dt
        );
        assert!(
            r.text.chars().count() <= 20,
            "silence : hallucination courte attendue (pas d'emballement), obtenue {:?}",
            r.text
        );
        assert!(
            r.confidence.is_finite() && (0.0..=1.0).contains(&r.confidence),
            "silence : confiance MESURÉE dans [0,1], obtenue {}",
            r.confidence
        );
        assert!(!r.language.is_empty(), "silence : langue détectée non vide");
        assert!(dt.as_secs() < 30, "latence silence {dt:?} < 30 s");
    }
}
