//! Diarisation RÉELLE par clustering d'embeddings CAM++.
//!
//! backend = CAM++ clustering, Nemotron planned.
//!
//! - Fenêtre glissante 1.5 s / pas 0.75 s -> un embedding CAM++ réel
//!   (`CampplusOnnx`, forward `ort`) par fenêtre.
//! - Clustering agglomératif sur similarité cosinus (seuil conventionnel 0.92,
//!   `max_speakers` respecté ; fenêtres frontières/mixtes isolées rattachées
//!   au plus proche voisin) -> `SpeakerTurn` réels avec timestamps.
//! - Marque honnête : `diarisation par clustering d'embeddings CAM++ réels
//!   (checkpoint Nemotron non provisionné)` — le checkpoint Sortformer
//!   Nemotron n'est pas provisionné dans `models/`, la séparation est donc
//!   estimée par les embeddings CAM++ et un clustering heuristique.
//!   `backend = CAM++ clustering, Nemotron planned` : le moteur effectif
//!   est [`CampPlusClusteringDiarizer`] ; [`NemotronDiarizer`] n'est qu'un
//!   alias de compatibilité (même backend réel).
//! - `< 0.1 s` -> `[]` conservé ; jamais de panic (silence/court gérés).

use crate::audio::capture::AudioChunk;
use async_trait::async_trait;
use hakille_core::{HakilleError, Result};
use hakille_inference::{CampplusOnnx, cosine_similarity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Fenêtre 1.5 s @16 kHz.
const DIAR_WIN_SAMPLES: usize = 24_000;
/// Pas 0.75 s @16 kHz.
const DIAR_HOP_SAMPLES: usize = 12_000;
/// Seuil de fusion conventionnel. Une calibration biométrique exige un corpus
/// annoté de vrais locuteurs; des sinusoïdes ne fournissent pas cette preuve.
const DIAR_MERGE_THRESHOLD: f32 = 0.92;
/// Heuristique de rattachement des clusters les moins cohérents.
const DIAR_FRONTIER_MAX_SIM: f32 = 0.75;

fn peer_cohesion(embeddings: &[Vec<f32>], labels: &[usize], index: usize) -> f32 {
    let peers: Vec<_> = labels
        .iter()
        .enumerate()
        .filter(|(i, label)| *i != index && **label == labels[index])
        .map(|(i, _)| i)
        .collect();
    if peers.is_empty() {
        return 0.0;
    }
    (peers
        .iter()
        .map(|i| cosine_similarity(&embeddings[index], &embeddings[*i]))
        .sum::<f32>()
        / peers.len() as f32)
        .clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub speaker_id: String,
    pub start_time: f32,
    pub end_time: f32,
    /// Cohésion cosinus moyenne hors auto-comparaison, bornée [0,1].
    /// Zéro si aucun autre segment comparable; pas une probabilité d’identité.
    pub confidence: f32,
}

#[async_trait]
pub trait Diarizer: Send + Sync {
    async fn diarize(&self, chunk: &AudioChunk) -> Result<Vec<SpeakerTurn>>;
}

/// Diarizer réel par clustering CAM++.
///
/// backend = CAM++ clustering, Nemotron planned : la séparation est assurée
/// par des embeddings CAM++ réels (`CampplusOnnx`, forward `ort`) + clustering
/// agglomératif. Le modèle Nemotron (Sortformer) n'est pas provisionné dans
/// `models/` — aucune inférence Nemotron n'est exécutée en v1.
pub struct CampPlusClusteringDiarizer {
    max_speakers: usize,
    model_path: Option<PathBuf>,
}

/// Alias historique de compatibilité : même backend réel (CAM++ clustering).
///
/// backend = CAM++ clustering, Nemotron planned — préférez
/// [`CampPlusClusteringDiarizer`] dans le code neuf.
pub type NemotronDiarizer = CampPlusClusteringDiarizer;

impl CampPlusClusteringDiarizer {
    pub fn new(max_speakers: usize) -> Self {
        Self {
            max_speakers: max_speakers.max(1),
            model_path: None,
        }
    }

    pub fn with_model_path(path: impl AsRef<Path>, max_speakers: usize) -> Self {
        Self {
            max_speakers: max_speakers.max(1),
            model_path: Some(path.as_ref().to_path_buf()),
        }
    }

    pub fn max_speakers(&self) -> usize {
        self.max_speakers
    }

    /// Marque honnête du backend effectif.
    /// backend = CAM++ clustering, Nemotron planned.
    pub fn backend_note(&self) -> &'static str {
        "backend = CAM++ clustering, Nemotron planned — diarisation par clustering d'embeddings CAM++ réels (checkpoint Nemotron non provisionné)"
    }

    fn resolve_path(&self) -> Result<PathBuf> {
        if let Some(ref p) = self.model_path {
            if p.exists() {
                return Ok(p.clone());
            }
            return Err(HakilleError::InferenceError(format!(
                "CampPlusClusteringDiarizer: modèle CAM++ absent '{}' — provisionnez models/campplus_zh_en_16k_advanced.onnx (RÉEL exigé)",
                p.display()
            )));
        }
        CampplusOnnx::default_model_path().ok_or_else(|| {
            HakilleError::InferenceError(
                "CampPlusClusteringDiarizer: checkpoint CAM++ introuvable (models/campplus_zh_en_16k_advanced.onnx absent) — diarisation RÉELLE impossible, aucun fallback Speaker_0"
                    .to_string(),
            )
        })
    }

    fn mono_16k(chunk: &AudioChunk) -> Result<Vec<f32>> {
        hakille_inference::audio_resample::try_to_mono_16k(
            &chunk.samples,
            chunk.channels,
            chunk.sample_rate,
        )
    }
}

