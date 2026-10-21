# Guide du Développeur et Manuel d'Ingénierie

> *"De la compilation à la conformité formelle : bâtir et étendre un système d'exploitation cognitif en Rust pur."*

Ce document constitue le guide de référence complet pour les développeurs, contributeurs et intégrateurs d'Hakille. Il détaille la configuration de l'environnement de développement, la compilation du monorepo, la suite de tests formelle (73 tests : 66 fast verts + 7 slow ignorés en gate hebdo, voir [CI](../.github/workflows/ci.yml)), l'utilisation approfondie du CLI interactif (`hakille-daemon`), l'état réel des agents de référence et du runtime WASM (non branché au démon), et l'implémentation de nouveaux backends `Substrate`.

---

## 1. Prérequis Système et Environnement de Développement

### 1.1. Chaîne d'Outils Requise
- **Compilateur Rust** : Version **1.95+** (support natif de l'édition **Rust 2024** requis).
  ```bash
  rustup update stable
  rustup default stable
  ```
- **Cibles WebAssembly** (agents WASI sandboxés) :
  ```bash
  rustup target add wasm32-wasip1          # cible agents WASI (WasmRunner non branché)
  ```
  Écart honnête : les agents cibles exigeraient `wasm32-wasip1` (`cargo build -p agent-calendar --target wasm32-wasip1 --release`, voir `docs/agents_guide.md`), mais aucun binaire `.wasm` n'est livré ni exécuté en v1 ; la démo Web Slint a été retirée avec le renderer (ADR-009).
- **Outils C/C++ et CMake** (pour les liaisons FFI des runtimes Candle et ONNX) :
  - **Windows** : Visual Studio Build Tools 2022 avec la charge de travail *"Développement Desktop en C++"*.
  - **Linux (Debian/Ubuntu)** : `sudo apt install build-essential cmake pkg-config libasound2-dev libssl-dev libudev-dev`
  - **macOS** : `xcode-select --install` et `brew install cmake`

### 1.2. Cloner le Dépôt
```bash
git clone https://github.com/hakille/hakille.git
cd hakille
```

---

## 2. Compilation du Projet

Le workspace Cargo est configuré pour compiler les 31 crates (+ 5 crates d'agents de référence, `conformance`, `xtask`) de façon unifiée :

```bash
# 1. Vérification statique rapide de l'ensemble des crates
cargo check --workspace

# 2. Compilation en mode debug (développement local)
cargo build --workspace

# 3. Compilation optimisée (profil production release)
cargo build --workspace --release
```

---

## 3. Exécution des Suites de Tests et Validation Formelle

Hakille impose une politique de **zéro régression** : aucun composant ne peut être intégré sans validation complète de la suite de tests.

### 3.1. Tests Unitaires et d'Intégration Workspace
```bash
cargo test --workspace
```
Cette commande valide :
- L'algèbre de chiffrement ChaCha20-Poly1305 et signatures Ed25519 (`hakille-core`).
- L'atténuation cryptographique hors-ligne des jetons Biscuit (`hakille-core`).
- La triple indexation SPO, POS, OSP et le moteur ACID `redb` (`hakille-memory`).
- Les filtres Seccomp-BPF avec simulateur de bytecode (`hakille-substrate-linux`).
- La capture GDI BitBlt et l'injection `SendInput` (`hakille-substrate-windows`).
- Les profils Sandbox Seatbelt `(deny default)` (`hakille-substrate-macos`).
- Les schemes natifs et la sérialisation `RedoxPacket` (`hakille-substrate-redox`).
- La réconciliation CRDT et les tombstones de suppression (`hakille-sync`).
- L'anti-spoofing et le classifieur de voix de synthèse (`hakille-auth`).

### 3.2. Suite de Conformité Formelle des Substrates (`cargo xtask conformance`)
```bash
cargo xtask conformance
# ou directement :
cargo test -p conformance
```
La suite `conformance` exécute **73 tests (66 fast verts en CI + 7 slow ignorés en gate hebdo `cargo xtask conformance-slow`)** couvrant l'ensemble des 6 couches :
1. `test_conformance_anti_spoofing_and_guest_manager` : Validation de l'heuristique de vivacité vocale (non certifiée) et expiration des invités.
2. `test_conformance_consent_registry_and_quota_enforcement` : Vérification du registre de consentement et auto-révocation des quotas.
3. `test_conformance_self_model_and_simulator` : Introspection du Self-Model et simulation d'action à blanc.
4. `test_conformance_working_memory_distillation_and_query_dsl` : Distillation de la mémoire de travail vers le graphe SPO.
5. `test_constitutional_priority_hierarchy` : Priorité stricte Constitution > Utilisateur > Défaut.
6. `test_confidence_domain_thresholds` : Seuils stricts 0.70 (Général), 0.90 (Finance), 0.95 (Médical).
7. `test_orchestrator_turn_flow` : Cycle complet Sense -> Reason -> Plan -> Act.
8. `test_conformance_deliberate_planner_pre_execution_check` : Validation pré-exécution du plan par le planificateur.
9. `test_real_onnx_models_inspection` : Inspection structurelle des graphes ONNX (entrées/sorties).
10. `test_offline_attenuation` : Impossibilité formelle d'élargir un jeton Biscuit.
11. `test_real_mimi_safetensors_parsing` : Désérialisation zéro-copie du codec Mimi.
12. `test_capability_creation_and_verification` : Vérification cryptographique des droits Biscuit.
13. `test_all_compute_backends` : Inspection des backends CPU/CUDA/Metal/WebGPU.
14. `test_all_substrates_manifest_and_attestation` : Validation du contrat d'attestation et de la signature locale de dev sur tous les OS (pas de preuve TPM).
15. `test_real_sensevoice_tokenizer_parsing` : Test du vocabulaire de 25 055 tokens de SenseVoice.
16. `test_linux_conformance_seccomp_and_sysfs` : Conformance Seccomp et cgroups sous Linux.
17. `test_ambient_multimodal_perception_and_interface_pipeline` : Pipeline continu caméra/audio/WorldModel.
18. `test_macos_conformance_seatbelt_and_mach` : Conformance Seatbelt et télémétrie Mach macOS.
19. `test_mobile_conformance_shadow_and_battery` : Conformance synchronisation d'ombre et veille mobile.
20. `test_redox_conformance_schemes_and_capabilities` : Conformance des schemes microkernel Redox.
21. `test_web_conformance_audio_and_webgpu` : Conformance rééchantillonnage audio et WebGPU.
22. `test_full_system_end_to_end_integration` : Intégration de bout en bout de toutes les couches réunies.
23. `test_cross_substrate_screen_capture_conformance` : Validation de la capture d'écran multi-plateformes.
24. `test_real_ui_tars_gguf_parsing` : Parsing du modèle UI-TARS au format GGUF v3.
25. `test_real_qwen2_gguf_parsing` : Parsing du modèle Qwen au format GGUF v3.
26. `test_unified_connect_all_models` : Connexion unifiée des 7 modèles fondations et de leurs tokenizers essentiels (Qwen et MiniLM).
27. `test_end_to_end_cross_crate_model_wiring` : Câblage complet entre le noyau cognitif et les runtimes neuronaux.

---

## 4. Démarrage et Utilisation du Démon Hakille (`hakille-daemon`)

Le binaire `hakille-daemon` est le point d'entrée exécutable du système d'exploitation cognitif.

### 4.1. Lancement Standard
```bash
cargo run -p hakille-daemon
```

Au démarrage, le démon affiche les six étapes d'initialisation du système :
```text
============================================================
  Hakille — Sovereign Cognitive Operating System (Rust Native)
  'Un système d'exploitation cognitif, pas un chatbot.'
============================================================

[1/6] Initializing Host Substrate Layer...
  Host OS: Windows 11 (24H2)
  Tier: Native
  Compute Backends: [Cuda, Vulkan, Cpu]
  Hardware Attestation: Windows-Substrate-Host (Valid)
  NOTE: attestation is a local dev signature, NOT a TPM/VBS hardware proof (tracked work).

[2/6] Loading Versioned Constitution & Security Primitives...
  Constitution Version: 1.0.0
  Active Rules: 10 rules enforced
  Audit Journal: Initialized append-only tamper-evident log

[3/6] Initializing Distributed Knowledge Graph & CRDT Sync...
  User Identity: 01a10ce2-402c-770c-be2b-daf41914e258 (Initialized)
  CRDT State: 0 tombstones, vector clock ready

[4/6] Bootstrapping Inference Models & Cognitive Pipeline...
  Audio Pipeline: 16kHz continuous capture buffer ready
  SenseVoice-Small: mel + LFR + forward ort + CTC greedy — real ASR, measured confidence
  Voice/GUI: Mimi RVQ nearest-L2, TTS formantique Klatt, grounding CV — pas de Moshi neural ni de forward VLM

[5/6] Registering Reference Agent Manifests...
  Registered 5 reference agent manifests (scopes déclarés ; aucun binaire .wasm chargé ni exécuté)

[6/6] Linking Cognitive Fabric & Orchestrator...
  Cognitive Fabric: System 1 / System 2 dynamic router online
  North-Star Metrics: Continuity, Bounded Initiative, Generality tracking active
  Presence State: Idle (Ready)
```

Au boot, le démon dimensionne le budget pipeline sur le **timeout LLM effectif** (`HAKILLE_QWEN_TIMEOUT_MS`, défaut `600000` ms — `crates/hakille-kernel/src/system1.rs:27`) via `PipelineBudgetConfig::aligned_with_llm_timeout` (`crates/hakille-kernel/src/budget.rs:42`) : `max_inference_ms = timeout + 30 s` (marge pour le chargement lazy du GGUF) et `max_total_ms = timeout + 60 s` ; les autres étages restent aux valeurs par défaut (perception 60 ms, routing 20 ms, action 40 ms). Le fast-path 100 ms de `docs/COGNITION.md` reste l'invariant produit et n'est pas modifié par défaut : sans cette config, un prompt réel échouait avec `Stage budget exceeded for 'inference' (elapsed ... > max 100 ms)`.

Le REPL est **résilient** : un tour raté affiche `✗ Tour échoué : ...` + suggestion actionnable (`→ ...`) et la session continue ; une commande inconnue n'est jamais envoyée au LLM (`✗ Commande inconnue` + suggestion par distance d'édition). La mémoire (`./hakille-memory.json`) est auto-sauvegardée après chaque tour (réussi ou raté), à `:exit` et à la sortie EOF ; Ctrl+C affiche `⚠ Interruption (Ctrl+C) — arrêt. La mémoire a été auto-sauvegardée après le dernier tour.` (exit 130).

### 4.2. Commandes de l'Invite Interactive REPL (`hakille>`)

Le REPL accepte du texte naturel (qui est routé vers le `CognitiveOrchestrator`) ainsi qu'une suite complète de méta-commandes d'inspection :

| Commande REPL | Description & Action Réalisée |
| :--- | :--- |
| **`:help`** | Affiche la liste complète des commandes interactives (y compris `:help` et `:quit`, alias `:exit`) et rappelle que toute saisie sans `:` est envoyée au modèle Qwen. |
| **`:status`** | Affiche le palier matériel, l'état de la mémoire vive, l'état de présence actuel et le timeout LLM effectif. |
| **`:metrics`** | Inspecte les 3 métriques North-Star (Continuité, Initiative Bornée, Généralité) et le détecteur de dérive des valeurs éthiques. |
| **`:agents`** | Liste les manifests d'agents enregistrés et leurs scopes déclarés ; rappelle qu'aucun binaire `.wasm` n'est chargé ni exécuté. |
| **`:cost`** | Affiche le compteur financier des appels Système 2 en temps réel. |
| **`:models`** | Liste le catalogue des modèles fondations et l'état du cache disque local. |
| **`:connected`** | Affiche le statut vérifié de connexion des 7 modèles fondations. |
| **`:inspect-models`** | Inspection binaire approfondie des tenseurs GGUF, Safetensors et ONNX. |
| **`:verify-models`** | Calcule et vérifie les empreintes cryptographiques SHA-256 de tous les modèles installés. |
| **`:download <id>`** | Télécharge et installe un modèle par son identifiant (ex: `:download silero-vad-v5`). |
| **`:camera`** | Capture une trame vidéo et évalue la situation visuelle sans fuite de pixels bruts. |
| **`:hud`** | Démontre l'affichage du HUD Spotlight éphémère avec puce d'action Braise `#FF7A2F`. |
| **`:shell`** | Snapshot shell ambient (présence/HUD/coût/silence → JSON compositor) + rappel lancement `hakille-desktop`. |
| **`:models-honest`** | Statuts Absent/Inspected/Simulated/Real dérivés des artefacts présents (jamais de faux Operational). |
| **`:agents-cards`** | 5 agents avec badges DEMO honnêtes + flags validation Braise. |
| **`:powers`** | Pouvoirs : périmètre, quota, dernier usage, expiration, interrupteur (+ verrou politique). |
| **`:journal-search <q> [page]`** | Recherche journal, plus récent d'abord, 20/page, cherche aussi dans la trace complète. |
| **`:journal-why <n\|id>`** | Pourquoi cette action : trace complète + nœuds graphe + mot de confiance. |
| **`:a11y [voice\|text\|visual\|haptic]`** | Affiche/change la modalité d'accessibilité + override canal en direct. |
| **`:onboarding`** | Manifeste limites-avant-capacités + état acknowledgement (bloquant). |
| **`:channel`** | Affiche l'arbitrage en direct du `ChannelSelector` selon les conditions ambiantes. |
| **`:settings`** | Réglages §8 : fournisseurs (pastille Local/Cloud), voix, canaux, langues, accessibilité, sauvegarde, foyer, simulation. |
| **`:budget`** | Inspecte le quota restant du Budget de Silence proactif (3/jour, spec §18/§29). |
| **`:ambient`** | Exécute un cycle complet de perception multimodale (audio + vision + WorldModel). |
| **`:memory`** | Affiche l'ensemble des triplets SPO actuellement stockés dans le graphe de connaissances. |
| **`:remember <S P O>`** | Insère un nouveau fait dans le graphe (ex: `:remember Alice aime Rust`). |
| **`:recall <mot>`** | Recherche les faits associés à un mot-clé dans le graphe mémoire. |
| **`:win-ram`** | Affiche la télémétrie réelle de la RAM et de la VRAM (sous Windows). |
| **`:win-power`** | Affiche l'état réel de l'alimentation secteur, de la batterie et de la charge (sous Windows). |
| **`:win-screen`** | Effectue une capture d'écran de l'affichage principal via GDI BitBlt. |
| **`:win-tray`** | Teste les états de l'icône de présence dans la barre des tâches Win32. |
| **`:exit` / `:quit`** | Arrête proprement le démon Hakille (mémoire auto-sauvegardée). |

> Note : une commande inconnue (`:hlep`) n'est jamais transmise au LLM — `✗ Commande inconnue` + suggestion par distance d'édition ; une commande sans argument (`:plan`, `:defer`, `:remember`, `:recall`, `:forget`, `:download`, `:asr-test`, `:embed-test`, `:journal-why`, `:why2`, `:gui-action`, `:import`, `:revalidate`, `:heritage-erase`) affiche son usage au lieu d'atteindre le modèle. Toute saisie sans `:` est envoyée au modèle Qwen.

### 4.3. Sous-Commandes en Ligne de Commande (Mode Batch / CLI)

Le binaire `hakille-daemon` peut être piloté directement sans entrer dans le REPL :

```bash
# Lister le catalogue des modèles et leur état d'installation
cargo run -p hakille-daemon -- models list

# Télécharger un modèle spécifique par son identifiant ou alias
cargo run -p hakille-daemon -- models download sensevoice-small
# ou avec l'ID catalogue complet :
cargo run -p hakille-daemon -- models download --model sensevoice-small-int8

# Vérifier l'intégrité cryptographique SHA-256 des fichiers de modèles locaux
cargo run -p hakille-daemon -- models verify

# Évaluer une requête en mode headless sans interface (mode script)
cargo run -p hakille-daemon -- --prompt "Planifier une réunion demain matin à 10h"
# ou via le drapeau formel :
cargo run -p hakille-daemon -- --eval-prompt "Planifier une réunion demain matin à 10h"
```

---

## 5. Agents de Référence et Runtime WASM (état réel)

`hakille-agents` contient un `WasmRunner` Wasmtime avec ABI `run(i32,i32)->(i32,i32)`, mémoire bornée, fuel et timeout. **Il n'est pas branché au démon en v1** : `Substrate::spawn_sandboxed` retourne un handle local sans lancer d'instance, et aucun fichier `.wasm` n'est livré dans `agents/`. Les cinq dossiers sont des crates natives avec manifests ; leurs fonctions sont testées, mais les scopes déclarés ne sont pas appliqués à une exécution d'agent.

### 5.1. Structure d'un Agent
Les agents de référence se situent dans le répertoire `agents/` :
- `agents/mail/` : Gestion des courriels sous capability déclarée `Filesystem` et `Network`.
- `agents/calendar/` : Planification d'agenda locale.
- `agents/code/` : Inspection et transformation de code source.
- `agents/home/` : Pilotage d'actionneurs domotiques.
- `agents/wallet-readonly/` : Inspection des soldes sans droit de dépense.

### 5.2. Exemple de Manifeste d'Agent (`AgentManifest`)
```rust
use hakille_agents::AgentManifest;
use hakille_core::CapabilityScope;

let manifest = AgentManifest {
    id: "agent-mail".to_string(),
    name: "MailAssistant".to_string(),
    version: "0.1.0".to_string(),
    description: "Lecture et organisation des emails dans la boîte de réception".to_string(),
    required_capabilities: vec![CapabilityScope::Filesystem, CapabilityScope::Network],
};
```

---

## 6. Ajout d'un Nouveau Backend `Substrate`

Pour porter Hakille sur un nouvel environnement (ex: `hakille-substrate-freebsd` ou un système robotique ROS2) :

1. Créer un crate Cargo : `crates/hakille-substrate-nom`.
2. Implémenter le trait `Substrate` de `hakille-substrate` :
   - Fournir le `PlatformManifest` avec le palier (`Tier`).
   - Implémenter la signature matérielle dans `attest()`.
   - Mapper les opérations de fichiers, capture d'écran, injection d'entrées et télémétrie.
3. Implémenter le trait `Compute` pour déclarer les backends d'accélération disponibles.
4. Ajouter le nouveau crate à la suite `conformance` : **le nouveau Substrate n'est mergeable que s'il réussit 100 % des tests de conformité**.

---

## 7. Compatibilité OS (Étape 7, ADR-010)

Windows 11 est la cible de référence. Linux puis macOS sont des projets de compatibilité distincts : un OS n'est déclaré « supporté » qu'après preuve labo par OS (`docs/SPIKES_LAB.md`), jamais parce qu'il compile.

| Cible | Commande de vérification | Statut |
| :--- | :--- | :--- |
| `x86_64-unknown-linux-gnu` | `cargo check --workspace --target x86_64-unknown-linux-gnu --locked` | compilation vérifiée en CI (`ubuntu-latest`) ; support après labo |
| `x86_64-apple-darwin` | `cargo check --workspace --target x86_64-apple-darwin --locked` | compilation non vérifiée en CI ; job macOS non bloquant à venir ; support après labo |
| `aarch64-apple-darwin` | `cargo check --workspace --target aarch64-apple-darwin --locked` | compilation non vérifiée en CI ; job macOS non bloquant à venir ; support après labo |

```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin aarch64-apple-darwin
```

Écart honnête : la vérification croisée depuis un hôte Windows échoue sans chaîne C pour la cible (`ring`, dépendance de `biscuit-auth`, compile du C) ; en CI, Linux passe sur runner natif `ubuntu-latest`, et **la CI macOS restera `continue-on-error` tant que le labo n'a pas validé l'OS** (ADR-010).

**Contrat d'honnêteté :** un chemin OS non implémenté renvoie une erreur ou un état explicite, jamais un faux succès — `TrayError::NotifierMissing` (hôte tray absent), `HotkeyError::Registration` (les deux causes réelles du raccourci, pas de succès feint), `PlatformKind` déduit uniquement des `cfg`. Les parseurs Linux/macOS (seccomp BPF, Seatbelt, Endpoint Security) sont testés host-indépendamment par la conformance (73 tests : 66 fast + 7 slow).
