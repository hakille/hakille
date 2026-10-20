# Cognitive Fabric & Routage Dynamique System 1 / System 2

Ce document détaille le fonctionnement du **Cognitive Fabric** d'Hakille et les mécanismes de décision cognitive.

---

## 1. Le Concept de Cognitive Fabric

Dans une architecture conventionnelle, le code applicatif lie en dur des bibliothèques de modèles spécifiques (`whisper.transcribe()`, `llm.generate()`). Cette approche présente deux défauts majeurs :
1. Elle rend le remplacement d'un modèle lourd et risqué.
2. Elle empêche d'absorber la convergence progressive vers des modèles omnimodaux unifiés.

### Découplage par Capacités
Hakille sépare strictement la notion de **Capacité** de la notion d'**Instance de Modèle**. Le noyau `hakille-kernel` n'interagit qu'avec des capacités canoniques :

```rust
pub enum CapabilityName {
    Listen,       // Perception audio instantanée (ASR)
    ListenLong,   // Transcription et diarisation long format (jusqu'à 60m)
    See,          // Compréhension de scène et vision ambiante
    Speak,        // Synthèse vocale ou codec neural speech-to-speech
    ActGui,       // Manipulation directe d'interfaces graphiques
    Reason,       // Délibération System 1 (locale, temps réel)
    ReasonHeavy,  // Délibération System 2 (complexe, distribuée)
    Identify,     // Extraction d'embeddings biométriques
    Diarize,      // Séparation acoustique des intervenants
}
```

Chaque capacité est liée dans le manifeste de plateforme (`manifests/*.toml`) à un binding d'exécution :
```toml
[capabilities.listen]
model_ref = "SenseVoice-Small"
runtime = "OnnxRuntime"
quantization = "INT8"
max_memory_mb = 250
loading_strategy = "AlwaysResident"
```

### Fusion Transparente des Modèles
Le Cognitive Fabric inspecte les bindings au démarrage. Si deux capacités différentes (par exemple `See` et `ActGui`) pointent vers le même `model_ref` (par exemple un modèle unifié multimodal de type Nemotron Omni ou Qwen-VL) :
- Le modèle n'est instancié qu'une seule fois en VRAM.
- Les sauts de sérialisation textuelle intermédiaire sont court-circuités automatiquement.
- La latence globale diminue sans modifier une seule ligne de code du noyau.

---

## 2. Le Routeur Cognitif : System 1 & System 2

Le système applique la théorie cognitive des processus duels (Kahneman) :

```
                        Requête Utilisateur
                                │
                                ▼
                    +───────────────────────+
                    │  Classifieur de       │
                    │  Complexité & Entropie│
                    +───────────────────────+
                                │
                 Score < Seuil ──┴── Score ≥ Seuil
                 (80% des cas)       (20% des cas)
                       │                   │
                       ▼                   ▼
               +───────────────+   +────────────────────────+
               │   SYSTEM 1    │   │        SYSTEM 2        │
               │  Modèle local │   │ Modèle lourd / distant │
               │     3-7B      │   │  (BYOM / Cluster GPU)  │
               │ Latence <100ms│   │   Compteur de coût     │
               +───────────────+   +────────────────────────+
```

### Critères de Routage
1. **Classifieur de Complexité Dédié** : Un modèle compact ou un encodeur spécialisé évalue l'arité du problème, la présence de raisonnement formel requis et la dépendance multi-étapes.
2. **Mesure d'Entropie Prédictive** : Si la distribution de probabilité des premiers tokens de réponse du System 1 présente une entropie trop élevée (doute épistémique), la requête est immédiatement basculée sur le System 2.
3. **Apprentissage du Routage** : Les seuils de basculement s'affinent dans le temps en fonction des rétroactions et corrections réelles apportées par l'utilisateur.

---

## 3. Budgets de Latence par Étage

Pour préserver la sensation d'immédiateté et de fluidité conversationnelle, chaque maillon de la chaîne cognitive dispose d'un budget temporel strict :

| Étage de Traitement | Budget Alloué (Maximal) | Composant Typique |
| :--- | :--- | :--- |
| **VAD + Wake Word** | $\le 30\text{ ms}$ | Silero VAD + openWakeWord INT8 |
| **ASR (Speech-to-Text)** | $\le 80\text{ ms}$ | SenseVoice-Small non-autorégressif |
| **Recherche Mémorielle** | $\le 25\text{ ms}$ | redb (SPO) + instant-distance (HNSW) |
| **Délibération System 1** | $\le 100\text{ ms}$ | Qwen 3B/7B Q4_K_M (Candle) |
| **TTS (Time-To-First-Audio)** | $\le 120\text{ ms}$ | CosyVoice2 streaming ou Moshi direct |
| **Budget Total Cascade** | **$\le 355\text{ ms}$** | Interaction conversationnelle complète |

En mode **Full-Duplex Speech-to-Speech (Kyutai Moshi)**, la cascade ASR → LLM → TTS est fusionnée en une boucle neuronale unique opérant à **160 ms de latence intrinsèque** (80 ms de trame Mimi + 80 ms acoustique).

---

## 4. BYOM (Bring Your Own Model) & Transparence des Coûts

Hakille refuse tout verrouillage vers un fournisseur d'API unique pour le System 2 :
- L'utilisateur peut configurer une clé d'API commerciale standard (OpenAI, Anthropic, Google) ou son propre cluster d'inférence privé (vLLM, Ollama, SGLang, cluster personnel TiKV).
- Chaque appel à System 2 calcule son coût financier réel et l'incrémente sur le `CostCounter` visible en direct dans l'interface, sans surprise en fin de mois.