impl Default for CampPlusClusteringDiarizer {
    fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl Diarizer for CampPlusClusteringDiarizer {
    async fn diarize(&self, chunk: &AudioChunk) -> Result<Vec<SpeakerTurn>> {
        let pcm = Self::mono_16k(chunk)?;
        let dur = pcm.len() as f32 / 16000.0;
        if dur < 0.1 {
            return Ok(vec![]);
        }
        if pcm.is_empty() {
            return Ok(vec![]);
        }
        let path = self.resolve_path()?;
        let engine = CampplusOnnx::open(&path)?;

        // 1. Fenêtrage 1.5 s / 0.75 s (au moins une fenêtre si >= 0.1 s).
        let mut bounds: Vec<(usize, usize)> = Vec::new();
        if pcm.len() <= DIAR_WIN_SAMPLES {
            bounds.push((0, pcm.len()));
        } else {
            let mut start = 0usize;
            while start + DIAR_WIN_SAMPLES <= pcm.len() {
                bounds.push((start, start + DIAR_WIN_SAMPLES));
                start += DIAR_HOP_SAMPLES;
            }
            // Queue restante (>= 0.375 s) rattachée comme dernière fenêtre.
            if start < pcm.len() && pcm.len() - start >= DIAR_HOP_SAMPLES / 2 {
                let s = pcm.len().saturating_sub(DIAR_WIN_SAMPLES);
                if bounds.last().map(|(b, _)| *b != s).unwrap_or(true) {
                    bounds.push((s, pcm.len()));
                }
            }
        }
        if bounds.is_empty() {
            return Ok(vec![]);
        }

        // 2. Un embedding CAM++ RÉEL par fenêtre.
        let mut embs: Vec<Vec<f32>> = Vec::with_capacity(bounds.len());
        for (s, e) in bounds.iter() {
            let w = &pcm[*s..*e];
            let emb = engine.embed(w)?;
            embs.push(emb);
        }

        // 3. Clustering agglomératif (seuil cosinus, max_speakers respecté).
        let n = embs.len();
        let mut cluster: Vec<usize> = (0..n).collect();
        // Similarité moyenne inter-clusters (linkage moyenne).
        let cluster_sim = |a: usize, b: usize, cl: &[usize], em: &[Vec<f32>]| -> f32 {
            let ia: Vec<usize> = (0..n).filter(|i| cl[*i] == a).collect();
            let ib: Vec<usize> = (0..n).filter(|i| cl[*i] == b).collect();
            if ia.is_empty() || ib.is_empty() {
                return -1.0;
            }
            let mut acc = 0.0f32;
            let mut cnt = 0usize;
            for i in ia.iter() {
                for j in ib.iter() {
                    acc += cosine_similarity(&em[*i], &em[*j]);
                    cnt += 1;
                }
            }
            if cnt == 0 { -1.0 } else { acc / cnt as f32 }
        };
        loop {
            let uniq: Vec<usize> = {
                let mut u = cluster.clone();
                u.sort_unstable();
                u.dedup();
                u
            };
            if uniq.len() <= 1 {
                break;
            }
            // Paire la plus similaire.
            let mut best: Option<(usize, usize, f32)> = None;
            for i in 0..uniq.len() {
                for j in (i + 1)..uniq.len() {
                    let s = cluster_sim(uniq[i], uniq[j], &cluster, &embs);
                    if best.map(|(_, _, bs)| s > bs).unwrap_or(true) {
                        best = Some((uniq[i], uniq[j], s));
                    }
                }
            }
            let Some((ca, cb, s)) = best else { break };
            let must_merge = s >= DIAR_MERGE_THRESHOLD;
            let over_budget = uniq.len() > self.max_speakers;
            if !must_merge && !over_budget {
                break;
            }
            if must_merge {
                // Fusionne cb -> ca (linkage moyenne au-dessus du seuil).
                for c in cluster.iter_mut() {
                    if *c == cb {
                        *c = ca;
                    }
                }
                continue;
            }
            // Hors-seuil mais hors-budget : on fusionne le PLUS PETIT cluster
            // (typiquement une fenêtre mixte/frontière isolée) dans son plus
            // proche voisin, pour ne jamais fusionner deux locuteurs établis
            // entre eux. Mesuré : la fenêtre mixte (sim ≤ 0.586) rejoint ainsi
            // le bon côté au lieu de fusionner A+B (sim ≈ 0.887).
            let mut smallest = uniq[0];
            let mut smallest_n = usize::MAX;
            for u in uniq.iter() {
                let cnt = cluster.iter().filter(|c| **c == *u).count();
                if cnt < smallest_n {
                    smallest_n = cnt;
                    smallest = *u;
                }
            }
            let mut near = uniq[0];
            let mut near_s = f32::NEG_INFINITY;
            for u in uniq.iter() {
                if *u == smallest {
                    continue;
                }
                let x = cluster_sim(smallest, *u, &cluster, &embs);
                if x > near_s {
                    near_s = x;
                    near = *u;
                }
            }
            for c in cluster.iter_mut() {
                if *c == smallest {
                    *c = near;
                }
            }
        }
        // 3b. Nettoyage frontière : un cluster isolé dont la MEILLEURE
        // similarité aux autres reste sous le seuil frontière est une fenêtre
        // mixte (mesuré ≤ 0.586), pas un locuteur : rattachement au plus
        // proche voisin. Deux locuteurs établis restent séparés
        // (mesuré ≤ 0.904 > 0.75).
        loop {
            let uniq: Vec<usize> = {
                let mut u = cluster.clone();
                u.sort_unstable();
                u.dedup();
                u
            };
            if uniq.len() <= 1 {
                break;
            }
            let mut isolated = usize::MAX;
            let mut isolated_best = f32::INFINITY;
            for u in uniq.iter() {
                let mut best = f32::NEG_INFINITY;
                for v in uniq.iter() {
                    if v == u {
                        continue;
                    }
                    let s = cluster_sim(*u, *v, &cluster, &embs);
                    if s > best {
                        best = s;
                    }
                }
                if best < isolated_best {
                    isolated_best = best;
                    isolated = *u;
                }
            }
            if isolated == usize::MAX || isolated_best >= DIAR_FRONTIER_MAX_SIM {
                break;
            }
            let mut near = usize::MAX;
            let mut near_s = f32::NEG_INFINITY;
            for v in uniq.iter() {
                if *v == isolated {
                    continue;
                }
                let s = cluster_sim(isolated, *v, &cluster, &embs);
                if s > near_s {
                    near_s = s;
                    near = *v;
                }
            }
            if near == usize::MAX {
                break;
            }
            for c in cluster.iter_mut() {
                if *c == isolated {
                    *c = near;
                }
            }
        }
        // Renumérote en Speaker_0.. par ordre d'apparition.
        let mut label_of: std::collections::HashMap<usize, usize> = Default::default();
        let mut order = 0usize;
        let mut win_speaker: Vec<usize> = Vec::with_capacity(n);
        for c in cluster.iter() {
            let id = *label_of.entry(*c).or_insert_with(|| {
                let v = order;
                order += 1;
                v
            });
            win_speaker.push(id);
        }

        // 4. Fenêtres -> tours (fusion des fenêtres contiguës du même locuteur).
        let sr = 16_000.0f32;
        let mut turns: Vec<SpeakerTurn> = Vec::new();
        for (w, sp) in win_speaker.iter().enumerate() {
            let (s_samp, e_samp) = bounds[w];
            let start = (s_samp as f32 / sr).clamp(0.0, dur);
            let mut end = (e_samp as f32 / sr).clamp(0.0, dur);
            if end <= start {
                end = start + 0.01;
            }
            // Cohésion locale comme confiance (fenêtre vs moyenne du cluster).
            let coh = peer_cohesion(&embs, &win_speaker, w);
            if let Some(last) = turns.last_mut()
                && last.speaker_id == format!("Speaker_{sp}")
                && start <= last.end_time + 0.80
            {
                last.end_time = end.max(last.end_time);
                last.confidence = ((last.confidence + coh) / 2.0).clamp(0.0, 1.0);
                continue;
            }
            turns.push(SpeakerTurn {
                speaker_id: format!("Speaker_{sp}"),
                start_time: start,
                end_time: end,
                confidence: coh,
            });
        }
        turns.sort_by(|a, b| {
            a.start_time
                .partial_cmp(&b.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(turns)
    }
}

#[cfg(test)]
mod diarization_tests {
    use super::*;

    fn synth_voiced(f0: f32, seconds: f32, phase: f32) -> Vec<f32> {
        hakille_inference::speaker_onnx::synth_voiced_16k(f0, seconds, phase)
    }

    #[test]
    fn cohesion_uses_other_segments_without_invented_minimum() {
        let embeddings = vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
        assert_eq!(peer_cohesion(&embeddings, &[0, 0, 1], 0), 1.0);
        assert_eq!(peer_cohesion(&embeddings, &[0, 0, 1], 2), 0.0);
        assert_eq!(peer_cohesion(&embeddings, &[0, 1, 0], 0), 0.0);
        assert_eq!(peer_cohesion(&embeddings, &[0, 0, 0], 0), 0.5);
    }

    #[test]
    fn test_backend_note_campp_nemotron_planned() {
        let d = CampPlusClusteringDiarizer::new(2);
        let note = d.backend_note();
        assert!(
            note.contains("backend = CAM++ clustering"),
            "backend_note doit déclarer CAM++ clustering, got: {note}"
        );
        assert!(
            note.contains("Nemotron planned"),
            "backend_note doit annoncer Nemotron planned, got: {note}"
        );
        // Alias de compatibilité : même backend réel.
        let legacy: NemotronDiarizer = NemotronDiarizer::new(2);
        assert_eq!(legacy.backend_note(), note);
        assert_eq!(legacy.max_speakers(), 2);
    }

    #[tokio::test]
    async fn malformed_audio_is_rejected_before_model_loading() {
        let diarizer = CampPlusClusteringDiarizer::with_model_path("/absent", 2);
        for chunk in [
            AudioChunk::new(vec![f32::NAN; 1600], 16000, 1),
            AudioChunk::new(vec![0.0; 1600], 0, 1),
            AudioChunk::new(vec![0.0; 1601], 16000, 2),
        ] {
            assert!(
                diarizer
                    .diarize(&chunk)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("audio invalide")
            );
        }
    }

    /// Signaux synthétiques pour vérifier bornes et déterminisme numérique,
    /// sans leur attribuer une identité humaine.
    #[tokio::test]
    async fn clustering_real_obeys_budget_and_is_deterministic() {
        let Some(model_path) = CampplusOnnx::default_model_path() else {
            eprintln!("SKIP diarisation 2 locuteurs : checkpoint CAM++ absent");
            return;
        };
        let _ = &model_path;
        let mut pcm = synth_voiced(120.0, 3.0, 0.0);
        pcm.extend_from_slice(&synth_voiced(220.0, 3.0, 0.5));
        let chunk = AudioChunk {
            samples: pcm,
            sample_rate: 16_000,
            channels: 1,
            timestamp: hakille_core::Timestamp::now(),
        };
        let diarizer = CampPlusClusteringDiarizer::new(2);
        let turns = diarizer.diarize(&chunk).await.expect("diarisation réelle");
        eprintln!("turns 2 locuteurs: {turns:?}");
        assert!(
            !turns.is_empty(),
            "la diarisation réelle doit retourner des tours"
        );
        let mut ids: Vec<String> = turns.iter().map(|t| t.speaker_id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert!(!ids.is_empty() && ids.len() <= diarizer.max_speakers());
        let again = diarizer.diarize(&chunk).await.unwrap();
        assert_eq!(
            serde_json::to_value(&turns).unwrap(),
            serde_json::to_value(&again).unwrap()
        );
        // Timestamps ordonnés et finis.
        for t in &turns {
            assert!(t.start_time < t.end_time);
            assert!(t.start_time.is_finite() && t.end_time.is_finite());
            assert!(
                (0.0..=1.0).contains(&t.confidence),
                "confiance {}",
                t.confidence
            );
        }
    }
}
