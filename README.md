<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="logo/hakille-logo-dark.png">
    <img src="logo/hakille-logo.png" alt="Hakille" width="160">
  </picture>
</p>

# Hakille — Sovereign Cognitive Operating System (Rust Native)

[![Rust Version](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)
[![Conformance](https://img.shields.io/badge/conformance-66%20fast%20%2B7%20slow%20ignored-yellow.svg)](.github/workflows/ci.yml)
[![Security](https://img.shields.io/badge/security-Biscuit%20Capabilities-red.svg)]()
[![No Python](https://img.shields.io/badge/python-0%25%20(Pure%20Rust)-purple.svg)]()

> *"Un système d'exploitation cognitif souverain, pas un chatbot."*

> **Statut : prototype / pré-alpha.** Voir [SECURITY.md](SECURITY.md) et
> [l'audit du 2026-10-08](docs/AUDIT_PROJET_2026-10-08.md) : l'inférence est
> **RÉELLE et golden-testée** (forwards `ort` VAD/ASR/CAM++/MiniLM, Qwen 2.5
> Candle, RVQ Mimi, TTS formantique, grounding CV) ; le VLM UI-TARS reste
> inspecté sans forward, le modèle neuronal Moshi 7,6B n'est pas provisionné
> (duplex RVQ/VAD v1, voir `MoshiEngine::is_simulated()`), l'attestation n'est
> pas matérielle, et le sandboxing WASM est partiel. Ne pas utiliser avec des
> secrets de production, de la biométrie réelle ou des paiements autonomes.
> Détail par moteur : [docs/MODELS.md](docs/MODELS.md).

---

## 🧭 Vision et Philosophie

Hakille n'est ni une application de chat, ni un wrapper d'API, ni un agent prompté dans un navigateur. C'est une **couche système cognitive souveraine** pensée dès le départ pour :

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

> Le bloc « Kyutai Moshi Full-Duplex » désigne la **cible** : le runtime actuel
> fournit un duplex RVQ Mimi + VAD réel, sans modèle neuronal Moshi 7,6B
> (`MoshiEngine::is_simulated() == true`). Le détail par moteur est dans
> [docs/MODELS.md](docs/MODELS.md).

---

## 📦 Structure du Workspace (31 Crates + 5 Agents + Conformance)

Le projet est un monorepo Cargo structuré en **31 crates modulaires**, **5 agents WASM de référence**, une suite de conformité exhaustive (73 tests : 66 fast + 7 slow ignorés, voir [CI](.github/workflows/ci.yml)) et des utilitaires `xtask` :

### 1. Fondations & Substrates OS
- [`hakille-core`](crates/hakille-core) : Primitives cryptographiques (Ed25519, ChaCha20Poly1305), jetons Biscuit, types temporels, bus de messages typés (`Message`) et niveaux de confiance (`TrustLevel`).
- [`hakille-substrate`](crates/hakille-substrate) : Traits d'abstraction OS (`Substrate`) et d'accélération matérielle (`Compute`), attestations TPM/Secure Enclave.
- [`hakille-substrate-windows`](crates/hakille-substrate-windows) : Backend Windows Win32 natif (GDI BitBlt display capture, injection `SendInput`, bac à sable AppContainer, surveillance RAM/Batterie, présence Tray et hotkey `Ctrl+Alt+Space`, alias `Alt+Space`).
- [`hakille-substrate-linux`](crates/hakille-substrate-linux) : Backend Linux natif (compilateur Seccomp-BPF avec simulateur de bytecode, Namespaces PID/Mount/Net, Cgroups v2, télémétrie `/proc` et `/sys`, injection `/dev/uinput`, Wayland/X11).
- [`hakille-substrate-macos`](crates/hakille-substrate-macos) : Backend macOS Darwin natif (profils Sandbox Seatbelt `(deny default)`, Endpoint Security muting, Mach `host_statistics64`, IOKit, capture Retina 2x CoreGraphics).
- [`hakille-substrate-redox`](crates/hakille-substrate-redox) : Backend micro-noyau souverain Redox OS (schemes natifs `chan:`, `orbital:`, `disk:`, `sys:`, `audio:`, sérialisation `RedoxPacket`, table de capacités microkernel).
- [`hakille-substrate-mobile`](crates/hakille-substrate-mobile) : Backend mobile Android/iOS (synchronisation d'ombre *Shadow Sync* avec horloges de Lamport, scoped storage, machine à états Doze).
- [`hakille-substrate-web`](crates/hakille-substrate-web) : Backend WebAssembly pour navigateur (bridge Web Audio avec rééchantillonnage 48kHz -> 16kHz, tampon anti-overrun, Canvas ImageData, limites WebGPU).

### 2. Mémoire & Synchronisation
- [`hakille-memory`](crates/hakille-memory) : Graphe en triplets SPO avec triple indexation (SPO, POS, OSP), backend local ACID `redb`, index vectoriel sémantique HNSW `instant-distance`, GraphQueryLang DSL, déclin temporel et résolution des contradictions.
- [`hakille-sync`](crates/hakille-sync) : Synchronisation CRDT (grow-only sets + tombstones, vector clocks), handoff de tâche cross-device et réplication TiKV.

### 3. Perception, Voix & Inférence Neuronale
- [`hakille-perception`](crates/hakille-perception) : Pipeline audio (SenseVoice-Small, Silero VAD, openWakeWord, CAM++ speaker ID, Nemotron-3 diarisation), CameraPipeline et WorldModel situationnel continu.
- [`hakille-audio`](crates/hakille-audio) : I/O audio temps réel via `cpal` — capture micro 16 kHz (downmix + rééchantillonnage) et lecture PCM ; aucune simulation, erreur explicite si aucun périphérique.
- [`hakille-voice`](crates/hakille-voice) : Moteur vocal honnête — décodeur RVQ Mimi (12,5 Hz) en Rust, TTS formantique Klatt (`FormantTts`, 24 kHz) et détecteur d'interruption Barge-In ; `MoshiEngine::is_simulated()` signale `true` tant que les vrais poids Mimi ne sont pas chargés.
- [`hakille-inference`](crates/hakille-inference) : Runtimes neuronaux natifs (parseur binaire streaming GGUF v3, parseur Safetensors Candle, inspecteur ONNX Runtime `ort`, registre LRU de pagination VRAM) et provisioner d'artefacts.

### 4. Cognition, Agents, Confiance & UI
- [`hakille-kernel`](crates/hakille-kernel) : Fabric cognitif, routeur System 1/2, Working Memory scratchpad, Deliberate Planner, Self-Model introspectif, Graph Curiosity Scanner, Dry-Run Simulator et budgets de latence par étage.
- [`hakille-agents`](crates/hakille-agents) : Runtime WASM/WASI (Wasmtime) avec vérification de capabilities Biscuit, quotas d'appels et auto-révocation, agent GUI UI-TARS et protocole inter-agents signé.
- [`hakille-audit`](crates/hakille-audit) : Journal append-only signé Ed25519 avec chaînage SHA-256, registre de consentement explicite, moteur de requêtes et d'explicabilité causale.
- [`hakille-auth`](crates/hakille-auth) : Authentification multimodale continue, détecteur anti-usurpation vocale (liveness), gestionnaire de sessions invités, profils mineurs et capability Wallet.
- [`hakille-keystore`](crates/hakille-keystore) : Registre de seeds 32 octets adossé au trousseau OS (Windows Credential Manager, Keychain, Secret Service) avec versions de clés et rotation.
- [`hakille-constitution`](crates/hakille-constitution) : Les 10 règles constitutionnelles machine-vérifiables, gouvernance d'amendement par quorum et batterie de tests éthiques.
- [`hakille-design`](crates/hakille-design) : Design system Rust — marque (`brand.rs`), jetons, cartes d'action, bandes de provenance, silence et présence vocale.
- [`hakille-ui`](crates/hakille-ui) : Indicateur de présence ambiant à 6 états (Idle/Listening/Acting/Speaking/DecisionRequired/Urgent, voir `presence.rs:90-99`), HUD Spotlight éphémère (`Ctrl+Alt+Space`, alias `Alt+Space`), sélecteur dynamique de canal (seuil bruit 65 dB), budget de silence proactif et compteur financier System 2 en direct.
- [`hakille-ui-protocol`](crates/hakille-ui-protocol) : Protocole UI ↔ noyau versionné (messages typés, négociation de version, schéma golden `schema_snapshot.json`).
- [`hakille-ui-model`](crates/hakille-ui-model) : Modèle de snapshot et d'état de l'interface, découplé du rendu.
- [`hakille-ui-shell`](crates/hakille-ui-shell) : Shell d'interface (vues, pont noyau ↔ UI, composition des cartes et de la présence).
- [`hakille-ui-platform`](crates/hakille-ui-platform) : Abstraction plateforme pour fenêtres, icône de zone de notification et intégration système.
- [`hakille-ui-desktop`](crates/hakille-ui-desktop) : Rendu desktop (scène, texte multi-script, accessibilité, runtime) sous forme d'application native.
- [`hakille-metrics`](crates/hakille-metrics) : Tableau de bord des 3 métriques cardinales (Continuité, Initiative Bornée, Généralité Pratique) et détecteur de dérive des valeurs éthiques.
- [`hakille-ipc`](crates/hakille-ipc) : Transport IPC local (named pipe Windows, socket Unix) pour les clients du démon ; pas de TCP exposé.
- [`hakille-spikes`](crates/hakille-spikes) : Spikes techniques isolés (arbre de rendu S1, rendu S2, accessibilité S3) utilisés pour valider l'UI avant intégration.
- [`hakille-daemon`](crates/hakille-daemon) : Démon système exécutable orchestrant l'ensemble de la pile avec invite interactive REPL et inspection approfondie.

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

> Les modèles neuronaux non provisionnés (Moshi 7,6B, VibeVoice, CosyVoice2/NeuTTS,
> UI-TARS, Moonshine) sont des **cibles**. Le runtime actuel signale leur état via
> `:models-honest` et `is_simulated()` ; il n'affiche jamais un statut `Operational` feint.

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
- Rust 1.95+ (avec support de l'édition Rust 2024)
- CMake & Compilateur C/C++ (MSVC sous Windows, GCC/Clang sous Linux/macOS)

### Compilation et Tests
```bash
# Vérifier la compilation de tous les crates
cargo check --workspace

# Exécuter l'intégralité des tests unitaires et d'intégration
cargo test --workspace

# Exécuter la suite de conformité formelle (73 tests : 66 fast verts + 7 slow ignorés, voir CI)
cargo xtask conformance
# Gate lente hebdo (décodage Qwen CPU réel) :
cargo xtask conformance-slow
```

### Lancement du Démon Hakille
```bash
cargo run -p hakille-daemon
```

Le premier prompt est lent : le premier tour inclut le chargement lazy du GGUF Qwen (~10-60 s) puis un décodage CPU réel — comptez ~478 s en debug contre ~24 s en release (64 tokens, i7-1165G7 mesuré), donc préférez `cargo run -p hakille-daemon --release`. Le REPL est résilient (un tour raté n'arrête pas la session) et la mémoire `./hakille-memory.json` est auto-sauvegardée après chaque tour. Dépannage, variables d'environnement et codes de sortie headless : [docs/getting_started.md § 5. Dépannage & Configuration](docs/getting_started.md#5-dépannage--configuration).

### Lancement du shell desktop natif (winit + wgpu + AccessKit, ADR-009)
```bash
# Shell desktop (fenêtre winit, fallback logiciel si GPU absent) :
cargo run -p hakille-ui-desktop --bin hakille-desktop

# Baseline headless de migration (snapshot, view-models — filet de sécurité) :
cargo test -p hakille-ui-model -p hakille-ui-shell

# Tokens design (source unique TOML → Rust/Swift/Kotlin/CSS, CI) :
cargo xtask tokens --check
cargo xtask tokens export-swift   # target/tokens/HakilleTokens.swift
cargo xtask tokens export-kotlin  # target/tokens/HakilleTokens.kt
cargo xtask tokens export-css     # target/tokens/hakille-tokens.css
```

Dans l'invite interactive (`hakille>`) :
```text
hakille> :status           # Afficher le Tier matériel, RAM et état de présence
hakille> :metrics          # Inspecter les 3 métriques North-Star et l'alignement éthique
hakille> :models           # Lister le catalogue des modèles et l'état du cache
hakille> :connected        # Afficher la connexion vérifiée des 7 modèles fondations
hakille> :inspect-models   # Inspection binaire des tenseurs GGUF, Safetensors et ONNX
hakille> :hud              # Tester le HUD Spotlight éphémère (Ctrl+Alt+Space, alias Alt+Space)
hakille> :shell            # Snapshot shell ambient (présence/HUD/coût/silence → JSON compositor)
hakille> :models-honest    # Statuts honnêtes Inspected/Simulated (jamais de faux Operational)
hakille> :agents-cards     # 5 agents avec badges DEMO honnêtes + validations Braise
hakille> :powers           # Pouvoirs : périmètre, quota, expiration, interrupteur
hakille> :journal-search EUR  # Recherche journal (plus récent d'abord, 20/page)
hakille> :journal-why 1    # Pourquoi cette action ? (trace complète + nœuds graphe)
hakille> :a11y text        # Modalité voice|text|visual|haptic + override canal live
hakille> :onboarding       # Manifeste limites-avant-capacités + état acknowledgement
hakille> :channel          # Afficher l'arbitrage dynamique du Sélecteur de Canal
hakille> :settings          # Réglages §8 : fournisseurs, voix, canaux, langues, accessibilité, sauvegarde, foyer, simulation
hakille> :budget           # Consulter le quota restant du Budget de Silence (3/jour, spec §18/§29)
hakille> :agents           # Lister les agents WASM sandboxés et leurs capabilities
hakille> :cost             # Suivre la consommation financière en temps réel
hakille> :memory           # Afficher les triplets SPO du graphe de connaissances
hakille> :remember Bob habite à Paris
hakille> :recall Paris     # Rechercher dans la mémoire sémantique
hakille> :exit             # Arrêt propre du démon
```

---

## 📚 Documentation Technique Détaillée

Pour explorer chaque sous-système en profondeur, consultez les guides dédiés :

- 🏛️ [**Architecture Système Complète**](docs/ARCHITECTURE.md) : Les 6 couches, flux de données, graphe des dépendances inter-crates et invariants.
- ⚙️ [**Spécification des Substrates OS**](docs/SUBSTRATES.md) : Implémentations natives des 6 OS (Windows, Linux, macOS, Redox, Mobile, Web), sandboxes et télémétrie.
- 🧠 [**Noyau Cognitif & Mémoire Distribuée**](docs/COGNITION.md) : Système 1/2, Cognitive Fabric, Working Memory, Deliberate Planner, Self-Model, Curiosité, Graphe SPO et DSL.
- 🤖 [**Modèles Fondations & Runtimes d'Inférence**](docs/MODELS.md) : Spécifications des 7 modèles, parseurs binaires GGUF v3 / Safetensors / ONNX et registre LRU VRAM.
- 🛡️ [**Sécurité, Capabilities & Gouvernance**](docs/SECURITY_AND_GOVERNANCE.md) : Jetons Biscuit, Registre de Consentement, Journal chaîné Ed25519, Constitution et Anti-Spoofing.
- 👁️ [**Interfaces Ambiantes & Perception**](docs/INTERFACES_AND_PERCEPTION.md) : Présence discrète, HUD Spotlight éphémère, Sélecteur de Canal, Budget de Silence, World Model et moteur vocal duplex (RVQ/VAD v1).
- 💻 [**Guide du Développeur & Manuel d'Ingénierie**](docs/DEVELOPER_GUIDE.md) : Guide pas à pas de compilation, suite de conformance 66 fast + 7 slow, commandes CLI REPL et création d'agents WASM.
- 📋 [**Index des Décisions d'Architecture (ADRs)**](adr/) : Historique et justifications des choix techniques fondamentaux.

---

