//! Embeddings MiniLM réels : tokenizer officiel complet, forward ONNX CPU,
//! mean-pooling masqué et normalisation L2. Aucun remplacement par hachage.
//!
//! La configuration JSON exécute BertNormalizer, BertPreTokenizer, WordPiece
//! et les tokens spéciaux officiels, y compris accents et ponctuation Unicode.
//! La fenêtre de 256 tokens correspond au modèle SentenceTransformers MiniLM.

use hakille_core::{HakilleError, Result};
use ort::{
    session::Session,
    value::{Tensor, TensorElementType},
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

pub const MINILM_SEQ_LEN: usize = 256;
pub const MINILM_DIM: usize = 384;

fn ort_err(context: &str, error: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("MiniLmOnnx::{context}: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenMode {
    WordPieceOfficiel,
}

impl TokenMode {
    pub fn label_fr(&self) -> &'static str {
        "WordPiece officiel (30 522 tokens, pipeline JSON complet)"
    }
}

/// Session CPU partagée ; sérialisation des forwards sans perdre le tokenizer.
pub struct MiniLmOnnx {
    _env: Arc<ort::environment::Environment>,
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    has_type_ids: bool,
}

impl MiniLmOnnx {
    pub fn open(model_path: impl AsRef<Path>, tokenizer_json: impl AsRef<Path>) -> Result<Self> {
        let tokenizer = load_official_tokenizer(tokenizer_json.as_ref())?;
        let env = crate::speaker_onnx::shared_ort_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("builder", e))?
            .with_intra_threads(4)
            .map_err(|e| ort_err("threads", e))?
            .with_inter_threads(1)
            .map_err(|e| ort_err("threads", e))?
            .commit_from_file(model_path.as_ref())
            .map_err(|e| ort_err("modèle", e))?;
        let names: Vec<_> = session.inputs().iter().map(|input| input.name()).collect();
        validate_input_names(&names)?;
        for input in session.inputs() {
            let shape = input
                .dtype()
                .tensor_shape()
                .ok_or_else(|| ort_err("signature", "entrée non tensorielle"))?;
            if input.dtype().tensor_type() != Some(TensorElementType::Int64)
                || shape.len() != 2
                || !matches!(shape[0], -1 | 1)
                || (shape[1] != -1 && shape[1] != MINILM_SEQ_LEN as i64)
            {
                return Err(ort_err(
                    "signature",
                    format!(
                        "entrée '{}' attendue int64 [1,256] ou dynamique, reçu {:?}",
                        input.name(),
                        input.dtype()
                    ),
                ));
            }
        }
        let output = session
            .outputs()
            .iter()
            .find(|output| output.name() == "last_hidden_state")
            .ok_or_else(|| ort_err("signature", "sortie last_hidden_state absente"))?;
        let shape = output
            .dtype()
            .tensor_shape()
            .ok_or_else(|| ort_err("signature", "sortie non tensorielle"))?;
        if output.dtype().tensor_type() != Some(TensorElementType::Float32)
            || shape.len() != 3
            || !matches!(shape[0], -1 | 1)
            || (shape[1] != -1 && shape[1] != MINILM_SEQ_LEN as i64)
            || (shape[2] != -1 && shape[2] != MINILM_DIM as i64)
        {
            return Err(ort_err(
                "signature",
                format!("sortie attendue f32 [1,256,384], reçu {:?}", output.dtype()),
            ));
        }
        let has_type_ids = names.contains(&"token_type_ids");
        Ok(Self {
            _env: env,
            session: Mutex::new(session),
            tokenizer,
            has_type_ids,
        })
    }

    pub fn default_model_path() -> Option<PathBuf> {
        default_artifact("all-minilm-l6-v2.onnx")
    }
    pub fn default_tokenizer_path() -> Option<PathBuf> {
        default_artifact("minilm_tokenizer.json")
    }

    pub fn open_default() -> Result<Self> {
        let model = Self::default_model_path()
            .ok_or_else(|| ort_err("open_default", "modèle absent : all-minilm-l6-v2.onnx"))?;
        let tokenizer = Self::default_tokenizer_path()
            .ok_or_else(|| ort_err("open_default", "tokenizer absent : minilm_tokenizer.json"))?;
        Self::open(model, tokenizer)
    }

    pub fn is_real(&self) -> bool {
        true
    }
    pub fn token_mode(&self) -> TokenMode {
        TokenMode::WordPieceOfficiel
    }
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.get_vocab_size(true)
    }

    pub fn real_input_names(&self) -> Vec<String> {
        // Les métadonnées sont immuables ; lire malgré un mutex empoisonné
        // n'autorise pas un nouveau forward (embed retourne alors une erreur).
        let session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        session
            .inputs()
            .iter()
            .map(|input| input.name().to_string())
            .collect()
    }
    pub fn real_output_names(&self) -> Vec<String> {
        let session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        session
            .outputs()
            .iter()
            .map(|output| output.name().to_string())
            .collect()
    }

    /// Encode avec la normalisation officielle, ajoute CLS/SEP puis tronque et
    /// complète à 256. Les erreurs de tokenisation restent explicites.
    pub fn tokenize(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
        encode(&self.tokenizer, text)
    }

    /// Embedding 384-d normalisé, y compris pour un texte vide (CLS + SEP).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let started = Instant::now();
        let (ids, mask, types) = self.tokenize(text)?;
        let ids = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], ids))
            .map_err(|e| ort_err("ids", e))?;
        let attention = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], mask.clone()))
            .map_err(|e| ort_err("mask", e))?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| ort_err("lock", "session empoisonnée"))?;
        let output = if self.has_type_ids {
            let types = Tensor::<i64>::from_array(([1usize, MINILM_SEQ_LEN], types)).map_err(|e| ort_err("types", e))?;
            session.run(ort::inputs!["input_ids" => ids, "attention_mask" => attention, "token_type_ids" => types])
        } else {
            session.run(ort::inputs!["input_ids" => ids, "attention_mask" => attention])
        }.map_err(|e| ort_err("forward", e))?;
        let (shape, values) = output
            .get("last_hidden_state")
            .ok_or_else(|| ort_err("sortie", "last_hidden_state absente"))?
            .try_extract_tensor::<f32>()
            .map_err(|e| ort_err("sortie", e))?;
        let embedding = mean_pool_normalize(shape, values, &mask)?;
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis(),
            "MiniLM : forward réel terminé"
        );
        Ok(embedding)
    }

    pub fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        cosine_sim(a, b)
    }
}

