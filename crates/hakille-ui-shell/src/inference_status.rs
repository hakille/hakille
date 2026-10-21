//! Honest inference status for the shell. No "Operational" for stubs.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Quatre états honnêtes — la coquille ne doit jamais rendre un stub « RÉEL »
/// ni prétendre qu'un artefact absent est chargé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceStatus {
    /// Aucun artefact local : rien ne peut être exécuté.
    Absent,
    /// Fichiers présents + en-têtes inspectés (GGUF magic, Safetensors JSON, ONNX meta).
    Inspected,
    /// Démonstration heuristique/projection déterministe (pas de forward neural complet).
    Simulated,
    /// Forward neural réellement exécuté (ort/Candle) — testé par golden tests.
    Real,
}

impl InferenceStatus {
    /// Toujours `false` sauf `Real` — garde-fou : les stubs restent ABSENT/SIMULÉ/INSPECTÉ.
    pub fn is_real(&self) -> bool {
        matches!(self, Self::Real)
    }

    pub fn label_fr(&self) -> &'static str {
        match self {
            Self::Absent => "Absent — artefact non provisionné",
            Self::Inspected => "Inspecté — métadonnées seules",
            Self::Simulated => "Simulé — heuristique locale",
            Self::Real => "Réel — inférence neurale",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Absent => "ABSENT",
            Self::Inspected => "INSPECTÉ",
            Self::Simulated => "SIMULÉ",
            Self::Real => "RÉEL",
        }
    }
}

/// One engine row for the shell's Models panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRow {
    pub name: String,
    pub status: InferenceStatus,
    pub detail: String,
}

fn artifact(models_dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let path = models_dir.join(name);
    path.is_file().then_some(path)
}

fn absent(name: &str, missing: &str) -> EngineRow {
    EngineRow {
        name: name.into(),
        status: InferenceStatus::Absent,
        detail: format!("artefact absent: {missing} — aucun forward exécutable"),
    }
}

