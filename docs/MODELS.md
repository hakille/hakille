# Spécification des Modèles Fondations et Runtimes d'Inférence

> *"Zéro runtime Python en production : du tenseur brut au code machine natif via Candle, ONNX Runtime et des parseurs binaires GGUF purs."*

Ce document constitue la référence exhaustive de la couche neuronale d'Annora ([`crates/annora-inference`](../crates/annora-inference)). Il détaille la sélection des sept modèles fondations, leurs métriques sur les benchmarks ouverts, leurs spécifications de quantification, ainsi que l'implémentation bas niveau des parseurs de formats de poids (**GGUF v3**, **Safetensors**, **ONNX**) et le registre de mémoire VRAM avec éviction LRU.

---

## 1. Les 7 Modèles Fondations d'Annora

L'ingénierie d'Annora privilégie une **chaîne modulaire découplée** où chaque composant excelle dans sa tâche plutôt qu'un modèle omnimodal monolithique opaque et énergivore.

```
+====================================================================================================+
| 1. ÉCOUTE & ASR INTERACTIF | SenseVoice-Small (234M) · Non-autorégressif · VAD, LID, Émotions, Sons|
+----------------------------------------------------------------------------------------------------+
| 2. AUDIO LONG & RÉUNIONS   | VibeVoice-ASR (8.3B) · Décodeur Qwen2 · Fenêtre 64k tokens · BitNet   |
+----------------------------------------------------------------------------------------------------+
| 3. DIARISATION CONTINUE    | Nemotron-3-Diarization (100M) · Sortformer · Jusqu'à 8 locuteurs      |
+----------------------------------------------------------------------------------------------------+
| 4. BIOMÉTRIE VOCALE        | CAM++ (7.2M) · 192-d Embeddings · Déterministe en enclave matérielle  |
+----------------------------------------------------------------------------------------------------+
| 5. DIALOGUE FULL-DUPLEX    | Kyutai Moshi (7.6B) + Codec Mimi (12.5 Hz, 80ms) · sub-160ms natif    |
+----------------------------------------------------------------------------------------------------+
| 6. CONTRÔLE D'INTERFACE    | UI-TARS-7B-DPO / 2B · Qwen2-VL · OSWorld SOTA · Click, Type, Drag     |
+----------------------------------------------------------------------------------------------------+
| 7. RAISONNEMENT LOCAL      | Qwen3-8B / Qwen2.5-7B (Q4_K_M) · Contexte 131K · Système 1.5 interne  |
+====================================================================================================+
```

---

### Détail des 7 Architectures

#### 1. SenseVoice-Small (Alibaba)
- **Rôle** : Front-end d'écoute continue et ASR interactif ultra-rapide.
- **Paramètres** : 234 millions (non-autorégressif).
- **Capacités condensées** : Transcription ASR (mandarin, anglais, japonais, coréen, cantonais), détection implicite de voix (VAD), identification de langue (LID), reconnaissance d'émotions vocales (joie, colère, tristesse) et classification d'événements sonores (toux, rire, applaudissements, musique).
- **Performances** : Facteur temps réel (RTF) de 0.02 sur GPU et < 0.08 sur CPU. 15x plus rapide que Whisper-Large.
- **Empreinte** : INT8 ONNX = 229 Mo d'espace disque, < 1 Go de RAM résidente.
- **Runtime** : ONNX Runtime (`ort`) via `crates/annora-inference/src/onnx_runtime.rs`.
- **Licence** : FunASR Open Source / MIT.

#### 2. VibeVoice-ASR (Microsoft Research)
- **Rôle** : Ingestion d'enregistrements audio longs (réunions, conférences, dictées jusqu'à 60 minutes d'un seul bloc).
- **Paramètres** : ~8.3 à 9 milliards (décodeur basé sur Qwen2, 28 couches).
- **Fenêtre contextuelle** : 64 000 tokens audio/texte.
- **Précision** : WER moyen de 7.77 % sur l'Open ASR Leaderboard (2.20 % sur LibriSpeech Clean). Taux d'erreur de diarisation (DER) de 4.28 % sur MLC-Challenge.
- **Variante BitNet** : Quantification hybride I8_S / I2_S compressant le modèle à 1.58 Go, garantissant un RTF < 1 sur seulement 3 threads CPU x86/ARM sans GPU.
- **Licence** : MIT.

