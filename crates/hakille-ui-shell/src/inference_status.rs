//! Inventaire léger des artefacts pour le shell. La présence d'un fichier
//! ne prouve ni son intégrité, ni son chargement, ni l'exécution d'un moteur.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// États de preuve distincts. Le rendu statique ne doit jamais promouvoir la
/// simple présence d'un artefact en inspection ou en inférence réussie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceStatus {
    /// Au moins un artefact requis est absent, vide ou inaccessible.
    Absent,
    /// Artefacts présents, sans inspection, validation ou forward dans cet inventaire.
    Available,
    /// En-têtes réellement inspectés (GGUF, Safetensors ou ONNX), sans forward.
    Inspected,
    /// Démonstration heuristique/projection déterministe, sans réseau complet.
    Simulated,
    /// Forward neuronal effectivement exécuté, avec résultat vérifié.
    /// Cet état doit provenir d'une exécution observée, jamais d'un `is_file`.
    Real,
}

impl InferenceStatus {
    pub fn is_real(&self) -> bool {
        matches!(self, Self::Real)
    }

    pub fn label_fr(&self) -> &'static str {
        match self {
            Self::Absent => "Absent — artefact requis manquant ou vide",
            Self::Available => "Présent — non testé",
            Self::Inspected => "Inspecté — métadonnées seules",
            Self::Simulated => "Simulé — heuristique locale",
            Self::Real => "Réel — inférence neurale vérifiée",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Absent => "ABSENT",
            Self::Available => "PRÉSENT",
            Self::Inspected => "INSPECTÉ",
            Self::Simulated => "SIMULÉ",
            Self::Real => "RÉEL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRow {
    pub name: String,
    pub status: InferenceStatus,
    pub detail: String,
}

/// Métadonnées du système de fichiers uniquement : aucun chargement de poids,
/// hash, parsing de header, appel réseau ou connexion à un serveur d'inférence.
fn artifact(models_dir: &Path, name: &str) -> bool {
    std::fs::metadata(models_dir.join(name))
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

fn absent(name: &str, missing: &str) -> EngineRow {
    EngineRow {
        name: name.into(),
        status: InferenceStatus::Absent,
        detail: format!(
            "artefact absent, vide ou inaccessible : {missing} — exécution non vérifiée"
        ),
    }
}

fn present(name: &str, detail: impl Into<String>) -> EngineRow {
    EngineRow {
        name: name.into(),
        status: InferenceStatus::Available,
        detail: detail.into(),
    }
}

/// Inventaire canonique des fichiers non vides sous `models_dir`.
///
/// Retourne uniquement `Absent` ou `Available` : même un fichier corrompu peut
/// être présent. L'intégrité et l'exécution se vérifient séparément via
/// `models verify` et `models test`. Aucune preuve ancienne n'est supposée
/// s'appliquer aux fichiers actuels. Les compteurs de disque sont un résumé,
/// jamais une preuve d'inférence ou une mesure de latence.
pub fn honest_engine_rows(
    models_dir: &Path,
    total_disk_gb: f64,
    models_count: usize,
) -> Vec<EngineRow> {
    let qwen = artifact(models_dir, "qwen2.5-0.5b-instruct-q4_k_m.gguf");
    let qwen_tok = artifact(models_dir, "tokenizer.json");
    let ui_tars = artifact(models_dir, "ui-tars-2b-sft-q4_k_m.gguf");
    let ui_projector = artifact(models_dir, "mmproj-UI-TARS-2B-SFT-f16.gguf");
    let mimi = artifact(models_dir, "mimi_candle.safetensors");
    let sensevoice = artifact(models_dir, "sensevoice_small.int8.onnx");
    let tokens = artifact(models_dir, "tokens.txt");
    let campplus = artifact(models_dir, "campplus_zh_en_16k_advanced.onnx");
    let minilm = artifact(models_dir, "all-minilm-l6-v2.onnx");
    let minilm_tok = artifact(models_dir, "minilm_tokenizer.json");
    let silero = artifact(models_dir, "silero_vad.onnx");

    vec![
        match (qwen, qwen_tok) {
            (true, true) => present(
                "Qwen 2.5 0.5B GGUF",
                "GGUF et tokenizer présents ; moteur Candle Q4_K_M disponible. Intégrité, chargement et forward non vérifiés par cet inventaire.",
            ),
            (true, false) => absent("Qwen 2.5 0.5B GGUF", "tokenizer.json (Qwen)"),
            _ => absent("Qwen 2.5 0.5B GGUF", "qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        },
        match (ui_tars, ui_projector) {
            (true, true) => present(
                "UI-TARS 2B GGUF",
                "GGUF et projecteur présents ; inférence multimodale via un serveur llama.cpp local configuré. Serveur, intégrité des fichiers et forward non vérifiés ici.",
            ),
            (true, false) => absent(
                "UI-TARS 2B GGUF",
                "mmproj-UI-TARS-2B-SFT-f16.gguf (projecteur multimodal ; serveur local également requis)",
            ),
            _ => absent(
                "UI-TARS 2B GGUF",
                "ui-tars-2b-sft-q4_k_m.gguf (serveur local également requis)",
            ),
        },
        if mimi {
            present(
                "Mimi Safetensors",
                "Checkpoint présent ; codec neuronal Mimi complet implémenté (SEANet, transformers et RVQ Candle). Chargement et encode/decode non vérifiés ici ; modèle conversationnel Moshi absent.",
            )
        } else {
            absent("Mimi Safetensors", "mimi_candle.safetensors")
        },
        match (sensevoice, tokens) {
            (true, true) => present(
                "SenseVoice ONNX",
                "Modèle et vocabulaire présents ; moteur mel/LFR, ort et CTC disponible. Intégrité, chargement et transcription non vérifiés ici.",
            ),
            (true, false) => absent("SenseVoice ONNX", "tokens.txt (SenseVoice)"),
            _ => absent("SenseVoice ONNX", "sensevoice_small.int8.onnx"),
        },
        if campplus {
            present(
                "CAM++ ONNX",
                "Modèle présent ; moteur Fbank, ort et embeddings locuteur disponible. Intégrité, chargement et forward non vérifiés ici.",
            )
        } else {
            absent("CAM++ ONNX", "campplus_zh_en_16k_advanced.onnx")
        },
        match (minilm, minilm_tok) {
            (true, true) => present(
                "MiniLM ONNX",
                "Modèle et tokenizer présents ; moteur WordPiece, ort et embeddings de texte disponible. Intégrité, chargement et forward non vérifiés ici.",
            ),
            (true, false) => absent("MiniLM ONNX", "minilm_tokenizer.json"),
            _ => absent("MiniLM ONNX", "all-minilm-l6-v2.onnx"),
        },
        if silero {
            present(
                "Silero VAD ONNX",
                format!(
                    "{models_count} artefacts, {total_disk_gb:.2} Go — modèle VAD ONNX présent. Intégrité, chargement, forward et latence non vérifiés ici."
                ),
            )
        } else {
            absent("Silero VAD ONNX", "silero_vad.onnx")
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: [&str; 11] = [
        "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        "tokenizer.json",
        "ui-tars-2b-sft-q4_k_m.gguf",
        "mmproj-UI-TARS-2B-SFT-f16.gguf",
        "mimi_candle.safetensors",
        "sensevoice_small.int8.onnx",
        "tokens.txt",
        "campplus_zh_en_16k_advanced.onnx",
        "all-minilm-l6-v2.onnx",
        "minilm_tokenizer.json",
        "silero_vad.onnx",
    ];

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
        // Volontairement invalide : la présence ne doit pas prouver un forward.
        std::fs::write(dir.join(name), b"not a valid model").unwrap();
    }

    #[test]
    fn all_rows_are_absent_without_artifacts_even_with_nonzero_summary() {
        let dir = temp_models("absent");
        let rows = honest_engine_rows(&dir, 100.0, 999);
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|row| row.status == InferenceStatus::Absent));
        assert!(
            rows.iter()
                .all(|row| row.detail.contains("artefact absent"))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn provisioned_fixtures_are_present_but_never_inspected_or_real() {
        let dir = temp_models("present");
        for name in FILES {
            touch(&dir, name);
        }
        let rows = honest_engine_rows(&dir, 1.5, FILES.len());
        assert_eq!(rows.len(), 7);
        assert!(
            rows.iter()
                .all(|row| row.status == InferenceStatus::Available)
        );
        assert!(rows.iter().all(|row| !row.status.is_real()));
        assert!(rows.iter().all(|row| row.detail.contains("non vérifiés")));
        assert!(rows.iter().all(|row| !row.detail.contains("p95")));
        let mimi = rows
            .iter()
            .find(|row| row.name.starts_with("Mimi"))
            .unwrap();
        assert!(mimi.detail.contains("codec neuronal Mimi complet"));
        assert!(mimi.detail.contains("Moshi absent"));
        let ui_tars = rows
            .iter()
            .find(|row| row.name.starts_with("UI-TARS"))
            .unwrap();
        assert!(ui_tars.detail.contains("serveur llama.cpp local"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn required_companions_and_projector_are_not_optional() {
        for (tag, model, companion) in [
            (
                "qwen",
                "qwen2.5-0.5b-instruct-q4_k_m.gguf",
                "tokenizer.json",
            ),
            ("sensevoice", "sensevoice_small.int8.onnx", "tokens.txt"),
            ("minilm", "all-minilm-l6-v2.onnx", "minilm_tokenizer.json"),
            (
                "ui",
                "ui-tars-2b-sft-q4_k_m.gguf",
                "mmproj-UI-TARS-2B-SFT-f16.gguf",
            ),
        ] {
            let dir = temp_models(tag);
            touch(&dir, model);
            let rows = honest_engine_rows(&dir, 0.0, 1);
            assert!(rows.iter().all(|row| row.status == InferenceStatus::Absent));
            assert!(rows.iter().any(|row| row.detail.contains(companion)));
            touch(&dir, companion);
            assert_eq!(
                honest_engine_rows(&dir, 0.0, 2)
                    .iter()
                    .filter(|row| row.status == InferenceStatus::Available)
                    .count(),
                1
            );
            std::fs::remove_file(dir.join(companion)).unwrap();
            assert!(
                honest_engine_rows(&dir, 0.0, 1)
                    .iter()
                    .all(|row| row.status == InferenceStatus::Absent)
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn directories_and_empty_files_do_not_count_as_provisioned_artifacts() {
        let dir = temp_models("invalid-paths");
        std::fs::create_dir(dir.join("mimi_candle.safetensors")).unwrap();
        std::fs::write(dir.join("silero_vad.onnx"), []).unwrap();
        assert!(
            honest_engine_rows(&dir, 0.0, 2)
                .iter()
                .all(|row| row.status == InferenceStatus::Absent)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn labels_and_serialization_distinguish_presence_from_real_execution() {
        assert_eq!(InferenceStatus::Available.label_fr(), "Présent — non testé");
        assert_eq!(InferenceStatus::Available.badge(), "PRÉSENT");
        for status in [
            InferenceStatus::Absent,
            InferenceStatus::Available,
            InferenceStatus::Inspected,
            InferenceStatus::Simulated,
            InferenceStatus::Real,
        ] {
            assert_eq!(status.is_real(), status == InferenceStatus::Real);
            let encoded = serde_json::to_string(&status).unwrap();
            assert_eq!(
                serde_json::from_str::<InferenceStatus>(&encoded).unwrap(),
                status
            );
        }
    }
}