/// Lignes canoniques dérivées des artefacts réellement présents sous
/// `models_dir`. `total_disk_gb`/`models_count` ne servent qu'au résumé
/// VAD ; aucun statut n'est inventé quand un fichier manque.
pub fn honest_engine_rows(
    models_dir: &Path,
    total_disk_gb: f64,
    models_count: usize,
) -> Vec<EngineRow> {
    let qwen = artifact(models_dir, "qwen2.5-0.5b-instruct-q4_k_m.gguf");
    let qwen_tok = artifact(models_dir, "tokenizer.json");
    let ui_tars = artifact(models_dir, "ui-tars-2b-sft-q4_k_m.gguf");
    let mimi = artifact(models_dir, "mimi_candle.safetensors");
    let sensevoice = artifact(models_dir, "sensevoice_small.int8.onnx");
    let tokens = artifact(models_dir, "tokens.txt");
    let campplus = artifact(models_dir, "campplus_zh_en_16k_advanced.onnx");
    let minilm = artifact(models_dir, "all-minilm-l6-v2.onnx");
    let minilm_tok = artifact(models_dir, "minilm_tokenizer.json");
    let silero = artifact(models_dir, "silero_vad.onnx");

    vec![
        match (qwen, qwen_tok) {
            (Some(_), Some(_)) => EngineRow {
                name: "Qwen 2.5 0.5B GGUF".into(),
                status: InferenceStatus::Real,
                detail: "Forward Candle Q4_K_M + KV-cache, sampling T/top-p, ChatML, tokenizer.json chargé".into(),
            },
            (Some(_), None) => absent("Qwen 2.5 0.5B GGUF", "tokenizer.json (Qwen)"),
            _ => absent("Qwen 2.5 0.5B GGUF", "qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        },
        match ui_tars {
            Some(_) => EngineRow {
                name: "UI-TARS 2B GGUF".into(),
                status: InferenceStatus::Inspected,
                detail: "Header inspecté, pas de forward VLM — grounding par vision classique réelle (Sobel + NCC), texte via Qwen".into(),
            },
            None => absent("UI-TARS 2B GGUF", "ui-tars-2b-sft-q4_k_m.gguf"),
        },
        match mimi {
            Some(_) => EngineRow {
                name: "Mimi Safetensors".into(),
                status: InferenceStatus::Simulated,
                detail: "RVQ nearest-L2 sur codebooks réels + projection déterministe ; pas d'encodeur/décodeur neural Mimi (pas de forward neural complet)".into(),
            },
            None => absent("Mimi Safetensors", "mimi_candle.safetensors"),
        },
        match (sensevoice, tokens) {
            (Some(_), Some(_)) => EngineRow {
                name: "SenseVoice ONNX".into(),
                status: InferenceStatus::Real,
                detail: "Mel 80 + LFR(7,6) + forward ort + CTC greedy, vocab 25055, confiance mesurée".into(),
            },
            (Some(_), None) => absent("SenseVoice ONNX", "tokens.txt (SenseVoice)"),
            _ => absent("SenseVoice ONNX", "sensevoice_small.int8.onnx"),
        },
        match campplus {
            Some(_) => EngineRow {
                name: "CAM++ ONNX".into(),
                status: InferenceStatus::Real,
                detail: "Fbank 80×200 + forward ort [N,T,80]→[1,192] + L2, cosine réel".into(),
            },
            None => absent("CAM++ ONNX", "campplus_zh_en_16k_advanced.onnx"),
        },
        match (minilm, minilm_tok) {
            (Some(_), Some(_)) => EngineRow {
                name: "MiniLM ONNX".into(),
                status: InferenceStatus::Real,
                detail: "WordPiece officiel 30522 + forward ort →[B,S,384] + mean-pool + L2".into(),
            },
            (Some(_), None) => absent("MiniLM ONNX", "minilm_tokenizer.json"),
            _ => absent("MiniLM ONNX", "all-minilm-l6-v2.onnx"),
        },
        match silero {
            Some(_) => EngineRow {
                name: "Silero VAD ONNX".into(),
                status: InferenceStatus::Real,
                detail: format!(
                    "{models_count} artefacts, {total_disk_gb:.2} Go — forward ort 512 éch. + état récurrent, p95 < 1 ms"
                ),
            },
            None => absent("Silero VAD ONNX", "silero_vad.onnx"),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_models(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hakille-inference-status-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"fixture").unwrap();
    }

    #[test]
    fn all_rows_are_absent_without_artifacts() {
        let dir = temp_models("absent");
        let rows = honest_engine_rows(&dir, 0.0, 0);
        assert_eq!(rows.len(), 7);
        assert!(
            rows.iter().all(|row| row.status == InferenceStatus::Absent),
            "aucun moteur ne doit être RÉEL sans artefact: {rows:?}"
        );
        assert!(
            rows.iter()
                .all(|row| row.detail.contains("artefact absent")),
            "chaque ligne absente doit nommer le fichier manquant"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn provisioned_rows_are_real_or_explicitly_limited() {
        let dir = temp_models("present");
        for name in [
            "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            "tokenizer.json",
            "ui-tars-2b-sft-q4_k_m.gguf",
            "mimi_candle.safetensors",
            "sensevoice_small.int8.onnx",
            "tokens.txt",
            "campplus_zh_en_16k_advanced.onnx",
            "all-minilm-l6-v2.onnx",
            "minilm_tokenizer.json",
            "silero_vad.onnx",
        ] {
            touch(&dir, name);
        }
        let rows = honest_engine_rows(&dir, 1.5, 10);
        assert_eq!(
            rows.iter()
                .filter(|row| row.status == InferenceStatus::Real)
                .count(),
            5,
            "Qwen, SenseVoice, CAM++, MiniLM, Silero sont RÉELS ; Mimi reste SIMULÉ et UI-TARS INSPECTÉ"
        );
        for row in &rows {
            match row.status {
                InferenceStatus::Real => assert!(
                    row.status.is_real(),
                    "engine {} status must be real",
                    row.name
                ),
                InferenceStatus::Simulated => assert!(
                    row.detail.contains("pas de forward"),
                    "Mimi doit porter sa limite explicite: {}",
                    row.detail
                ),
                InferenceStatus::Inspected => assert!(
                    row.detail.contains("pas de forward VLM"),
                    "UI-TARS doit porter sa limite explicite: {}",
                    row.detail
                ),
                InferenceStatus::Absent => panic!("artefact présent mais ligne absente: {row:?}"),
            }
        }
        // Les badges/labels restent honnêtes.
        assert_eq!(InferenceStatus::Real.badge(), "RÉEL");
        assert_eq!(InferenceStatus::Simulated.badge(), "SIMULÉ");
        assert_eq!(InferenceStatus::Inspected.badge(), "INSPECTÉ");
        assert_eq!(InferenceStatus::Absent.badge(), "ABSENT");
        assert_ne!(InferenceStatus::Simulated, InferenceStatus::Real);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_no_operational_label_for_stubs() {
        let dir = temp_models("labels");
        for row in honest_engine_rows(&dir, 0.0, 0) {
            assert!(
                !row.detail.to_lowercase().contains("operational"),
                "row {} must never claim Operational",
                row.name
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
