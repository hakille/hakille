<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="logo/hakille-logo-dark.png">
    <img src="logo/hakille-logo.png" alt="Hakille" width="160">
  </picture>
</p>

# Hakille — Sovereign Cognitive Operating System (Rust Native)

[![Rust Version](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://github.com/hakille/hakille/actions/workflows/ci.yml/badge.svg)](https://github.com/hakille/hakille/actions/workflows/ci.yml)
[![Conformance](https://img.shields.io/badge/conformance-66%20fast%20%2B12%20slow%20ignored-yellow.svg)](.github/workflows/ci.yml)
[![Security](https://img.shields.io/badge/security-Biscuit%20Capabilities-red.svg)]()
[![No Python](https://img.shields.io/badge/python-0%25%20(Pure%20Rust)-purple.svg)]()

> *"Un système d'exploitation cognitif souverain, pas un chatbot."*

> **Statut : prototype / pré-alpha.** Voir [SECURITY.md](SECURITY.md) :
> l'inférence est **réelle et golden-testée lorsque les artefacts sont
> provisionnés** (forwards `ort` VAD/ASR/CAM++/MiniLM, Qwen 2.5 Candle, TTS
> formantique). Le codec Mimi exécute son encodeur/décodeur neuronal Candle ;
> UI-TARS exécute son VLM via un serveur llama.cpp local et son projecteur.
> Voir le [guide d’inférence locale](docs/INFERENCE_LOCALE.md). Le modèle de dialogue Moshi 7,6B n’est pas provisionné
> (`MoshiEngine::is_simulated() == true`). Les cinq agents métier de `agents/` restent des manifests déclaratifs :
> aucun binaire `.wasm` métier n'est livré ni exécuté, et `spawn_sandboxed`
> n'est toujours pas branché pour ces crates. En revanche, les deux agents de
> référence (`echo-agent`, `count-agent`) sont compilés et exécutés dans un
> bac à sable Wasmtime, avec artefacts content-addressed vérifiés.
> L'attestation est une signature locale de développement, pas une preuve
> TPM/Secure Enclave. Ne pas utiliser avec des secrets de production, de la
> biométrie réelle ou des paiements autonomes. Détail par moteur :
> [docs/MODELS.md](docs/MODELS.md).

---

## 🧭 Vision et Philosophie

Hakille n'est ni une application de chat, ni un wrapper d'API, ni un agent prompté dans un navigateur. C'est une **couche système cognitive souveraine** pensée dès le départ pour :

1. **La Continuité d'État (*No-Session Continuity*)** : Aucun concept de session qui s'achève. Un fil de vie continu persistant dans un graphe de connaissances distribué.
2. **L'Initiative Bornée (*Bounded Initiative*)** : Le système agit et anticipe de façon proactive dans un périmètre strict garanti par des contrats cryptographiques et un budget de silence.
3. **La Généralité Pratique (*Practical Generality*)** : Coder, auditer des contrats, planifier des événements, piloter l'environnement graphique ou la domotique sans changer de contexte.
4. **L'Éradication Absolue de Python en Production (*Zero-Python Runtime*)** : Les moteurs locaux utilisent **Candle**, **ONNX Runtime (`ort`)** et, pour UI-TARS, **llama.cpp** natif sur loopback. Aucun runtime Python n’est nécessaire au produit. Les modèles non provisionnés restent explicitement indisponibles.

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
|                         5. ACTION (AGENTS — CIBLE)                                |
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
> fournit le codec neuronal Mimi + VAD réel, sans modèle de dialogue Moshi 7,6B
> (`MoshiEngine::is_simulated() == true`). De même, la couche 5 est partiellement réelle : le démon exécute deux agents
> WASM de référence, mais les cinq agents métier restent des manifests
> déclaratifs. Le détail par moteur est dans
> [docs/MODELS.md](docs/MODELS.md).

---

## 📦 Structure du Workspace (37 Crates + 5 Agents + Conformance)

Le projet est un monorepo Cargo structuré en **37 crates modulaires**, **2 agents WASM de référence réellement exécutés**, **5 manifests métier déclaratifs**, une suite de conformité exhaustive (66 tests fast de conformité et 10 slow ignorés — 7 dans `conformance`, 3 dans `hakille-kernel` — voir [CI](.github/workflows/ci.yml)) et des utilitaires `xtask` :

### 1. Fondations & Substrates OS
- [`hakille-core`](crates/hakille-core) : Primitives cryptographiques (Ed25519, ChaCha20Poly1305), jetons Biscuit, types temporels, bus de messages typés (`Message`) et niveaux de confiance (`TrustLevel`).
- [`hakille-substrate`](crates/hakille-substrate) : Traits d'abstraction OS (`Substrate`) et d'accélération matérielle (`Compute`) ; le contrat d'attestation existe, mais les backends signent localement en dev (pas de preuve TPM/Secure Enclave).
- [`hakille-substrate-windows`](crates/hakille-substrate-windows) : Backend Windows Win32 natif (GDI BitBlt display capture, injection `SendInput`, bac à sable AppContainer, surveillance RAM/Batterie, présence Tray et hotkey `Ctrl+Alt+Space`, alias `Alt+Space`).
- [`hakille-substrate-linux`](crates/hakille-substrate-linux) : Backend Linux natif (compilateur Seccomp-BPF avec simulateur de bytecode, Namespaces PID/Mount/Net, Cgroups v2, télémétrie `/proc` et `/sys`, injection `/dev/uinput`, Wayland/X11).
- [`hakille-substrate-macos`](crates/hakille-substrate-macos) : Backend macOS Darwin natif (profils Sandbox Seatbelt `(deny default)`, Endpoint Security muting, Mach `host_statistics64`, IOKit, capture Retina 2x CoreGraphics).
- [`hakille-substrate-redox`](crates/hakille-substrate-redox) : Backend micro-noyau souverain Redox OS (schemes natifs `chan:`, `orbital:`, `disk:`, `sys:`, `audio:`, sérialisation `RedoxPacket`, table de capacités microkernel).
- [`hakille-substrate-mobile`](crates/hakille-substrate-mobile) : Substrat invité : conteneur de fichiers confiné, file de synchronisation avec ACK, politiques batterie/cycle de vie et pilotes natifs explicites. Voir le [guide mobile](mobile/README.md).
- [`hakille-substrate-web`](crates/hakille-substrate-web) : Backend WebAssembly pour navigateur (bridge Web Audio avec rééchantillonnage 48kHz -> 16kHz, tampon anti-overrun, Canvas ImageData, limites WebGPU).

### 2. Mémoire & Synchronisation
- [`hakille-memory`](crates/hakille-memory) : Graphe en triplets SPO avec triple indexation (SPO, POS, OSP), backend local ACID `redb`, index vectoriel sémantique HNSW `instant-distance`, GraphQueryLang DSL, déclin temporel et résolution des contradictions.
- [`hakille-sync`](crates/hakille-sync) : Fusion CRDT avec tombstones et horloges vectorielles, handoff signé v2 entre pairs explicitement approuvés. Le transport réseau et la réplication TiKV ne sont pas déployés ; voir les [limites de convergence et de persistance](crates/hakille-sync/README.md).

### 3. Perception, Voix & Inférence Neuronale
- [`hakille-perception`](crates/hakille-perception) : Pipeline audio (SenseVoice-Small, Silero VAD, CAM++ et diarisation par clustering), client UI-TARS local, CameraPipeline et WorldModel. L’activation passe par Whisper puis un contrôleur lexical ; aucun frontend openWakeWord compatible ni modèle Nemotron n’est fourni.
- [`hakille-audio`](crates/hakille-audio) : I/O audio temps réel via `cpal` — capture micro 16 kHz (downmix + rééchantillonnage) et lecture PCM ; aucune simulation, erreur explicite si aucun périphérique.
- [`hakille-voice`](crates/hakille-voice) : Service d’écoute locale Silero → Whisper → activation, codec Mimi neuronal complet (24 kHz, trames de 80 ms), TTS formantique Klatt et détecteur d’interruption Barge-In. Le modèle de dialogue Moshi est absent ; `MoshiEngine::is_simulated()` reste `true`.
- [`hakille-inference`](crates/hakille-inference) : Runtimes neuronaux natifs (parseur binaire streaming GGUF v3, parseur Safetensors Candle, inspecteur ONNX Runtime `ort`, registre de réservations mémoire bornées) et provisioner d’artefacts vérifiés. Le registre ne décharge pas automatiquement les tenseurs des moteurs.

### 4. Cognition, Agents, Confiance & UI
- [`hakille-kernel`](crates/hakille-kernel) : Fabric cognitif, routeur System 1/2, Working Memory scratchpad, Deliberate Planner, Self-Model introspectif, Graph Curiosity Scanner, Dry-Run Simulator et budgets de latence par étage.
- [`hakille-agents`](crates/hakille-agents) : `AgentSandbox` (Wasmtime) avec ABI `run`, fuel/mémoire/timeout bornés, matérialisation content-addressed et exécution réelle de `echo-agent`/`count-agent` ; les cinq agents métier et `spawn_sandboxed` restent déclaratifs.
- [`hakille-audit`](crates/hakille-audit) : Journal append-only signé Ed25519 avec chaînage SHA-256, registre de consentement explicite, moteur de requêtes et d'explicabilité causale.
- [`hakille-auth`](crates/hakille-auth) : Authentification multimodale continue, détecteur anti-usurpation vocale (liveness), gestionnaire de sessions invités, profils mineurs et capability Wallet.
- [`hakille-keystore`](crates/hakille-keystore) : Registre de seeds 32 octets adossé au trousseau OS (Windows Credential Manager, Keychain, Secret Service) avec versions de clés et rotation.
- [`hakille-constitution`](crates/hakille-constitution) : Les 10 règles constitutionnelles machine-vérifiables, gouvernance d'amendement par quorum et batterie de tests éthiques.
- [`hakille-design`](crates/hakille-design) : Design system Rust — marque (`brand.rs`), jetons, cartes d'action, bandes de provenance, silence et présence vocale.
- [`hakille-ui`](crates/hakille-ui) : Indicateur de présence ambiant à 6 états (Idle/Listening/Acting/Speaking/DecisionRequired/Urgent, voir `presence.rs:90-99`), HUD Spotlight éphémère (`Ctrl+Alt+Space`, alias `Alt+Space`), sélecteur dynamique de canal (seuil bruit 65 dB), budget de silence proactif et compteur financier System 2 en direct.
- [`hakille-ui-protocol`](crates/hakille-ui-protocol) : Protocole UI ↔ noyau versionné (messages typés, négociation de version, schéma golden `schema_snapshot.json`) ; extension additive v1 `ShowCard`/`CardDecisionAck`.
- [`hakille-ui-model`](crates/hakille-ui-model) : Modèle de snapshot et d'état de l'interface, découplé du rendu.
- [`hakille-ui-shell`](crates/hakille-ui-shell) : Shell d'interface (vues, pont noyau ↔ UI, composition des cartes et de la présence).
- [`hakille-ui-platform`](crates/hakille-ui-platform) : Abstraction plateforme pour fenêtres, icône de zone de notification et intégration système.
- [`hakille-ui-desktop`](crates/hakille-ui-desktop) : Rendu desktop (scène, texte multi-script, accessibilité, runtime) sous forme d'application native ; carte d'action 720×360, décision clavier/AccessKit et accusé d'audit réel.
- [`hakille-metrics`](crates/hakille-metrics) : Tableau de bord des 3 métriques cardinales (Continuité, Initiative Bornée, Généralité Pratique) et détecteur de dérive des valeurs éthiques.
- [`hakille-ipc`](crates/hakille-ipc) : Transport IPC local du démon (named pipe Windows, socket Unix).
- [`hakille-mobile`](crates/hakille-mobile) : État du client mobile Rust et liaisons UniFFI pour les interfaces natives Android/iOS ; checkpoint v2 et suivi durable des opérations à l'issue inconnue.
- [`hakille-mobile-gateway`](crates/hakille-mobile-gateway) : Passerelle HTTP loopback, lancée explicitement et authentifiée par un jeton distinct, vers le hub personnel. Voir [ADR-011](adr/011-mobile-native-client-and-hub.md) pour le périmètre et les limites mobiles.
- [`hakille-spikes`](crates/hakille-spikes) : Spikes techniques isolés (arbre de rendu S1, rendu S2, accessibilité S3) utilisés pour valider l'UI avant intégration.
- [`hakille-daemon`](crates/hakille-daemon) : Démon système exécutable orchestrant l'ensemble de la pile avec invite interactive REPL, mode `--serve` IPC et cartes d'action auditées.

### 5. Agents Métier (`agents/` — manifests déclaratifs)
- `mail` : Gestion des courriels sous capability déclarée `Filesystem` + `Network`.
- `calendar` : Planification d'agenda locale.
- `code` : Agent d'inspection et de transformation de code.
- `home` : Gestion des actionneurs domotiques.
- `wallet-readonly` : Inspection des soldes et autorisations budgétaires.

> Ces crates exposent des fonctions natives et des tests unitaires ; leurs
> `agent.toml` **sont chargés par le démon** (inclus à la compilation : le
> binaire et les fichiers du dépôt ne peuvent pas diverger ; un manifeste
> invalide, non isolé ou avec une capability inconnue fait échouer le boot).
> Aucun binaire métier `.wasm` n'est versionné : le bac à sable à zéro import
> hôte ne donne donc aucune capability aux agents, et les scopes déclarés sont
> un **manifeste d'intention**, pas une autorisation en exécution. Les deux
> agents de référence (`echo-agent`, `count-agent`) sont, eux, réellement
> exécutés par le démon.

---

## 🎯 Paliers de Déploiement Matériel (Tiers)

| Palier (Tier) | Cible Matérielle | Perception Audio & Vision | Raisonnement & Mémoire | Rôle et Confinement |
| :--- | :--- | :--- | :--- | :--- |
| **Souverain** | Station dédiée Redox OS / Linux Souverain | SenseVoice-Small + VibeVoice (60m) + Moshi (Full-Duplex sub-200ms) + UI-TARS 7B | Qwen3-8B local + escalade System 2 + redb / cluster TiKV | Contrôle total des schemes natifs, zéro sandbox intermédiaire. |
| **Natif** | Laptop Windows, Linux, macOS | SenseVoice-Small (veille CPU) + CAM++ + CosyVoice2 / NeuTTS + UI-TARS (on demand) | Qwen 7B (Q4) local + redb local | Contrôle quasi-total via sandboxing OS (AppContainer, Seccomp, Seatbelt). |
| **Invité (cible)** | Mobile Android / iOS / Web WASM | Android/iOS : dictée OS locale optionnelle selon moteur installé ; aucun poids Hakille embarqué. NPU mobile à raccorder. | Client du hub personnel ; réplication réseau CRDT à raccorder | Accès restreint par l'OS hôte ; voir [l'état mobile](adr/011-mobile-native-client-and-hub.md). |

> Les modèles neuronaux non provisionnés (Moshi 7,6B, VibeVoice, CosyVoice2/NeuTTS,
> UI-TARS 7B, Moonshine) sont des **cibles**. UI-TARS 2B est exécutable localement avec son projecteur et llama.cpp. Le runtime actuel signale leur état via
> `:models-honest` et `is_simulated()` ; il n'affiche jamais un statut `Operational` feint.

---

## 🛡️ Modèle de Sécurité et Garanties Formelles

1. **Biscuit Capabilities** : Les actions médiées par le noyau et les substrats exigent un jeton Biscuit signé ; les délégations sont atténuées. Le runtime de référence est sandboxé (zéro import), mais les scopes Biscuit des agents métier ne sont pas encore appliqués.
2. **Tag `Untrusted` Immuable** : Tout contenu ingéré depuis l'extérieur (courriels, pages web, documents, OCR) est marqué `TrustLevel::Untrusted`. Il ne peut jamais être promu en instruction motrice, neutralisant les prompt injections.
3. **Journal Append-Only Chaîné** : Toutes les actions motrices sont enregistrées avec signature Ed25519, empreinte SHA-256 chaînée ($H_i = \text{SHA256}(H_{i-1} \parallel \text{Entry}_i)$) et pointeurs vers les faits du graphe justifiant la décision.
4. **Constitution Inviolable** : 10 règles constitutionnelles machine-vérifiables s'imposent à tout composant et surpassent toute instruction humaine.
5. **Anti-Spoofing Biométrique** : `VoiceLivenessDetector` fournit une heuristique locale de vivacité ; elle n'est ni certifiée ni évaluée contre des clones vocaux récents, et ne doit pas servir de contrôle de sécurité autonome.

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

# Exécuter la suite de conformité formelle (78 tests : 66 fast verts + 12 slow ignorés, voir CI)
cargo xtask conformance
# Gate lente hebdo (décodage Qwen CPU réel) :
cargo xtask conformance-slow
```

### Lancement du Démon Hakille
```bash
cargo run -p hakille-daemon
```

Le premier prompt est lent : le premier tour inclut le chargement lazy du GGUF Qwen (~10-60 s) puis un décodage CPU réel — comptez ~478 s en debug contre ~24 s en release (64 tokens, i7-1165G7 mesuré), donc préférez `cargo run -p hakille-daemon --release`. Le REPL est résilient (un tour raté n'arrête pas la session) et la mémoire `<data_dir>/memory.redb` conserve les faits, conflits, corrections et échanges complets par transactions (migration automatique des anciens JSON). Dépannage, variables d'environnement et codes de sortie headless : [docs/getting_started.md § 5. Dépannage & Configuration](docs/getting_started.md#5-dépannage--configuration).

### Lancement du shell desktop natif (winit + wgpu + AccessKit, ADR-009)
```bash
# Shell desktop (fenêtre winit, fallback logiciel si GPU absent) :
cargo run -p hakille-ui-desktop --bin hakille-desktop

# Baseline headless de migration (snapshot, view-models — filet de sécurité) :
cargo test -p hakille-ui-model -p hakille-ui-shell

# Tokens design (vérifie aussi les exports Kotlin/Swift mobiles versionnés) :
cargo xtask tokens --check
cargo xtask tokens export-swift   # target/tokens/HakilleTokens.swift
cargo xtask tokens export-kotlin  # target/tokens/HakilleTokens.kt
cargo xtask tokens export-css     # target/tokens/hakille-tokens.css
```

Le contrôle de types macOS Intel ne démontre pas une exécution ONNX :
ONNX Runtime 1.31 ne distribue plus de binaire Intel. La liaison et les
moteurs ONNX sur cette architecture restent à valider avec un runtime compatible.

### Applications mobiles natives

Les applications Kotlin/Compose et Swift/SwiftUI partagent le cœur Rust via
UniFFI et rejoignent explicitement le hub personnel. La passerelle utilise
un jeton distinct et écoute sur loopback ; un téléphone distant demande un
reverse proxy ou tunnel HTTPS configuré. Le démon ne sert qu'une connexion
IPC : déconnecter le HUD desktop pendant l'utilisation de la passerelle.

La dictée est optionnelle et locale via l'OS : Android 12/API 31+ exige un
moteur sur l'appareil disponible ; iOS 16+ exige la prise en charge locale
de la langue choisie. Sans ce support, elle est indisponible, sans repli cloud.
Une action explicite demande les permissions, puis le résultat final complète
le brouillon ; **Envoyer** reste une action séparée. Aucun modèle Hakille
n'est chargé sur le téléphone et aucun réveil vocal permanent n'y est fourni.

L’interface privilégie désormais la voix : quatre espaces **Voix, Mémoire,
Activité et Hub**, logo officiel, police Inter et thèmes issus des tokens.
Le mode conversation, activé explicitement, envoie la transcription finale
au hub et lit uniquement la réponse corrélée avec une voix locale disponible.
Chaque tour exige une pression ; les cartes gardent leur approbation séparée.
Le panneau téléphone propose caméra, fichier choisi, localisation ponctuelle
et ouverture confirmée d’applications autorisées. Les aperçus restent locaux.
Voir le [design et son périmètre](docs/DESIGN_MOBILE_2026-11-04.md).

Avant HTTP, les hôtes sauvegardent un checkpoint v2 durable. Une mutation
encore marquée en cours revient comme `unknown` au redémarrage, sans rejeu
automatique.
Le statut et le journal restent consultables ; prendre en compte les
avertissements permet la reprise mais ne confirme pas les effets métier.
Le handoff signé v2 de la bibliothèque de synchronisation exige séparément
une paire identité/clé approuvée ; il ne fournit pas un appairage ni une
synchronisation réseau automatique de ces applications.

Le [guide mobile](mobile/README.md) décrit la construction, la connexion et
les scripts d'inspection APK, de parcours sur émulateur Android et de tests
iOS sur simulateur. Le [rapport de seconde itération](docs/VALIDATION_MOBILE_SUITE_2026-11-04.md)
sépare les résultats acquis des validations restantes. La CI macOS et Android
a ensuite validé la version `22510d6` ; les résultats de la nouvelle interface
sont suivis dans le [rapport design/voix](docs/VALIDATION_DESIGN_VOIX_MOBILE_2026-11-04.md).
La dictée avec microphone physique, TalkBack et VoiceOver restent à valider
sur les appareils ciblés.

### Cartes d'action (démon → desktop)

Le flux de décision est implémenté et a été rejoué en smoke IPC local puis en smoke réel avec la fenêtre desktop (2026-10-21) :

1. le démon reçoit un prompt irréversible (`transférer 25 EUR à Awa`, `supprime /vfs/notes.txt`, etc.) ;
2. le noyau produit un **plan sec** (`Plan/Dry-run`, zéro exécution) ;
3. le démon répond `HudReply`, puis `Presence(DecisionRequired)` et `ShowCard` avec une carte réelle (coût, sortie de données, réversibilité, provenance) ;
4. le desktop ouvre le panneau 720×360. `Entrée` maintenue **600 ms** approuve, `R` refuse, `Échap` masque sans décider ; les nœuds AccessKit 40/41/42 valent décision explicite ; **les trois boutons de la carte sont cliquables à la souris** (maintien 600 ms conservé pour une carte définitive) ;
5. chaque proposition (`ActionCardProposed`), approbation (`ActionCardApproved`) ou refus (`ActionCardRejected`) est auditée avant l'accusé `CardDecisionAck`.

Limites explicites : **une approbation n'exécute encore aucune action métier** ; l'effet réel est la **mise en file durable de l'action approuvée** (outbox idempotente et persistée, avec le plan d'exécution et son risque dans l'entrée de journal et dans l'objet mis en file). AccessKit est couvert par l'arbre et les tests unitaires mais pas encore validé avec un lecteur d'écran réel. La carte n'est retirée qu'après l'accusé d'audit ; si l'audit échoue, elle reste en attente.

### Fenêtre desktop : fond transparent et déplaçable

- Le fond n'est **plus un rectangle noir** : sous Windows, la fenêtre est passée en couche Win32 (`WS_EX_LAYERED` + clé couleur) et le fond peint (`0x010101`) devient réellement transparent — le bureau transparaît. Sur les autres plateformes, le fond reste opaque : la limite est **annoncée** (`SMOKE transparency=...`), jamais un noir silencieux.
- La fenêtre est **déplaçable à la souris** : glisser la barre du HUD (ou l'en-tête du panneau) déplace la fenêtre. Pendant une décision de carte, seule la barre du HUD et l'en-tête restent déplaçables — jamais les boutons.
- Pendant un tour, le HUD affiche l'attente **avec le temps réel écoulé** (« En attente de réponse du démon… Ns ») et la saisie est verrouillée : pas de second tour concurrent.
- La réponse **arrive en streaming** : les tokens réellement produits par le moteur sont poussés au HUD pendant la génération (`TokenDelta`), et la réponse finale la remplace (c'est la référence). On ne passe plus plusieurs minutes devant un HUD muet.

### Agents réels depuis le HUD (F4)

Le démon expose son runtime WASM au desktop par un contrat IPC additif :

- `F4` ouvre le panneau **Agents** : la liste affichée distingue explicitement les agents **réellement exécutables** (binaire WASM vérifié + SHA-256) des **manifests déclaratifs** (aucun binaire) ;
- `:agent <nom> <json>` dans le champ de requête exécute un agent réel dans le bac à sable (fuel 10 M, timeout 2 s, 32 pages, zéro import hôte) et affiche sa sortie JSON réelle ;
- chaque exécution est **auditée** (`WasmAgentRunCompleted`, empreintes SHA-256 de l'entrée et de la sortie, jamais la charge utile complète) ;
- une demande de panneau faite avant la connexion au démon est **rejouée** à la connexion (sinon le panneau resterait vide).

Limites explicites : seuls `echo-agent` et `count-agent` ont un binaire réel ; les cinq agents métier de `agents/` restent des manifests déclaratifs (pas de `.wasm` livré, `spawn_sandboxed` non branché).

### Activation vocale locale : microphone explicite

Le worker vocal raccorde capture, Silero VAD, Whisper et reconnaissance
textuelle de « Hakille ». L'énergie sonore seule ne peut plus activer le
mot-clé. Le microphone est **désactivé par défaut**.

```bash
cargo run --locked -p hakille-daemon -- models download --model silero-vad-v5
cargo run --locked -p hakille-daemon -- models download --model whisper-tiny-multilingual
cargo run --locked -p hakille-daemon -- voice devices
# Session continue ; Ctrl+C arrête :
cargo run --locked -p hakille-daemon -- --listen --models-dir models --voice-language fr
# Diagnostic WAV sans microphone ni exécution de commande :
cargo run --locked -p hakille-daemon -- voice test --audio commande.wav --models-dir models --language fr
```

Dans le desktop connecté au démon `--serve`, utiliser **F8 avec la fenêtre
active**, le bouton microphone ou l'option `--voice`. F8 n'est pas global ;
masquer le HUD ne coupe pas une écoute explicitement activée. Les commandes
vocales suivent les contrôles habituels, et les segments coupés à la limite
de 12 secondes sont refusés. `--listen` et `--serve` sont incompatibles.

Whisper expose 99 codes de langue, sans garantie de précision sur chacune :
les phrases courtes et le nom propre Hakille restent sensibles aux erreurs
ASR. Le service fournit l'indice lexical explicite « Hakille » au décodeur,
sans remplacer le texte reconnu ; cela ne garantit pas une commande correcte.
Ce lot est testé sur Linux sans microphone physique ; Windows/macOS et
les périphériques réels restent à vérifier. Commandes, alias, états du HUD
et preuves : [Activation vocale](docs/ACTIVATION_VOCALE.md).

### Voix : timeline de visèmes (approximation documentée)

`hakille-voice` expose désormais `FormantStream::viseme_timeline()` : une timeline de **visèmes phonémiques** (A, E, I, O, U, M/B/P, F/V, L, neutre, silence) dérivée de la **source phonémique réelle** du TTS formantique. C'est une approximation documentée (la coarticulation n'est pas modélisée), **pas** du lip-sync image par image ; l'invariant « somme des durées == durée de l'audio » est vérifié par test, donc la timeline ne peut pas diverger du son.

```bash
# Vérifier les cartes sans lancer l'interface graphique
cargo test -p hakille-kernel action_card_preview_tests
cargo test -p hakille-daemon card_
cargo test -p hakille-ui-desktop card_

# Smoke manuel : démon IPC puis desktop
cargo run -p hakille-daemon -- --serve
cargo run -p hakille-ui-desktop --bin hakille-desktop -- --show
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
hakille> :models-honest    # Statuts Absent/Inspected/Simulated/Real dérivés des artefacts (jamais de faux Operational)
hakille> :agents-cards     # 5 agents avec badges DEMO honnêtes + validations Braise
hakille> :powers           # Pouvoirs : périmètre, quota, expiration, interrupteur
hakille> :journal-search EUR  # Recherche journal (plus récent d'abord, 20/page)
hakille> :journal-why 1    # Pourquoi cette action ? (trace complète + nœuds graphe)
hakille> :a11y text        # Modalité voice|text|visual|haptic + override canal live
hakille> :onboarding       # Manifeste limites-avant-capacités + état acknowledgement
hakille> :channel          # Afficher l'arbitrage dynamique du Sélecteur de Canal
hakille> :settings          # Réglages §8 : fournisseurs, voix, canaux, langues, accessibilité, sauvegarde, foyer, simulation
hakille> :budget           # Consulter le quota restant du Budget de Silence (3/jour, spec §18/§29)
hakille> :agents           # Manifests métier déclaratifs + état du runtime WASM réel
hakille> :agents-run echo-agent {"msg":"bonjour"}  # Exécuter un agent WASM de référence
hakille> :cost             # Suivre la consommation financière en temps réel
hakille> :memory           # Afficher les triplets SPO du graphe de connaissances
hakille> :remember Bob habite à Paris
hakille> :recall Paris     # Rappel BM25 + MiniLM réel si installé
hakille> :episodes Atlas   # Retrouver les échanges complets, même après redémarrage
hakille> :contradictions   # Conflits persistants ; :resolve <uuid> existing|incoming
hakille> :memory-history   # Historique des corrections
hakille> :memory-scope professional  # Partition active persistante
hakille> :exit             # Arrêt propre du démon
```

---

## Mémoire continue

Les faits sourcés et les échanges complets sont conservés séparément. Le rappel
hybride et les épisodes pertinents alimentent **S1 et S2 avant génération**,
avec un contexte borné. Les réponses du modèle ne deviennent pas des faits
utilisateur certains. Les conflits, leur résolution et les suppressions sont
durables. Détails, commandes et limites : [mémoire de bout en bout](docs/MEMORY_END_TO_END.md)
et [diagnostic architectural](docs/MEMORY_REDESIGN.md).

## 📚 Documentation Technique Détaillée

Pour explorer chaque sous-système en profondeur, consultez les guides dédiés :

- [**Analyse multiplateforme et améliorations du 3 novembre 2026**](docs/ANALYSE_MULTIPLATEFORME_2026-11-03.md) : état réel des six plateformes, corrections et preuves de validation, limites restantes et priorités.
- [**Analyse approfondie et deuxième lot de corrections**](docs/ANALYSE_APPROFONDIE_2026-11-03.md) : stockage des clés, persistance, calendrier, patches, dépendances corrigées et vérification du workspace.

- 🏛️ [**Architecture Système Complète**](docs/ARCHITECTURE.md) : Les 6 couches, flux de données, graphe des dépendances inter-crates et invariants.
- ⚙️ [**Spécification des Substrates OS**](docs/SUBSTRATES.md) : Implémentations natives des 6 OS (Windows, Linux, macOS, Redox, Mobile, Web), sandboxes et télémétrie.
- 🧠 [**Noyau Cognitif & Mémoire Distribuée**](docs/COGNITION.md) : Système 1/2, Cognitive Fabric, Working Memory, Deliberate Planner, Self-Model, Curiosité, Graphe SPO et DSL.
- [**Utiliser et tester l’inférence locale**](docs/INFERENCE_LOCALE.md) : commandes des sept modèles, fichiers audio, texte et images.
- [**Validation des sept modèles locaux**](docs/VALIDATION_INFERENCE_LOCALE_2026-11-03.md) : forwards réels, résultats des tests, provenance et limites.
- 🤖 [**Modèles Fondations & Runtimes d'Inférence**](docs/MODELS.md) : Spécifications des 7 modèles, parseurs binaires GGUF v3 / Safetensors / ONNX et registre LRU VRAM.
- 🛡️ [**Sécurité, Capabilities & Gouvernance**](docs/SECURITY_AND_GOVERNANCE.md) : Jetons Biscuit, Registre de Consentement, Journal chaîné Ed25519, Constitution et Anti-Spoofing.
- 👁️ [**Interfaces Ambiantes & Perception**](docs/INTERFACES_AND_PERCEPTION.md) : Présence discrète, HUD Spotlight éphémère, Sélecteur de Canal, Budget de Silence, World Model et moteur vocal duplex (RVQ/VAD v1).
- [**Guide du Développeur & Manuel d'Ingénierie**](docs/DEVELOPER_GUIDE.md) : Guide pas à pas de compilation, suite de conformance 66 fast + 12 slow, commandes CLI REPL et état réel du runtime d'agents (2 agents de référence exécutés, 5 manifests métier déclaratifs).
- 📋 [**Index des Décisions d'Architecture (ADRs)**](adr/) : Historique et justifications des choix techniques fondamentaux.

---


## Harness et recherche web

Le noyau partage ses instructions système entre S1/S2, transmet une échéance
au décodeur et distingue les étapes proposées des opérations vérifiées.
`:research <requête>` (Brave ou SearXNG configuré) et `:read-web <URL>`
déclenchent une collecte bornée avec autorisation réseau, sources horodatées,
contrôle des références et traces conservées dans la mémoire.
Les erreurs et résultats partiels restent explicites ; aucun fournisseur
réseau n'est activé à partir du contenu d'une page.

Voir [le fonctionnement et la configuration](docs/HARNESS_END_TO_END.md).
