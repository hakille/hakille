# Le Noyau Cognitif et la Mémoire Distribuée d'Hakille

> *"Penser en continu, planifier avant d'agir, et retenir ce qui compte sans jamais encombrer le graphe."*

Ce document constitue la référence exhaustive de la couche cognitive d'Hakille : le **Noyau Cognitif** ([`crates/hakille-kernel`](../crates/hakille-kernel)) et la **Mémoire Distribuée** ([`crates/hakille-memory`](../crates/hakille-memory)). Il détaille la dynamique bicamérale Système 1 / Système 2, l'abstraction du Fabric cognitif, la mémoire de travail éphémère, la planification délibérée, le modèle introspectif de soi, le scanner de curiosité, ainsi que l'architecture du triple store symbolique et vectoriel.

---

## 1. Architecture Bicamérale Dynamique : Système 1 et Système 2

Inspiré des neurosciences cognitives (Kahneman) et adapté aux contraintes de systèmes distribués souverains, le noyau d'Hakille articule deux régimes de pensée complémentaires :

```
                                 [ Intention Perceptive ]
                                            │
                                            ▼
                             ┌──────────────────────────────┐
                             │       CognitiveRouter        │
                             │   Évaluation de Complexité   │
                             └──────────────┬───────────────┘
                                            │
                     ┌──────────────────────┴──────────────────────┐
                     │ Complexité ≤ Seuil                          │ Complexité > Seuil
                     ▼ (< 100 ms)                                  ▼ (< 1200 ms)
        ┌──────────────────────────────┐              ┌──────────────────────────────┐
        │          SYSTÈME 1           │              │          SYSTÈME 2           │
        │ - Modèle local 3-7B / GGUF   │              │ - Raisonnement lourd / VLM   │
        │ - Dialogue continu Moshi     │              │ - Cloud sécurisé / BYOM      │
        │ - Réflexes & Intentions GUI  │              │ - Décomposition math / code  │
        │ - Couvre 80% des requêtes    │              │ - Suivi financier en direct  │
        └──────────────┬───────────────┘              └──────────────┬───────────────┘
                       │                                             │
                       └──────────────────────┬──────────────────────┘
                                              │
                                              ▼
                             ┌──────────────────────────────┐
                             │       ConfidenceEngine       │
                             │  Auto-Critique si Incertain  │
                             └──────────────────────────────┘
```

### Le Routeur Cognitif (`CognitiveRouter`)
Le routeur de complexité n'est pas une simple table heuristique. Il évalue en continu :
1. **L'entropie linguistique** de la requête utilisateur.
2. **La présence d'indicateurs de raisonnement formel** (algorithmes, logique booléenne, transactions financières, modifications d'infrastructure).
3. **Le budget de latence disponible** fixé par le contexte interactif.

### Budgets de Latence par Étage
Pour préserver la fluidité de la conversation sans introduire de silences déroutants, chaque maillon respecte un plafond de temps strict :

| Étage de Traitement | Modèle / Brique | Budget Cible (ms) | Seuil Critique (ms) |
| :--- | :--- | :--- | :--- |
| **1. Détection Voix (VAD)** | Silero VAD | **< 10 ms** | 15 ms |
| **2. Mot d'Activation** | openWakeWord | **< 20 ms** | 30 ms |
| **3. Transcription (ASR)** | SenseVoice-Small | **< 50 ms** | 80 ms |
| **4. Diarisation & Biométrie** | Nemotron-3 + CAM++ | **< 25 ms** | 40 ms |
| **5. Résolution Mémorielle** | redb SPO + recherche brute-force (HNSW non branché, `vector/edge.rs:22-34`) | **< 15 ms** | 30 ms |
| **6. Raisonnement Système 1** | Qwen 7B Q4 / Candle | **< 100 ms** | 150 ms |
| **7. Raisonnement Système 2** | Modèle Distant / BYOM | **< 1200 ms** | 2000 ms |
| **8. Synthèse Vocale (TTS)** | Moshi (Mimi) / CosyVoice2 | **< 80-150 ms** | 200 ms |

> Écart honnête : cette table vise les étages perceptifs ; le budget pipeline exécutable est `PipelineBudgetConfig` (perception 60 / routing 20 / inférence 100 / action 40 / total 220 ms, mapping `memory_extract|recall|context_record`→perception etc., `crates/hakille-kernel/src/budget.rs:14-24,54-68`).

---

## 2. Le Fabric Cognitif (`CognitiveFabric`)

