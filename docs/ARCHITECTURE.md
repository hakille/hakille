# Architecture Système Complète d'Annora

> *"Un système d'exploitation cognitif souverain, pas un chatbot."*

Ce document constitue la référence architecturale exhaustive d'Annora. Il détaille les principes fondamentaux de conception, la décomposition en 6 couches, le modèle d'interaction sans session, les flux de données inter-couches, les protocoles de passage de messages typés, et le graphe complet des dépendances du monorepo Cargo.

---

## 1. Principes Fondamentaux de Conception

Annora rompt formellement avec le paradigme des assistants virtuels traditionnels (ChatGPT, Claude, Siri, Copilot) qui reposent sur des sessions conversationnelles éphémères, des serveurs centralisés et des environnements managés lents. Annora est un **système d'exploitation cognitif natif** gouverné par quatre axiomes d'ingénierie :

### Axiome 1 : Continuité d'État Sans Session (*No-Session Continuity*)
Le système n'a aucune notion de "début de chat" ou de "fin de session". La vie cognitive d'Annora est un fil temporel continu. Tout événement, perception, réflexion ou interaction enrichit un graphe de connaissances distribué persistant. L'état situationnel persiste à travers les redémarrages, les changements de réseau et les bascules multi-appareils.

### Axiome 2 : Initiative Bornée (*Bounded Initiative*)
Une véritable assistance exige une autonomie proactive : organiser des fichiers, suggérer des actions, détecter des contradictions ou alerter sur une anomalie sans attendre d'être explicitement invoquée. Cependant, cette initiative est mathématiquement bornée par des **contrats cryptographiques (capabilities Biscuit)** et un **budget de silence**. Le système ne peut jamais outrepasser son mandat.

### Axiome 3 : Généralité Pratique (*Practical Generality*)
Annora n'est pas confinée dans une boîte de dialogue. Elle contrôle directement le matériel hôte via l'abstraction `Substrate` : manipulation d'interfaces graphiques (GUI grounding via UI-TARS), injection d'entrées physiques (clavier/souris), capture d'écran native, contrôle audio, ordonnancement de processus et gestion de fichiers, sans changer d'application.

### Axiome 4 : Éradication Absolue de Python en Production (*Zero-Python Runtime*)
Garantir le temps réel strict (< 150 ms de latence vocale bout-en-bout), la sûreté mémoire (`#![deny(unsafe_code)]` là où possible), l'absence totale de pauses de ramasse-miettes (GC pauses) et une empreinte mémoire prédictible interdit formellement l'utilisation de CPython ou de runtimes managés en production. **100 % de la pile est compilée en Rust natif (édition 2024)** et en WebAssembly/WASI via Wasmtime, Candle, et ONNX Runtime (`ort`).

---

## 2. Décomposition des 6 Couches du Système

Annora est structuré selon une hiérarchie stricte en 6 couches hermétiques. Chaque couche communique avec les couches adjacentes par des structures de données typées et des canaux asynchrones Tokio.