fn validate_input_names(names: &[&str]) -> Result<()> {
    if !names.contains(&"input_ids")
        || !names.contains(&"attention_mask")
        || names
            .iter()
            .any(|name| !matches!(*name, "input_ids" | "attention_mask" | "token_type_ids"))
        || names.iter().collect::<std::collections::HashSet<_>>().len() != names.len()
    {
        return Err(ort_err(
            "signature",
            format!("entrées MiniLM incompatibles : {names:?}"),
        ));
    }
    Ok(())
}

fn default_artifact(filename: &str) -> Option<PathBuf> {
    if let Some(directory) = std::env::var_os("HAKILLE_MODELS_DIR") {
        // Une sélection explicite ne doit jamais basculer sur un autre modèle.
        let path = PathBuf::from(directory).join(filename);
        return path.is_file().then_some(path);
    }
    [
        PathBuf::from("models"),
        PathBuf::from("../models"),
        PathBuf::from("../../models"),
    ]
    .into_iter()
    .map(|directory| directory.join(filename))
    .find(|path| path.is_file())
}

fn load_official_tokenizer(path: &Path) -> Result<Tokenizer> {
    let bytes = std::fs::read(path).map_err(|e| ort_err("tokenizer officiel", e))?;
    let config: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| ort_err("configuration tokenizer", e))?;
    validate_tokenizer_config(&config)?;
    let mut tokenizer =
        Tokenizer::from_bytes(&bytes).map_err(|e| ort_err("tokenizer officiel", e))?;
    if tokenizer.get_vocab_size(true) != 30_522 {
        return Err(ort_err(
            "tokenizer",
            "vocabulaire MiniLM attendu : 30 522 tokens",
        ));
    }
    for (token, id) in [("[PAD]", 0), ("[UNK]", 100), ("[CLS]", 101), ("[SEP]", 102)] {
        if tokenizer.token_to_id(token) != Some(id) {
            return Err(ort_err(
                "tokenizer",
                format!("identifiant spécial incorrect : {token}"),
            ));
        }
    }
    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: MINILM_SEQ_LEN,
            ..Default::default()
        }))
        .map_err(|e| ort_err("troncature", e))?;
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(MINILM_SEQ_LEN),
        ..Default::default()
    }));
    // Refuser aussi un JSON avec les bons IDs mais sans post-processeur CLS/SEP.
    encode(&tokenizer, "")?;
    Ok(tokenizer)
}

