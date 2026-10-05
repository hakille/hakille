# Architecture Technique Complète d'Annora

Ce document détaille l'architecture système, les flux de données inter-couches, les invariants de conception et le cycle de vie d'une requête cognitive au sein d'Annora.

---

## 1. Principes Fondamentaux de Conception

Annora rompt avec l'architecture conventionnelle des assistants conversationnels basée sur le paradigme requête/réponse synchrone dans un fil de discussion.
Elle pose quatre axiomes d'ingénierie :

1. **Continuité d'état sans session** : Le système ne s'arrête jamais d'accumuler de la connaissance et de maintenir un modèle de situation persistant. Les interactions ne démarrent ni ne terminent de "session" ; elles s'inscrivent dans un flux continu.
2. **Système asynchrone à passage de messages typés** : Aucune couche ne dépend directement de l'état interne d'une autre. Toutes les communications traversent un bus de messages typés (`annora_core::Message`) avec des garanties statiques de niveau de confiance (`TrustLevel`).
3. **Éradication de tout runtime managé/Python** : Garantir le temps réel strict, l'empreinte mémoire bornée et l'auditabilité formelle interdit formellement l'utilisation de CPython en production. Toute l'infrastructure est compilée en code machine natif via Rust et WASM.
4. **Cloisonnement formel par capabilities** : Aucun composant, de l'agent périphérique jusqu'au pilote d'entrée graphique, ne dispose de droits implicites. Tout accès exige un jeton cryptographique Biscuit atténué.

---

## 2. Décomposition des 6 Couches

```
+---------------------------------------------------------------------------------+
|                                6. CONFIANCE                                     |
|  - Journal append-only signé Ed25519 (annora-audit)                             |
|  - Chaînage d'empreintes SHA-256                                                |
|  - Constitution versionnée et moteur de priorités (annora-constitution)         |
|  - Attestation matérielle TPM / Secure Enclave (annora-substrate)               |
+---------------------------------------------------------------------------------+
                                       ▲
                                       │ Traçabilité & Validation
                                       ▼
+---------------------------------------------------------------------------------+
|                                5. ACTION                                        |
|  - Runtime de bac à sable WASM/WASI basé sur Wasmtime (annora-agents)           |
|  - Agent GUI UI-TARS émettant des intentions typées                             |
|  - Protocole inter-agents avec passeports cryptographiques                      |
|  - Enregistrement et atténuation des tokens de permissions                      |
+---------------------------------------------------------------------------------+
                                       ▲
                                       │ Intentions & Résultats
                                       ▼
+---------------------------------------------------------------------------------+
|                                4. MÉMOIRE                                       |
|  - Graphe de triplets SPO (Sujet, Prédicat, Objet) (annora-memory)              |
|  - Stockage local ACID mmap via redb (Tier Edge)                                |
|  - Index vectoriel sémantique HNSW via instant-distance                         |
|  - Synchronisation CRDT grow-only sets + tombstones (annora-sync)               |
|  - Décroissance temporelle exponentielle & résolution de contradictions         |
+---------------------------------------------------------------------------------+
                                       ▲
                                       │ Faits & Contextes
                                       ▼
+---------------------------------------------------------------------------------+
|                            3. NOYAU COGNITIF                                    |
|  - Cognitive Fabric & Manifeste de capacités (annora-kernel)                    |
|  - Routeur dynamique System 1 (local sub-100ms) / System 2 (distant)            |
|  - Calculateur de confiance pondéré & auto-critique interne                     |
|  - Gestionnaire de contexte temporel & budget de latence par étage              |
+---------------------------------------------------------------------------------+
                                       ▲
                                       │ Événements perceptifs
                                       ▼
+---------------------------------------------------------------------------------+
|                            2. PERCEPTION                                        |
|  - Détection VAD (Silero) & mot d'activation ("Dis Annora") (annora-perception) |
|  - ASR condensé multi-tâches SenseVoice-Small & VibeVoice-ASR                   |
|  - Diarisation continue Nemotron-3 & signature biométrique CAM++                |
|  - Speech-to-Speech full-duplex Kyutai Moshi (Candle Rust) (annora-voice)       |
|  - Modèle de situation persistant (World Model) & perception d'écran            |
+---------------------------------------------------------------------------------+
                                       ▲
                                       │ Abstraction matérielle
                                       ▼
+---------------------------------------------------------------------------------+
|                            1. COUCHE OS                                         |
|  - Trait Substrate & Compute (annora-substrate)                                 |
|  - Backends OS : Windows, Linux, macOS, Redox OS (Micro-noyau), Mobile, Web     |
|  - Gestion des schemes de périphériques, timers, réseau et ordonnancement       |
+---------------------------------------------------------------------------------+
```

---

## 3. Cycle de Vie d'une Interaction Utilisateur

### Étape 1 : Ingestion Perceptive
1. Le microphone capture un flux PCM 16 kHz continu sans blocage.
2. `VoiceActivityDetector` identifie la présence de voix humaine et alimente `WakeWordDetector`.
3. Dès que "Dis Annora" est identifié, `SenseVoiceSmall` transcrit la parole, extrait la valence émotionnelle et détecte les bruits ambiants en moins de 50 ms.
4. En parallèle, `Nemotron3Diarization` isole le locuteur tandis que `CAM++` génère une empreinte vocale (192-d).
5. `annora_auth::AuthenticationEngine` confronte cette empreinte dans l'enclave sécurisée aux profils du foyer et délivre un `CapabilityToken` root pour cet utilisateur.

### Étape 2 : Routage Cognitif & Résolution Mémorielle
1. L'intention utilisateur est soumise à `CognitiveFabric`.
2. Le `CognitiveRouter` évalue la complexité syntaxique et sémantique :
   - Si la requête relève du quotidien (80% des cas) : routage vers **System 1** (modèle local 3-7B via Candle/GGUF, réponse sous 100 ms).
   - Si la requête exige une délibération lourde, du code complexe ou un raisonnement mathématique : escalade vers **System 2** (modèle distant / BYOM) avec affichage du coût en temps réel (`CostCounter`).
3. En arrière-plan, `annora-memory` extrait simultanément les triplets SPO pertinents depuis `redb` et réalise une recherche vectorielle sémantique HNSW.

### Étape 3 : Délibération & Confiance
1. `ConfidenceEngine` calcule le score combiné :
   $$\text{Score} = (\text{Confiance Modèle} \times 0.6) + (\text{Corrélation Graphe} \times 0.4)$$
2. Comparaison au seuil de risque du domaine concerné :
   - Domaine Général : seuil $0.70$
   - Domaine Juridique / Financier : seuil $0.90$
   - Domaine Médical : seuil $0.95$
3. Si le score est dans la zone grise limite, le moteur exécute une **auto-critique interne** : deux passes de raisonnement se contredisent avant d'interrompre l'utilisateur.

### Étape 4 : Exécution Action & Journalisation Audit
1. Si l'action est motrice (ex: manipulation d'interface via `UI-TARS` ou envoi d'email via un agent WASM) :
   - L'agent WASM reçoit un token Biscuit atténué.
   - Les appels d'entrée système sont validés par `annora-kernel` avant transmission au `Substrate`.
2. L'action est consignée dans `annora-audit` :
   - Id unique UUIDv7
   - Signature Ed25519
   - Hash SHA-256 chaîné à l'entrée précédente
   - Trace de raisonnement et pointeurs vers les faits du graphe justifiant la décision.
3. La synthèse vocale est générée via `Kyutai Moshi` (full-duplex direct) ou `CosyVoice2` en streaming sub-150ms.