```
+===================================================================================+
|                            COUCHE 6 : CONFIANCE & AUDIT                           |
|  - Journal append-only signé Ed25519 (annora-audit)                               |
|  - Chaînage d'empreintes SHA-256 (H_i = SHA256(H_{i-1} || Entry_i))               |
|  - Registre de consentement dynamique & révocation                                |
|  - Constitution immuable en 10 règles & batterie éthique (annora-constitution)    |
|  - Authentification biométrique continue CAM++ & anti-spoofing (annora-auth)      |
+===================================================================================+
                                         ▲
                                         │ Validation des invariants & Traçabilité
                                         ▼
+===================================================================================+
|                            COUCHE 5 : ACTION (AGENTS WASM)                        |
|  - Runtime de bac à sable WASI / Wasmtime (annora-agents)                         |
|  - Vérification & atténuation hors-ligne de capabilities Biscuit                  |
|  - Quotas stricts d'appels et révocation automatique en cas d'abus (état réel : quotas WASM enforced — `crates/annora-agents/src/runtime.rs:53-62` ; egress loopback-only, allowlist TODO — `crates/annora-substrate-linux/src/substrate_impl.rs:334-346`) |
|  - Agent GUI UI-TARS (intentions d'action abstraites : Click, Type, Drag)         |
|  - Protocole inter-agents signé Ed25519 (négociation d'agenda et de tâches)       |
+===================================================================================+
                                         ▲
                                         │ Déclenchement d'intentions
                                         ▼
+===================================================================================+
|                            COUCHE 4 : MÉMOIRE DISTRIBUÉE                          |
|  - Triple Store SPO / POS / OSP (annora-memory)                                   |
|  - Moteur local embarqué ACID redb (Tier Edge - zéro configuration)               |
|  - Index vectoriel sémantique HNSW instant-distance (non branché au recall — brute-force documentée, `crates/annora-memory/src/vector/edge.rs:22-34`) |
|  - Cluster distribué Raft transactionnel TiKV (Tier Cluster)                      |
|  - Synchronisation CRDT (Grow-only Sets + Tombstones, Vector Clocks) (annora-sync)|
|  - Décroissance temporelle exponentielle & Résolution de contradictions           |
+===================================================================================+
                                         ▲
                                         │ Rapprochement contextuel & Faits
                                         ▼
+===================================================================================+
|                            COUCHE 3 : NOYAU COGNITIF                              |
|  - Cognitive Fabric & Manifeste dynamique de capacités (annora-kernel)            |
|  - Cognitive Orchestrator & boucle de décision (Sense -> Reason -> Plan -> Act)   |
|  - Routeur dynamique System 1 (3-7B local <100ms) / System 2 (distant lourd)      |
|  - Working Memory (tampon temporaire d'hypothèses et distillation)                |
|  - Deliberate Planner & Dry-Run Simulator (simulation avant action irréversible)  |
|  - Self-Model introspectif & Graph Curiosity Scanner (veille cognitive)           |
+===================================================================================+
                                         ▲
                                         │ Flux perceptifs & Événements
                                         ▼
+===================================================================================+
|                         COUCHE 2 : PERCEPTION MULTIMODALE                         |
|  - Détection VAD (Silero) & Mot d'activation openWakeWord ("Dis Annora")          |
|  - Transcription SenseVoice-Small (234M, non-autorégressif, ASR, émotions, bruits)|
|  - Diarisation continue Nemotron-3 (100M, Sortformer, 8 locuteurs)                |
|  - Biométrie vocale déterministe CAM++ (7.2M, 192-d)                              |
|  - Speech-to-Speech Full-Duplex Kyutai Moshi (Candle Rust) & Codec Mimi (12.5Hz)  |
|  - Synthèse vocale CosyVoice2-0.5B streaming sub-150ms                            |
|  - CameraPipeline & WorldModel multimodal (suivi spatial continu de la scène)     |
+===================================================================================+
                                         ▲
                                         │ Abstraction matérielle & Télémétrie
                                         ▼
+===================================================================================+
|                      COUCHE 1 : COUCHE OS (SUBSTRATE & COMPUTE)                    |
|  - Traits d'abstraction Substrate & Compute (annora-substrate)                    |
|  - Backends OS natifs :                                                           |
|    * Windows (Win32 BitBlt, SendInput, AppContainer, GlobalMemoryStatusEx)        |
|    * Linux (Seccomp-BPF, Namespaces, Cgroups v2, /proc & /sys, uinput, Wayland)   |
|    * macOS (Sandbox Seatbelt profiles, Endpoint Security, Mach host_statistics64) |
|    * Redox OS (Micro-noyau, Schemes natifs chan:, orbital:, sys:, RedoxPacket)    |
|    * Mobile (Shadow sync TiKV, Scoped storage, Battery conservation state machine)|
|    * Web (WebAssembly, Web Audio resampling 48->16kHz, Canvas, WebGPU limits)     |
|  - Attestation matérielle cryptographique (état réel : signature dev locale, pas une preuve TPM — `crates/annora-daemon/src/main.rs:300`, tracked) |
+===================================================================================+
```

---

## 3. Matrice Complète des 26 Crates du Projet (+ 5 Agents)

Le monorepo Cargo regroupe 26 crates modulaires (+ 5 agents WASM `agents/`) garantissant un découplage rigoureux :

