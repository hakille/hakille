# Architecture Système Complète d'Hakille

> *"Un système d'exploitation cognitif souverain, pas un chatbot."*

Ce document constitue la référence architecturale exhaustive d'Hakille. Il détaille les principes fondamentaux de conception, la décomposition en 6 couches, le modèle d'interaction sans session, les flux de données inter-couches, les protocoles de passage de messages typés, et le graphe complet des dépendances du monorepo Cargo.

---

## 1. Principes Fondamentaux de Conception

Hakille rompt formellement avec le paradigme des assistants virtuels traditionnels (ChatGPT, Claude, Siri, Copilot) qui reposent sur des sessions conversationnelles éphémères, des serveurs centralisés et des environnements managés lents. Hakille est un **système d'exploitation cognitif natif** gouverné par quatre axiomes d'ingénierie :

### Axiome 1 : Continuité d'État Sans Session (*No-Session Continuity*)
Le système n'a aucune notion de "début de chat" ou de "fin de session". La vie cognitive d'Hakille est un fil temporel continu. Tout événement, perception, réflexion ou interaction enrichit un graphe de connaissances distribué persistant. L'état situationnel persiste à travers les redémarrages, les changements de réseau et les bascules multi-appareils.

### Axiome 2 : Initiative Bornée (*Bounded Initiative*)
Une véritable assistance exige une autonomie proactive : organiser des fichiers, suggérer des actions, détecter des contradictions ou alerter sur une anomalie sans attendre d'être explicitement invoquée. Cependant, cette initiative est mathématiquement bornée par des **contrats cryptographiques (capabilities Biscuit)** et un **budget de silence**. Le système ne peut jamais outrepasser son mandat.

### Axiome 3 : Généralité Pratique (*Practical Generality*)
Hakille n'est pas confinée dans une boîte de dialogue. Elle contrôle directement le matériel hôte via l'abstraction `Substrate` : manipulation d'interfaces graphiques (GUI grounding via UI-TARS), injection d'entrées physiques (clavier/souris), capture d'écran native, contrôle audio, ordonnancement de processus et gestion de fichiers, sans changer d'application.

### Axiome 4 : Éradication Absolue de Python en Production (*Zero-Python Runtime*)
**Viser** un temps réel strict (< 150 ms de latence vocale bout-en-bout) et une empreinte mémoire prédictible oriente l'architecture ; ces latences sont des cibles, pas des garanties mesurées sur toutes les machines. La pile d'exécution est compilée en Rust natif (édition 2024) avec Candle, ONNX Runtime (`ort`) et Wasmtime ; aucun runtime Python n'est requis en production. L'absence de pauses de GC et la sûreté mémoire restent des invariants de conception, vérifiés au cas par cas.

---

## 2. Décomposition des 6 Couches du Système

Hakille est structuré selon une hiérarchie stricte en 6 couches hermétiques. Chaque couche communique avec les couches adjacentes par des structures de données typées et des canaux asynchrones Tokio.

