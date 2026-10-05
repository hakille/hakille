# Annora — Sovereign Cognitive Operating System (Rust Native)

[![Rust Version](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20AGPL--3.0-blue.svg)](LICENSE-APACHE)
[![Security](https://img.shields.io/badge/security-Biscuit%20Capabilities-red.svg)]()
[![No Python](https://img.shields.io/badge/python-0%25%20(Pure%20Rust)-purple.svg)]()

> *"Un système d'exploitation cognitif, pas un chatbot."*

---

## 🧭 Vision et Philosophie

Annora n'est ni une application de chat, ni un wrapper d'API, ni un agent prompté dans un navigateur. C'est une **couche système cognitive souveraine** pensée dès le départ pour :
1. **La Continuité d'État** : Aucun concept de session qui s'achève. Un fil de vie continu persistant dans un graphe de connaissances distribué.
2. **L'Initiative Bornée** : Le système agit et anticipe de façon proactive dans un périmètre strict garanti par des contrats cryptographiques.
3. **La Généralité Pratique** : Coder, auditer des contrats, planifier des événements, piloter l'environnement graphique ou la domotique sans changer de contexte.
4. **L'Éradication Absolue de Python en Production** : Tous les modèles (ASR, Full-Duplex Speech, VLM, LLM) s'exécutent en Rust natif via **Candle**, **ONNX Runtime (`ort`)** et des loaders GGUF/BitNet, garantissant sûreté mémoire et absence de garbage collector.

---

## 🏛️ Architecture en 6 Couches

```
+-----------------------------------------------------------------------------------+
|                            6. CONFIANCE & AUDIT                                   |
|   Journal Append-Only Ed25519 · Chaînage Cryptographique · Constitution Versionnée |
+-----------------------------------------------------------------------------------+
|                            5. ACTION (AGENTS WASM)                                |
|   Runtime WASM/WASI (Wasmtime) · Capabilities Biscuit · Agent GUI UI-TARS        |
+-----------------------------------------------------------------------------------+
|                            4. MÉMOIRE DISTRIBUÉE                                  |
|   Graphe SPO (redb) · Index Vectoriel HNSW · CRDT Sync Multi-Device               |
+-----------------------------------------------------------------------------------+
|                            3. NOYAU COGNITIF                                      |
|   Cognitive Fabric · Routeur Dynamique System 1/2 · Auto-Critique Interne         |
+-----------------------------------------------------------------------------------+
|                            2. PERCEPTION MULTIMODALE                              |
|   SenseVoice-Small · Kyutai Moshi Full-Duplex · CAM++ Biométrie · Vision Scene    |
+-----------------------------------------------------------------------------------+
|                            1. COUCHE OS (SUBSTRATE)                               |
|   Abstraction Substrate · Compute Backends (CUDA/Metal/Vulkan/CPU) · Attestation  |
|   [Windows] [Linux] [macOS] [Redox OS (Micro-noyau)] [Mobile] [Web WASM]         |
+-----------------------------------------------------------------------------------+
```

---

## 📦 Structure du Workspace

Le projet est un monorepo Cargo structuré en **21 crates**, **5 agents WASM**, un harnais de conformité et des utilitaires `xtask` :

### 1. Fondations & Substrate
- [`annora-core`](crates/annora-core) : Primitives cryptographiques (Ed25519, ChaCha20Poly1305), jetons Biscuit, types temporels et d'identité.
- [`annora-substrate`](crates/annora-substrate) : Traits d'abstraction OS (`Substrate`) et de calcul matériel (`Compute`).
- [`annora-substrate-windows`](crates/annora-substrate-windows) : Implémentation native Windows (AppContainer, simulation d'entrées, capture).
- [`annora-substrate-linux`](crates/annora-substrate-linux) : Implémentation Linux (seccomp, namespaces, uinput).
- [`annora-substrate-macos`](crates/annora-substrate-macos) : Implémentation macOS (Sandbox.framework, Endpoint Security).
- [`annora-substrate-redox`](crates/annora-substrate-redox) : Implémentation pour micro-noyau Redox OS (Schemes natifs).
- [`annora-substrate-mobile`](crates/annora-substrate-mobile) & [`annora-substrate-web`](crates/annora-substrate-web) : Cibles mobiles et Web WASM.

### 2. Mémoire & Synchronisation
- [`annora-memory`](crates/annora-memory) : Graphe en triplets SPO avec backend local ACID `redb`, index vectoriel HNSW `instant-distance`, décroissance temporelle et résolution des contradictions.
- [`annora-sync`](crates/annora-sync) : Synchronisation CRDT (grow-only sets + tombstones) et protocole de continuité cross-device.

### 3. Perception & Interaction Vocale
- [`annora-perception`](crates/annora-perception) : Pipeline audio (SenseVoice-Small, Silero VAD, openWakeWord, Nemotron-3 diarisation) et vision (world model, GUI grounding).
- [`annora-voice`](crates/annora-voice) : Dialogue parlé full-duplex sans tour de parole basé sur Kyutai Moshi en Candle Rust, codec Mimi (12.5 Hz, 80ms) et CosyVoice2.
- [`annora-inference`](crates/annora-inference) : Runtimes d'inférence unifiés (Candle, ONNX, GGUF) et registre de pagination VRAM LRU.

### 4. Cognition, Agents & Sécurité
- [`annora-kernel`](crates/annora-kernel) : Fabric cognitif, routeur de complexité System 1 (local 3-7B) / System 2 (distant lourd / BYOM), évaluation de confiance et budgets de latence.
- [`annora-agents`](crates/annora-agents) : Runtime WASM/WASI avec vérification formelle de capabilities Biscuit, agent GUI UI-TARS et protocole inter-agents signé.
- [`annora-audit`](crates/annora-audit) : Journal append-only signé Ed25519 traçant actions, raisonnements, coûts et nœuds du graphe.
- [`annora-auth`](crates/annora-auth) : Enrôlement et authentification biométrique continue CAM++, gestion multi-utilisateurs et wallet capability.
- [`annora-constitution`](crates/annora-constitution) : 10 règles constitutionnelles machine-vérifiables, gouvernance et batterie de tests éthiques.
- [`annora-ui`](crates/annora-ui) : Indicateur de présence ambiante (3 états), journal d'audit navigable, affichage des coûts en temps réel.
- [`annora-metrics`](crates/annora-metrics) : Tableau de bord des 3 métriques cardinales (Continuité, Initiative Bornée, Généralité) et détecteur de dérive éthique.
- [`annora-daemon`](crates/annora-daemon) : Démon système exécutable orchestrant l'ensemble de la pile.

### 5. Agents Sandboxés de Référence (`agents/`)
- `mail` : Gestion des courriels sous capability `Network` + `Filesystem` scopée.
- `calendar` : Planification d'agenda locale.
- `code` : Agent d'inspection et de transformation de code.
- `home` : Gestion des actionneurs domotiques.
- `wallet-readonly` : Inspection des soldes et autorisations budgétaires.

---

## 🚀 Démarrage Rapide

### Prérequis
- Rust 1.85+ (Edition 2024 recommandée)
- CMake & C++ Build Tools (pour les backends d'inférence optionnels)

### Compilation et Tests
```bash
# Vérifier la compilation de tous les crates
cargo check --workspace

# Exécuter l'intégralité des tests unitaires et d'intégration
cargo test --workspace

# Exécuter la suite de conformité formelle (Substrates & Capabilities)
cargo xtask conformance
```

### Lancement du Démon Annora
```bash
cargo run -p annora-daemon
```

Dans l'invite interactive :
```text
annora> :status     # Afficher le Tier matériel et l'état de présence
annora> :metrics    # Inspecter les métriques North-Star (continuité, conformité)
annora> :agents     # Lister les agents WASM sandboxés et leurs capabilities
annora> :cost       # Suivre la consommation financière System 2
annora> :exit       # Arrêt propre du démon
```

---

## 🎯 Paliers de Déploiement (Tiers)

| Palier (Tier) | Cible Matérielle | Runtimes Audio & Vision | Raisonnement & Mémoire |
| :--- | :--- | :--- | :--- |
| **Souverain** | Station dédiée Redox OS / Linux Souverain | SenseVoice-Small + VibeVoice (60m) + Moshi (Full-Duplex sub-200ms) + UI-TARS 7B | Qwen3-8B local + escalade System 2 + redb / cluster |
| **Natif** | Laptop Windows, Linux, macOS | SenseVoice-Small (veille CPU) + CAM++ + CosyVoice2 / NeuTTS + UI-TARS (on demand) | Qwen 7B (Q4) local + redb local |
| **Invité** | Mobile Android / iOS / Web WASM | Moonshine + SenseVoice Mobile (NPU/CoreML) + TTS OS | Ombre déportée sur le Hub via synchronisation CRDT |

---

## 🛡️ Modèle de Sécurité

1. **Biscuit Capabilities** : Toute action (accès fichier, réseau, frappe clavier, dépense) requiert un jeton Biscuit signé. Les sous-agents ne reçoivent que des jetons *atténués* (privilèges strictement décroissants).
2. **Tag `Untrusted` Immuable** : Tout contenu ingéré depuis l'extérieur (courriels, pages web, documents) est tagué `Untrusted`. Il ne peut jamais être promu en instruction motrice.
3. **Journal Append-Only** : Toutes les actions motrices sont enregistrées avec signature Ed25519, empreinte SHA-256 chaînée et trace de raisonnement complète.
4. **Constitution Inviolable** : La constitution prime sur toute instruction humaine ou déduction d'agent.

---

## 📚 Documentation Détaillée

- [Architecture Système Complète](docs/architecture.md)
- [Cognitive Fabric & Routeur System 1/2](docs/cognitive_fabric.md)
- [Sécurité, Capabilities & Attestation](docs/security_and_capabilities.md)
- [Sélection des Modèles & Runtimes d'Inférence](docs/models_and_inference.md)
- [Guide de Déploiement par Palier (Tiers)](docs/tier_deployment.md)
- [Guide de Développement d'Agents WASM](docs/agents_guide.md)
- [Constitution & Éthique](docs/constitution_and_ethics.md)
- [Index des Décisions d'Architecture (ADRs)](adr/)

---

## ⚖️ Licences

- Composants Edge et Client (`annora-core`, `annora-kernel`, `annora-memory`, `annora-substrate-*`, `annora-agents`, `annora-ui`) : **Apache License 2.0**.
- Composants Serveur et Cluster (`annora-sync`, wrappers de réplication) : **AGPLv3**.
