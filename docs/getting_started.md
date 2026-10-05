# Guide de Prise en Main & Déploiement

Ce guide détaille pas à pas l'installation, la compilation, la configuration et l'exécution d'Annora.

---

## 1. Prérequis Système

- **Compilateur Rust** : Rust 1.85+ (avec support de l'édition Rust 2024).
  ```bash
  rustup update stable
  ```
- **Outils de Compilation C/C++ & CMake** : Indispensables pour compiler les liaisons natives C/FFI des runtimes d'inférence (Candle CUDA, ONNX Runtime).
  - Sous Windows : Visual Studio Build Tools avec le composant C++.
  - Sous Linux : `build-essential cmake pkg-config libasound2-dev libssl-dev`.
  - Sous macOS : Xcode Command Line Tools.

---

## 2. Compilation Complète du Projet

Depuis la racine du projet :

```bash
# 1. Vérification statique de l'ensemble des crates
cargo check --workspace

# 2. Compilation en mode debug (développement)
cargo build --workspace

# 3. Compilation optimisée (production)
cargo build --workspace --release
```

---

## 3. Exécution de la Suite de Tests & Conformance

Annora impose une discipline stricte de conformité avant toute mise en production :

### Tests Unitaires et d'Intégration
```bash
cargo test --workspace
```
Cela exécute les tests de :
- Chiffrement au repos ChaCha20-Poly1305 et signatures Ed25519 (`annora-core`).
- Atténuation de jetons Biscuit cryptographiques (`annora-core`).
- Indexation vectorielle HNSW et graphe SPO (`annora-memory`).
- Contrôle des budgets de portefeuilles (`annora-auth`).

### Suite de Conformance des Substrates (`cargo xtask conformance`)
```bash
cargo xtask conformance
```
Cette suite vérifie que chaque backend de système d'exploitation et chaque moteur d'accélération respecte formellement les garanties de capability et d'attestation du trait `Substrate`.

---

## 4. Démarrage du Démon Annora (`annora-daemon`)

Pour lancer le démon cognitif interactif :

```bash
cargo run -p annora-daemon
```

Le démon affiche les six étapes d'initialisation :
```text
============================================================
  Annora — Sovereign Cognitive Operating System (Rust Native)
  'Un système d'exploitation cognitif, pas un chatbot.'
============================================================

[1/6] Initializing Host Substrate Layer...
  Host OS: Windows 11 (24H2)
  Tier: Native
  Compute Backends: [Cuda, Vulkan, Cpu]
  Hardware Attestation: Windows-Substrate-Host (Valid)

[2/6] Loading Versioned Constitution & Security Primitives...
  Constitution Version: 1.0.0
  Active Rules: 10 rules enforced
  Audit Journal: Initialized append-only tamper-evident log

[3/6] Initializing Distributed Knowledge Graph & CRDT Sync...
  User Identity: 01a10ce2-402c-770c-be2b-daf41914e258 (Initialized)
  CRDT State: 0 tombstones, vector clock ready

[4/6] Bootstrapping Perception Pipeline & Moshi Voice Engine...
  Audio Pipeline: 16kHz continuous capture buffer ready
  SenseVoice-Small: Multi-task transcription & acoustic events loaded
  Moshi: Full-duplex speech-to-speech engine ready (160ms theoretical latency)

[5/6] Spawning Sandboxed WASM Agent Registry...
  Registered 5 reference WASM agents with strict Biscuit scopes

[6/6] Linking Cognitive Fabric & Orchestrator...
  Cognitive Fabric: System 1 / System 2 dynamic router online
  North-Star Metrics: Continuity, Bounded Initiative, Generality tracking active
  Presence State: Idle (Ready)

============================================================
Annora is running. Type a prompt, or a command (:help, :status, :metrics, :exit)
============================================================

annora> 
```

### Commandes Interactives Disponibles
- `:status` : Vérifie l'état de présence (`Idle`, `Listening`, `Acting`), le tier actif et les dépenses courantes.
- `:metrics` : Affiche les 3 métriques cardinales (Continuité, Respect d'initiative bornée, Généralité pratique) et l'audit de dérive éthique.
- `:agents` : Affiche les agents WASM enregistrés et leurs restrictions de capabilities.
- `:cost` : Affiche le montant cumulé consommé pour les inférences lourdes System 2.
- `:help` : Affiche l'aide des commandes.
- `:exit` : Décharge les modèles et clôture proprement le démon.