```
+===================================================================================+
|                            COUCHE 6 : CONFIANCE & AUDIT                           |
|  - Journal append-only signé Ed25519 (hakille-audit)                               |
|  - Chaînage d'empreintes SHA-256 (H_i = SHA256(H_{i-1} || Entry_i))               |
|  - Registre de consentement dynamique & révocation                                |
|  - Constitution immuable en 10 règles & batterie éthique (hakille-constitution)    |
|  - Authentification biométrique continue CAM++ & anti-spoofing (hakille-auth)      |
+===================================================================================+
                                         ▲
                                         │ Validation des invariants & Traçabilité
                                         ▼
+===================================================================================+
|                  COUCHE 5 : ACTION (AGENTS WASM — RÉFÉRENCE + CIBLE)                  |
|  - Runtime de bac à sable WASI / Wasmtime (hakille-agents)                         |
|  - Vérification & atténuation hors-ligne de capabilities Biscuit                  |
|  - Quotas réellement appliqués par le runtime de référence (fuel/timeout/mémoire) ; les cinq agents métier ne sont pas branchés ; `spawn_sandboxed` retourne encore un handle local pour eux |
|  - Agent GUI UI-TARS (intentions d'action abstraites : Click, Type, Drag)         |
|  - Protocole inter-agents signé Ed25519 (négociation d'agenda et de tâches)       |
+===================================================================================+
                                         ▲
                                         │ Déclenchement d'intentions
                                         ▼
+===================================================================================+
|                            COUCHE 4 : MÉMOIRE DISTRIBUÉE                          |
|  - Triple Store SPO / POS / OSP (hakille-memory)                                   |
|  - Moteur local embarqué ACID redb (Tier Edge - zéro configuration)               |
|  - Index vectoriel sémantique HNSW instant-distance (non branché au recall — brute-force documentée, `crates/hakille-memory/src/vector/edge.rs:22-34`) |
|  - Cluster distribué Raft transactionnel TiKV (Tier Cluster)                      |
|  - Synchronisation CRDT (Grow-only Sets + Tombstones, Vector Clocks) (hakille-sync)|
|  - Décroissance temporelle exponentielle & Résolution de contradictions           |
+===================================================================================+
                                         ▲
                                         │ Rapprochement contextuel & Faits
                                         ▼
+===================================================================================+
|                            COUCHE 3 : NOYAU COGNITIF                              |
|  - Cognitive Fabric & Manifeste dynamique de capacités (hakille-kernel)            |
|  - Cognitive Orchestrator & boucle de décision (Sense -> Reason -> Plan -> Act)   |
|  - Routeur System 1 local (Qwen 0.5B ; cible <100ms non garantie) / System 2       |
|  - Working Memory (tampon temporaire d'hypothèses et distillation)                |
|  - Deliberate Planner & Dry-Run Simulator (simulation avant action irréversible)  |
|  - Self-Model introspectif & Graph Curiosity Scanner (veille cognitive)           |
+===================================================================================+
                                         ▲
                                         │ Flux perceptifs & Événements
                                         ▼
+===================================================================================+
|                         COUCHE 2 : PERCEPTION MULTIMODALE                         |
|  - Détection VAD (Silero) & Mot d'activation openWakeWord ("Dis Hakille")          |
|  - Transcription SenseVoice-Small (234M, non-autorégressif, ASR, émotions, bruits)|
|  - Diarisation continue Nemotron-3 (100M, Sortformer, 8 locuteurs)                |
|  - Biométrie vocale déterministe CAM++ (7.2M, 192-d)                              |
|  - Cible S2S : Kyutai Moshi (non provisionné) & codec Mimi (RVQ nearest-L2, 12.5Hz)|
|  - Cible TTS neuronale CosyVoice2 (non provisionnée) ; v1 = formant Klatt          |
|  - CameraPipeline & WorldModel multimodal (suivi spatial continu de la scène)     |
+===================================================================================+
                                         ▲
                                         │ Abstraction matérielle & Télémétrie
                                         ▼
+===================================================================================+
|                      COUCHE 1 : COUCHE OS (SUBSTRATE & COMPUTE)                    |
|  - Traits d'abstraction Substrate & Compute (hakille-substrate)                    |
|  - Backends OS natifs :                                                           |
|    * Windows (Win32 BitBlt, SendInput, AppContainer, GlobalMemoryStatusEx)        |
|    * Linux (Seccomp-BPF, Namespaces, Cgroups v2, /proc & /sys, uinput, Wayland)   |
|    * macOS (Sandbox Seatbelt profiles, Endpoint Security, Mach host_statistics64) |
|    * Redox OS (Micro-noyau, Schemes natifs chan:, orbital:, sys:, RedoxPacket)    |
|    * Mobile (Shadow sync TiKV, Scoped storage, Battery conservation state machine)|
|    * Web (WebAssembly, Web Audio resampling 48->16kHz, Canvas, WebGPU limits)     |
|  - Attestation de plateforme (état réel : signature dev locale, pas une preuve TPM — `crates/hakille-daemon/src/main.rs`, tracked) |
+===================================================================================+
```

