//! # RVQ résiduel RÉEL (étage de quantification du codec Mimi)
//!
//! Vrai codage vectoriel résiduel (Residual Vector Quantization) utilisant les
//! VRAIS centroïdes du checkpoint `mimi_candle.safetensors` :
//!
//! - Découverte des tenseurs par nom depuis le header safetensors (jamais de
//!   dimensions en dur sans vérification : shapes `[E, D]` et `[E]` relues du
//!   header via [`SafetensorsModel`](hakille_inference::SafetensorsModel)).
//! - Dans ce checkpoint, les codebooks Mimi sont stockés au format EMA
//!   d'entraînement : `...layers.{i}.codebook.embed_sum` `[E, D]` +
//!   `...layers.{i}.codebook.cluster_usage` `[E]`.
//!   Centroïde réel : `centroid[e] = embed_sum[e] / max(cluster_usage[e], eps)`
//!   (`usage <= 0` → vecteur nul, honnête : entrée jamais observée).
//! - `encode` = plus proche voisin L2 résiduel par codebook (vrai RVQ).
//! - `decode` = somme des centroïdes (reconstruction RVQ exacte).
//!
//! ## REAL vs v1 (documenté)
//! - REAL : recherche nearest-neighbor L2, boucle résiduelle, reconstruction
//!   par somme de centroïdes, poids lus du fichier (float32 little-endian).
//! - v1 (documenté, pas du faux-neural) : le forward encodeur complet
//!   (convolutions + transformer Mimi) n'est PAS exécuté dans ce crate
//!   (std uniquement, pas de runtime Candle) — voir `codec.rs` pour la
//!   projection d'entrée qui remplace cet étage.
//! - SECOURS (`synthetic_v1`, tests / absence de checkpoint) : codebooks
//!   déterministes (xorshift, seed fixe) avec la MÊME géométrie que Mimi.
//!   Marqué `RvqSource::SyntheticV1`, jamais présenté comme poids Mimi.

use hakille_core::{HakilleError, Result};
use hakille_inference::SafetensorsModel;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const ACOUSTIC_PREFIX: &str = "quantizer.acoustic_residual_vector_quantizer";
pub const EMBED_SUM_SUFFIX: &str = "codebook.embed_sum";
pub const CLUSTER_USAGE_SUFFIX: &str = "codebook.cluster_usage";

/// Origine des poids — traçabilité honnête affichée dans les logs/tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RvqSource {
    /// Vrais poids Mimi lus du fichier safetensors.
    MimiWeights { path: String, tensor_prefix: String },
    /// Géométrie réelle lue du header, poids non chargés (bytes indisponibles).
    GeometryOnlyV1 { tensor_prefix: String },
    /// Codebooks déterministes de secours (tests / checkpoint absent).
    SyntheticV1 { seed: u64 },
}

/// Un codebook : `entries` centroïdes de dimension `dim`, row-major.
#[derive(Debug, Clone)]
pub struct Codebook {
    pub entries: usize,
    pub dim: usize,
    pub centroids: Vec<f32>,
}

/// Codebooks RVQ résiduels chargés depuis un checkpoint Mimi (ou secours).
#[derive(Debug, Clone)]
pub struct RvqCodebooks {
    pub books: Vec<Codebook>,
    pub tensor_names: Vec<String>,
    pub source: RvqSource,
}

impl RvqCodebooks {
    pub fn num_books(&self) -> usize {
        self.books.len()
    }

    pub fn dim(&self) -> usize {
        self.books.first().map(|b| b.dim).unwrap_or(0)
    }

    pub fn entries_per_book(&self) -> usize {
        self.books.first().map(|b| b.entries).unwrap_or(0)
    }

    /// RMS moyen des centroïdes (diagnostic d'échelle, cf. `codec.rs`).
    pub fn centroid_rms(&self) -> f32 {
        let mut sum = 0.0f64;
        let mut n = 0u64;
        for b in &self.books {
            for &v in &b.centroids {
                sum += (v as f64) * (v as f64);
                n += 1;
            }
        }
        if n == 0 {
            0.0
        } else {
            ((sum / n as f64).sqrt()) as f32
        }
    }

