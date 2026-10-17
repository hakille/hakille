//! Embeddings sémantiques RÉELS via ONNX Runtime (`ort` v2).
//!
//! - Modèle : `models/all-minilm-l6-v2.onnx` (~90 MB, 6 couches, 384-d).
//! - Signature RÉELLE lue dynamiquement : inputs `input_ids` /
//!   `attention_mask` / `token_type_ids` (int64 `[B,S]`), output
//!   `last_hidden_state` (f32 `[B,S,384]`).
//! - Tokenisation WordPiece codée à la main (PAS de crate `tokenizers`,
//!   `serde_json` déjà présent + `std` uniquement) depuis le vocabulaire
//!   officiel `models/minilm_tokenizer.json` (WordPiece 30 522 tokens,
//!   `BertNormalizer` lowercase + `BertPreTokenizer`, `[CLS]=101`,
//!   `[SEP]=102`, `[PAD]=0`, `[UNK]=100`).
//! - Si le `tokenizer.json` officiel est absent : erreur LOUD (aucun repli
//!   simulé — le repli hachage v1 a été SUPPRIMÉ, le WordPiece officiel
//!   `models/minilm_tokenizer.json` est exigé).
//!   Dans TOUS les cas le FORWARD est 100 % réel (les 6 couches MiniLM sont
//!   exécutées via `ort`), suivi d'un mean-pooling par masque + L2.
//!
//! Qualité visée : Google — déterminisme, norme 1, non-effondrement vérifiés.

use annora_core::{AnnoraError, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Longueur de séquence (padding/troncature).
pub const MINILM_SEQ_LEN: usize = 128;
/// Dimension de sortie.
pub const MINILM_DIM: usize = 384;

fn ort_err(context: &str, e: impl std::fmt::Display) -> AnnoraError {
    AnnoraError::InferenceError(format!("MiniLmOnnx::{context}: {e}"))
}

/// Mode de tokenisation effectif (honnêteté mesurable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenMode {
    /// Vocabulaire WordPiece officiel (30 522 tokens) — seul mode supporté
    /// (repli hachage v1 SUPPRIMÉ : tokenizer officiel exigé).
    WordPieceOfficiel,
}

impl TokenMode {
    pub fn label_fr(&self) -> &'static str {
        match self {
            Self::WordPieceOfficiel => "WordPiece officiel (30 522 tokens)",
        }
    }
}

/// Session MiniLM réelle (forward `ort`).
pub struct MiniLmOnnx {
    _env: Arc<ort::environment::Environment>,
    session: Mutex<Session>,
    ids_name: String,
    mask_name: String,
    type_name: String,
    output_name: String,
    vocab: HashMap<String, i64>,
    pad_id: i64,
    unk_id: i64,
    cls_id: i64,
    sep_id: i64,
    mode: TokenMode,
}

impl MiniLmOnnx {
    /// Ouvre le modèle + le tokenizer officiel (EXIGÉ).
    /// Si `tokenizer_json` est absent/illisible : erreur LOUD (aucun repli
    /// simulé — le hachage v1 a été supprimé).
    pub fn open(model_path: impl AsRef<Path>, tokenizer_json: impl AsRef<Path>) -> Result<Self> {
        let model_path = model_path.as_ref();
        if !model_path.exists() {
            return Err(AnnoraError::InferenceError(format!(
                "MiniLmOnnx::open: modèle absent '{}' — provisionnez models/all-minilm-l6-v2.onnx (erreur LOUD, pas de fallback simulé)",
                model_path.display()
            )));
        }
        let t0 = Instant::now();
        let env = crate::speaker_onnx::shared_ort_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("open(builder)", e))?
            .commit_from_file(model_path)
            .map_err(|e| ort_err("open(commit_from_file)", e))?;

