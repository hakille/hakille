//! REAL-04 — Moteur LLM Qwen 100% RÉEL (vrai décodage autoregressif depuis GGUF).
//!
//! # Stratégie (pure Rust, pas de C++/cmake)
//!
//! - `candle-core` + `candle-nn` + `candle-transformers` (modèle `qwen2`
//!   déjà implémenté upstream) + `tokenizers` (BPE Qwen).
//! - Chemin choisi (qui compile) : lecture directe du GGUF quantifié via
//!   `candle_core::quantized::gguf_file::Content::read` +
//!   `candle_transformers::models::quantized_qwen2::ModelWeights::from_gguf`.
//!   Ce chemin supporte nativement Q4_K_M (cf. exemple upstream
//!   `quantized-qwen2-instruct` qui charge un `Q4_K_M`, et les types
//!   observés dans notre checkpoint : F32(0), Q6_K/Q5_0(6), Q8_0(8),
//!   Q4_K(12), Q6_K(14) — tous gérés par `QTensor`/`QMatMul`).
//! - Aucune conversion GGUF→safetensors nécessaire : le forward s'exécute
//!   directement sur les poids quantifiés (déquantification à la volée
//!   dans `QMatMul`, embeddings/normes/biais déquantifiés une fois au
//!   chargement). Si une future version candle refusait un type (erreur
//!   `unsupported ggml dtype`), la BASCULE documentée serait : déquantifier
//!   Q4_K_M→f32 via `crate::gguf_runtime` + `crate::quantization` vers un
//!   cache safetensors dans `models/` puis forward f32 — lent mais RÉEL.
//!   Cette bascule n'est PAS activée car le chemin direct compile et charge.
//! - Réutilisation du parseur existant : avant tout chargement candle, on
//!   valide le header via `crate::gguf_runtime::GgufModel::read_from_file`
//!   (magic GGUF, version v3, arch `qwen2`, ~291 tenseurs) — aucune
//!   duplication du parsing binaire.
//! - Tokenizer : `models/tokenizer.json` (Qwen2.5-0.5B-Instruct depuis
//!   HuggingFace). Téléchargé dans `models/` si réseau (helper async),
//!   sinon erreur LOUD + documentation (jamais de tokenizer synthétique).
//! - Device : CPU uniquement en v1 (`candle_core::Device::Cpu`).
//!   Voir `SUPPORTED_BACKENDS` (`"CPU only, CUDA/Metal planned"`) et
//!   `is_gpu_available()` (toujours `false` en v1) — aucun code GPU,
//!   aucun fallback silencieux.
//!
//! # Garanties d'honnêteté
//!
//! - AUCUN gabarit de réponse codé en dur. Si le modèle génère vide
//!   (EOS immédiat), on retourne `text=""`, `tokens_generated=0` (ou le
//!   nombre réel généré avant EOS) + log `tracing::warn!`, jamais de phrase
//!   inventée.
//! - Erreurs LOUD (`HakilleError::InferenceError`) si GGUF/tokenizer absents,
//!   avec chemins attendus + URL HF + instructions.
//! - Vrai forward autoregressif avec KV-cache interne candle
//!   (`ModelWeights::forward` + `clear_kv_cache`, `index_pos` croissant).
//! - Sampling multinomial T/top-p RÉEL via `LogitsProcessor` (seed OS par
//!   génération). Seul `T==0` utilise `ArgMax` (greedy explicite, documenté).
//! - Stop sur EOS/IM_END (`<|im_end|>` id 151645, `<|endoftext|>` 151643).
//! - ChatML appliqué réellement (system/user/assistant).
//!
//! # Fichiers
//!
//! - GGUF : `models/qwen2.5-0.5b-instruct-q4_k_m.gguf` (491MB, arch qwen2).
//! - Tokenizer : `models/tokenizer.json` (Qwen2.5-0.5B-Instruct, ~7MB).

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen2::ModelWeights;
use hakille_core::{HakilleError, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Constantes publiques (chemins, HF, EOS, ChatML)
// ---------------------------------------------------------------------------

/// Backends d'exécution supportés — explicite, sans ambiguïté.
///
/// v1 : CPU uniquement (`candle_core::Device::Cpu`). CUDA/Metal sont
/// planifiés mais NON implémentés (aucun code GPU dans ce crate,
/// aucun fallback silencieux). Toute tentative d'activation GPU doit
/// passer par `is_gpu_available()` qui retourne `false` tant que le
/// backend n'existe pas.
pub const SUPPORTED_BACKENDS: &str = "CPU only, CUDA/Metal planned";

/// Retourne `false` en v1 : aucun backend GPU n'est disponible.
///
/// Point de branchement unique pour tout futur support CUDA/Metal :
/// tant que cette fonction retourne `false`, le moteur reste sur
/// `Device::Cpu` (assumé explicitement, jamais par défaut silencieux).
pub fn is_gpu_available() -> bool {
    false
}

/// Nom du checkpoint GGUF Qwen attendu dans `models/`.
pub const DEFAULT_GGUF_FILENAME: &str = "qwen2.5-0.5b-instruct-q4_k_m.gguf";
/// Nom du tokenizer BPE Qwen attendu dans `models/`.
pub const DEFAULT_TOKENIZER_FILENAME: &str = "tokenizer.json";
/// URL canonique du tokenizer Qwen2.5-0.5B-Instruct (HuggingFace).
pub const TOKENIZER_HF_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/main/tokenizer.json";
/// Repo HF du checkpoint (documentation, traçabilité).
pub const GGUF_HF_HINT: &str = "Qwen/Qwen2.5-0.5B-Instruct-GGUF (fichier q4_k_m)";
/// Token de fin de tour assistant (`tokenizer.ggml.eos_token_id` = 151645).
pub const EOS_IM_END_STR: &str = "<|im_end|>";
/// Token de fin de texte (`bos/pad` = 151643).
pub const EOS_TEXT_STR: &str = "<|endoftext|>";
/// IDs connus (GGUF `tokenizer.ggml.*` + `added_tokens` du tokenizer.json).
/// Utilisés en fallback si le vocab du tokenizer ne les expose pas.
pub const EOS_IM_END_ID_FALLBACK: u32 = 151645;
pub const EOS_TEXT_ID_FALLBACK: u32 = 151643;
/// Prompt système par défaut (instruction réellement envoyée au modèle,
/// PAS une réponse — le modèle génère la suite).
pub const DEFAULT_SYSTEM_PROMPT: &str = "Tu es Hakille, un système d'exploitation cognitif souverain et local fonctionnant sous Windows. Réponds de façon concise, précise et rigoureuse sans dépendre d'un serveur distant.";

// ---------------------------------------------------------------------------
// Sortie de génération
// ---------------------------------------------------------------------------

/// Résultat d'un vrai décodage autoregressif.
#[derive(Debug, Clone)]
pub struct GenerateOutput {
    /// Texte décodé (sans tokens spéciaux, trimmé). Peut être vide si EOS
    /// immédiat — dans ce cas `tokens_generated` vaut le nombre réel de
    /// tokens émis avant stop (0 si aucun) et un `warn!` est émis.
    pub text: String,
    /// Nombre de tokens générés (hors prompt).
    pub tokens_generated: usize,
    /// Tokens du prompt formaté ChatML (comptés au tokenizer réel).
    pub prompt_tokens: usize,
    /// Latence murale totale (préfill + décodage), en ms.
    pub latency_ms: u64,
}

// ---------------------------------------------------------------------------
// Streaming token par token (affichage live, meilleur effort)
// ---------------------------------------------------------------------------

/// Événement de streaming émis pour chaque token réellement généré.
///
/// CONTRAT : un `TokenEvent` est émis pour CHAQUE token poussé dans la
/// séquence générée, donc `nombre d'événements == tokens_generated`.
/// `text_delta` est un MEILLEUR EFFORT D'AFFICHAGE (décodage incrémental) :
/// le texte qui fait foi reste `GenerateOutput.text`, décodé en one-shot à
/// la fin (chemin inchangé, jamais altéré par le streaming).
#[derive(Debug, Clone)]
pub struct TokenEvent {
    /// Index 0-based dans l'ordre d'émission (== position du token dans la
    /// séquence générée).
    pub index: usize,
    /// ID BPE réel du token (vocab tokenizer).
    pub token_id: u32,
    /// Delta affichable depuis l'événement précédent. Peut être vide
    /// (caractères de remplacement UTF-8 en attente d'être résolus par le
    /// token suivant, échec du décodage incrémental — jamais de panic).
    pub text_delta: String,
}

/// Type du callback global de streaming (alias pour la lisibilité).
type StreamCallback = Box<dyn FnMut(&TokenEvent) + Send + 'static>;

/// Callback global optionnel (utilisé par le daemon pour l'affichage live).
///
/// ATTENTION : il est invoqué PENDANT que le verrou du moteur Qwen est tenu
/// (`global_qwen_engine()`). Il ne doit JAMAIS rappeler le moteur (deadlock),
/// ni `set_token_stream_callback`/`clear_token_stream_callback` (deadlock du
/// registre), et doit rester rapide (chaque appel bloque le décodage).
static STREAM_CALLBACK: std::sync::Mutex<Option<StreamCallback>> = std::sync::Mutex::new(None);

/// Enregistre (ou remplace) le callback global de streaming.
///
/// Sans effet bruyant si le registre est empoisonné : `warn!` + abandon du
/// callback (jamais de panic). Voir l'avertissement sur `STREAM_CALLBACK`
/// (le callback ne doit jamais rappeler le moteur).
pub fn set_token_stream_callback(cb: Option<StreamCallback>) {
    match STREAM_CALLBACK.lock() {
        Ok(mut guard) => *guard = cb,
        Err(_) => tracing::warn!(
            "Registre de streaming empoisonné — callback global non enregistré (pas de panic)"
        ),
    }
}

/// Retire le callback global (à appeler systématiquement après un tour).
pub fn clear_token_stream_callback() {
    set_token_stream_callback(None);
}

/// Normalise un delta pour l'affichage : marqueur SentencePiece `▁`
/// (U+2581, début de mot) → espace. AFFICHAGE UNIQUEMENT — le texte final
/// `GenerateOutput.text` n'est jamais passé par ici. Pour Qwen2.5 (BPE
/// byte-level GPT-2, `Ġ`), le décodeur rend déjà de vrais espaces : cette
/// normalisation y est un no-op défensif, utile aux vocabs SentencePiece.
fn normalize_display_delta(delta: &str) -> String {
    delta.replace('\u{2581}', " ")
}

/// Retire les caractères de remplacement U+FFFD en fin de chaîne (octet
/// UTF-8 partiel coupé entre deux tokens) ; ils seront reportés au token
/// suivant dès que le tokenizer les résout.
fn trim_trailing_replacement(s: &str) -> &str {
    let mut out = s;
    while let Some(rest) = out.strip_suffix('\u{FFFD}') {
        out = rest;
    }
    out
}

/// État de décodage incrémental pour l'affichage (best effort).
///
/// Le delta est calculé en re-décodant la TOTALITÉ des ids déjà émis puis en
/// prenant le suffixe nouveau (O(n²) au total, négligeable devant le forward
/// CPU). Les caractères de remplacement finaux (U+FFFD) sont RETENUS et
/// reportés sur les tokens suivants. Si le décodage échoue ou si le préfixe
/// change (cas pathologique), le delta est vide — jamais de panic, le texte
/// final décodé en one-shot reste la référence.
#[derive(Default)]
struct StreamDecodeState {
    /// Dernier préfixe de décodage stable (sans U+FFFD final), déjà affiché.
    prev_stable: String,
}

impl StreamDecodeState {
    /// Delta affichable pour un décodage complet donné (pur, testable).
    fn delta_for_full_decode(&mut self, full: &str) -> String {
        let stable = trim_trailing_replacement(full);
        let raw_delta = match stable.strip_prefix(self.prev_stable.as_str()) {
            Some(rest) => rest,
            None => {
                // Pathologique (le décodage a changé avant la fin) : on
                // n'émet rien et on resynchronise sur le nouveau préfixe.
                self.prev_stable = stable.to_string();
                return String::new();
            }
        };
        let delta = normalize_display_delta(raw_delta);
        self.prev_stable = stable.to_string();
        delta
    }

    /// Décodage incrémental réel : `decode(ids)` puis delta (jamais de panic).
    fn push_token(&mut self, tokenizer: &tokenizers::Tokenizer, ids: &[u32]) -> String {
        match tokenizer.decode(ids, true) {
            Ok(full) => self.delta_for_full_decode(&full),
            Err(_) => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Résolution des chemins (workspace-root agnostique)
// ---------------------------------------------------------------------------

/// Cherche `models/` en remontant depuis le CWD (max 5 niveaux) + override
/// `HAKILLE_MODELS_DIR`. Retourne le dossier `models/` s'il contient (ou
/// peut contenir) les artefacts.
pub fn resolve_models_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("HAKILLE_MODELS_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }
    // Candidats directs (CWD = racine workspace dans 99% des cas).
    for cand in [
        PathBuf::from("models"),
        PathBuf::from("./models"),
        PathBuf::from("../models"),
        PathBuf::from("../../models"),
    ] {
        if cand.is_dir() {
            return cand;
        }
    }
    // Remontée générique (tests exécutés depuis crates/hakille-inference/, etc.).
    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..6 {
        let cand = cur.join("models");
        if cand.is_dir() {
            return cand;
        }
        if !cur.pop() {
            break;
        }
    }
    // Dernier recours : `models` relatif (l'erreur LOUD affichera l'absolu).
    PathBuf::from("models")
}

/// Chemin du GGUF (override `HAKILLE_QWEN_GGUF` prioritaire).
pub fn resolve_gguf_path() -> PathBuf {
    if let Ok(p) = std::env::var("HAKILLE_QWEN_GGUF") {
        return PathBuf::from(p);
    }
    resolve_models_dir().join(DEFAULT_GGUF_FILENAME)
}

/// Chemin du tokenizer (override `HAKILLE_QWEN_TOKENIZER` prioritaire).
pub fn resolve_tokenizer_path() -> PathBuf {
    if let Ok(p) = std::env::var("HAKILLE_QWEN_TOKENIZER") {
        return PathBuf::from(p);
    }
    resolve_models_dir().join(DEFAULT_TOKENIZER_FILENAME)
}

// ---------------------------------------------------------------------------
// Moteur Qwen réel
// ---------------------------------------------------------------------------

/// Moteur LLM Qwen 100% réel : forward quantifié candle sur CPU + BPE Qwen.
///
/// - `model` : poids Q4_K_M chargés via `gguf_file` (KV-cache interne).
/// - `tokenizer` : BPE Qwen2.5 (`tokenizer.json` HF, jamais synthétique).
/// - `device` : toujours `Cpu` en v1 — voir `SUPPORTED_BACKENDS`
///   (`"CPU only, CUDA/Metal planned"`) et `is_gpu_available() == false`.
///   CPU-only explicite, jamais assumé par défaut silencieux.
pub struct QwenEngine {
    model: ModelWeights,
    tokenizer: tokenizers::Tokenizer,
    device: Device,
    eos_im_end_id: u32,
    eos_text_id: u32,
    gguf_path: PathBuf,
    tokenizer_path: PathBuf,
}

// `ModelWeights` + `Tokenizer` sont `Send` (tensors + BPE) ; le singleton
// global exige `Send`. Vérification statique explicite.
fn _assert_send_sync() {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}
    is_send::<QwenEngine>();
    is_sync::<QwenEngine>();
}

impl QwenEngine {
    /// Formate un prompt ChatML Qwen2.5-Instruct (réellement envoyé au modèle).
    ///
    /// ```text
    /// <|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n
    /// ```
    pub fn format_chatml(system: &str, user: &str) -> String {
        format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
        )
    }

    /// Charge depuis les chemins par défaut (`models/*.gguf` + `tokenizer.json`).
    ///
    /// Erreurs LOUD si fichiers absents (avec URL HF + instructions).
    /// Ne tente PAS de téléchargement réseau en synchrone (risque de
    /// deadlock sous runtime Tokio) — voir `ensure_tokenizer_downloaded().await`.
    pub fn load_default() -> Result<Self> {
        Self::load_with_paths(resolve_gguf_path(), resolve_tokenizer_path())
    }

    /// Charge depuis des chemins explicites (tests, overrides).
    pub fn load_with_paths(
        gguf_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let gguf_path = gguf_path.as_ref().to_path_buf();
        let tokenizer_path = tokenizer_path.as_ref().to_path_buf();

        // 1. Fichiers présents ? (LOUD, pas de synthèse)
        if !gguf_path.is_file() {
            return Err(HakilleError::InferenceError(format!(
                "Qwen GGUF introuvable : '{}' (attendu : {} dans models/). \
                 Placez '{}' (491MB, {}) dans models/ ou définissez HAKILLE_QWEN_GGUF. \
                 AUCUNE génération synthétique — échec honnête.",
                gguf_path.display(),
                DEFAULT_GGUF_FILENAME,
                DEFAULT_GGUF_FILENAME,
                GGUF_HF_HINT
            )));
        }
        if !tokenizer_path.is_file() {
            return Err(HakilleError::InferenceError(format!(
                "Tokenizer Qwen introuvable : '{}' (attendu : {}). \
                 Téléchargez {} vers '{}' (ex: curl -L {} -o {}) ou définissez \
                 HAKILLE_QWEN_TOKENIZER. Sous réseau, appelez \
                 `QwenEngine::ensure_tokenizer_downloaded().await` avant `load_default()`. \
                 AUCUN tokenizer de secours — échec honnête.",
                tokenizer_path.display(),
                DEFAULT_TOKENIZER_FILENAME,
                TOKENIZER_HF_URL,
                tokenizer_path.display(),
                TOKENIZER_HF_URL,
                tokenizer_path.display()
            )));
        }

        // 2. Validation du header via le parseur existant (AUCUNE duplication
        //    du parsing binaire — on réutilise `crate::gguf_runtime`).
        let meta = crate::gguf_runtime::GgufModel::read_from_file(&gguf_path)?;
        let arch = meta.architecture().unwrap_or("?");
        if arch != "qwen2" {
            return Err(HakilleError::InferenceError(format!(
                "Architecture GGUF inattendue : '{}' (attendu 'qwen2') dans '{}'. \
                 Le moteur quantifié qwen2 ne peut pas décoder une autre arch.",
                arch,
                gguf_path.display()
            )));
        }
        if meta.tensor_count() < 280 {
            return Err(HakilleError::InferenceError(format!(
                "GGUF suspect : {} tenseurs (attendu ~291 pour Qwen2.5-0.5B) dans '{}'.",
                meta.tensor_count(),
                gguf_path.display()
            )));
        }
        tracing::info!(
            gguf = %gguf_path.display(),
            arch = %arch,
            tensors = meta.tensor_count(),
            ctx = meta.context_length().unwrap_or(0),
            "GGUF header validé via gguf_runtime (réutilisation, pas de duplication)"
        );

        // 3. Tokenizer BPE réel (jamais synthétique).
        let tokenizer =
            tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
                HakilleError::InferenceError(format!(
                    "Échec chargement tokenizer '{}' : {} (fichier corrompu ? re-téléchargez depuis {})",
                    tokenizer_path.display(),
                    e,
                    TOKENIZER_HF_URL
                ))
            })?;

        // 4. Modèle quantifié candle (lecture directe Q4_K_M, CPU v1 explicite).
        //    Backends : voir `SUPPORTED_BACKENDS` ("CPU only, CUDA/Metal planned").
        //    `is_gpu_available()` retourne `false` — aucun `Device::new_cuda(0)` /
        //    Metal ici (features `cuda`/`metal` = build C++/dylibs, hors scope v1
        //    pure-Rust). CPU-only assumé explicitement, jamais par défaut silencieux.
        //    Quand un backend GPU existera, il devra être sélectionné via
        //    `is_gpu_available()` (seul point de branchement autorisé).
        debug_assert!(
            !is_gpu_available(),
            "v1 CPU-only : is_gpu_available() doit rester false"
        );
        let device = Device::Cpu;
        let mut file = std::fs::File::open(&gguf_path).map_err(|e| {
            HakilleError::InferenceError(format!(
                "Ouverture GGUF '{}' : {}",
                gguf_path.display(),
                e
            ))
        })?;
        let content =
            gguf_file::Content::read(&mut file).map_err(|e| {
                HakilleError::InferenceError(format!(
                    "Lecture GGUF quantifié '{}' via candle (gguf_file) : {}. \
                     Si 'unsupported ggml dtype' : la BASCULE documentée est de déquantifier \
                     Q4_K_M→f32 vers un cache safetensors dans models/ puis forward f32 \
                     (lent mais RÉEL) — non nécessaire pour ce checkpoint (types 0/6/8/12/14 supportés).",
                    gguf_path.display(),
                    e
                ))
            })?;
        tracing::info!(
            tensors = content.tensor_infos.len(),
            "GGUF candle chargé (métadonnées quantifiées, Q4_K_M direct, CPU)"
        );
        let model = ModelWeights::from_gguf(content, &mut file, &device).map_err(|e| {
            HakilleError::InferenceError(format!(
                "Construction ModelWeights qwen2 depuis '{}' : {}. \
                 Vérifiez que le GGUF est bien qwen2.5-0.5b-instruct-q4_k_m (24 blocs, 896 hidden).",
                gguf_path.display(),
                e
            ))
        })?;

        // 5. IDs EOS (vocab réel, fallback sur IDs GGUF connus).
        let vocab = tokenizer.get_vocab(true);
        let eos_im_end_id = vocab
            .get(EOS_IM_END_STR)
            .copied()
            .unwrap_or(EOS_IM_END_ID_FALLBACK);
        let eos_text_id = vocab
            .get(EOS_TEXT_STR)
            .copied()
            .unwrap_or(EOS_TEXT_ID_FALLBACK);
        tracing::info!(
            im_end = eos_im_end_id,
            endoftext = eos_text_id,
            vocab = vocab.len(),
            "Tokenizer BPE Qwen chargé (vocab réel)"
        );

        Ok(Self {
            model,
            tokenizer,
            device,
            eos_im_end_id,
            eos_text_id,
            gguf_path,
            tokenizer_path,
        })
    }

    /// Télécharge `tokenizer.json` depuis HF si absent (async, réseau).
    ///
    /// À appeler avant `load_default()` quand le fichier manque. Si le
    /// réseau est indisponible, retourne une erreur LOUD (pas de silencieux).
    pub async fn ensure_tokenizer_downloaded() -> Result<PathBuf> {
        let path = resolve_tokenizer_path();
        if path.is_file() {
            return Ok(path);
        }
        tracing::info!(url = TOKENIZER_HF_URL, path = %path.display(), "Téléchargement tokenizer.json Qwen2.5-0.5B-Instruct…");
        let resp = reqwest::get(TOKENIZER_HF_URL).await.map_err(|e| {
            HakilleError::InferenceError(format!(
                "Téléchargement tokenizer impossible (réseau ?) depuis {} : {}. \
                 Téléchargez manuellement vers '{}'.",
                TOKENIZER_HF_URL,
                e,
                path.display()
            ))
        })?;
        if !resp.status().is_success() {
            return Err(HakilleError::InferenceError(format!(
                "HF a répondu {} pour {} — vérifiez l'URL ou téléchargez manuellement vers '{}'.",
                resp.status(),
                TOKENIZER_HF_URL,
                path.display()
            )));
        }
        let bytes = resp.bytes().await.map_err(|e| {
            HakilleError::InferenceError(format!("Lecture corps tokenizer.json : {}", e))
        })?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                HakilleError::InferenceError(format!(
                    "Création dossier models '{}' : {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        tokio::fs::write(&path, &bytes).await.map_err(|e| {
            HakilleError::InferenceError(format!("Écriture tokenizer '{}' : {}", path.display(), e))
        })?;
        tracing::info!(bytes = bytes.len(), path = %path.display(), "Tokenizer téléchargé");
        Ok(path)
    }

    /// Compte les tokens BPE réels d'un texte (remplace `len/4`).
    pub fn count_tokens(&self, text: &str) -> Result<usize> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| HakilleError::InferenceError(format!("Encode tokenizer : {}", e)))?;
        Ok(enc.get_ids().len())
    }

    /// Statut honnête : ce moteur est RÉEL (vrai forward neural).
    pub fn status(&self) -> crate::ModelStatus {
        crate::ModelStatus::Real
    }

    pub fn is_real(&self) -> bool {
        true
    }

    pub fn gguf_path(&self) -> &Path {
        &self.gguf_path
    }

    pub fn tokenizer_path(&self) -> &Path {
        &self.tokenizer_path
    }

    /// Génération réelle (prompt utilisateur brut + prompt système par défaut).
    ///
    /// - `prompt` : consigne utilisateur (ChatML appliqué en interne).
    /// - `max_tokens` : borne de décodage (hors prompt).
    /// - `temperature` : 0.0 = greedy explicite (`ArgMax`) ; >0 = sampling
    ///   multinomial réel. `<0` = erreur LOUD.
    /// - `top_p` : nucleus sampling réel quand `T>0` (doit être dans (0,1]).
    pub fn generate(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
    ) -> Result<GenerateOutput> {
        self.generate_with_system(prompt, None, max_tokens, temperature, top_p)
    }

    /// Génération réelle avec prompt système explicite (`None` = défaut Hakille).
    ///
    /// Streaming : si un callback global est enregistré (voir
    /// `set_token_stream_callback`), chaque token lui est également envoyé —
    /// le daemon obtient l'affichage live sans changer ses appelants.
    pub fn generate_with_system(
        &mut self,
        user_prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
    ) -> Result<GenerateOutput> {
        self.generate_core(
            user_prompt,
            system_prompt,
            max_tokens,
            temperature,
            top_p,
            &mut |_| {},
            None,
        )
    }

    /// Génération réelle avec **échéance dure** (deadline).
    ///
    /// Le deadline est vérifié avant le préfill et à chaque token poussé :
    /// à l'échéance, la génération **s'arrête** et renvoie une erreur
    /// explicite. C'est ce qui supprime le « zombie » : une génération qui
    /// continue de tourner après un timeout asyncio garde le verrou global
    /// du moteur et fait attendre toutes les requêtes suivantes.
    pub fn generate_with_deadline(
        &mut self,
        user_prompt: &str,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
        deadline: Option<Instant>,
    ) -> Result<GenerateOutput> {
        self.generate_core(
            user_prompt,
            None,
            max_tokens,
            temperature,
            top_p,
            &mut |_| {},
            deadline,
        )
    }

    /// Génération réelle avec callback token par token (streaming explicite).
    ///
    /// `on_token` est appelé PENDANT la génération, verrou moteur tenu : il ne
    /// doit JAMAIS rappeler le moteur (deadlock), ni toucher au registre
    /// global de streaming. Il doit rester rapide (chaque appel bloque le
    /// décodage). Un événement est émis pour CHAQUE token poussé, donc
    /// `nombre d'événements == GenerateOutput::tokens_generated`.
    /// `TokenEvent::text_delta` est un meilleur effort d'affichage :
    /// `GenerateOutput.text` (décodage one-shot final) reste la référence.
    pub fn generate_with_callback(
        &mut self,
        user_prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
        on_token: &mut dyn FnMut(&TokenEvent),
    ) -> Result<GenerateOutput> {
        self.generate_core(
            user_prompt,
            system_prompt,
            max_tokens,
            temperature,
            top_p,
            on_token,
            None,
        )
    }

    /// Cœur de génération partagé (validation + forward + décodage final).
    ///
    /// Le streaming est un MEILLEUR EFFORT : le texte FINAL reste décodé en
    /// one-shot (`tokenizer.decode` de toute la séquence), chemin inchangé.
    ///
    /// `deadline` : échéance dure. Vérifiée avant le préfill et à chaque
    /// token ; dépassée ⇒ erreur explicite (le verrou moteur est relâché
    /// normalement, aucune génération ne continue en arrière-plan).
    #[allow(clippy::too_many_arguments)]
    fn generate_core(
        &mut self,
        user_prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: usize,
        temperature: f64,
        top_p: f64,
        on_token: &mut dyn FnMut(&TokenEvent),
        deadline: Option<Instant>,
    ) -> Result<GenerateOutput> {
        if max_tokens == 0 {
            return Err(HakilleError::InferenceError(
                "max_tokens=0 : rien à générer (borne invalide)".into(),
            ));
        }
        if temperature < 0.0 {
            return Err(HakilleError::InferenceError(format!(
                "temperature={} invalide (attendu >= 0 ; 0 = greedy explicite)",
                temperature
            )));
        }
        if temperature > 0.0 && !(0.0 < top_p && top_p <= 1.0) {
            return Err(HakilleError::InferenceError(format!(
                "top_p={} invalide (attendu dans (0,1] quand T>0)",
                top_p
            )));
        }

        let system = system_prompt.unwrap_or(DEFAULT_SYSTEM_PROMPT);
        let formatted = Self::format_chatml(system, user_prompt);

        // Encodage BPE réel (tokens spéciaux ajoutés).
        let tokens = self
            .tokenizer
            .encode(formatted.clone(), true)
            .map_err(|e| HakilleError::InferenceError(format!("Encode ChatML : {}", e)))?;
        let prompt_ids: Vec<u32> = tokens.get_ids().to_vec();
        if prompt_ids.is_empty() {
            return Err(HakilleError::InferenceError(
                "Prompt ChatML encodé en 0 token (tokenizer vide ?)".into(),
            ));
        }
        // Garde contexte (32768 pour ce checkpoint) : erreur honnête si dépassé.
        if prompt_ids.len() + max_tokens > 32768 {
            return Err(HakilleError::InferenceError(format!(
                "Contexte dépassé : prompt {} + max {} > 32768 (qwen2.context_length). Réduisez le prompt ou max_tokens.",
                prompt_ids.len(),
                max_tokens
            )));
        }

        // Sampling RÉEL : seul T==0 est greedy (documenté, pas déguisé).
        // Seed OS par génération (nanos + pid) → multinomial non rejouable.
        let seed: u64 = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15);
            nanos ^ ((std::process::id() as u64).wrapping_mul(0x9E3779B97F4A7C15))
        };
        let sampling = if temperature <= 0.0 {
            Sampling::ArgMax
        } else if top_p >= 1.0 {
            Sampling::All { temperature }
        } else {
            Sampling::TopP {
                p: top_p,
                temperature,
            }
        };
        let mut logits_processor = LogitsProcessor::from_sampling(seed, sampling);

        let start = Instant::now();
        // KV-cache : reset entre conversations (pas de fuite inter-requêtes).
        self.model.clear_kv_cache();

        // Échéance dure : si elle est déjà dépassée, on ne démarre aucun
        // forward (pas de CPU gaspillé, erreur immédiate et honnête).
        if let Some(limit) = deadline
            && Instant::now() >= limit
        {
            return Err(HakilleError::InferenceError(format!(
                "échéance de génération dépassée avant le préfill ({} token(s) de prompt)",
                prompt_ids.len()
            )));
        }

        // Préfill : forward complet du prompt à pos 0 (seuls les logits du
        // dernier token sont utilisés, cf. `ModelWeights::forward`).
        let input = Tensor::new(prompt_ids.clone(), &self.device)
            .map_err(|e| HakilleError::InferenceError(format!("Tensor prompt : {}", e)))?;
        let input = input
            .unsqueeze(0)
            .map_err(|e| HakilleError::InferenceError(format!("Unsqueeze prompt : {}", e)))?;
        let logits = self.model.forward(&input, 0).map_err(|e| {
            HakilleError::InferenceError(format!(
                "Forward préfill (prompt {} tokens) : {}",
                prompt_ids.len(),
                e
            ))
        })?;
        let logits = logits
            .squeeze(0)
            .map_err(|e| HakilleError::InferenceError(format!("Squeeze logits préfill : {}", e)))?;
        let mut next_token = logits_processor
            .sample(&logits)
            .map_err(|e| HakilleError::InferenceError(format!("Sampling préfill : {}", e)))?;

        let mut generated: Vec<u32> = Vec::with_capacity(max_tokens);
        // Décodage incrémental d'affichage (best effort, jamais bloquant ;
        // le texte final reste décodé en one-shot plus bas).
        let mut stream_state = StreamDecodeState::default();
        // Boucle autoregressive : 1 token → forward(pos) → sample → stop EOS.
        for index in 0..max_tokens {
            // Échéance dure : on arrête ICI, pas après un timeout externe.
            // Sans ce garde-fou, le décodage continuerait en arrière-plan et
            // garderait le verrou moteur (requêtes suivantes bloquées).
            if let Some(limit) = deadline
                && Instant::now() >= limit
            {
                return Err(HakilleError::InferenceError(format!(
                    "échéance de génération dépassée après {} token(s) ({} ms écoulés)",
                    generated.len(),
                    start.elapsed().as_millis()
                )));
            }
            if next_token == self.eos_im_end_id || next_token == self.eos_text_id {
                break;
            }
            generated.push(next_token);
            // Streaming : un TokenEvent par token poussé (donc exactement
            // `tokens_generated` événements). Callback local d'abord, puis
            // callback global enregistré s'il existe (jamais de panic si le
            // registre est empoisonné).
            let event = TokenEvent {
                index: generated.len() - 1,
                token_id: next_token,
                text_delta: stream_state.push_token(&self.tokenizer, &generated),
            };
            on_token(&event);
            // Forme explicite verrou→callback (pas de let-chain) : jamais de
            // panic si le registre est empoisonné.
            #[allow(clippy::collapsible_if)]
            if let Ok(mut guard) = STREAM_CALLBACK.lock() {
                if let Some(cb) = guard.as_mut() {
                    cb(&event);
                }
            }
            if generated.len() >= max_tokens {
                break;
            }
            let input = Tensor::new(vec![next_token], &self.device).map_err(|e| {
                HakilleError::InferenceError(format!("Tensor step {} : {}", index, e))
            })?;
            let input = input.unsqueeze(0).map_err(|e| {
                HakilleError::InferenceError(format!("Unsqueeze step {} : {}", index, e))
            })?;
            let pos = prompt_ids.len() + index;
            let logits = self.model.forward(&input, pos).map_err(|e| {
                HakilleError::InferenceError(format!(
                    "Forward step {} (pos {}) : {}",
                    index, pos, e
                ))
            })?;
            let logits = logits.squeeze(0).map_err(|e| {
                HakilleError::InferenceError(format!("Squeeze step {} : {}", index, e))
            })?;
            next_token = logits_processor.sample(&logits).map_err(|e| {
                HakilleError::InferenceError(format!("Sampling step {} : {}", index, e))
            })?;
        }
        // Si le dernier `next_token` est EOS, on ne l'ajoute pas (déjà break).
        let latency_ms = start.elapsed().as_millis() as u64;

        // Décodage BPE réel (tokens spéciaux sautés, jamais de gabarit).
        let text = if generated.is_empty() {
            tracing::warn!(
                prompt_len = prompt_ids.len(),
                latency_ms,
                "Le modèle n'a rien généré (EOS immédiat) — retour vide honnête, aucune phrase inventée"
            );
            String::new()
        } else {
            self.tokenizer
                .decode(&generated, true)
                .map_err(|e| HakilleError::InferenceError(format!("Decode BPE : {}", e)))?
                .trim()
                .to_string()
        };

        tracing::info!(
            prompt_tokens = prompt_ids.len(),
            generated = generated.len(),
            latency_ms,
            temp = temperature,
            top_p = top_p,
            empty = text.is_empty(),
            "Génération Qwen RÉELLE (forward quantifié CPU + KV-cache)"
        );

        Ok(GenerateOutput {
            text,
            tokens_generated: generated.len(),
            prompt_tokens: prompt_ids.len(),
            latency_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Singleton paresseux (lazy, thread-safe, honnête)
// ---------------------------------------------------------------------------

static GLOBAL_ENGINE: std::sync::OnceLock<std::sync::Mutex<QwenEngine>> =
    std::sync::OnceLock::new();

/// Retourne le moteur global (chargé une fois, réutilisé ensuite).
///
/// - Premier appel : `QwenEngine::load_default()` (peut prendre ~10-60s :
///   lecture GGUF 491MB + construction des 24 blocs quantifiés).
/// - Appels suivants : verrou sur l'instance existante (KV-cache reset à
///   chaque `generate`, donc pas de fuite entre requêtes).
/// - Si le checkpoint/tokenizer manque : `Err` LOUD (jamais de fallback
///   synthétique). En cas de course d'initialisation, le perdant est
///   abandonné (double chargement ponctuel, résultat identique).
pub fn global_qwen_engine() -> Result<std::sync::MutexGuard<'static, QwenEngine>> {
    if let Some(m) = GLOBAL_ENGINE.get() {
        return m.lock().map_err(|e| {
            HakilleError::InferenceError(format!("Mutex moteur Qwen empoisonné : {}", e))
        });
    }
    let engine = QwenEngine::load_default()?;
    let _ = GLOBAL_ENGINE.set(std::sync::Mutex::new(engine));
    GLOBAL_ENGINE.get().unwrap().lock().map_err(|e| {
        HakilleError::InferenceError(format!("Mutex moteur Qwen empoisonné après init : {}", e))
    })
}

// ---------------------------------------------------------------------------
// Golden tests (vrai GGUF 491MB, CPU — prévoir 2-5 min pour quelques tokens)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod llm_engine_tests {
    use super::*;

    #[test]
    fn test_supported_backends_cpu_only_explicite() {
        // Honnêteté GPU : CPU-only assumé explicitement, jamais par défaut silencieux.
        assert_eq!(SUPPORTED_BACKENDS, "CPU only, CUDA/Metal planned");
        assert!(
            SUPPORTED_BACKENDS.contains("CPU only"),
            "SUPPORTED_BACKENDS doit déclarer CPU only, got: {}",
            SUPPORTED_BACKENDS
        );
        assert!(
            SUPPORTED_BACKENDS.contains("CUDA/Metal planned"),
            "SUPPORTED_BACKENDS doit annoncer CUDA/Metal planned, got: {}",
            SUPPORTED_BACKENDS
        );
        assert!(
            !is_gpu_available(),
            "v1 : is_gpu_available() doit retourner false (aucun backend GPU)"
        );
    }

    #[test]
    fn test_chatml_contains_im_start() {
        let s = QwenEngine::format_chatml("sys-prompt", "hello");
        assert!(
            s.contains("<|im_start|>"),
            "ChatML doit contenir <|im_start|>, got: {}",
            s
        );
        assert!(s.contains("<|im_end|>"), "ChatML doit contenir <|im_end|>");
        assert!(s.contains("sys-prompt"));
        assert!(s.contains("hello"));
        assert!(s.contains("assistant"));
    }

    #[test]
    fn test_tokenizer_roundtrip_real() {
        let tok_path = resolve_tokenizer_path();
        if !tok_path.is_file() {
            eprintln!(
                "SKIP: tokenizer absent : '{}'. Téléchargez {} vers '{}' (voir TOKENIZER_HF_URL) ou définissez HAKILLE_QWEN_TOKENIZER.",
                tok_path.display(),
                TOKENIZER_HF_URL,
                tok_path.display()
            );
            return;
        }
        let tok = tokenizers::Tokenizer::from_file(&tok_path)
            .expect("tokenizer.json corrompu — re-téléchargez depuis HF");
        let text = "Bonjour Hakille";
        let enc = tok.encode(text, true).expect("encode");
        let ids = enc.get_ids();
        assert!(!ids.is_empty(), "L'encodage BPE ne doit pas être vide");
        let dec = tok.decode(ids, true).expect("decode");
        assert!(
            !dec.trim().is_empty(),
            "Le décodage BPE ne doit pas être vide"
        );
        // Roundtrip : le texte décodé doit contenir la racine (BPE ≠ byte-identique à cause du ▁ initial).
        assert!(
            dec.contains("Bonjour"),
            "Roundtrip BPE : attendu 'Bonjour' dans '{}'",
            dec
        );
        // IDs EOS connus présents dans le vocab ?
        let vocab = tok.get_vocab(true);
        assert_eq!(
            vocab.get(EOS_IM_END_STR),
            Some(&EOS_IM_END_ID_FALLBACK),
            "Le vocab doit contenir <|im_end|>=151645"
        );
    }

    /// GOLDEN TEST — vrai décodage autoregressif depuis le GGUF 491MB.
    /// Prompt court "2+2=", max 8 tokens, greedy (T=0, déterministe).
    /// Assert : non-vide, tokens>0, latence loggée. Prévoir 2-5 min CPU.
    #[test]
    fn test_qwen_real_generation_tiny() {
        let gguf = resolve_gguf_path();
        let tok = resolve_tokenizer_path();
        if !gguf.is_file() {
            eprintln!(
                "SKIP: GGUF absent : '{}'. Placez {} dans models/ (voir GGUF_HF_HINT).",
                gguf.display(),
                DEFAULT_GGUF_FILENAME
            );
            return;
        }
        if !tok.is_file() {
            eprintln!(
                "SKIP: tokenizer absent : '{}'. Téléchargez {} (voir TOKENIZER_HF_URL).",
                tok.display(),
                TOKENIZER_HF_URL
            );
            return;
        }
        let mut engine = QwenEngine::load_with_paths(&gguf, &tok)
            .expect("Chargement moteur réel (GGUF+tokenizer)");
        assert!(engine.is_real());
        assert_eq!(engine.status(), crate::ModelStatus::Real);

        // Streaming sur la MÊME génération (zéro coût supplémentaire) :
        // compteur d'événements + concaténation des deltas affichables.
        let mut events: Vec<TokenEvent> = Vec::new();
        let out = engine
            .generate_with_callback("2+2=", None, 8, 0.0, 1.0, &mut |ev: &TokenEvent| {
                events.push(ev.clone());
            })
            .expect("Génération réelle (forward quantifié CPU)");
        // Preuves exigées : texte réel, nb tokens, latence.
        eprintln!(
            "[GOLDEN] prompt='2+2=' text={:?} tokens_generated={} prompt_tokens={} latency_ms={}",
            out.text, out.tokens_generated, out.prompt_tokens, out.latency_ms
        );
        tracing::info!(
            text = %out.text,
            tokens = out.tokens_generated,
            latency_ms = out.latency_ms,
            "Preuve génération réelle (golden test)"
        );
        assert!(out.tokens_generated > 0, "tokens_generated doit être > 0");
        assert!(
            !out.text.is_empty(),
            "Le modèle réel doit générer un texte non-vide pour '2+2=' (vide = échec honnête, pas de gabarit)"
        );
        assert!(
            out.prompt_tokens > 0,
            "prompt_tokens (tokenizer réel) doit être > 0"
        );
        // Latence loggée (toujours présente, même rapide en CI chaude).
        assert!(
            out.latency_ms > 0 || out.tokens_generated > 0,
            "latence loggée"
        );

        // Contrat de streaming : un événement par token poussé, indices
        // 0-based croissants, deltas affichables non vides quand le texte
        // final l'est (le texte final, lui, reste le décodage one-shot).
        assert_eq!(
            events.len(),
            out.tokens_generated,
            "nombre d'événements == tokens_generated"
        );
        assert!(!events.is_empty(), "au moins un TokenEvent émis");
        assert!(
            events.iter().enumerate().all(|(i, ev)| ev.index == i),
            "indices de streaming 0-based et croissants"
        );
        let streamed: String = events.iter().map(|ev| ev.text_delta.as_str()).collect();
        if !out.text.is_empty() {
            assert!(
                !streamed.trim().is_empty(),
                "la concaténation des deltas non vides doit refléter le texte final non vide, got: {:?} (final: {:?})",
                streamed,
                out.text
            );
        }
    }

    #[test]
    fn test_normalize_display_delta_sentencepiece_marker() {
        // `▁` (marqueur SentencePiece début de mot) → espace, affichage seul.
        assert_eq!(normalize_display_delta("\u{2581}Bonjour"), " Bonjour");
        assert_eq!(normalize_display_delta("\u{2581}Bon"), " Bon");
        assert_eq!(normalize_display_delta("jour"), "jour");
        assert_eq!(normalize_display_delta(""), "");
        assert_eq!(normalize_display_delta("a\u{2581}b\u{2581}c"), "a b c");
    }

    #[test]
    fn test_trim_trailing_replacement_utf8_partiel() {
        // Les U+FFFD finaux (octet UTF-8 coupé entre 2 tokens) sont retenus.
        assert_eq!(trim_trailing_replacement("Bon"), "Bon");
        assert_eq!(trim_trailing_replacement("Bon\u{FFFD}"), "Bon");
        assert_eq!(trim_trailing_replacement("Bon\u{FFFD}\u{FFFD}"), "Bon");
        assert_eq!(trim_trailing_replacement("\u{FFFD}"), "");
        assert_eq!(trim_trailing_replacement("é"), "é");
        assert_eq!(trim_trailing_replacement("é\u{FFFD}x"), "é\u{FFFD}x");
    }

    #[test]
    fn test_stream_decode_state_delta_and_partial_utf8_report() {
        // Sans modèle : logique pure du décodage incrémental d'affichage.
        let mut state = StreamDecodeState::default();
        assert_eq!(state.delta_for_full_decode("Bon"), "Bon");
        assert_eq!(state.delta_for_full_decode("Bonjour"), "jour");
        // U+FFFD final retenu : rien de nouveau tant que non résolu…
        assert_eq!(state.delta_for_full_decode("Bonjour\u{FFFD}"), "");
        // …puis reporté dès que le token suivant le résout.
        assert_eq!(state.delta_for_full_decode("Bonjour é"), " é");
        // Décodage identique (token spécial sauté) : delta vide, pas d'erreur.
        assert_eq!(state.delta_for_full_decode("Bonjour é"), "");
        // Cas pathologique (préfixe inattendu) : delta vide + resynchronisation.
        assert_eq!(state.delta_for_full_decode("X"), "");
        assert_eq!(state.delta_for_full_decode("Xy"), "y");
    }

    #[test]
    fn test_stream_decode_state_normalise_marqueur_dans_delta() {
        let mut state = StreamDecodeState::default();
        assert_eq!(state.delta_for_full_decode("\u{2581}Bon"), " Bon");
        // Seul LE DELTA est normalisé : le marqueur initial n'est pas réémis.
        assert_eq!(state.delta_for_full_decode("\u{2581}Bonjour"), "jour");
    }
}