Le noyau ne code jamais en dur des références directes à des fichiers de modèles. Il déclare des **capacités cognitives abstraites** au sein d'un manifeste dynamique :

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityName {
    Listen,      // Écoute passive VAD / WakeWord
    See,         // Perception visuelle & caméra
    Speak,       // Synthèse vocale & codec Mimi
    ActGui,      // Grounding visuel d'interface UI-TARS
    Reason,      // Raisonnement Système 1 local
    ReasonHeavy, // Raisonnement Système 2 lourd
    Identify,    // Extraction d'embedding biométrique CAM++
    Diarize,     // Diarisation multi-locuteurs Nemotron-3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub model_ref: ModelReference,
    pub runtime: RuntimeType,
    pub quantization: Option<String>,
    pub max_memory_mb: u64,
    pub loading_strategy: LoadingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CognitiveManifest {
    pub capabilities: HashMap<CapabilityName, CapabilityBinding>,
}
```

Plusieurs capacités peuvent être liées au même modèle unifié en mémoire vive. Lorsque la technologie évolue (par exemple vers un modèle multimodal unifié de type *Nemotron Nano Omni*), **aucune ligne du noyau n'est modifiée** : seules les liaisons du manifeste sont réassociées, éliminant les étapes de sérialisation intermédiaires.

---

## 3. Moteur de Confiance et Seuils de Risque par Domaine

Toute action ou réponse générée est soumise au `ConfidenceEngine` qui calcule un score combiné pondéré :

$$\text{Confiance Combinée} = (\text{Certitude Modèle} \times 0.6) + (\text{Corrélation Graphe} \times 0.4)$$

Le score est confronté au seuil de risque du domaine concerné :
- **Domaine Général (conversation, agenda, météo)** : Seuil minimal = **$0.70$**.
- **Domaine Juridique & Financier** : Seuil minimal = **$0.90$**.
- **Domaine Médical** : Seuil minimal = **$0.95$**.

### Auto-Critique Interne à Deux Passes
Si le score de confiance se situe dans la "zone grise" (entre $0.65$ et le seuil cible), le système s'interdit d'interrompre immédiatement l'utilisateur. Il exécute une **auto-critique interne** : deux passes de raisonnement contradictoires s'opposent en mémoire vive pour tenter de consolider la certitude avant de consommer un crédit du budget de silence.

---

## 4. Composants Introspectifs Avancés du Noyau

### 4.1. Mémoire de Travail Éphémère (`WorkingMemory`)
Conformément au principe §28a, le graphe de connaissances long terme ne doit pas être pollué par le bruit des calculs temporaires (variables intermédiaires, brouillons de recherche, hypothèses non prouvées).
- **Structure** : Contient `scratchpad_notes`, `hypotheses` (`WorkingHypothesis`), `observations` (`IntermediateObservation`) et une table clé-valeur de variables d'exécution.
- **Distillation** : À la fin d'une tâche, seules les conclusions validées (`distillation_queue`) sont converties en faits et insérées dans le graphe durable. Le reste est purgé.

### 4.2. Planificateur Délibéré (`DeliberatePlanner`)
Avant d'exécuter la moindre action, le noyau génère un `ExecutionPlan` ordonné.
- Chaque étape (`PlannedStep`) spécifie son identifiant, le scope Biscuit requis, l'action précise (`CapabilityAction`) et si elle est **irréversible**.
- Toute étape marquée irréversible (ou touchant au `CapabilityScope::Wallet`) bascule le plan en `RiskLevel::Irreversible` ou `RiskLevel::High`, déclenchant une demande de confirmation humaine obligatoire avec aperçu clair.

### 4.3. Modèle Introspectif de Soi (`SelfModel`)
Maintient l'état interne direct de l'agent cognitif :
- États : `Standby`, `Perceiving`, `Reasoning`, `Simulating`, `Executing`, `Hesitating { reason_code }`.
- Si le système hésite, `hesitation_rationale` consigne en clair la raison de l'hésitation (ex: *"Score de confiance 0.68 inférieur au seuil médical 0.95"*). L'état est interrogeable en temps réel par la commande `:status` ou via l'interface UI.

### 4.4. Simulateur d'Exécution à Blanc (`DryRunSimulator`)
Rejoue les actions risquées dans un environnement virtuel avant tout engagement dans le monde réel.
- Pour une transaction de paiement, il calcule le solde avant/après, vérifie les plafonds et génère un `SimulatedDiff` avec prévisualisation humaine (`human_readable_preview`).

### 4.5. Scanner de Curiosité Mémorielle (`GraphCuriosityScanner`)
Opère pendant les périodes d'inactivité nocturne ou à faible intensité carbone :
- Explore le graphe pour identifier les entités orphelines, les objectifs sans date limite ou les informations lacunaires.
- Prépare des questions ciblées (`CuriosityInquiry`) qui ne sont posées que lorsque l'utilisateur reprend l'initiative de la parole.

### 4.6. Hiérarchie de Priorité Constitutionnelle
En cas de conflit d'instructions, la résolution suit une règle mathématique d'ordre total :

$$\text{Règle Constitutionnelle} > \text{Instruction Humaine Explicite} > \text{Politique par Défaut}$$

---

## 5. Mémoire Distribuée : Le Graphe SPO et le DSL de Requête

Hakille rejette le simple RAG textuel au profit d'un **graphe de triplets symboliques SPO** (*Sujet, Prédicat, Objet*) enrichi d'un index vectoriel. Écriture SPO déterministe (registre nom→UUID FNV-1a, `crates/hakille-memory/src/graph.rs:53-67`, `crates/hakille-kernel/src/orchestrator.rs:71-135`) ; contradiction ambigüe = AskUser bloquant sans écrasement (`orchestrator.rs:115-134`).

### Modèle de Données des Triplets
Chaque fait dans le graphe est une structure typée avec métadonnées associées :
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TripletMetadata {
    pub weight: f64,                 // Poids d'activation initial (1.0)
    pub created_at: Timestamp,
    pub last_activated: Timestamp,
    pub source: TripletSource,       // User, System, Inferred, External
    pub confidence: f64,             // Indice de certitude (0.0..1.0)
    pub context: ContextPartition,   // Default, Professional, Personal, Shared, Custom
    pub trust_level: TrustLevel,     // Trusted, Untrusted
    pub decay_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Triplet {
    pub id: TripletId,
    pub subject: NodeId,
    pub predicate: PredicateId,
    pub object: NodeValue, // Entity(NodeId), Text(String), Number(f64), Boolean(bool), Timestamp, Embedding
    pub metadata: TripletMetadata,
}
```

### Triple Indexation (SPO, POS, OSP)
Pour garantir des traversées rapides sans scan complet (nuance : EdgeStore écrit SPO/POS/OSP mais la lecture par préfixe est O(N) bornée au tier edge, `crates/hakille-memory/src/storage/edge.rs:164-167`) :
1. **Index SPO** : Permet de trouver instantanément toutes les propriétés d'une entité donnée.
2. **Index POS** : Permet de trouver tous les sujets partageant un prédicat et une valeur cible.
3. **Index OSP** : Permet d'effectuer des recherches inversées depuis les objets vers les sujets.

### Moteur de Stockage Double Tier
- **Tier Edge (`redb`)** : Moteur de stockage clé-valeur 100% Rust, basé sur B-Trees et mémoire mappée (`mmap`). Transactionnel ACID, zéro dépendance externe, latence de lecture < 1 ms.
- **Tier Cluster (`TiKV`)** : Base clé-valeur distribuée à consensus Raft pour la synchronisation multi-machines à grande échelle.

### Recherche Sémantique Vectorielle HNSW (`instant-distance`)
Pour les requêtes approximatives où la formulation exacte diffère du prédicat enregistré, un index HNSW (*Hierarchical Navigable Small World*) tourne localement, permettant de retrouver des nœuds voisins en quelques microsecondes par distance cosinus (nuance : `instant-distance` non branché au recall — brute-force documentée, `crates/hakille-memory/src/vector/edge.rs:22-34` ; recall réel 3 étages + cosinus 0.40 dernier recours, `crates/hakille-kernel/src/orchestrator.rs:150-248`).

### Décroissance Temporelle Exponentielle
Les faits qui ne sont pas réactivés voient leur poids décroître selon la loi exponentielle :

$$W(t) = W_0 \cdot e^{-\lambda (t - t_0)}$$

Les faits anciens ne sont **jamais effacés silencieusement** : leur poids d'activation diminue, induisant une nuance dans le ton de la voix (*"Il y a quelque temps, tu préférais..."* au lieu d'une affirmation catégorique).

### Résolution Formelle des Contradictions
Lorsqu'un fait nouveau contredit un fait préexistant (ex: *adresse = Paris* vs *adresse = Lyon*) :
1. Si le nouveau fait a une confiance et une fraîcheur nettement supérieures, il prévaut et l'ancien est archivé avec un lien de succession historique.
2. Si l'écart de certitude est ambigu, la contradiction est inscrite dans la table des conflits et signalée lors de la prochaine interaction naturelle.

### Langage de Requête Déclaratif (`GraphQueryLang` DSL)
Permet d'interroger le graphe de façon déclarative et fortement typée :
```rust
let query = GraphQuery::new()
    .with_subject(user_id)
    .with_predicate(PredicateId::new("travaille_sur"))
    .with_partition(ContextPartition::Professional)
    .with_min_confidence(0.85);

let results: Vec<&Triplet> = query.execute(&graph);
```

### Le Crépuscule Numérique (*Digital Twilight*)
Le modèle intègre la gestion de fin de vie des données :
- Destruction cryptographique certifiée des clés de déchiffrement.
- Mode héritage : désignation explicite des partitions transmissibles à des proches.