#### 3. Nemotron-3-Diarization (NVIDIA)
- **Rôle** : Séparation temporelle continue des flux de locuteurs en direct (jusqu'à 8 personnes).
- **Paramètres** : 100 millions (topologie Sortformer).
- **Entrée/Sortie** : Spectrogrammes de Mel 16 kHz vers tenseurs de probabilités d'activité par pas de 10 ms.
- **Profils de latence** : Tampon glissant paramétrable : mode ultra-basse latence (80 ms à 320 ms) ou mode haute fidélité (1.04 s).
- **Empreinte** : FP16 = ~1.2 Go VRAM, INT8 = ~400 Mo VRAM.
- **Licence** : OpenMDW-1.1 (usage commercial autorisé).

#### 4. CAM++ (Alibaba ModelScope)
- **Rôle** : Extraction d'empreintes biométriques vocales mono-locuteur.
- **Paramètres** : 7.2 millions.
- **Sortie** : Vecteur compact de 192 dimensions.
- **Latence** : < 15 ms par segment de parole.
- **Sécurité** : Le modèle n'apprend aucune identité dans ses poids. Le vecteur extrait est comparé par distance cosinus **exclusivement au sein du TPM ou du Secure Enclave** contre le gabarit chiffré de l'utilisateur.
- **Licence** : Apache 2.0.

#### 5. Kyutai Moshi & Codec Mimi (Kyutai Labs)
- **Rôle** : Dialogue oral bidirectionnel *full-duplex* sans tour de parole.
- **Paramètres** : 7.6 milliards (Temporal Transformer 7B + Depth Transformer).
- **Codec Mimi** : Échantillonnage 24 kHz compressé à 12.5 Hz (1.1 kbps) avec une latence causale de trame de **80 millisecondes**.
- **Monologue Intérieur (*Inner Monologue*)** : Projette des tokens textuels alignés dans le temps juste avant de générer les tokens audio, stabilisant la grammaire et fournissant une transcription temps réel gratuite.
- **Latence de bout en bout** : ~160 ms théorique, ~200 ms mesuré sur carte NVIDIA L4.
- **Runtime** : Implémentation **100 % Rust native** basée sur le framework **Candle** (`crates/annora-inference/src/candle_runtime.rs`).
- **Licence** : Code Apache 2.0 / MIT, Poids CC-BY 4.0.

#### 6. UI-TARS (ByteDance Seed / Tsinghua)
- **Rôle** : Agent autonome de contrôle d'interface graphique (*Computer-Use*).
- **Paramètres** : 7B (DPO) ou 2B (SFT), adossé à un encodeur visuel ViT de 675 millions de paramètres.
- **Perception** : Vision pure sur capture d'écran brute sans arbre DOM ni sélecteurs d'accessibilité.
- **Résolution spatiale** : 94.2 % d'exactitude de coordonnées sur ScreenSpot-v2, 27.5 % à 47.5 % de taux de réussite sur OSWorld (surpassant Claude 3.5 Sonnet en boucle fermée).
- **Espace d'actions** : `Click(x, y)`, `Type(text)`, `Hotkey(keys)`, `Scroll(dir, amt)`, `Drag(x1, y1, x2, y2)`.
- **Licence** : Apache 2.0.

#### 7. Qwen3-8B / Qwen2.5-7B (Alibaba)
- **Rôle** : Raisonnement local Système 1.5, structuration JSON, planification et génération textuelle.
- **Contexte** : 131 072 tokens natifs.
- **Quantification** : Format GGUF Q4_K_M (4.8 Go de VRAM) ou Q8_0 (8.5 Go de VRAM).
- **Runtime** : Exécution locale via Candle et le parseur GGUF v3 natif.

---

## 2. Runtimes et Parseurs Binaires Développés en Rust

Le crate `annora-inference` intègre ses propres parseurs et moteurs sans dépendre de wrappers CPython :

### 2.1. Parseur Binaire Streaming GGUF v3 (`gguf_runtime.rs`)
Le parseur lit directement la spécification binaire GGUF v3 petit-boutiste (*little-endian*) :
- **Magic Number** : `0x46554747` (`"GGUF"`).
- **En-tête** : Version, nombre de tenseurs (`tensor_count`), nombre de paires métadonnées clé-valeur (`metadata_kv_count`).
- **Types de métadonnées supportés** : `Uint8`, `Int8`, `Uint16`, `Int16`, `Uint32`, `Int32`, `Float32`, `Bool`, `String`, `Array`, `Uint64`, `Int64`, `Float64`.
- **Alignement des tenseurs** : Calcule les décalages d'alignement mémoire (32 octets par défaut) et extrait la topologie de chaque couche (`dimensions`, `tensor_type`, `offset`).
- **Support des quantifications** : Gère `Q4_0`, `Q4_1`, `Q5_0`, `Q5_1`, `Q8_0`, `Q4_K`, `Q5_K`, `Q6_K`, `F16`, `F32`.

### 2.2. Parseur Safetensors Natif (`candle_runtime.rs`)
Pour les modèles distribués au format Hugging Face Safetensors (ex: checkpoints Moshi et Mimi) :
- **En-tête JSON** : Lecture des 8 premiers octets non signés indiquant la taille $N$ du header JSON, puis désérialisation sans copie (*zero-copy*).
- **Métadonnées de Tenseurs** : Extrait `dtype` (`F32`, `BF16`, `F16`, `I32`), `shape` (dimensions) et les plages d'octets `data_offsets: [debut, fin]`.
- **Tampon `TensorBuffer`** : Implémente l'algèbre tensorielle de base (argmax, softmax, calcul de produit matriciel et normalisation) optimisée avec vectorisation SIMD.

### 2.3. Inspecteur ONNX Runtime (`onnx_runtime.rs`)
Encapsule la bibliothèque dynamique ONNX Runtime via `ort` :
- **Sessions Déclaratives** : `OnnxSession` configurant le nombre de threads d'exécution et les noms des tenseurs d'entrée et de sortie.
- **Exécution d'Inférence** : Conversion des tampons `TensorBuffer` vers les tenseurs du runtime et dispatch sur les cibles matérielles sélectionnées (CUDA, DirectML, CoreML, CPU).

---

## 3. Gestion de la Mémoire Vive et Registre de Pagination LRU (`ModelRegistry`)

La coexistence de modèles vocaux, visuels et linguistiques sur un poste personnel disposant de 8 à 16 Go de mémoire exige une gestion rigoureuse de l'enveloppe VRAM :

```rust
pub struct ModelRegistry {
    max_memory_budget_mb: u64,
    current_memory_used_mb: u64,
    loaded_models: HashMap<ModelReference, ModelMetadata>,
}
```

### Stratégie d'Éviction LRU (*Least Recently Used*)
1. **Priorité Résidente Permanente** : Les modèles légers d'écoute continue (`SenseVoice-Small` à ~230 Mo et `CAM++` à ~30 Mo) sont marqués `LoadingStrategy::PermanentResident` et ne sont **jamais déchargés**.
2. **Chargement à la Demande avec Éviction** : Les modèles lourds (`UI-TARS 7B` à ~5 Go ou `VibeVoice-ASR` à ~9 Go) sont chargés uniquement lors de la demande d'action graphique ou d'ingestion de fichier.
3. Si la mémoire requise dépasse le budget alloué (`max_memory_budget_mb`), le registre décharge automatiquement le modèle inactif le plus ancien (`evict_lru()`) pour faire de la place.

---

## 4. Matrice de Synthèse des Modèles
 
| Modèle | ID Catalogue | Format | Paramètres | Empreinte VRAM / RAM | Latence Étage | Rôle dans Annora |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **SenseVoice-Small** | `sensevoice-small-int8` | ONNX INT8 | 234M | 229 Mo (RAM < 1 Go) | < 50 ms | ASR réactif, VAD, Émotions, Audio Events |
| **VibeVoice-ASR** | *(Traitement différé)* | BitNet / GGUF | ~8.3B | 1.58 Go (BitNet) / 9 Go | Traitement différé | Transcription longue durée (jusqu'à 60 min) |
| **Nemotron-3-Diar** | *(Intégré perception)* | TensorRT / ONNX | 100M | 400 Mo (INT8) | 80 ms - 320 ms | Diarisation continue jusqu'à 8 locuteurs |
| **CAM++** | `campplus-speaker-id` | ONNX | 7.2M | 30 Mo | < 15 ms | Signature biométrique vocale (192-d) |
| **Kyutai Moshi / Mimi**| `moshi-mimi-s2s` | Safetensors / Q8 | 7.6B | ~8 Go (INT8) | ~160 ms | Voix Full-Duplex (Mimi + Moshi) |
| **UI-TARS 2B / 7B** | `ui-tars-2b-sft-q4km` | GGUF Q4_K_M | 2B / 7B | ~1.1 Go à 4.8 Go | ~2.5 s / action | Contrôle autonome d'interface graphique |
| **Qwen2.5 / Qwen3** | `qwen2.5-0.5b-instruct-q4km` | GGUF Q4_K_M | 0.5B à 8B | ~470 Mo à 4.9 Go | < 100 ms | Raisonnement local Système 1 & Intention |
| **Silero VAD v5** | `silero-vad-v5` | ONNX | ~1M | 2.2 Mo (RAM 15 Mo) | < 1 ms | Détection d'activité vocale temps réel |
| **All-MiniLM-L6-v2** | `all-minilm-l6-v2` | ONNX | 22M | 86.2 Mo (RAM 90 Mo) | < 10 ms | Embeddings vectoriels 384-d pour HNSW |