| Crate | Chemin | Rôle & Composants Clés | Couche |
| :--- | :--- | :--- | :--- |
| **`annora-core`** | [`../crates/annora-core`](../crates/annora-core) | Types universels, identifiants UUIDv7, primitives crypto Ed25519 / ChaCha20Poly1305, jetons Biscuit, niveaux de confiance (`TrustLevel`), bus de messages typés (`Message`). | Fondations |
| **`annora-substrate`** | [`../crates/annora-substrate`](../crates/annora-substrate) | Contrats système : traits `Substrate` et `Compute`, `PlatformManifest`, attestation matérielle, configurations de caméra, événements de capture et d'injection. | Couche 1 |
| **`annora-substrate-windows`** | [`../crates/annora-substrate-windows`](../crates/annora-substrate-windows) | Implémentation Windows native : GDI BitBlt display capture, injection Win32 `SendInput`, bac à sable AppContainer avec SIDs, surveillance batterie/RAM Win32, icône Tray et hook Alt+Space. | Couche 1 |
| **`annora-substrate-linux`** | [`../crates/annora-substrate-linux`](../crates/annora-substrate-linux) | Implémentation Linux : compilateur Seccomp-BPF avec simulateur de bytecode, isolation Namespaces et Cgroups v2, télémétrie `/proc` et `/sys`, injection `/dev/uinput`, capture Wayland/X11. | Couche 1 |
| **`annora-substrate-macos`** | [`../crates/annora-substrate-macos`](../crates/annora-substrate-macos) | Implémentation macOS : profils Sandbox Seatbelt (`(deny default)`), interception Endpoint Security, télémétrie Mach `host_statistics64` et IOKit, capture Retina 2x CoreGraphics. | Couche 1 |
| **`annora-substrate-redox`** | [`../crates/annora-substrate-redox`](../crates/annora-substrate-redox) | Implémentation Redox OS micro-noyau : gestionnaire de schemes natifs (`chan:`, `orbital:`, `disk:`, `sys:`, `audio:`), sérialisation `RedoxPacket`, table de capacités microkernel. | Couche 1 |
| **`annora-substrate-mobile`** | [`../crates/annora-substrate-mobile`](../crates/annora-substrate-mobile) | Implémentation mobile (Android/iOS) : synchronisation d'ombre (*Shadow sync*) avec horloge de Lamport, sandbox de stockage scopé, machine à états d'économie de batterie (Doze). | Couche 1 |
| **`annora-substrate-web`** | [`../crates/annora-substrate-web`](../crates/annora-substrate-web) | Implémentation WebAssembly : bridge Web Audio avec rééchantillonnage linéaire 48kHz -> 16kHz, tampon circulaire anti-overrun, capture Canvas ImageData, détection WebGPU. | Couche 1 |
| **`annora-perception`** | [`../crates/annora-perception`](../crates/annora-perception) | Ingestion sensorielle : capture audio 16kHz, Silero VAD, openWakeWord, SenseVoice-Small ASR, CAM++ speaker ID, Nemotron-3 diarisation, CameraPipeline et WorldModel spatial continu. | Couche 2 |
| **`annora-voice`** | [`../crates/annora-voice`](../crates/annora-voice) | Moteur vocal : codec neural causal Kyutai Mimi (12.5Hz, 80ms), moteur full-duplex Moshi S2S en Candle Rust, CosyVoice2 streaming TTS, détecteur d'interruption naturelle (Barge-in). | Couche 2 |
| **`annora-inference`** | [`../crates/annora-inference`](../crates/annora-inference) | Runtimes neuronaux : parseur Candle Safetensors, inspecteur ONNX Runtime (`ort`), parseur binaire streaming GGUF v3, ModelRegistry avec éviction LRU VRAM et provisioner d'artefacts. | Transverse |
| **`annora-kernel`** | [`../crates/annora-kernel`](../crates/annora-kernel) | Cerveau cognitif : `CognitiveFabric`, routeur System 1/2, `WorkingMemory`, `DeliberatePlanner`, `SelfModel`, `GraphCuriosityScanner`, `DryRunSimulator`, budgets de latence par étage. | Couche 3 |
| **`annora-memory`** | [`../crates/annora-memory`](../crates/annora-memory) | Mémoire à long terme : graphe de triplets SPO avec triple indexation (SPO, POS, OSP — écriture 3 index, lecture par préfixe O(N) documentée, `crates/annora-memory/src/storage/edge.rs:164-167`), backend local ACID `redb`, index vectoriel HNSW `instant-distance` (non branché au recall, brute-force `vector/edge.rs:22-34`), GraphQueryLang DSL, déclin temporel. | Couche 4 |
| **`annora-sync`** | [`../crates/annora-sync`](../crates/annora-sync) | Continuité multi-appareils : ensembles CRDT à croissance seule avec tombstones, horloges vectorielles, coordinateur de handoff de tâche et protocole de réplication TiKV. | Couche 4 |
| **`annora-agents`** | [`../crates/annora-agents`](../crates/annora-agents) | Runtime d'action : bac à sable WASI via Wasmtime, vérification de capabilities Biscuit, quotas d'appels et auto-révocation, agent GUI UI-TARS, protocole inter-agents signé. | Couche 5 |
| **`annora-audit`** | [`../crates/annora-audit`](../crates/annora-audit) | Journalisation inviolable : journal append-only avec signatures Ed25519, chaînage de hachages SHA-256, registre de consentement explicite, moteur de requêtes et d'explicabilité du "pourquoi". | Couche 6 |
| **`annora-auth`** | [`../crates/annora-auth`](../crates/annora-auth) | Gestion d'identité : authentification multimodale continue, détecteur anti-usurpation vocale (liveness), gestionnaire de sessions invités bornées, profils mineurs, capability Wallet de paiement. | Couche 6 |
| **`annora-constitution`** | [`../crates/annora-constitution`](../crates/annora-constitution) | Règles suprêmes : 10 règles constitutionnelles machine-vérifiables, gouvernance d'amendement par quorum, batterie de tests éthiques automatisée. | Couche 6 |
| **`annora-ui`** | [`../crates/annora-ui`](../crates/annora-ui) | Présence ambiante : indicateur minimal à 3 états, HUD Spotlight éphémère (Alt+Space), sélecteur de canal dynamique, budget de silence proactif, compteur financier System 2 en temps réel. | Présentation |
| **`annora-metrics`** | [`../crates/annora-metrics`](../crates/annora-metrics) | Mesure des objectifs : 3 métriques North-Star (Taux de continuité, Conformité d'initiative bornée, Généralité pratique) et détecteur de dérive des valeurs éthiques. | Télémétrie |
| **`annora-daemon`** | [`../crates/annora-daemon`](../crates/annora-daemon) | Démon exécutable principal avec boucle REPL interactive, initialisation en 6 étapes et inspection approfondie de tous les sous-systèmes. | Binaire |

---

## 4. Graphe de Dépendances Inter-Crates

Le schéma suivant illustre l'arbre d'appels et de compilation entre les briques :

```
                                   [ annora-daemon ]
                                          │
         ┌────────────────────────────────┼────────────────────────────────┐
         │                                │                                │
         ▼                                ▼                                ▼
   [ annora-ui ]                   [ annora-kernel ]               [ annora-metrics ]
         │                                │                                │
         │         ┌──────────────────────┴──────────────────────┐         │
         │         │              kernel dépend :                │         │
         │         │  inference + memory + constitution (+core)  │         │
         │         └──────────────────────┬──────────────────────┘         │
         ▼                                ▼                                ▼
  [ annora-agents ]               [ annora-voice ]                [ annora-memory ]──►[ annora-sync ]
         │                                │                                │
         ├────────────────────────────────┼────────────────────────────────┤
         │                                │                                │
         ▼                                ▼                                ▼
   [ annora-auth ]               [ annora-perception ]            [ annora-audit ]──►[ annora-constitution ]
         │                                │                                │
         └────────────────────────────────┴────────────────────────────────┘
                                          │
                                          ▼
                                [ annora-inference ]  ◄── couche basse (GGUF/Safetensors/ONNX ort,
                                          │               Candle Qwen, registre LRU VRAM)
                                          ▼
  [ annora-substrate ] ◄─── [ annora-substrate-windows / linux / macos / redox / mobile / web ]
                                          │
                                          ▼
                                    [ annora-core ]
```

> Écart corrigé : l'ancien arbre plaçait `annora-inference` au-dessus du
> substrate comme dépendant de l'audit ; en réalité `inference` est bas
> (ne dépend que de `core` + `substrate`), et `kernel` dépend de
> `inference` + `memory` + `constitution` (+ `core`/`substrate`), voir
> `crates/annora-kernel/Cargo.toml` et `crates/annora-inference/Cargo.toml`.

---

## 5. Flux de Données et Cycle de Vie d'une Interaction

Une requête cognitive suit un cycle rigoureux garantissant réactivité sub-seconde, sécurité par capabilities et traçabilité inviolable :

```
Utilisateur        Perception           Kernel            Mémoire           Agents / OS          Audit
    │                  │                   │                 │                   │                 │
    │── Audio 16kHz ──►│                   │                 │                   │                 │
    │   "Dis Annora"   │── VAD + ASR ─────►│                 │                   │                 │
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
    │                  │                   │── Action WASM ──┼──────────────────►│ UI-TARS / Exec  │
    │                  │                   │                 │                   │ Action effectuée│
    │                  │                   │                 │                   │                 │
    │                  │                   │── Trace Action ─┼───────────────────┼────────────────►│
    │                  │                   │                 │                   │                 │ H_i = SHA256(H_i-1 || Entry)
    │                  │                   │                 │                   │                 │ Sign Ed25519
    │                  │                   │── Streaming S2S ┼───────────────────┼─────────────────┤
    │◄── Audio Mimi ───┼───────────────────│   (sub-160ms)   │                   │                 │
    │    Full-Duplex   │                   │                 │                   │                 │
```

### Étape 1 : Ingestion Perceptive & Biométrie Continue
1. Le flux microphone (16 kHz, PCM 16-bit) est ingéré par `annora-perception`.
2. Le `VoiceActivityDetector` (Silero VAD) filtre le bruit et active `WakeWordDetector` ("Dis Annora").
3. `SenseVoiceSmall` réalise la transcription phonétique, l'analyse d'émotions et la détection d'événements ambiants en < 50 ms.
4. Simultanément, `Nemotron3Diarization` isole la voix du locuteur et `CAM++` génère un vecteur d'empreinte de 192 dimensions.
5. `annora-auth` vérifie ce vecteur contre les gabarits chiffrés dans l'enclave sécurisée (TPM / Secure Enclave) et génère un jeton Biscuit root pour la personne authentifiée.

### Étape 2 : Résolution Mémorielle & Contexte
1. L'intention transcrite est envoyée à `annora-kernel::CognitiveFabric`.
2. `annora-memory` interroge simultanément :
   - Le graphe de triplets SPO local `redb` pour les relations symboliques exactes.
   - L'index HNSW `instant-distance` pour les concepts sémantiques proches.
3. Les faits récupérés subissent une pondération temporelle (décroissance exponentielle des faits non réactivés).

### Étape 3 : Routage Dynamique System 1 / System 2
1. Le `CognitiveRouter` calcule l'entropie et la complexité de la demande :
   - **System 1** (80 % des cas) : résolu localement par le modèle 3-7B (Qwen / Moshi) en moins de 100 ms.
   - **System 2** : si la complexité syntaxique/sémantique dépasse le seuil ou concerne du raisonnement formel/code lourd, la tâche est escaladée vers un modèle distant ou BYOM (*Bring Your Own Model*), avec mise à jour du `CostCounter` en temps réel.
2. Le `ConfidenceEngine` calcule le score de certitude combiné :
   $$\text{Confiance} = (\text{Score Modèle} \times 0.6) + (\text{Corrélation Graphe} \times 0.4)$$
   Si la confiance est inférieure au seuil du domaine (Général 0.70, Juridique/Finance 0.90, Médical 0.95), une auto-critique interne à deux passes est déclenchée.

### Étape 4 : Délibération, Planification & Simulation à Blanc
1. Si une action motrice est envisagée (clic UI, commande domotique, modification de fichier) :
   - Le `DeliberatePlanner` produit la séquence ordonnée d'étapes.
   - Le `DryRunSimulator` simule l'action dans un bac à sable pour vérifier l'absence d'effets secondaires destructeurs.
2. Le jeton Biscuit est atténué avec le périmètre le plus strict possible (scopes minimaux, quota de requêtes borné, durée de vie brève).

### Étape 5 : Exécution Sécurisée & Journalisation d'Audit
1. L'agent WASM exécute l'action dans le bac à sable Wasmtime ou transmet une intention typée (`Action::Click(x, y)`) au `Substrate` de l'hôte.
2. L'action est enregistrée dans `annora-audit` :
   - Horodatage et identifiant UUIDv7.
   - Signature cryptographique Ed25519 de l'acteur.
   - Empreinte SHA-256 chaînée : $H_n = \text{SHA-256}(H_{n-1} \parallel \text{Payload}_n)$.
   - Pointeurs explicites vers les nœuds du graphe mémoire justifiant l'action.
3. La réponse orale est renvoyée en streaming temps réel via `annora-voice` (codec Kyutai Mimi + Moshi ou CosyVoice2).

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