    /// Encode un vecteur latent : RVQ résiduel, nearest-neighbor L2 par étage.
    ///
    /// `latent.len()` doit valoir `dim`. Retourne un indice `< entries` par
    /// codebook (erreur si hors bornes — jamais d'indice inventé).
    pub fn encode(&self, latent: &[f32]) -> Result<Vec<u16>> {
        if self.books.is_empty() {
            return Err(HakilleError::InferenceError(
                "RVQ: aucun codebook chargé".into(),
            ));
        }
        let dim = self.dim();
        if latent.len() != dim {
            return Err(HakilleError::InferenceError(format!(
                "RVQ: latent dim {} != codebook dim {}",
                latent.len(),
                dim
            )));
        }
        let mut residual = latent.to_vec();
        let mut indices = Vec::with_capacity(self.books.len());
        for book in &self.books {
            let mut best = 0usize;
            let mut best_d = f32::INFINITY;
            for e in 0..book.entries {
                let base = e * dim;
                let mut d = 0.0f32;
                for (r, c) in residual.iter().zip(book.centroids[base..base + dim].iter()) {
                    let diff = r - c;
                    d += diff * diff;
                }
                if d < best_d {
                    best_d = d;
                    best = e;
                }
            }
            if best >= book.entries || best > u16::MAX as usize {
                return Err(HakilleError::InferenceError(format!(
                    "RVQ: indice {} hors bornes (entries={})",
                    best, book.entries
                )));
            }
            // Soustraction résiduelle (cœur du RVQ).
            let base = best * dim;
            for (r, c) in residual
                .iter_mut()
                .zip(book.centroids[base..base + dim].iter())
            {
                *r -= c;
            }
            indices.push(best as u16);
        }
        Ok(indices)
    }

    /// Décode des indices en vecteur latent : somme des centroïdes.
    pub fn decode(&self, indices: &[u16]) -> Result<Vec<f32>> {
        if self.books.is_empty() {
            return Err(HakilleError::InferenceError(
                "RVQ: aucun codebook chargé".into(),
            ));
        }
        if indices.len() != self.books.len() {
            return Err(HakilleError::InferenceError(format!(
                "RVQ: {} indices pour {} codebooks",
                indices.len(),
                self.books.len()
            )));
        }
        let dim = self.dim();
        let mut out = vec![0.0f32; dim];
        for (book, &idx) in self.books.iter().zip(indices.iter()) {
            let e = idx as usize;
            if e >= book.entries {
                return Err(HakilleError::InferenceError(format!(
                    "RVQ: indice {} hors bornes (entries={})",
                    e, book.entries
                )));
            }
            let base = e * dim;
            for (o, c) in out.iter_mut().zip(book.centroids[base..base + dim].iter()) {
                *o += c;
            }
        }
        Ok(out)
    }

