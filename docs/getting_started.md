# Guide de Prise en Main & Déploiement

Ce guide détaille pas à pas l'installation, la compilation, la configuration et l'exécution d'Hakille.

---

## 1. Prérequis Système

- **Compilateur Rust** : Rust 1.95+ (avec support de l'édition Rust 2024).
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

Hakille impose une discipline stricte de conformité avant toute mise en production :

### Tests Unitaires et d'Intégration
```bash
cargo test --workspace
```
Cela exécute les tests de :
- Chiffrement au repos ChaCha20-Poly1305 et signatures Ed25519 (`hakille-core`).
- Atténuation de jetons Biscuit cryptographiques (`hakille-core`).
- Indexation vectorielle HNSW et graphe SPO (`hakille-memory`).
- Contrôle des budgets de portefeuilles (`hakille-auth`).

### Suite de Conformance des Substrates (`cargo xtask conformance`)
```bash
cargo xtask conformance
```
Cette suite vérifie que chaque backend de système d'exploitation et chaque moteur d'accélération respecte formellement les garanties de capability et d'attestation du trait `Substrate`.

---

## 4. Démarrage du Démon Hakille (`hakille-daemon`)

Pour lancer le démon cognitif interactif :

```bash
cargo run -p hakille-daemon
```

Le démon affiche les six étapes d'initialisation :
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

[5/6] Registering Agent Manifests & WASM Runtime...
  Registered 5 declarative business agent manifests (scopes déclarés, aucun binaire métier fourni)
  WASM runtime: 2 agent(s) de référence réellement chargé(s) depuis <data_dir>/agents
  WASM self-test: OK (fuel 10M, timeout 2 s, 32 pages ; aucun import hôte)

[6/6] Linking Cognitive Fabric & Orchestrator...
  Cognitive Fabric: System 1 / System 2 dynamic router online
  North-Star Metrics: Continuity, Bounded Initiative, Generality tracking active
  Presence State: Idle (Ready)

============================================================
Hakille is running. Type a prompt, or a command (:help, :status, :metrics, :exit)
============================================================

hakille>
```

### Commandes Interactives Disponibles
- `:status` : Vérifie l'état de présence (`Idle`, `Listening`, `Acting`), le tier actif et les dépenses courantes.
- `:metrics` : Affiche les 3 métriques cardinales (Continuité, Respect d'initiative bornée, Généralité pratique) et l'audit de dérive éthique.
- `:agents` : Affiche les cinq manifests métier déclaratifs et l'état du runtime WASM de référence (chemins, hashes, auto-test).
- `:agents-run <name> <json>` : Exécute réellement `echo-agent` ou `count-agent`, avec audit avant/après.
- `:cost` : Affiche le montant cumulé consommé pour les inférences lourdes System 2.
- `:help` : Affiche l'aide des commandes.
- `:exit` : Décharge les modèles et clôture proprement le démon.

### Cartes d'action et mode IPC (`--serve`)

Pour le shell desktop, démarrer le démon en mode IPC plutôt qu'en REPL :

```bash
# Terminal 1 : démon IPC (pipe nommé Windows / socket Unix)
cargo run -p hakille-daemon -- --serve

# Terminal 2 : shell desktop
cargo run -p hakille-ui-desktop --bin hakille-desktop -- --show
```

Saisir un prompt irréversible (`transférer 25 EUR à Awa`, `supprime /vfs/notes.txt`). Le démon renvoie d'abord `HudReply`, puis `Presence(DecisionRequired)` et `ShowCard`. Le panneau 720×360 affiche le coût, la sortie de données, la réversibilité et la provenance.

- `Entrée` maintenue **600 ms** : approuver (un simple appui est refusé avec un avertissement).
- `R` : refuser.
- `Échap` : masquer sans décider ; la carte reste en attente et revient au prochain affichage.
- AccessKit : les boutons 40/41/42 valent décision explicite.

L'approbation ou le refus produit une entrée d'audit (`ActionCardApproved` / `ActionCardRejected`) **avant** l'accusé `CardDecisionAck`. Aucun effet métier n'est exécuté : le plan reste sec. Les boutons visuels ne sont pas encore cliquables à la souris ; le clavier est le chemin exercé, et l'arbre AccessKit est testé sans validation par un lecteur d'écran réel.

Le jeton IPC est écrit dans `%LOCALAPPDATA%\Hakille\ipc.token` (Windows) ou sous `XDG_RUNTIME_DIR/hakille/ipc.token` (Unix) ; il n'est jamais journalisé.

---

## 5. Dépannage & Configuration

### Performance : debug vs release

Le décodeur Qwen 2.5 0.5B Q4_K_M est CPU-only en v1 (les backends CUDA/Metal sont déclarés dans la couche Compute mais pas encore actifs pour ce décodeur). Mesures réelles sur un laptop i7-1165G7 sans GPU dédié, 64 tokens générés :

| Mode | Commande | Durée du tour |
| :--- | :--- | :--- |
| Debug | `cargo run -p hakille-daemon` | ~478 s |
| Release | `cargo run -p hakille-daemon --release` | ~24 s |

Le premier tour inclut en plus le **chargement lazy du GGUF** (~10-60 s). Conseil : `cargo run -p hakille-daemon --release` pour tout usage réel. Sur GPU (CUDA/Metal), ce sera bien plus rapide une fois ces backends actifs.

### Variables d'environnement utiles

| Variable | Effet | Défaut |
| :--- | :--- | :--- |
| `HAKILLE_QWEN_TIMEOUT_MS` | Timeout LLM effectif (System 1/2, `crates/hakille-kernel/src/system1.rs`). Le budget pipeline du boot en dérive : `max_inference = timeout + 30 s`, `max_total = timeout + 60 s` (autres étages inchangés : perception 60 ms, routing 20 ms, action 40 ms). | `600000` (10 min) |
| `HAKILLE_MODELS_DIR` | Dossier des modèles, prioritaire sur la découverte automatique (`./models`, puis remontée depuis le CWD — voir `docs/models_and_inference.md`). | `./models` |
| `HAKILLE_QWEN_GGUF` | Chemin explicite du GGUF Qwen (prioritaire sur le dossier modèles). | `models/qwen2.5-0.5b-instruct-q4_k_m.gguf` |
| `HAKILLE_QWEN_TOKENIZER` | Chemin explicite du `tokenizer.json` Qwen (prioritaire). | `models/tokenizer.json` |
| `HAKILLE_UI_FONT` | Police TTF/TTC prioritaire du shell desktop `hakille-desktop` (liste vide = texte non rendu, jamais de faux glyphes). | polices système de l'OS |
| `HAKILLE_IPC_ENDPOINT` | Endpoint IPC explicite (`\\.\pipe\nom` sous Windows, chemin de socket sous Unix). À réserver aux tests ou à une instance isolée. | pipe/socket Hakille standard |
| `HAKILLE_IPC_TOKEN_FILE` | Chemin du jeton IPC partagé entre le démon et le desktop. | `%LOCALAPPDATA%\Hakille\ipc.token` ou `XDG_RUNTIME_DIR/hakille/ipc.token` |

Le fast-path de 100 ms (`docs/COGNITION.md`) reste **l'invariant produit** ; le budget aligné est la configuration de production pour un vrai décodeur LLM local. Sans lui, un prompt réel échouait avec `Stage budget exceeded for 'inference' (elapsed ... > max 100 ms)` ; désormais un tour réel de ~30 s passe.

### Comportement du REPL (`hakille>`)

- Un tour raté n'arrête plus la session : `✗ Tour échoué : ...` + suggestion actionnable (`→ ...`, ex. augmenter `HAKILLE_QWEN_TIMEOUT_MS`), puis retour à `hakille>`.
- Une commande inconnue (ex. `:hlep`) n'est plus envoyée au LLM : `✗ Commande inconnue` + suggestion par distance d'édition.
- Une commande sans argument (`:plan`, `:defer`, `:remember`, `:recall`, `:forget`, `:download`, `:asr-test`, `:embed-test`, `:journal-why`, `:why2`, `:gui-action`, `:import`, `:revalidate`, `:heritage-erase`) affiche son usage au lieu d'aller au LLM.
- Pendant un tour : `⏳ Hakille réfléchit… Ns (Qwen 0.5B CPU, 1er tour = chargement du modèle)`, rafraîchi chaque seconde si le terminal est interactif (sinon une seule ligne).
- La mémoire `./hakille-memory.json` est **auto-sauvegardée** après chaque tour (réussi ou raté), à `:exit` et à la sortie EOF.
- Ctrl+C : `⚠ Interruption (Ctrl+C) — arrêt. La mémoire a été auto-sauvegardée après le dernier tour.` (code de sortie 130).

### Mode headless et codes de sortie

```bash
cargo run -p hakille-daemon --release -- --eval-prompt "Bonjour"
```

| Code de sortie | Signification |
| :--- | :--- |
| `0` | Tour complété. Une réponse vide est annoncée honnêtement : `⚠ Headless evaluation : réponse vide (EOS immédiat) — aucun succès simulé.` |
| `1` | Échec du tour : `✗ Évaluation headless échouée : ...` + suggestion ; la mémoire est sauvegardée. |
| `2` | Erreur d'usage CLI. |