fn validate_tokenizer_config(config: &serde_json::Value) -> Result<()> {
    let model = &config["model"];
    let normalizer = &config["normalizer"];
    if model["type"] != "WordPiece"
        || model["unk_token"] != "[UNK]"
        || model["continuing_subword_prefix"] != "##"
        || model["max_input_chars_per_word"] != 100
        || normalizer["type"] != "BertNormalizer"
        || normalizer["lowercase"] != true
        || normalizer["clean_text"] != true
        || normalizer["handle_chinese_chars"] != true
        || !(normalizer["strip_accents"].is_null() || normalizer["strip_accents"] == true)
        || config["pre_tokenizer"]["type"] != "BertPreTokenizer"
    {
        return Err(ort_err(
            "tokenizer",
            "configuration incompatible avec le pipeline BERT uncased MiniLM",
        ));
    }
    Ok(())
}

fn encode(tokenizer: &Tokenizer, text: &str) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| ort_err("tokenisation", e))?;
    let ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
    let mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|m| i64::from(*m))
        .collect();
    let types: Vec<i64> = encoding
        .get_type_ids()
        .iter()
        .map(|id| i64::from(*id))
        .collect();
    if ids.len() != MINILM_SEQ_LEN || mask.len() != MINILM_SEQ_LEN || types.len() != MINILM_SEQ_LEN
    {
        return Err(ort_err("tokenisation", "longueur incohérente"));
    }
    let active = mask.iter().take_while(|&&m| m == 1).count();
    if active < 2
        || ids[0] != 101
        || ids[active - 1] != 102
        || mask[active..].iter().any(|&m| m != 0)
        || ids[active..].iter().any(|&id| id != 0)
        || ids.iter().any(|&id| !(0..30_522).contains(&id))
        || types.iter().any(|&id| id != 0)
    {
        return Err(ort_err(
            "tokenisation",
            "masque, tokens spéciaux ou identifiants incompatibles",
        ));
    }
    Ok((ids, mask, types))
}

/// Pooling calculé en f64 pour ne pas fabriquer un vecteur nul par débordement.
fn mean_pool_normalize(shape: &[i64], values: &[f32], mask: &[i64]) -> Result<Vec<f32>> {
    if shape != [1, MINILM_SEQ_LEN as i64, MINILM_DIM as i64]
        || values.len() != MINILM_SEQ_LEN * MINILM_DIM
        || mask.len() != MINILM_SEQ_LEN
    {
        return Err(ort_err("pooling", format!("forme inattendue : {shape:?}")));
    }
    if values.iter().any(|v| !v.is_finite()) || mask.iter().any(|&m| m != 0 && m != 1) {
        return Err(ort_err("pooling", "sortie non finie ou masque non binaire"));
    }
    let active = mask.iter().filter(|&&m| m == 1).count();
    if active == 0 {
        return Err(ort_err("pooling", "masque vide"));
    }
    let mut mean = vec![0.0f64; MINILM_DIM];
    for (row, &attended) in values.chunks_exact(MINILM_DIM).zip(mask) {
        if attended == 1 {
            for (sum, value) in mean.iter_mut().zip(row) {
                *sum += f64::from(*value);
            }
        }
    }
    for value in &mut mean {
        *value /= active as f64;
    }
    let norm = mean.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= f64::EPSILON {
        return Err(ort_err("pooling", "norme nulle ou invalide"));
    }
    Ok(mean
        .into_iter()
        .map(|value| (value / norm) as f32)
        .collect())
}