    /// Erreur quadratique moyenne résiduelle après quantification.
    pub fn residual_mse(&self, latent: &[f32], indices: &[u16]) -> Result<f32> {
        let recon = self.decode(indices)?;
        if recon.len() != latent.len() {
            return Err(HakilleError::InferenceError(
                "RVQ: dimensions incohérentes".into(),
            ));
        }
        let mse = recon
            .iter()
            .zip(latent.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            / latent.len().max(1) as f32;
        Ok(mse)
    }
}

/// Extrait l'index numérique de `...layers.{i}.codebook...` pour tri réel.
fn layer_index(name: &str) -> Option<usize> {
    let marker = "layers.";
    let pos = name.find(marker)?;
    let rest = &name[pos + marker.len()..];
    let end = rest.find('.')?;
    rest[..end].parse::<usize>().ok()
}

/// Découvre les tenseurs `embed_sum` du quantizer acoustique, triés par couche.
///
/// Shapes VÉRIFIÉES (pas de dims en dur) : chaque candidat doit avoir un
/// `embed_sum` `[E, D]` (2D) et un `cluster_usage` `[E]` (1D, même E).
/// Retourne les noms `embed_sum` triés par index de couche.
pub fn discover_embed_sum_names(model: &SafetensorsModel) -> Vec<String> {
    let mut layers: Vec<(usize, String)> = Vec::new();
    for name in model.tensors.keys() {
        if !name.starts_with(ACOUSTIC_PREFIX) || !name.ends_with(EMBED_SUM_SUFFIX) {
            continue;
        }
        let usage_name = name.replace(EMBED_SUM_SUFFIX, CLUSTER_USAGE_SUFFIX);
        let (Some(sum), Some(usage)) = (model.tensors.get(name), model.tensors.get(&usage_name))
        else {
            continue;
        };
        // Vérification réelle des shapes du header.
        if sum.shape.len() != 2 || usage.shape.len() != 1 {
            continue;
        }
        if sum.shape[0] != usage.shape[0] || sum.dtype != "F32" || usage.dtype != "F32" {
            continue;
        }
        if let Some(idx) = layer_index(name) {
            layers.push((idx, name.clone()));
        }
    }
    layers.sort_by_key(|(i, _)| *i);
    layers.into_iter().map(|(_, n)| n).collect()
}

/// Géométrie (nb couches dispo, dim, entries) lue du header — jamais en dur.
pub fn geometry_from_header(model: &SafetensorsModel) -> Option<(usize, usize, usize)> {
    let names = discover_embed_sum_names(model);
    let first = names.first()?;
    let sum = model.tensors.get(first)?;
    if sum.shape.len() != 2 {
        return None;
    }
    Some((names.len(), sum.shape[1], sum.shape[0]))
}

/// Lit un tenseur F32 brut depuis le fichier (offsets relatifs au header).
fn read_f32_tensor_raw(
    path: &Path,
    header_size: u64,
    dtype: &str,
    shape: &[usize],
    offsets: (u64, u64),
) -> Result<Vec<f32>> {
    if dtype != "F32" {
        return Err(HakilleError::InferenceError(format!(
            "RVQ: dtype {} non supporté (lecture F32 LE uniquement, refus honnête)",
            dtype
        )));
    }
    let expect: usize = shape.iter().product();
    let byte_len = (offsets.1 - offsets.0) as usize;
    if byte_len != expect * 4 {
        return Err(HakilleError::InferenceError(format!(
            "RVQ: taille incohérente pour le tenseur ({} octets, attendu {})",
            byte_len,
            expect * 4
        )));
    }
    let mut file = File::open(path).map_err(|e| {
        HakilleError::InferenceError(format!("RVQ: ouverture {}: {}", path.display(), e))
    })?;
    file.seek(SeekFrom::Start(8 + header_size + offsets.0))
        .map_err(|e| HakilleError::InferenceError(format!("RVQ: seek: {}", e)))?;
    let mut buf = vec![0u8; byte_len];
    file.read_exact(&mut buf)
        .map_err(|e| HakilleError::InferenceError(format!("RVQ: lecture poids: {}", e)))?;
    Ok(buf
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Charge jusqu'à `max_books` codebooks acoustiques RÉELS depuis le fichier.
///
/// `model` fournit les shapes/offsets (header déjà parsé par
/// `SafetensorsModel::read_from_file`), `path` fournit les bytes.
pub fn load_acoustic_from_file(
    path: impl AsRef<Path>,
    model: &SafetensorsModel,
    max_books: usize,
) -> Result<RvqCodebooks> {
    let path = path.as_ref();
    let mut names = discover_embed_sum_names(model);
    if names.is_empty() {
        return Err(HakilleError::InferenceError(
            "RVQ: aucun codebook embed_sum/cluster_usage trouvé dans le header".into(),
        ));
    }
    names.truncate(max_books.max(1));
    let mut books = Vec::with_capacity(names.len());
    let mut tensor_names = Vec::with_capacity(names.len());
    for embed_name in &names {
        let usage_name = embed_name.replace(EMBED_SUM_SUFFIX, CLUSTER_USAGE_SUFFIX);
        let sum_info = model.tensors.get(embed_name).ok_or_else(|| {
            HakilleError::InferenceError(format!("RVQ: tenseur manquant {}", embed_name))
        })?;
        let usage_info = model.tensors.get(&usage_name).ok_or_else(|| {
            HakilleError::InferenceError(format!("RVQ: tenseur manquant {}", usage_name))
        })?;
        // Shapes relues du header (vérification, pas de constante).
        let (entries, dim) = (sum_info.shape[0], sum_info.shape[1]);
        if entries == 0 || entries > u16::MAX as usize || dim == 0 {
            return Err(HakilleError::InferenceError(format!(
                "RVQ: shape invalide {:?} pour {}",
                sum_info.shape, embed_name
            )));
        }
        let sums = read_f32_tensor_raw(
            path,
            model.header_size,
            &sum_info.dtype,
            &sum_info.shape,
            sum_info.data_offsets,
        )?;
        let usage = read_f32_tensor_raw(
            path,
            model.header_size,
            &usage_info.dtype,
            &usage_info.shape,
            usage_info.data_offsets,
        )?;
        // Centroïdes réels : moyenne EMA.
        let mut centroids = vec![0.0f32; entries * dim];
        for (e, &u) in usage.iter().enumerate().take(entries) {
            if u > 0.0 && u.is_finite() {
                let base = e * dim;
                for (c, s) in centroids[base..base + dim]
                    .iter_mut()
                    .zip(sums[base..base + dim].iter())
                {
                    let v = s / u;
                    *c = if v.is_finite() { v } else { 0.0 };
                }
            }
            // usage <= 0 : entrée jamais observée → centroïde nul (documenté).
        }
        books.push(Codebook {
            entries,
            dim,
            centroids,
        });
        tensor_names.push(embed_name.clone());
    }
    Ok(RvqCodebooks {
        books,
        tensor_names,
        source: RvqSource::MimiWeights {
            path: path.display().to_string(),
            tensor_prefix: ACOUSTIC_PREFIX.to_string(),
        },
    })
}

/// Codebooks déterministes de secours (SECOURS v1 — tests / checkpoint absent).
///
/// PRNG xorshift64, seed fixe : reproductible, jamais présenté comme Mimi.
pub fn synthetic_v1(num_books: usize, dim: usize, entries: usize, seed: u64) -> RvqCodebooks {
    let mut state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // Convertit en uniforme [-1, 1].
        ((state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32
    };
    // Échelle DÉCROISSANTE par étage (vrai RVQ : grain de plus en plus fin,
    // comme les codebooks Mimi entraînés) : norme petite devant le résidu
    // pour une convergence de type matching-pursuit (chaque étage réduit
    // l'erreur au lieu de l'augmenter).
    let mut books = Vec::with_capacity(num_books);
    for b in 0..num_books {
        let s = 0.09 * 0.55f32.powi(b as i32);
        let mut centroids = Vec::with_capacity(entries * dim);
        for _ in 0..entries * dim {
            centroids.push(next() * s);
        }
        books.push(Codebook {
            entries,
            dim,
            centroids,
        });
    }
    RvqCodebooks {
        books,
        tensor_names: Vec::new(),
        source: RvqSource::SyntheticV1 { seed },
    }
}

/// Bruit pseudo-aléatoire déterministe (données de test, pas du signal réel).
#[cfg(test)]
pub fn test_noise(n: usize, seed: u64) -> Vec<f32> {
    let mut s = if seed == 0 { 0x12345678 } else { seed };
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * 0.3
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mimi_path() -> String {
        format!(
            "{}/../../models/mimi_candle.safetensors",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    #[test]
    fn test_rvq_roundtrip_bruit_erreur_bornee() {
        let rvq = synthetic_v1(8, 256, 2048, 42);
        let latent = test_noise(256, 7);
        let idx = rvq.encode(&latent).expect("encode");
        assert_eq!(idx.len(), 8);
        assert!(idx.iter().all(|&i| (i as usize) < 2048));
        let recon = rvq.decode(&idx).expect("decode");
        assert_eq!(recon.len(), 256);
        let mse = rvq.residual_mse(&latent, &idx).expect("mse");
        let signal_e: f32 = latent.iter().map(|x| x * x).sum::<f32>() / 256.0;
        // Erreur bornée : le RVQ résiduel doit battre le signal nul.
        assert!(mse < signal_e, "mse={} signal_e={}", mse, signal_e);
        assert!(mse.is_finite());
    }

    #[test]
    fn test_rvq_erreur_decroit_avec_les_etages() {
        let full = synthetic_v1(8, 64, 512, 99);
        let latent = test_noise(64, 1234);
        let idx_full = full.encode(&latent).expect("encode");
        let mse_full = full.residual_mse(&latent, &idx_full).expect("mse");
        // Premier étage seul (même premier codebook) : erreur >= erreur 8 étages.
        let mut one = full.clone();
        one.books.truncate(1);
        let idx_one = one.encode(&latent).expect("encode1");
        let mse_one = one.residual_mse(&latent, &idx_one).expect("mse1");
        assert!(
            mse_full <= mse_one + 1e-6,
            "mse_full={} mse_one={}",
            mse_full,
            mse_one
        );
    }

    #[test]
    fn test_rvq_determinisme_et_indices_valides() {
        let rvq = synthetic_v1(8, 64, 256, 2024);
        let latent = test_noise(64, 555);
        let a = rvq.encode(&latent).expect("a");
        let b = rvq.encode(&latent).expect("b");
        assert_eq!(a, b, "l'encodeur doit être déterministe");
        for &i in &a {
            assert!((i as usize) < rvq.entries_per_book());
        }
        // Entrée différente -> (très probablement) indices différents.
        let other = test_noise(64, 777);
        let c = rvq.encode(&other).expect("c");
        assert_ne!(
            a, c,
            "deux latents différents doivent donner des indices différents"
        );
    }

    #[test]
    fn test_rvq_decode_rejette_indices_hors_bornes() {
        let rvq = synthetic_v1(2, 16, 32, 1);
        assert!(rvq.decode(&[0, 99]).is_err());
        assert!(rvq.decode(&[0]).is_err());
        assert!(rvq.encode(&[0.0; 8]).is_err());
    }

    /// Chargeur RÉEL vérifié contre un mini-fichier safetensors synthétique :
    /// prouve que shapes/offsets sont lus du header (pas en dur).
    #[test]
    fn test_rvq_loader_lit_shapes_du_header() {
        let dir = std::env::temp_dir().join("hakille_rvq_test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("mini.safetensors");
        // 2 codebooks [4 entries x 3 dim] + usages.
        let entries = 4usize;
        let dim = 3usize;
        let mut sums0 = vec![0.0f32; entries * dim];
        let mut sums1 = vec![0.0f32; entries * dim];
        for e in 0..entries {
            for k in 0..dim {
                sums0[e * dim + k] = (e as f32 + 1.0) * (k as f32 + 1.0);
                sums1[e * dim + k] = -((e as f32 + 1.0) * (k as f32 + 1.0));
            }
        }
        let usage = vec![2.0f32; entries];
        let tensors: Vec<(String, Vec<f32>, Vec<usize>)> = vec![
            (
                format!("{}.layers.0.codebook.embed_sum", ACOUSTIC_PREFIX),
                sums0.clone(),
                vec![entries, dim],
            ),
            (
                format!("{}.layers.0.codebook.cluster_usage", ACOUSTIC_PREFIX),
                usage.clone(),
                vec![entries],
            ),
            (
                format!("{}.layers.1.codebook.embed_sum", ACOUSTIC_PREFIX),
                sums1.clone(),
                vec![entries, dim],
            ),
            (
                format!("{}.layers.1.codebook.cluster_usage", ACOUSTIC_PREFIX),
                usage.clone(),
                vec![entries],
            ),
        ];
        // Construit le fichier : header JSON + blobs F32 LE.
        let mut header = serde_json::Map::new();
        let mut offset = 0u64;
        for (name, data, shape) in &tensors {
            let nbytes = (data.len() * 4) as u64;
            header.insert(
                name.clone(),
                serde_json::json!({
                    "dtype": "F32",
                    "shape": shape,
                    "data_offsets": [offset, offset + nbytes],
                }),
            );
            offset += nbytes;
        }
        let header_str = serde_json::Value::Object(header).to_string();
        let mut file = File::create(&path).expect("create");
        use std::io::Write;
        file.write_all(&(header_str.len() as u64).to_le_bytes())
            .expect("hlen");
        file.write_all(header_str.as_bytes()).expect("hdr");
        for (_, data, _) in &tensors {
            for &v in data {
                file.write_all(&v.to_le_bytes()).expect("blob");
            }
        }
        drop(file);

        let model = SafetensorsModel::read_from_file(&path).expect("parse header");
        let found = discover_embed_sum_names(&model);
        assert_eq!(found.len(), 2, "découverte par nom depuis le header");
        // Ordre trié par couche : layers.0 avant layers.1.
        assert!(found[0].contains("layers.0"), "tri numérique: {:?}", found);
        let (n, d, e) = geometry_from_header(&model).expect("géométrie");
        assert_eq!((n, d, e), (2, dim, entries));
        let rvq = load_acoustic_from_file(&path, &model, 8).expect("load");
        assert_eq!(rvq.num_books(), 2);
        assert_eq!((rvq.dim(), rvq.entries_per_book()), (dim, entries));
        // Centroïde réel = embed_sum / usage = sums0 / 2.
        assert!((rvq.books[0].centroids[0] - sums0[0] / 2.0).abs() < 1e-6);
        assert!((rvq.books[1].centroids[0] - sums1[0] / 2.0).abs() < 1e-6);
        // Roundtrip exact sur un centroïde avec UN SEUL étage : erreur ~0
        // (avec 2 étages, le 2ᵉ quantifie le résidu — c'est le RVQ normal).
        let mut one = rvq.clone();
        one.books.truncate(1);
        let target = vec![
            one.books[0].centroids[0],
            one.books[0].centroids[1],
            one.books[0].centroids[2],
        ];
        let idx = one.encode(&target).expect("encode centroid");
        assert_eq!(idx, vec![0], "le centroïde exact doit être retrouvé");
        let mse = one.residual_mse(&target, &idx).expect("mse");
        assert!(mse < 1e-6, "mse={}", mse);
        // À 2 étages : erreur finie et bornée.
        let idx2 = rvq.encode(&target).expect("encode 2 étages");
        let mse2 = rvq.residual_mse(&target, &idx2).expect("mse2");
        assert!(mse2.is_finite() && mse2 < 10.0, "mse2={}", mse2);
        std::fs::remove_file(&path).ok();
    }

    /// Poids Mimi RÉELS : charge les vrais codebooks du checkpoint s'il est
    /// présent (skip gracieux sinon — test rapide : 1 trame).
    #[test]
    fn test_rvq_vrais_poids_mimi_si_presents() {
        let path = mimi_path();
        if !std::path::Path::new(&path).exists() {
            eprintln!("SKIP: {} absent", path);
            return;
        }
        let model = SafetensorsModel::read_from_file(&path).expect("header Mimi");
        let found = discover_embed_sum_names(&model);
        assert!(
            found.len() >= 8,
            "attendu >= 8 couches acoustiques, trouvé {}",
            found.len()
        );
        let (n, dim, entries) = geometry_from_header(&model).expect("géométrie Mimi");
        eprintln!(
            "Mimi RVQ réel: {} couches, dim={}, entries={}",
            n, dim, entries
        );
        assert!(dim > 0 && entries > 0 && entries <= u16::MAX as usize);
        let rvq = load_acoustic_from_file(&path, &model, 8).expect("vrais poids");
        assert_eq!(rvq.num_books(), 8);
        assert!(matches!(rvq.source, RvqSource::MimiWeights { .. }));
        assert!(rvq.centroid_rms() > 1e-6, "centroïdes non triviaux");
        let latent = test_noise(dim, 31337);
        let idx = rvq.encode(&latent).expect("encode réel");
        assert_eq!(idx.len(), 8);
        assert!(idx.iter().all(|&i| (i as usize) < entries));
        let idx2 = rvq.encode(&latent).expect("encode réel 2");
        assert_eq!(idx, idx2, "déterminisme sur vrais poids");
        let mse = rvq.residual_mse(&latent, &idx).expect("mse");
        assert!(mse.is_finite(), "erreur finie sur vrais poids");
        eprintln!("RVQ Mimi réel: indices={:?} mse={:.6}", idx, mse);
    }
}