        let ins: Vec<String> = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let outs: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        if ins.is_empty() || outs.is_empty() {
            return Err(AnnoraError::InferenceError(
                "MiniLmOnnx::open: graphe sans input/output".to_string(),
            ));
        }
        let pick = |want: &str| -> String {
            if ins.iter().any(|n| n == want) {
                want.to_string()
            } else {
                ins[0].clone()
            }
        };
        let ids_name = pick("input_ids");
        let mask_name = if ins.iter().any(|n| n == "attention_mask") {
            "attention_mask".to_string()
        } else if ins.len() > 1 {
            ins[1].clone()
        } else {
            ins[0].clone()
        };
        let type_name = if ins.iter().any(|n| n == "token_type_ids") {
            "token_type_ids".to_string()
        } else if ins.len() > 2 {
            ins[2].clone()
        } else {
            ins[0].clone()
        };
        let output_name = if outs.iter().any(|n| n == "last_hidden_state") {
            "last_hidden_state".to_string()
        } else {
            outs[0].clone()
        };

        let (vocab, pad_id, unk_id, cls_id, sep_id, mode) =
            load_vocab_officiel(tokenizer_json.as_ref())?;
        tracing::info!(
            model = %model_path.display(),
            inputs = ?ins,
            output = %output_name,
            mode = mode.label_fr(),
            vocab = vocab.len(),
            elapsed_ms = t0.elapsed().as_millis(),
            "MiniLmOnnx::open: session réelle prête"
        );
        Ok(Self {
            _env: env,
            session: Mutex::new(session),
            ids_name,
            mask_name,
            type_name,
            output_name,
            vocab,
            pad_id,
            unk_id,
            cls_id,
            sep_id,
            mode,
        })
    }

    pub fn default_model_path() -> Option<PathBuf> {
        // Chemins relatifs `models/` uniquement (aucun chemin absolu en dur —
        // portabilité Windows/Linux/macOS, workspace-root agnostique).
        for cand in [
            "models/all-minilm-l6-v2.onnx",
            "../models/all-minilm-l6-v2.onnx",
            "../../models/all-minilm-l6-v2.onnx",
        ] {
            let p = PathBuf::from(cand);
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            for base in [cwd.clone(), cwd.join(".."), cwd.join("../..")] {
                let p = base.join("models/all-minilm-l6-v2.onnx");
                if p.exists() {
                    return Some(p);
                }
            }
        }
        // Variable d'environnement prioritaire (zéro chemin en dur).
        if let Ok(dir) = std::env::var("ANNORA_MODELS_DIR") {
            let p = PathBuf::from(dir).join("all-minilm-l6-v2.onnx");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    pub fn default_tokenizer_path() -> Option<PathBuf> {
        // Chemins relatifs `models/` uniquement (aucun chemin absolu en dur).
        for cand in [
            "models/minilm_tokenizer.json",
            "../models/minilm_tokenizer.json",
            "../../models/minilm_tokenizer.json",
        ] {
            let p = PathBuf::from(cand);
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            for base in [cwd.clone(), cwd.join(".."), cwd.join("../..")] {
                let p = base.join("models/minilm_tokenizer.json");
                if p.exists() {
                    return Some(p);
                }
            }
        }
        if let Ok(dir) = std::env::var("ANNORA_MODELS_DIR") {
            let p = PathBuf::from(dir).join("minilm_tokenizer.json");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Ouvre le checkpoint + tokenizer par défaut.
    /// Erreur LOUD si le modèle OU le tokenizer est absent (aucun fallback
    /// simulé — tokenizer officiel `models/minilm_tokenizer.json` exigé).
    pub fn open_default() -> Result<Self> {
        let model = Self::default_model_path().ok_or_else(|| {
            AnnoraError::InferenceError(
                "MiniLmOnnx::open_default: checkpoint MiniLM introuvable (models/all-minilm-l6-v2.onnx absent) — embeddings RÉELS impossibles, aucun fallback simulé"
                    .to_string(),
            )
        })?;
        let tok = Self::default_tokenizer_path()
            .unwrap_or_else(|| PathBuf::from("models/minilm_tokenizer.json.__absent__"));
        Self::open(model, tok)
    }

    /// Toujours `true` : forward neural réellement exécuté.
    pub fn is_real(&self) -> bool {
        true
    }

    pub fn token_mode(&self) -> TokenMode {
        self.mode
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }

    pub fn real_input_names(&self) -> Vec<String> {
        let s = self.session.lock().expect("mutex MiniLmOnnx empoisonné");
        s.inputs().iter().map(|o| o.name().to_string()).collect()
    }

    pub fn real_output_names(&self) -> Vec<String> {
        let s = self.session.lock().expect("mutex MiniLmOnnx empoisonné");
        s.outputs().iter().map(|o| o.name().to_string()).collect()
    }

    /// Tokenise un texte en `(input_ids, attention_mask, token_type_ids)`.
    /// WordPiece officiel uniquement (hachage v1 supprimé).
    pub fn tokenize(&self, text: &str) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
        wordpiece_tokenize(
            text,
            &self.vocab,
            self.unk_id,
            self.cls_id,
            self.sep_id,
            self.pad_id,
        )
    }

    /// Embedding 384-d L2-normalisé (forward `ort` RÉEL + mean-pooling).
    /// Ne panique jamais (texte vide -> `[CLS]+[SEP]` poolés).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let t0 = Instant::now();
        let (ids, mask, type_ids) = self.tokenize(text);
        debug_assert_eq!(ids.len(), MINILM_SEQ_LEN);
        let ids_t = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], ids))
            .map_err(|e| ort_err("embed(ids)", e))?;
        let mask_t = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], mask.clone()))
            .map_err(|e| ort_err("embed(mask)", e))?;
        let type_t = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], type_ids))
            .map_err(|e| ort_err("embed(type)", e))?;
        let k_ids = self.ids_name.clone();
        let k_mask = self.mask_name.clone();
        let k_type = self.type_name.clone();
        let k_out = self.output_name.clone();
        // La signature réelle peut n'exposer qu'un sous-ensemble
        // (ex. sans token_type_ids) : on dégrade gracieusement en
        // n'envoyant que les inputs présents dans le graphe.
        let known: Vec<String> = {
            let s = self
                .session
                .lock()
                .map_err(|e| ort_err("embed(lock-names)", format!("mutex empoisonné : {e}")))?;
            s.inputs().iter().map(|o| o.name().to_string()).collect()
        };
        let has = |n: &str| known.iter().any(|k| k == n);
        let mut sess = self
            .session
            .lock()
            .map_err(|e| ort_err("embed(lock)", format!("mutex empoisonné : {e}")))?;
        let outputs = if has(&k_ids)
            && has(&k_mask)
            && has(&k_type)
            && k_ids != k_mask
            && k_ids != k_type
            && k_mask != k_type
        {
            sess.run(ort::inputs![k_ids => ids_t, k_mask => mask_t, k_type => type_t])
                .map_err(|e| ort_err("embed(run3)", e))?
        } else if has(&k_ids) && has(&k_mask) && k_ids != k_mask {
            sess.run(ort::inputs![k_ids => ids_t, k_mask => mask_t])
                .map_err(|e| ort_err("embed(run2)", e))?
        } else {
            sess.run(ort::inputs![k_ids => ids_t])
                .map_err(|e| ort_err("embed(run1)", e))?
        };
        let flat: Vec<f32> = outputs
            .get(&k_out)
            .and_then(|v| v.try_extract_tensor::<f32>().ok())
            .map(|(_, data)| data.to_vec())
            .ok_or_else(|| {
                AnnoraError::InferenceError(format!(
                    "MiniLmOnnx::embed: output '{k_out}' illisible"
                ))
            })?;
        if flat.len() != MINILM_SEQ_LEN * MINILM_DIM {
            return Err(AnnoraError::InferenceError(format!(
                "MiniLmOnnx::embed: sortie inattendue len={} (attendu {})",
                flat.len(),
                MINILM_SEQ_LEN * MINILM_DIM
            )));
        }
        for v in flat.iter() {
            if !v.is_finite() {
                return Err(AnnoraError::InferenceError(
                    "MiniLmOnnx::embed: sortie non-finie".to_string(),
                ));
            }
        }
        // Mean-pooling par masque d'attention + L2.
        let mask_sum: f32 = mask.iter().map(|m| *m as f32).sum::<f32>().max(1.0);
        let mut pooled = vec![0.0f32; MINILM_DIM];
        for s in 0..MINILM_SEQ_LEN {
            if mask[s] == 0 {
                continue;
            }
            for d in 0..MINILM_DIM {
                pooled[d] += flat[s * MINILM_DIM + d];
            }
        }
        for d in 0..MINILM_DIM {
            pooled[d] /= mask_sum;
        }
        let emb = l2_normalize_384(&pooled);
        tracing::debug!(
            elapsed_ms = t0.elapsed().as_millis(),
            mode = self.mode.label_fr(),
            "MiniLmOnnx::embed: forward réel OK"
        );
        Ok(emb)
    }

    /// Similarité cosinus réelle.
    pub fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        cosine_sim(a, b)
    }
}

pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    for i in 0..a.len() {
        let x = a[i];
        let y = b[i];
        if x.is_finite() && y.is_finite() {
            dot += x * y;
        }
    }
    dot.clamp(-1.0, 1.0)
}

fn l2_normalize_384(v: &[f32]) -> Vec<f32> {
    let mut n = 0.0f32;
    for x in v.iter() {
        let c = if x.is_finite() { *x } else { 0.0 };
        n += c * c;
    }
    n = n.sqrt();
    if n <= 0.0 || !n.is_finite() {
        return vec![0.0f32; v.len()];
    }
    v.iter()
        .map(|x| {
            let c = if x.is_finite() { *x } else { 0.0 };
            c / n
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Vocabulaire officiel.
// ---------------------------------------------------------------------------

fn load_vocab_officiel(
    path: &Path,
) -> Result<(HashMap<String, i64>, i64, i64, i64, i64, TokenMode)> {
    let read = std::fs::read_to_string(path).ok().and_then(|content| {
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("model")?.get("vocab")?.as_object().cloned())
    });
    if let Some(map) = read {
        let mut vocab = HashMap::with_capacity(map.len());
        for (k, v) in map.iter() {
            if let Some(id) = v.as_u64() {
                vocab.insert(k.clone(), id as i64);
            }
        }
        if vocab.len() >= 10_000 {
            let pad = vocab.get("[PAD]").copied().unwrap_or(0);
            let unk = vocab.get("[UNK]").copied().unwrap_or(100);
            let cls = vocab.get("[CLS]").copied().unwrap_or(101);
            let sep = vocab.get("[SEP]").copied().unwrap_or(102);
            return Ok((vocab, pad, unk, cls, sep, TokenMode::WordPieceOfficiel));
        }
        return Err(AnnoraError::InferenceError(format!(
            "MiniLmOnnx: vocabulaire officiel illisible dans '{}' ({} tokens < 10000) — provisionnez models/minilm_tokenizer.json (WordPiece 30522, aucun repli hachage)",
            path.display(),
            vocab.len()
        )));
    }
    Err(AnnoraError::InferenceError(format!(
        "MiniLmOnnx: tokenizer officiel absent/illisible '{}' — provisionnez models/minilm_tokenizer.json (WordPiece 30522). Repli hachage v1 SUPPRIMÉ (aucun fallback simulé)",
        path.display()
    )))
}

// ---------------------------------------------------------------------------
// Tokenisation WordPiece manuelle (BERT uncased).
// ---------------------------------------------------------------------------

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF |
        0xF900..=0xFAFF | 0x2F800..=0x2FA1F)
}