/// Cosinus réel, également correct pour des vecteurs non normalisés.
/// Les entrées invalides ou nulles n'ont pas de similarité définie : score 0.
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() || a.iter().chain(b).any(|v| !v.is_finite()) {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum();
    let norm_a = a.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>().sqrt();
    let norm_b = b.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0) as f32
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn open_or_skip() -> Option<MiniLmOnnx> {
        let (Some(model), Some(tokenizer)) = (
            MiniLmOnnx::default_model_path(),
            MiniLmOnnx::default_tokenizer_path(),
        ) else {
            eprintln!("SKIP MiniLM : modèle ou tokenizer absent");
            return None;
        };
        Some(
            MiniLmOnnx::open(model, tokenizer)
                .expect("artefacts présents : tout échec réel doit faire échouer le test"),
        )
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
        let (ids, mask, types) = sess
            .tokenize("le chat dort")
            .expect("tokenisation officielle");
        assert_eq!(ids.len(), MINILM_SEQ_LEN);
        assert_eq!(mask.len(), MINILM_SEQ_LEN);
        assert_eq!(types.len(), MINILM_SEQ_LEN);
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
    #[test]
    fn minilm_signature_rejects_positional_guesses_and_unknown_inputs() {
        assert!(validate_input_names(&["input_ids", "attention_mask"]).is_ok());
        assert!(validate_input_names(&["token_type_ids", "attention_mask", "input_ids"]).is_ok());
        for names in [
            vec!["wrong_ids", "attention_mask"],
            vec!["input_ids"],
            vec!["input_ids", "attention_mask", "unknown"],
            vec!["input_ids", "attention_mask", "input_ids"],
        ] {
            assert!(validate_input_names(&names).is_err());
        }
    }

    #[test]
    fn minilm_pooling_respects_mask_and_rejects_invalid_shapes_or_values() {
        let shape = [1, MINILM_SEQ_LEN as i64, MINILM_DIM as i64];
        let mut values = vec![0.0; MINILM_SEQ_LEN * MINILM_DIM];
        values[0] = 3.0;
        values[MINILM_DIM + 1] = 4.0;
        values[2 * MINILM_DIM] = f32::MAX; // padding ignoré dans la moyenne
        let mut mask = vec![0; MINILM_SEQ_LEN];
        mask[0] = 1;
        mask[1] = 1;
        let output = mean_pool_normalize(&shape, &values, &mask).unwrap();
        assert!((output[0] - 0.6).abs() < 1e-6);
        assert!((output[1] - 0.8).abs() < 1e-6);
        assert!(
            mean_pool_normalize(
                &[1, MINILM_DIM as i64, MINILM_SEQ_LEN as i64],
                &values,
                &mask
            )
            .is_err()
        );
        assert!(mean_pool_normalize(&shape, &values, &vec![0; MINILM_SEQ_LEN]).is_err());
        assert!(mean_pool_normalize(&shape, &vec![0.0; values.len()], &mask).is_err());
        mask[0] = 2;
        assert!(mean_pool_normalize(&shape, &values, &mask).is_err());
        mask[0] = 1;
        values[2 * MINILM_DIM] = f32::NAN;
        assert!(mean_pool_normalize(&shape, &values, &mask).is_err());
    }

    #[test]
    fn minilm_cosine_is_scale_invariant_and_finite() {
        assert!(
            (cosine_sim(&[2.0, 0.0], &[1.0, 1.0]) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6
        );
        assert_eq!(cosine_sim(&[f32::MAX; 3], &[f32::MAX; 3]), 1.0);
        assert_eq!(cosine_sim(&[f32::MIN_POSITIVE], &[f32::MIN_POSITIVE]), 1.0);
        assert_eq!(cosine_sim(&[f32::NAN, 1.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn minilm_official_normalizer_handles_accents_unicode_and_truncation() {
        let Some(path) = MiniLmOnnx::default_tokenizer_path() else {
            eprintln!("SKIP MiniLM tokenizer absent");
            return;
        };
        let tokenizer = load_official_tokenizer(&path).expect("tokenizer présent valide");
        assert_eq!(
            encode(&tokenizer, "ÉLÉPHANT").unwrap(),
            encode(&tokenizer, "elephant").unwrap()
        );
        assert_eq!(
            encode(&tokenizer, "café").unwrap(),
            encode(&tokenizer, "cafe\u{301}").unwrap()
        );
        let (ids, mask, _) = encode(&tokenizer, "Éléphant, café — 日本語 🙂").unwrap();
        assert_eq!(
            &ids[..10],
            &[101, 10777, 1010, 7668, 1517, 1864, 1876, 1950, 100, 102]
        );
        assert_eq!(mask.iter().sum::<i64>(), 10);
        let (ids, mask, _) = encode(&tokenizer, &"word ".repeat(1000)).unwrap();
        assert_eq!(ids.len(), MINILM_SEQ_LEN);
        assert_eq!(ids[MINILM_SEQ_LEN - 1], 102);
        assert!(mask.iter().all(|&m| m == 1));
        let (ids, _, _) = encode(&tokenizer, &"x".repeat(101)).unwrap();
        assert_eq!(
            &ids[..3],
            &[101, 100, 102],
            "limite officielle WordPiece de 100 caractères"
        );
    }

    #[test]
    fn minilm_real_forward_matches_independent_python_onnx_golden() {
        use sha2::{Digest, Sha256};
        let Some(session) = open_or_skip() else {
            return;
        };
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/minilm_reference.json")).unwrap();
        for (path, key) in [
            (MiniLmOnnx::default_model_path().unwrap(), "model_sha256"),
            (
                MiniLmOnnx::default_tokenizer_path().unwrap(),
                "tokenizer_sha256",
            ),
        ] {
            let digest = format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()));
            assert_eq!(
                digest,
                fixture[key].as_str().unwrap(),
                "golden spécifique au checkpoint officiel vérifié"
            );
        }
        for record in fixture["records"].as_array().unwrap() {
            let text = record["text"].as_str().unwrap();
            let expected_ids: Vec<i64> = record["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_i64().unwrap())
                .collect();
            let (ids, _, _) = session.tokenize(text).unwrap();
            assert_eq!(&ids[..expected_ids.len()], expected_ids);
            let actual = session.embed(text).unwrap();
            let expected: Vec<f32> = record["embedding"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            assert_eq!(actual.len(), expected.len());
            let maximum_error = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(
                maximum_error < 3e-5,
                "écart maximum au forward Python indépendant : {maximum_error}"
            );
            assert!(cosine_sim(&actual, &expected) > 0.99999);
        }
    }
    #[test]
    fn minilm_tokenizer_configuration_rejects_partial_or_cased_pipeline() {
        let config = serde_json::json!({
            "model": {"type":"WordPiece", "unk_token":"[UNK]", "continuing_subword_prefix":"##", "max_input_chars_per_word":100},
            "normalizer": {"type":"BertNormalizer", "lowercase":true, "clean_text":true, "handle_chinese_chars":true, "strip_accents":null},
            "pre_tokenizer":{"type":"BertPreTokenizer"}
        });
        assert!(validate_tokenizer_config(&config).is_ok());
        let mut incompatible = config.clone();
        incompatible["normalizer"]["strip_accents"] = false.into();
        assert!(validate_tokenizer_config(&incompatible).is_err());
        incompatible = config;
        incompatible["normalizer"] = serde_json::Value::Null;
        assert!(validate_tokenizer_config(&incompatible).is_err());
    }
}
