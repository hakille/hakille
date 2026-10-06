# Annora — Sovereign Cognitive Operating System (Rust Native)

[![Rust Version](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()
[![Conformance](https://img.shields.io/badge/conformance-27%2F27%20passing-brightgreen.svg)]()
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20AGPL--3.0-blue.svg)](LICENSE-APACHE)
[![Security](https://img.shields.io/badge/security-Biscuit%20Capabilities-red.svg)]()
[![No Python](https://img.shields.io/badge/python-0%25%20(Pure%20Rust)-purple.svg)]()

> *"Un système d'exploitation cognitif souverain, pas un chatbot."*

---

## 🧭 Vision et Philosophie

Annora n'est ni une application de chat, ni un wrapper d'API, ni un agent prompté dans un navigateur. C'est une **couche système cognitive souveraine** pensée dès le départ pour :

1. **La Continuité d'État (*No-Session Continuity*)** : Aucun concept de session qui s'achève. Un fil de vie continu persistant dans un graphe de connaissances distribué.
2. **L'Initiative Bornée (*Bounded Initiative*)** : Le système agit et anticipe de façon proactive dans un périmètre strict garanti par des contrats cryptographiques et un budget de silence.
3. **La Généralité Pratique (*Practical Generality*)** : Coder, auditer des contrats, planifier des événements, piloter l'environnement graphique ou la domotique sans changer de contexte.
4. **L'Éradication Absolue de Python en Production (*Zero-Python Runtime*)** : Tous les modèles (ASR, Full-Duplex Speech, VLM, LLM) s'exécutent en Rust natif via **Candle**, **ONNX Runtime (`ort`)** et des chargeurs binaires GGUF v3 purs, garantissant sûreté mémoire et absence de garbage collector.

---

## 🏛️ Architecture en 6 Couches

```
+===================================================================================+
|                            6. CONFIANCE & AUDIT                                   |
|   Journal Append-Only Ed25519 · Chaînage Cryptographique SHA-256 · Constitution    |
|   Registre de Consentement · Détecteur Anti-Usurpation Vocale · Batterie Éthique   |
+===================================================================================+
                                         ▲
                                         │ Validation des invariants & Traçabilité
                                         ▼
+===================================================================================+
|                            5. ACTION (AGENTS WASM)                                |
|   Runtime WASM/WASI (Wasmtime) · Capabilities Biscuit · Quotas d'Appels Stricts   |
|   Agent GUI UI-TARS (Grounding Visuel) · Protocole Inter-Agents Signé Ed25519     |
+===================================================================================+
                                         ▲
                                         │ Déclenchement d'intentions
                                         ▼
+===================================================================================+
|                            4. MÉMOIRE DISTRIBUÉE                                  |
|   Graphe de Triplets SPO · Stockage Local ACID redb · Index Vectoriel HNSW        |
|   Cluster Distribué TiKV · Sync CRDT Multi-Device · Décroissance Temporelle       |
+===================================================================================+
                                         ▲
                                         │ Faits & Contextes
                                         ▼
+===================================================================================+
|                            3. NOYAU COGNITIF                                      |
|   Cognitive Fabric · Routeur Dynamique System 1/2 · Working Memory Scratchpad     |
|   Deliberate Planner · Self-Model Introspectif · Graph Curiosity Scanner          |
+===================================================================================+
                                         ▲
                                         │ Événements perceptifs
                                         ▼
+===================================================================================+
|                            2. PERCEPTION MULTIMODALE                              |
|   SenseVoice-Small · Kyutai Moshi Full-Duplex · CAM++ Biométrie · Vision Scene    |
|   Codec Mimi (12.5 Hz, 80ms) · Barge-In Interruption · Persistent WorldModel      |
+===================================================================================+
                                         ▲
                                         │ Abstraction matérielle & Télémétrie
                                         ▼
+===================================================================================+
|                            1. COUCHE OS (SUBSTRATE & COMPUTE)                     |
|   Abstraction Substrate & Compute (CUDA / Metal / Vulkan / WebGPU / CPU)          |
|   [Windows Win32] [Linux Seccomp] [macOS Seatbelt] [Redox OS] [Mobile] [Web WASM] |
+===================================================================================+
```

---

## 📦 Structure du Workspace (20 Crates + Conformance)

Le projet est un monorepo Cargo structuré en **20 crates modulaires**, **5 agents WASM de référence**, une suite de conformité exhaustive et des utilitaires `xtask` :

### 1. Fondations & Substrates OS
- [`annora-core`](crates/annora-core) : Primitives cryptographiques (Ed25519, ChaCha20Poly1305), jetons Biscuit, types temporels, bus de messages typés (`Message`) et niveaux de confiance (`TrustLevel`).
- [`annora-substrate`](crates/annora-substrate) : Traits d'abstraction OS (`Substrate`) et d'accélération matérielle (`Compute`), attestations TPM/Secure Enclave.
- [`annora-substrate-windows`](crates/annora-substrate-windows) : Backend Windows Win32 natif (GDI BitBlt display capture, injection `SendInput`, bac à sable AppContainer, surveillance RAM/Batterie, présence Tray et hotkey `Alt+Space`).
- [`annora-substrate-linux`](crates/annora-substrate-linux) : Backend Linux natif (compilateur Seccomp-BPF avec simulateur de bytecode, Namespaces PID/Mount/Net, Cgroups v2, télémétrie `/proc` et `/sys`, injection `/dev/uinput`, Wayland/X11).
- [`annora-substrate-macos`](crates/annora-substrate-macos) : Backend macOS Darwin natif (profils Sandbox Seatbelt `(deny default)`, Endpoint Security muting, Mach `host_statistics64`, IOKit, capture Retina 2x CoreGraphics).
- [`annora-substrate-redox`](crates/annora-substrate-redox) : Backend micro-noyau souverain Redox OS (schemes natifs `chan:`, `orbital:`, `disk:`, `sys:`, `audio:`, sérialisation `RedoxPacket`, table de capacités microkernel).
- [`annora-substrate-mobile`](crates/annora-substrate-mobile) : Backend mobile Android/iOS (synchronisation d'ombre *Shadow Sync* avec horloges de Lamport, scoped storage, machine à états Doze).
- [`annora-substrate-web`](crates/annora-substrate-web) : Backend WebAssembly pour navigateur (bridge Web Audio avec rééchantillonnage 48kHz -> 16kHz, tampon anti-overrun, Canvas ImageData, limites WebGPU).

### 2. Mémoire & Synchronisation
- [`annora-memory`](crates/annora-memory) : Graphe en triplets SPO avec triple indexation (SPO, POS, OSP), backend local ACID `redb`, index vectoriel sémantique HNSW `instant-distance`, GraphQueryLang DSL, déclin temporel et résolution des contradictions.
- [`annora-sync`](crates/annora-sync) : Synchronisation CRDT (grow-only sets + tombstones, vector clocks), handoff de tâche cross-device et réplication TiKV.

### 3. Perception, Voix & Inférence Neuronale
- [`annora-perception`](crates/annora-perception) : Pipeline audio (SenseVoice-Small, Silero VAD, openWakeWord, CAM++ speaker ID, Nemotron-3 diarisation), CameraPipeline et WorldModel situationnel continu.
- [`annora-voice`](crates/annora-voice) : Dialogue parlé full-duplex sans tour de parole basé sur Kyutai Moshi en Candle Rust, codec Mimi (12.5 Hz, 80ms causal), CosyVoice2 TTS et détecteur d'interruption naturelle Barge-In.
- [`annora-inference`](crates/annora-inference) : Runtimes neuronaux natifs (parseur binaire streaming GGUF v3, parseur Safetensors Candle, inspecteur ONNX Runtime `ort`, registre LRU de pagination VRAM).

### 4. Cognition, Agents, Confiance & UI
- [`annora-kernel`](crates/annora-kernel) : Fabric cognitif, routeur System 1/2, Working Memory scratchpad, Deliberate Planner, Self-Model introspectif, Graph Curiosity Scanner, Dry-Run Simulator et budgets de latence par étage.
- [`annora-agents`](crates/annora-agents) : Runtime WASM/WASI (Wasmtime) avec vérification de capabilities Biscuit, quotas d'appels et auto-révocation, agent GUI UI-TARS et protocole inter-agents signé.
- [`annora-audit`](crates/annora-audit) : Journal append-only signé Ed25519 avec chaînage SHA-256, registre de consentement explicite, moteur de requêtes et d'explicabilité causale.
- [`annora-auth`](crates/annora-auth) : Authentification multimodale continue, détecteur anti-usurpation vocale (liveness), gestionnaire de sessions invités, profils mineurs et capability Wallet.
- [`annora-constitution`](crates/annora-constitution) : Les 10 règles constitutionnelles machine-vérifiables, gouvernance d'amendement par quorum et batterie de tests éthiques.
- [`annora-ui`](crates/annora-ui) : Indicateur de présence ambiant à 3 états, HUD Spotlight éphémère (`Alt+Space`), sélecteur dynamique de canal, budget de silence proactif et compteur financier System 2 en direct.
- [`annora-metrics`](crates/annora-metrics) : Tableau de bord des 3 métriques cardinales (Continuité, Initiative Bornée, Généralité Pratique) et détecteur de dérive des valeurs éthiques.
- [`annora-daemon`](crates/annora-daemon) : Démon système exécutable orchestrant l'ensemble de la pile avec invite interactive REPL et inspection approfondie.

### 5. Agents Sandboxés de Référence (`agents/`)
- `mail` : Gestion des courriels sous capability `Filesystem` + `Network` scopée.
- `calendar` : Planification d'agenda locale.
- `code` : Agent d'inspection et de transformation de code.
- `home` : Gestion des actionneurs domotiques.
- `wallet-readonly` : Inspection des soldes et autorisations budgétaires.

---

## 🎯 Paliers de Déploiement Matériel (Tiers)

| Palier (Tier) | Cible Matérielle | Perception Audio & Vision | Raisonnement & Mémoire | Rôle et Confinement |
| :--- | :--- | :--- | :--- | :--- |
| **Souverain** | Station dédiée Redox OS / Linux Souverain | SenseVoice-Small + VibeVoice (60m) + Moshi (Full-Duplex sub-200ms) + UI-TARS 7B | Qwen3-8B local + escalade System 2 + redb / cluster TiKV | Contrôle total des schemes natifs, zéro sandbox intermédiaire. |
| **Natif** | Laptop Windows, Linux, macOS | SenseVoice-Small (veille CPU) + CAM++ + CosyVoice2 / NeuTTS + UI-TARS (on demand) | Qwen 7B (Q4) local + redb local | Contrôle quasi-total via sandboxing OS (AppContainer, Seccomp, Seatbelt). |
| **Invité** | Mobile Android / iOS / Web WASM | Moonshine + SenseVoice Mobile (NPU/CoreML) + WebAudio 16kHz | Ombre déportée sur Hub personnel via sync CRDT | Accès restreint par l'OS hôte, continuité portée par le cluster. |

---

## 🛡️ Modèle de Sécurité et Garanties Formelles

1. **Biscuit Capabilities** : Toute action (accès disque, réseau, frappe clavier, dépense) requiert un jeton Biscuit signé. Les sous-agents ne reçoivent que des jetons *atténués* (privilèges strictement décroissants).
2. **Tag `Untrusted` Immuable** : Tout contenu ingéré depuis l'extérieur (courriels, pages web, documents, OCR) est marqué `TrustLevel::Untrusted`. Il ne peut jamais être promu en instruction motrice, neutralisant les prompt injections.
3. **Journal Append-Only Chaîné** : Toutes les actions motrices sont enregistrées avec signature Ed25519, empreinte SHA-256 chaînée ($H_i = \text{SHA256}(H_{i-1} \parallel \text{Entry}_i)$) et pointeurs vers les faits du graphe justifiant la décision.
4. **Constitution Inviolable** : 10 règles constitutionnelles machine-vérifiables s'imposent à tout composant et surpassent toute instruction humaine.
5. **Anti-Spoofing Biométrique** : Le `VoiceLivenessDetector` inspecte les micro-variations physiques et neutralise les clones vocaux neuronaux et attaques par rejeu.

---

## 🚀 Démarrage Rapide

### Prérequis
- Rust 1.85+ (avec support de l'édition Rust 2024)
- CMake & Compilateur C/C++ (MSVC sous Windows, GCC/Clang sous Linux/macOS)

### Compilation et Tests
```bash
# Vérifier la compilation de tous les crates
cargo check --workspace

# Exécuter l'intégralité des tests unitaires et d'intégration
cargo test --workspace

# Exécuter la suite de conformité formelle (27/27 tests vérifiés)
cargo xtask conformance
```

### Lancement du Démon Annora
```bash
cargo run -p annora-daemon
```

Dans l'invite interactive (`annora>`) :
```text
annora> :status           # Afficher le Tier matériel, RAM et état de présence
annora> :metrics          # Inspecter les 3 métriques North-Star et l'alignement éthique
annora> :models           # Lister le catalogue des modèles et l'état du cache
annora> :connected        # Afficher la connexion vérifiée des 7 modèles fondations
annora> :inspect-models   # Inspection binaire des tenseurs GGUF, Safetensors et ONNX
annora> :hud              # Tester le HUD Spotlight éphémère (Alt+Space)
annora> :channel          # Afficher l'arbitrage dynamique du Sélecteur de Canal
annora> :budget           # Consulter le quota restant du Budget de Silence
annora> :agents           # Lister les agents WASM sandboxés et leurs capabilities
annora> :cost             # Suivre la consommation financière en temps réel
annora> :memory           # Afficher les triplets SPO du graphe de connaissances
annora> :remember Bob habite à Paris
annora> :recall Paris     # Rechercher dans la mémoire sémantique
annora> :exit             # Arrêt propre du démon
```

---

## 📚 Documentation Technique Détaillée

Pour explorer chaque sous-système en profondeur, consultez les guides dédiés :

- 🏛️ [**Architecture Système Complète**](docs/ARCHITECTURE.md) : Les 6 couches, flux de données, graphe des dépendances inter-crates et invariants.
- ⚙️ [**Spécification des Substrates OS**](docs/SUBSTRATES.md) : Implémentations natives des 6 OS (Windows, Linux, macOS, Redox, Mobile, Web), sandboxes et télémétrie.
- 🧠 [**Noyau Cognitif & Mémoire Distribuée**](docs/COGNITION.md) : Système 1/2, Cognitive Fabric, Working Memory, Deliberate Planner, Self-Model, Curiosité, Graphe SPO et DSL.
- 🤖 [**Modèles Fondations & Runtimes d'Inférence**](docs/MODELS.md) : Spécifications des 7 modèles, parseurs binaires GGUF v3 / Safetensors / ONNX et registre LRU VRAM.
- 🛡️ [**Sécurité, Capabilities & Gouvernance**](docs/SECURITY_AND_GOVERNANCE.md) : Jetons Biscuit, Registre de Consentement, Journal chaîné Ed25519, Constitution et Anti-Spoofing.
- 👁️ [**Interfaces Ambiantes & Perception**](docs/INTERFACES_AND_PERCEPTION.md) : Présence discrète, HUD Spotlight éphémère, Sélecteur de Canal, Budget de Silence, World Model et Moshi Full-Duplex.
- 💻 [**Guide du Développeur & Manuel d'Ingénierie**](docs/DEVELOPER_GUIDE.md) : Guide pas à pas de compilation, suite de conformance 27/27, commandes CLI REPL et création d'agents WASM.
- 📋 [**Index des Décisions d'Architecture (ADRs)**](adr/) : Historique et justifications des choix techniques fondamentaux.

---

## ⚖️ Licences

- Composants Edge et Client (`annora-core`, `annora-kernel`, `annora-memory`, `annora-substrate-*`, `annora-agents`, `annora-ui`, `annora-inference`) : **Apache License 2.0**.
- Composants Serveur et Cluster (`annora-sync`, wrappers de réplication TiKV) : **AGPLv3**.