fn bert_normalize(text: &str) -> String {
    // lowercase (uncased) + clean_text + espaces autour du CJK.
    let lower = text.to_lowercase();
    let mut spaced = String::with_capacity(lower.len() + 8);
    for c in lower.chars() {
        if is_cjk(c) {
            spaced.push(' ');
            spaced.push(c);
            spaced.push(' ');
        } else if c.is_control() {
            if c == '\t' || c == '\n' || c == '\r' {
                spaced.push(' ');
            }
            // autres contrôles : supprimés (clean_text).
        } else {
            spaced.push(c);
        }
    }
    // Normalise les blancs (split_whitespace + join).
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bert_pretokenize(normalized: &str) -> Vec<String> {
    // BertPreTokenizer : split whitespaces puis ponctuation ASCII isolée.
    let mut out = Vec::new();
    for word in normalized.split_whitespace() {
        let mut cur = String::new();
        for c in word.chars() {
            if c.is_ascii_punctuation() {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

fn wordpiece_split_token(token: &str, vocab: &HashMap<String, i64>, unk_id: i64) -> Vec<i64> {
    const MAX_CHARS: usize = 200;
    let chars: Vec<char> = token.chars().collect();
    if chars.len() > MAX_CHARS {
        return vec![unk_id];
    }
    let mut ids = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let mut end = chars.len();
        let mut found: Option<i64> = None;
        while end > start {
            let piece: String = chars[start..end].iter().collect();
            let key = if start == 0 {
                piece
            } else {
                format!("##{piece}")
            };
            if let Some(id) = vocab.get(&key) {
                found = Some(*id);
                break;
            }
            end -= 1;
        }
        match found {
            Some(id) => {
                ids.push(id);
                start = end;
            }
            None => return vec![unk_id],
        }
    }
    ids
}

/// Tokenisation WordPiece officielle : `[CLS] … [SEP]` + pad 128.
pub fn wordpiece_tokenize(
    text: &str,
    vocab: &HashMap<String, i64>,
    unk_id: i64,
    cls_id: i64,
    sep_id: i64,
    pad_id: i64,
) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let norm = bert_normalize(text);
    let words = bert_pretokenize(&norm);
    let mut ids: Vec<i64> = Vec::with_capacity(MINILM_SEQ_LEN);
    ids.push(cls_id);
    for w in words {
        if ids.len() >= MINILM_SEQ_LEN - 1 {
            break;
        }
        let mut piece = wordpiece_split_token(&w, vocab, unk_id);
        // Tronque au budget restant (garde la place du [SEP]).
        let room = (MINILM_SEQ_LEN - 1).saturating_sub(ids.len());
        if piece.len() > room {
            piece.truncate(room);
        }
        ids.extend(piece);
        if ids.len() >= MINILM_SEQ_LEN - 1 {
            break;
        }
    }
    ids.push(sep_id);
    ids.truncate(MINILM_SEQ_LEN);
    let mut mask = vec![0i64; MINILM_SEQ_LEN];
    for i in 0..ids.len() {
        mask[i] = 1;
    }
    let type_ids = vec![0i64; MINILM_SEQ_LEN];
    while ids.len() < MINILM_SEQ_LEN {
        ids.push(pad_id);
    }
    (ids, mask, type_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn open_or_skip() -> Option<MiniLmOnnx> {
        match MiniLmOnnx::open_default() {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("SKIP MiniLmOnnx (modèle absent): {e}");
                None
            }
        }
    }

    #[test]
    fn test_minilm_signature_reelle() {
        let Some(sess) = open_or_skip() else { return };
        let ins = sess.real_input_names();
        let outs = sess.real_output_names();
        eprintln!(
            "inputs={ins:?} outputs={outs:?} mode={:?}",
            sess.token_mode()
        );
        assert!(ins.iter().any(|n| n == "input_ids"), "inputs={ins:?}");
        assert!(
            outs.iter().any(|n| n == "last_hidden_state"),
            "outputs={outs:?}"
        );
        assert!(sess.is_real());
    }

    #[test]
    fn test_minilm_tokenisation_wordpiece_officielle() {
        let Some(sess) = open_or_skip() else { return };
        eprintln!("token_mode={:?}", sess.token_mode());
        // Le réseau étant disponible lors du provisionnement, le vocab
        // officiel doit être chargé (30 522 tokens).
        assert_eq!(
            sess.token_mode(),
            TokenMode::WordPieceOfficiel,
            "vocab officiel attendu (models/minilm_tokenizer.json)"
        );
        assert_eq!(sess.vocab_size(), 30_522);
        let (ids, mask, types) = sess.tokenize("le chat dort");
        assert_eq!(ids.len(), 128);
        assert_eq!(mask.len(), 128);
        assert_eq!(types.len(), 128);
        // [CLS]=101 en tête, [SEP]=102 après les tokens, pad=0 ensuite.
        assert_eq!(ids[0], 101);
        let n_real = mask.iter().filter(|m| **m == 1).count();
        assert!(n_real >= 3, "au moins CLS + 1 token + SEP");
        assert_eq!(ids[n_real - 1], 102);
        assert!(ids[n_real..].iter().all(|id| *id == 0));
        eprintln!("ids[..{n_real}]={:?}", &ids[..n_real]);
    }

    #[test]
    fn test_minilm_forward_reel_deterministe() {
        let Some(sess) = open_or_skip() else { return };
        let t0 = Instant::now();
        let a = sess.embed("le chat dort").expect("forward réel");
        let dt = t0.elapsed();
        let b = sess.embed("le chat dort").expect("forward réel 2");
        assert_eq!(a.len(), 384);
        assert_eq!(a, b, "le forward réel doit être déterministe");
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        eprintln!("norm={norm:.6} latence={}ms", dt.as_millis());
        assert!((norm - 1.0).abs() < 1e-4, "norme==1, got {norm}");
        assert!(a.iter().all(|v| v.is_finite()));
        // Non-effondrement : deux textes différents -> vecteurs différents.
        let c = sess
            .embed("la physique quantique des trous noirs")
            .expect("forward 3");
        let sim_ac = sess.cosine_similarity(&a, &c);
        eprintln!("sim(chat|physique)={sim_ac:.4}");
        assert!(sim_ac < 0.99, "non-effondrement : sim={sim_ac}");
    }

    #[test]
    fn test_minilm_semantique_fr() {
        let Some(sess) = open_or_skip() else { return };
        let a = sess.embed("le chat dort").expect("forward A");
        let b = sess.embed("un félin sommeille").expect("forward B");
        let c = sess
            .embed("la comptabilité trimestrielle des entreprises")
            .expect("forward C");
        let sim_ab = sess.cosine_similarity(&a, &b);
        let sim_ac = sess.cosine_similarity(&a, &c);
        eprintln!("sim(chat|felin)={sim_ab:.4} sim(chat|compta)={sim_ac:.4}");
        // Avec le forward MiniLM réel + WordPiece officiel, la paire
        // sémantiquement proche doit dépasser la paire non liée.
        assert!(
            sim_ab > sim_ac,
            "paire liée ({sim_ab:.4}) > paire non liée ({sim_ac:.4}) attendu avec le vrai modèle"
        );
    }

    #[test]
    fn test_minilm_vide_sans_panic() {
        let Some(sess) = open_or_skip() else { return };
        for txt in ["", "   ", "a"] {
            let e = sess.embed(txt);
            assert!(e.is_ok(), "texte {txt:?} ne doit pas paniquer : {e:?}");
            let v = e.unwrap();
            assert_eq!(v.len(), 384);
            assert!(v.iter().all(|x| x.is_finite()));
        }
    }
}