---

## 3. Matrice Complète des 31 Crates du Projet (+ 5 Agents de Référence)

Le monorepo Cargo regroupe 31 crates modulaires (+ 5 crates d'agents métier déclaratifs dans `agents/`, sans binaire `.wasm` livré, `conformance`, `xtask`) garantissant un découplage rigoureux :

| Crate | Chemin | Rôle & Composants Clés | Couche |
| :--- | :--- | :--- | :--- |
| **`hakille-core`** | [`../crates/hakille-core`](../crates/hakille-core) | Types universels, identifiants UUIDv7, primitives crypto Ed25519 / ChaCha20Poly1305, jetons Biscuit, niveaux de confiance (`TrustLevel`), bus de messages typés (`Message`). | Fondations |
| **`hakille-substrate`** | [`../crates/hakille-substrate`](../crates/hakille-substrate) | Contrats système : traits `Substrate` et `Compute`, `PlatformManifest`, attestation de plateforme (signature locale de dev), configurations de caméra, événements de capture et d'injection. | Couche 1 |
| **`hakille-substrate-windows`** | [`../crates/hakille-substrate-windows`](../crates/hakille-substrate-windows) | Implémentation Windows native : GDI BitBlt display capture, injection Win32 `SendInput`, bac à sable AppContainer avec SIDs, surveillance batterie/RAM Win32, icône Tray et hook Alt+Space. | Couche 1 |
| **`hakille-substrate-linux`** | [`../crates/hakille-substrate-linux`](../crates/hakille-substrate-linux) | Implémentation Linux : compilateur Seccomp-BPF avec simulateur de bytecode, isolation Namespaces et Cgroups v2, télémétrie `/proc` et `/sys`, injection `/dev/uinput`, capture Wayland/X11. | Couche 1 |
| **`hakille-substrate-macos`** | [`../crates/hakille-substrate-macos`](../crates/hakille-substrate-macos) | Implémentation macOS : profils Sandbox Seatbelt (`(deny default)`), interception Endpoint Security, télémétrie Mach `host_statistics64` et IOKit, capture Retina 2x CoreGraphics. | Couche 1 |
| **`hakille-substrate-redox`** | [`../crates/hakille-substrate-redox`](../crates/hakille-substrate-redox) | Implémentation Redox OS micro-noyau : gestionnaire de schemes natifs (`chan:`, `orbital:`, `disk:`, `sys:`, `audio:`), sérialisation `RedoxPacket`, table de capacités microkernel. | Couche 1 |
| **`hakille-substrate-mobile`** | [`../crates/hakille-substrate-mobile`](../crates/hakille-substrate-mobile) | Implémentation mobile (Android/iOS) : synchronisation d'ombre (*Shadow sync*) avec horloge de Lamport, sandbox de stockage scopé, machine à états d'économie de batterie (Doze). | Couche 1 |
| **`hakille-substrate-web`** | [`../crates/hakille-substrate-web`](../crates/hakille-substrate-web) | Implémentation WebAssembly : bridge Web Audio avec rééchantillonnage linéaire 48kHz -> 16kHz, tampon circulaire anti-overrun, capture Canvas ImageData, détection WebGPU. | Couche 1 |
| **`hakille-perception`** | [`../crates/hakille-perception`](../crates/hakille-perception) | Ingestion sensorielle : capture audio 16kHz, Silero VAD, openWakeWord, SenseVoice-Small ASR, CAM++ speaker ID, Nemotron-3 diarisation, CameraPipeline et WorldModel spatial continu. | Couche 2 |
| **`hakille-audio`** | [`../crates/hakille-audio`](../crates/hakille-audio) | I/O audio temps réel `cpal` : capture micro, downmix mono et rééchantillonnage vers 16 kHz par blocs de 10 ms, lecture PCM. Erreur explicite si périphérique indisponible — jamais de simulation. | Couche 2 |
| **`hakille-voice`** | [`../crates/hakille-voice`](../crates/hakille-voice) | Moteur vocal : RVQ Mimi nearest-L2 sur codebooks réels (12,5 Hz), TTS formantique Klatt (`FormantTts`, 24 kHz), détecteur d'interruption (Barge-in). Pas de décodeur neural Mimi complet ; `MoshiEngine::is_simulated()` vaut `true`. | Couche 2 |
| **`hakille-inference`** | [`../crates/hakille-inference`](../crates/hakille-inference) | Runtimes neuronaux : parseur Candle Safetensors, inspecteur ONNX Runtime (`ort`), parseur binaire streaming GGUF v3, ModelRegistry avec éviction LRU VRAM et provisioner d'artefacts. | Transverse |
| **`hakille-ipc`** | [`../crates/hakille-ipc`](../crates/hakille-ipc) | Contrat IPC local : named pipe Windows et socket Unix, messages délimités et gestion de cycle de vie ; aucun transport TCP exposé. | Transverse |
| **`hakille-kernel`** | [`../crates/hakille-kernel`](../crates/hakille-kernel) | Cerveau cognitif : `CognitiveFabric`, routeur System 1/2, `WorkingMemory`, `DeliberatePlanner`, `SelfModel`, `GraphCuriosityScanner`, `DryRunSimulator`, budgets de latence par étage. | Couche 3 |
| **`hakille-memory`** | [`../crates/hakille-memory`](../crates/hakille-memory) | Mémoire à long terme : graphe de triplets SPO avec triple indexation (SPO, POS, OSP — écriture 3 index, lecture par préfixe O(N) documentée, `crates/hakille-memory/src/storage/edge.rs:164-167`), backend local ACID `redb`, index vectoriel HNSW `instant-distance` (non branché au recall, brute-force `vector/edge.rs:22-34`), GraphQueryLang DSL, déclin temporel. | Couche 4 |
| **`hakille-sync`** | [`../crates/hakille-sync`](../crates/hakille-sync) | Continuité multi-appareils : ensembles CRDT à croissance seule avec tombstones, horloges vectorielles, coordinateur de handoff de tâche et protocole de réplication TiKV. | Couche 4 |
| **`hakille-agents`** | [`../crates/hakille-agents`](../crates/hakille-agents) | `AgentSandbox` Wasmtime (ABI `run`, fuel/timeout/mémoire), matérialisation content-addressed et exécution réelle de `echo-agent`/`count-agent`. Les cinq agents métier restent déclaratifs et `spawn_sandboxed` ne les exécute pas. | Couche 5 |
| **`hakille-audit`** | [`../crates/hakille-audit`](../crates/hakille-audit) | Journalisation inviolable : journal append-only avec signatures Ed25519, chaînage de hachages SHA-256, registre de consentement explicite, moteur de requêtes et d'explicabilité du "pourquoi". | Couche 6 |
| **`hakille-auth`** | [`../crates/hakille-auth`](../crates/hakille-auth) | Gestion d'identité : authentification multimodale continue, détecteur anti-usurpation vocale (liveness), gestionnaire de sessions invités bornées, profils mineurs, capability Wallet de paiement. | Couche 6 |
| **`hakille-keystore`** | [`../crates/hakille-keystore`](../crates/hakille-keystore) | Trousseau de clés longues durées : seeds 32 octets stockés dans le keychain OS (Credential Manager, Keychain, Secret Service), utilisateurs versionnés (`capability-root-v1`, `journal-signing-v1`) et rotation. | Couche 6 |
| **`hakille-constitution`** | [`../crates/hakille-constitution`](../crates/hakille-constitution) | Règles suprêmes : 10 règles constitutionnelles machine-vérifiables, gouvernance d'amendement par quorum, batterie de tests éthiques automatisée. | Couche 6 |
| **`hakille-ui`** | [`../crates/hakille-ui`](../crates/hakille-ui) | Présence ambiante : indicateur minimal à 3 états, HUD Spotlight éphémère (Alt+Space), sélecteur de canal dynamique, budget de silence proactif, compteur financier System 2 en temps réel. | Présentation |
| **`hakille-design`** | [`../crates/hakille-design`](../crates/hakille-design) | Design system Rust : identité de marque (`brand.rs`), cartes d'action, bandes de provenance, silence et voix, alignement sur les jetons `tokens/`. | Présentation |
| **`hakille-ui-protocol`** | [`../crates/hakille-ui-protocol`](../crates/hakille-ui-protocol) | Protocole UI ↔ noyau versionné : messages typés, `PROTOCOL_VERSION`, négociation de compatibilité, schéma golden verrouillé par test et extension additive v1 des cartes (`ShowCard`, `CardDecisionAck`). | Présentation |
| **`hakille-ui-model`** | [`../crates/hakille-ui-model`](../crates/hakille-ui-model) | Modèle de snapshot de l'interface (état de présence, cartes, provenance), indépendant du backend de rendu. | Présentation |
| **`hakille-ui-shell`** | [`../crates/hakille-ui-shell`](../crates/hakille-ui-shell) | Shell d'interface : vues, pont entre le noyau et l'UI, composition des cartes et de la présence. | Présentation |
| **`hakille-ui-platform`** | [`../crates/hakille-ui-platform`](../crates/hakille-ui-platform) | Abstraction plateforme : fenêtres, icône de zone de notification, intégration système et cycle de vie de l'application. | Présentation |
| **`hakille-ui-desktop`** | [`../crates/hakille-ui-desktop`](../crates/hakille-ui-desktop) | Application desktop : scène, moteur de texte multi-script, accessibilité et runtime natif ; carte 720×360, maintien clavier 600 ms et décisions AccessKit. Les boutons visuels ne sont pas encore cliquables à la souris. | Présentation |
| **`hakille-spikes`** | [`../crates/hakille-spikes`](../crates/hakille-spikes) | Spikes techniques isolés (arbre de rendu S1, rendu S2, accessibilité S3) pour valider l'UI avant intégration. | Lab |
| **`hakille-metrics`** | [`../crates/hakille-metrics`](../crates/hakille-metrics) | Mesure des objectifs : 3 métriques North-Star (Taux de continuité, Conformité d'initiative bornée, Généralité pratique) et détecteur de dérive des valeurs éthiques. | Télémétrie |
| **`hakille-daemon`** | [`../crates/hakille-daemon`](../crates/hakille-daemon) | Démon exécutable principal avec boucle REPL interactive, mode `--serve` IPC, cartes d'action auditées et initialisation en 6 étapes. | Binaire |

---

## 4. Graphe de Dépendances Inter-Crates

Le schéma suivant illustre l'arbre d'appels et de compilation entre les briques :

```
                                   [ hakille-daemon ]
                                          │
         ┌────────────────────────────────┼────────────────────────────────┐
         │                                │                                │
         ▼                                ▼                                ▼
   [ hakille-ui ]                   [ hakille-kernel ]               [ hakille-metrics ]
         │                                │                                │
         │         ┌──────────────────────┴──────────────────────┐         │
         │         │              kernel dépend :                │         │
         │         │  inference + memory + constitution (+core)  │         │
         │         └──────────────────────┬──────────────────────┘         │
         ▼                                ▼                                ▼
  [ hakille-agents ]               [ hakille-voice ]                [ hakille-memory ]──►[ hakille-sync ]
         │                                │                                │
         ├────────────────────────────────┼────────────────────────────────┤
         │                                │                                │
         ▼                                ▼                                ▼
   [ hakille-auth ]               [ hakille-perception ]            [ hakille-audit ]──►[ hakille-constitution ]
         │                                │                                │
         └────────────────────────────────┴────────────────────────────────┘
                                          │
                                          ▼
                                [ hakille-inference ]  ◄── couche basse (GGUF/Safetensors/ONNX ort,
                                          │               Candle Qwen, registre LRU VRAM)
                                          ▼
  [ hakille-substrate ] ◄─── [ hakille-substrate-windows / linux / macos / redox / mobile / web ]
                                          │
                                          ▼
                                    [ hakille-core ]
```

> Écart corrigé : l'ancien arbre plaçait `hakille-inference` au-dessus du
> substrate comme dépendant de l'audit ; en réalité `inference` est bas
> (ne dépend que de `core` + `substrate`), et `kernel` dépend de
> `inference` + `memory` + `constitution` (+ `core`/`substrate`), voir
> `crates/hakille-kernel/Cargo.toml` et `crates/hakille-inference/Cargo.toml`.

---

## 5. Flux de Données et Cycle de Vie d'une Interaction

Une requête cognitive suit un cycle rigoureux garantissant réactivité sub-seconde, sécurité par capabilities et traçabilité inviolable :

```
Utilisateur        Perception           Kernel            Mémoire           Agents / OS          Audit
    │                  │                   │                 │                   │                 │
    │── Audio 16kHz ──►│                   │                 │                   │                 │
    │   "Dis Hakille"   │── VAD + ASR ─────►│                 │                   │                 │
    │                  │   (SenseVoice)    │                 │                   │                 │
    │                  │── Biométrie ─────►│                 │                   │                 │
    │                  │   (CAM++ 192d)    │                 │                   │                 │
    │                  │                   │── Authentifier ─┼───────────────────┼────────────────►│
    │                  │                   │   Identity OK   │                   │                 │
    │                  │                   │                 │                   │                 │
    │                  │                   │── Query SPO ───►│                   │                 │
    │                  │                   │   + HNSW Vector │                   │                 │
    │                  │                   │◄── Faits / Graph│                   │                 │
    │                  │                   │                 │                   │                 │
    │                  │                   │── Routeur S1/S2 │                   │                 │
    │                  │                   │   Score & Plan  │                   │                 │
    │                  │                   │                 │                   │                 │
    │                  │                   │── Planification ┼──────────────────►│ (DryRun Sim)    │
    │                  │                   │   Vérif Biscuit │                   │                 │
    │                  │                   │                 │                   │                 │
    │                  │                   │─ Action WASM ───┼──────────────────►│ UI-TARS / Exec  │
    │                  │                   │     (cible)     │                   │                 │
    │                  │                   │                 │                   │ Action effectuée│
    │                  │                   │                 │                   │                 │
    │                  │                   │── Trace Action ─┼───────────────────┼────────────────►│
    │                  │                   │                 │                   │                 │ H_i = SHA256(H_i-1 || Entry)
    │                  │                   │                 │                   │                 │ Sign Ed25519
    │                  │                   │── Streaming S2S ┼───────────────────┼─────────────────┤
    │◄── Audio Mimi ───┼───────────────────│ (cible 160ms)   │                   │                 │
    │    Full-Duplex   │                   │                 │                   │                 │
```

### Étape 1 : Ingestion Perceptive & Biométrie Continue
1. Le flux microphone (16 kHz, PCM 16-bit) est ingéré par `hakille-perception`.
2. Le `VoiceActivityDetector` (Silero VAD) filtre le bruit et active `WakeWordDetector` ("Dis Hakille").
3. `SenseVoiceSmall` réalise la transcription phonétique, l'analyse d'émotions et la détection d'événements ambiants ; la latence dépend du CPU et n'est pas garantie sous 50 ms.
4. Simultanément, la diarisation locale utilise un clustering d'embeddings CAM++ (Nemotron n'est pas provisionné) et `CAM++` génère un vecteur de 192 dimensions.
5. `hakille-auth` compare ce vecteur à des gabarits locaux et génère un jeton Biscuit ; l'attestation de plateforme est une signature de développement, pas une preuve TPM/Secure Enclave.

### Étape 2 : Résolution Mémorielle & Contexte
1. L'intention transcrite est envoyée à `hakille-kernel::CognitiveFabric`.
2. `hakille-memory` interroge simultanément :
   - Le graphe de triplets SPO local `redb` pour les relations symboliques exactes.
   - Le recall vectoriel (HNSW présent mais non branché ; similarité brute-force documentée côté `hakille-memory`) pour les concepts proches.
3. Les faits récupérés subissent une pondération temporelle (décroissance exponentielle des faits non réactivés).

### Étape 3 : Routage Dynamique System 1 / System 2
1. Le `CognitiveRouter` calcule l'entropie et la complexité de la demande :
   - **System 1** (cible) : résolu localement par Qwen 2.5 0.5B ; le décodage CPU réel mesuré est de l'ordre de la seconde à la dizaine de secondes selon le mode, pas sous 100 ms.
   - **System 2** : si la complexité syntaxique/sémantique dépasse le seuil ou concerne du raisonnement formel/code lourd, la tâche est escaladée vers un modèle distant ou BYOM (*Bring Your Own Model*), avec mise à jour du `CostCounter` en temps réel.
2. Le `ConfidenceEngine` calcule le score de certitude combiné :
   $$\text{Confiance} = (\text{Score Modèle} \times 0.6) + (\text{Corrélation Graphe} \times 0.4)$$
   Si la confiance est inférieure au seuil du domaine (Général 0.70, Juridique/Finance 0.90, Médical 0.95), une auto-critique interne à deux passes est déclenchée.

### Étape 4 : Délibération, Planification & Simulation à Blanc
1. Si une action motrice est envisagée (clic UI, commande domotique, modification de fichier) :
   - Le `DeliberatePlanner` produit la séquence ordonnée d'étapes.
   - Le `DryRunSimulator` simule l'action dans un bac à sable pour vérifier l'absence d'effets secondaires destructeurs.
2. Le jeton Biscuit est atténué avec le périmètre le plus strict possible (scopes minimaux, quota de requêtes borné, durée de vie brève).

### Étape 4 bis : Décision humaine via carte d'action (implémenté v1)

Lorsqu'un prompt irréversible est détecté, le noyau ne passe pas directement à l'étape 5 : il produit un plan sec et le démon expose une carte de décision.

```
Prompt irréversible
   │
   ▼
CognitiveOrchestrator::action_card_preview  (plan + dry-run, zéro effet)
   │
   ▼
Audit ActionCardProposed ──► ShowCard(card) + Presence(DecisionRequired)
   │                                      │
   │                         Desktop 720×360 : Entrée maintenue 600 ms,
   │                         R = refuser, Échap = masquer,
   │                         AccessKit 40/41/42 = décision explicite
   ▼                                      │
Approve / Reject / Edit ──────────────────┘
   │
   ▼
Audit ActionCardApproved|ActionCardRejected ──► CardDecisionAck(audit_id)
```

Règles vérifiées :

- une carte n'est créée que pour une action irréversible détectée ; le plan reste sec ;
- l'audit de proposition précède l'envoi de la carte ; l'audit de décision précède l'accusé ;
- si l'audit échoue, la carte reste en attente et le HUD reçoit un refus explicite ;
- `CardDecisionAck` contient l'UUID réel de l'entrée d'audit, et le HUD ne retire la carte qu'à sa réception ;
- **aucun effet métier n'est exécuté** par l'approbation ou le refus en v1 ;
- les boutons visuels ne sont pas encore cliquables à la souris : le clavier est le chemin exercé ; l'arbre AccessKit est testé, mais pas encore activé par un lecteur d'écran réel.

Le flux a été rejoué en smoke IPC puis dans la fenêtre winit réelle avec un vrai démon : `HudReply` → `Presence(DecisionRequired)` → `ShowCard`, appui court refusé avec avertissement, maintien 600 ms → approbation auditée, puis `R` → refus audité. Les entrées `ActionCardProposed`, `ActionCardApproved` et `ActionCardRejected` ont été vérifiées dans le journal.

### Étape 5 : Exécution Sécurisée & Journalisation d'Audit
1. **(Réel pour la référence, cible pour le métier)** Les agents `echo-agent` et `count-agent` sont exécutés dans le bac à sable Wasmtime. Les cinq agents métier ne sont pas encore routés vers un runner ; leurs manifests restent déclaratifs.
2. L'action est enregistrée dans `hakille-audit` :
   - Horodatage et identifiant UUIDv7.
   - Signature cryptographique Ed25519 de l'acteur.
   - Empreinte SHA-256 chaînée : $H_n = \text{SHA-256}(H_{n-1} \parallel \text{Payload}_n)$.
   - Pointeurs explicites vers les nœuds du graphe mémoire justifiant l'action.
3. **(Cible)** La réponse orale serait renvoyée en streaming via Moshi/CosyVoice2. En v1, `hakille-voice` fournit un RVQ Mimi nearest-L2 et un TTS formantique, sans modèle neuronal Moshi.

---

## 6. Portabilité Multi-Plateformes et Paliers Matériels (Tiers)

L'architecture supporte trois paliers d'accueil honnêtes, sans prétendre que toutes les plateformes offrent le même niveau de liberté :

```
                  +-------------------------------------------------------+
                  |                     TIER SOUVERAIN                    |
                  |  Matériel Dédié / Redox OS / Linux Souverain         |
                  |  - Contrôle total du scheduler et des schemes natifs  |
                  |  - Zéro sandbox intermédiaire                         |
                  |  - Modèles locaux lourds (Qwen 8B, UI-TARS 7B, Moshi) |
                  +-------------------------------------------------------+
                                              │
                                              ▼
                  +-------------------------------------------------------+
                  |                      TIER NATIF                       |
                  |  PC Portable / Desktop (Windows, Linux, macOS)        |
                  |  - Contrôle quasi-total via isolation de l'OS         |
                  |  - AppContainer, Seccomp-BPF, Sandbox Seatbelt        |
                  |  - Modèles quantisés (Q4), pagination VRAM à la demande|
                  +-------------------------------------------------------+
                                              │
                                              ▼
                  +-------------------------------------------------------+
                  |                      TIER INVITÉ                      |
                  |  Mobile (Android, iOS) / Navigateur Web (WASM)        |
                  |  - Sandboxes constructeurs restrictives               |
                  |  - Perception légère locale (VAD, Moonshine, WebAudio)|
                  |  - Ombre cognitive déportée sur Hub TiKV personnel    |
                  |  - Continuité garantie par synchronisation CRDT       |
                  +-------------------------------------------------------+
```

---

## 7. Directives Architecturales Non Négociables

1. **Aucune fuite de privilège non atténuée** : Un sous-agent ne peut sous aucun prétexte recevoir un jeton de capability avec des droits supérieurs ou égaux à son parent. Les droits sont **strictement décroissants**.
2. **Le tag `Untrusted` ne se dissipe jamais** : Tout octet ingéré d'une source externe (mail, flux web, document lu) porte un tag immuable `TrustLevel::Untrusted`. Il peut servir de donnée de raisonnement, mais ne peut jamais être promu en instruction motrice directe.
3. **Immutabilité de l'audit** : Le journal d'audit est append-only. Aucune primitive du système ne permet la modification ou la suppression rétroactive d'une entrée.
4. **La Constitution prime toujours** : Si une instruction utilisateur contredit une règle de la Constitution (ex: auto-préservation humaine, intégrité du système), l'instruction est rejetée avec justification formelle.
