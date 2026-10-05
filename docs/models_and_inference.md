# Sélection des Modèles & Runtimes d'Inférence

Ce document consigne la matrice des modèles fondations ouverts retenus pour Annora, leurs runtimes d'exécution natifs en Rust, et les stratégies d'allocation mémoire.

---

## 1. Matrice Globale des Modèles Retenus

| Rôle Fonctionnel | Modèle Retenu | Paramètres | Empreinte / Quantification | Latence / RTF | Runtime Rust | Licence |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Écoute Continue & ASR Instantané** | **SenseVoice-Small** (Alibaba) | 234M | 229 Mo (INT8) | Non-autorégressif ; RTF ~0.02 GPU, ~0.08 CPU | ONNX Runtime (`ort` / `sherpa-onnx`) | MIT / FunASR |
| **Transcription Long Format (60 min)** | **VibeVoice-ASR** (Microsoft) | ~8.3B - 9B | 1.58 Go (BitNet I8/I2) ; ~9 Go (GGUF Q8) | BitNet RTF < 1 sur 3 cœurs CPU | `annora-inference` (GGUF / BitNet) | MIT |
| **Diarisation Continue en Direct** | **Nemotron-3-Diarization** (NVIDIA) | 100M | ~400 Mo (INT8) ; ~1.2 Go (FP16) | Tampon ajustable : 80 ms à 320 ms | ONNX Runtime (`ort`) | OpenMDW-1.1 |
| **Signature Biométrique Locuteur** | **CAM++** | 7.2M | ~30 Mo (INT8) | Sub-15 ms par segment | ONNX Runtime (`sherpa-onnx`) | Apache-2.0 |
| **Dialogue Full-Duplex Speech-to-Speech** | **Kyutai Moshi** (Kyutai Labs) | ~7.6B (7B Temporel + Depth) | ~8 Go VRAM (INT8) ; ~15 Go (BF16) | ~160 ms théorique (Codec Mimi 80ms + 80ms) | **Candle (Rust natif direct)** | Code: Apache/MIT, Poids: CC-BY 4.0 |
| **Contrôle Graphique (Computer-Use)** | **UI-TARS-7B-DPO** (ByteDance Seed) | 7B | ~4-6 Go (GGUF Q4_K_M) | ~2.5 s par décision visuelle | GGUF / Candle | Apache-2.0 |
| **Synthèse Vocale Secondaire (TTS)** | **CosyVoice2-0.5B** / **NeuTTS Air** | ~0.5B | ~800 Mo (INT8) / ~1.9 Go (GGUF) | ~150 ms en streaming | Candle / GGUF | Apache-2.0 |
| **Raisonnement Local System 1** | **Qwen3-8B** / **Qwen-7B** | 7B - 8B | ~4.5 Go (GGUF Q4_K_M) | 45-60 tokens/s | Candle / GGUF | Apache-2.0 |

---

## 2. Runtimes d'Inférence Sans Python

Pour garantir l'absence de runtime Python en production, trois moteurs sont orchestrés par `annora-inference` :

### A. Candle (Hugging Face)
- **Raison d'être** : Framework de calcul tensoriel natif écrit en Rust pur.
- **Usage principal** : Kyutai Moshi (code officiel Kyutai sous `rust/` dans leur dépôt), LLMs System 1 (Qwen) et modèles de synthèse vocale.
- **Accélération** : Metal sous macOS, CUDA sous Linux/Windows, CPU avec instructions AVX-512 / NEON.

### B. ONNX Runtime via `ort` & `sherpa-onnx`
- **Raison d'être** : Exécution ultra-légère et déterministe de modèles audio compacts.
- **Usage principal** : SenseVoice-Small (ASR, détection d'émotions et bruits ambiants en 229 Mo), Silero VAD, openWakeWord et CAM++.

### C. Moteur GGUF / BitNet
- **Raison d'être** : Chargement de gros modèles quantifiés sans conversion complexe.
- **Usage principal** : UI-TARS-7B pour l'agent GUI et VibeVoice-ASR pour la transcription d'enregistrements longs.

---

## 3. Gestionnaire de Mémoire et Pagination VRAM LRU

Sur les postes de travail grand public (ex: 8 à 16 Go de VRAM) :
- Les micro-modèles d'écoute passive (**SenseVoice-Small**, **CAM++**, **Silero VAD**) mobilisent ensemble moins de **300 Mo de mémoire** et restent résidents en permanence sur le CPU sans réveiller la carte graphique dédiée.
- Le modèle de contrôle d'interface graphique (**UI-TARS-7B-DPO**) et le transcripteur long (**VibeVoice-ASR**) sont configurés avec la stratégie `LoadingStrategy::OnDemand`.
- Dès qu'une tâche graphique est déclenchée, le `ModelRegistry` pagine UI-TARS en VRAM, et le décharge dès la fin de la séquence d'automatisation via un algorithme de remplacement LRU (Least Recently Used) pour restituer l'espace au modèle de raisonnement.
