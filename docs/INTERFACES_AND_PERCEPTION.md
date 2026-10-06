# Interfaces Ambiantes et Perception Multimodale

> *"Moins l'utilisateur voit d'interface, mieux le système fonctionne. Une présence discrète, pas une application intrusive."*

Ce document constitue la référence exhaustive des interactions utilisateur et de la perception physique d'Annora ([`crates/annora-ui`](../crates/annora-ui), [`crates/annora-perception`](../crates/annora-perception) et [`crates/annora-voice`](../crates/annora-voice)). Il détaille la philosophie de l'interface minimale, l'indicateur de présence ambiant, le HUD Spotlight éphémère, le sélecteur dynamique de canal, le budget de silence, le modèle de situation du monde et le moteur vocal full-duplex avec détection d'interruption naturelle (*Barge-In*).

---

## 1. Philosophie : L'Interface qui n'Existe Presque Pas

Annora rompt avec l'esthétique des assistants virtuels qui imposent de vastes fenêtres de discussion textuelle, des bulles de messages colorées et des avatars bavards. Les principes de design directeurs sont :

1. **Aucune fenêtre de chat par défaut** : Sur le bureau, Annora se résume à un point lumineux discret dans la barre des tâches ou le coin de l'écran.
2. **Voix d'abord, texte en secours, écran jamais forcé** : Le dialogue se déroule oralement ou par des impulsions tactiles/sonores.
3. **Le journal d'audit est le produit** : L'interface visuelle la plus riche du système est l'explorateur d'audit, navigable comme un historique Git avec annulation en un clic.
4. **Charte graphique fonctionnelle et couleur d'alerte unique** : Thème sombre absolu (`#0D1117`), texte clair contrasté, et **une seule couleur d'accent vive : Coral `#FF5722`**. Cette teinte est strictement réservée aux demandes de confirmation humaine explicite (transactions, actions irréversibles). L'œil apprend instantanément à la repérer sans avoir à lire le texte.

---

## 2. Indicateur de Présence Ambiant à 3 États

L'indicateur de présence (`crates/annora-ui/src/presence.rs`) matérialise l'état interne du système par trois comportements visuels élémentaires :

```
    [ État 1 : VEILLE ]            [ État 2 : ÉCOUTE ]            [ État 3 : ACTION ]
         ● (Fixe)                   ◉ (Pulsation lente)           ✺ (Pulsation rapide)
  Système passif en attente      Voix humaine détectée           Raisonnement ou action
    Zéro CPU consommé             Écoute active VAD               Lien direct vers l'audit
```

- **Veille (`Idle`)** : Point statique atténué (luminosité 30 %). Le système est en veille passive, seuls Silero VAD et le mot d'activation tournent en basse consommation.
- **Écoute (`Listening`)** : Pulsation sinusoïdale lente (période de 2 secondes, luminosité 100 %). Déclenchée dès que la voix est entendue ou lors de la prise de parole.
- **Action (`Acting`)** : Pulsation rythmée rapide (période de 500 ms). Indique qu'un agent WASM ou le moteur Système 2 s'exécute. Un simple clic dessus déploie le journal d'audit en direct.

---

## 3. Le HUD Spotlight Éphémère (`EphemeralHud`)

Invoqué par le raccourci global universel **`Alt + Space`** (géré nativement par `annora-substrate-windows`, `linux` ou `macos`) :

```rust
pub enum HudState {
    Hidden,
    Prompting { query: String, cursor_pos: usize },
    StreamingResponse { query: String, response_text: String, is_final: bool },
    ValidationRequired { action_id: String, description: String, cost_cents: Option<u64>, risk_level: String },
    Evaporating { last_text: String, remaining_ms: u32 },
}
```

### Cycle de Vie d'une Interaction HUD
1. **Apparition Instantanée** : L'appui sur `Alt + Space` projette une ligne de saisie épurée au centre supérieur de l'écran, sans contour de fenêtre ni bordure de titre.
2. **Streaming Inline** : La réponse commence à s'écrire au fur et à mesure que les tokens sont générés par le modèle local Système 1.
3. **Puce de Validation Coral** : Si l'action comporte un risque (ex: virement Wallet, suppression de fichier), le HUD affiche un badge Coral `#FF5722` avec le coût estimé et les options `[Entrée: Valider]` ou `[Échap: Annuler]`.
4. **Auto-Évaporation (1500 ms)** : Dès que l'interaction s'achève, le HUD se dissipe progressivement dans le néant en 1,5 seconde sans nécessiter de fermeture manuelle.

---

## 4. Le Sélecteur de Canal Dynamique (`ChannelSelector`)

Un bon compagnon sait se taire quand les circonstances l'exigent. Le `ChannelSelector` arbitre en continu le meilleur canal de restitution en croisant les signaux ambiants :

```rust
pub struct InteractionContext {
    pub ambient_noise_db: f32,
    pub third_parties_present: bool,
    pub in_meeting: bool,
    pub user_attentive_to_screen: bool,
    pub user_wearing_headphones: bool,
    pub device_type: DeviceType,
    pub urgency: UrgencyLevel,
    pub is_night_time: bool,
}
```

### Table d'Arbitrage des Canaux

| Contexte Détecté | Canal Sélectionné (`OutputChannel`) | Justification Ergonomique |
| :--- | :--- | :--- |
| **Bruit < 45 dB, Seul, Mains libres** | `VoiceNormal` | Confort vocal optimal, dialogue naturel. |
| **Bruit ambiant élevé (> 65 dB)** | `EphemeralHud` (ou `VoiceNormal` si casque) | La voix serait inaudible ou forcerait à crier. |
| **Tiers présents dans la pièce** | `EphemeralHud` / `VoiceWhisper` | Préservation stricte de la confidentialité privée. |
| **Réunion détectée (Calendrier / Zoom)** | `EphemeralHud` / `AmbientPulse` | Interdiction formelle d'interrompre la réunion oralement. |
| **Nuit tombée (> 22h00)** | `VoiceWhisper` ou `EphemeralHud` | Ne pas réveiller les autres occupants du foyer. |
| **Appareil porté (Montre / Wearable)** | `Haptic` (Vibration double tap) | Information discrète au poignet sans écran. |
| **Urgence faible et utilisateur non attentif** | `Defer` | Rétention de l'information jusqu'au prochain regard sur l'écran. |

---

## 5. Le Budget de Silence et l'Initiative Bornée

L'autonomie proactive ne doit pas virer au harcèlement de notifications. Annora applique un **budget de silence quotidien** (`crates/annora-ui/src/silence_budget.rs`) :

$$\text{Crédits Quotidiens} = 5 \text{ interventions autonomes par jour}$$

- **Rappels programmés par l'utilisateur (`UserScheduled`)** : Coût = **0 crédit**. Ils s'exécutent sans restriction.
- **Suggestions autonomes inférées par le système (`SystemInferred`)** : Coût = **1 crédit**.
- **Seuil de confiance minimal** : Pour qu'une suggestion déduite ait le droit de consommer un crédit, sa certitude doit être $\ge 0.85$.
- **Métrique Nord du produit** : Contrairement aux applications commerciales cherchant à maximiser le temps passé et le taux de clics, **Annora vise la diminution du temps d'attention requis**. Un système qui s'améliore interrompt moins souvent et résout les problèmes plus vite.

---

## 6. Pipeline de Perception Multimodale Continue

Le crate [`crates/annora-perception`](../crates/annora-perception) capture en continu l'environnement sensoriel :

### 6.1. Chaîne Audio Non Bloquante
1. **Tampon Circulaire 16 kHz** : Capture continue via le microphone hôte par blocs de 10 ms (160 échantillons).
2. **Silero VAD** : Évalue la probabilité de présence vocale humaine. Si la voix est absente, l'ASR et le GPU restent en sommeil profond.
3. **openWakeWord** : Détecte l'empreinte acoustique de "Dis Annora" en moins de 20 ms sur un seul cœur CPU.
4. **SenseVoice-Small & CAM++** : Dès l'activation, transcription phonétique multi-tâches et extraction du vecteur biométrique 192-d.

### 6.2. Pipeline Visuel & Grounding d'Écran
- `CameraPipeline` : Capture d'images périodiques (1 trame/s ou sur événement) pour évaluer la présence et la distance de l'utilisateur.
- `GuiGrounder` : Coordonne la capture d'écran avec l'agent visuel UI-TARS pour convertir les intentions d'interface en coordonnées physiques réelles.

### 6.3. Modèle de Situation du Monde (`WorldModel`)
Au lieu de traiter chaque requête comme une image isolée, Annora maintient une structure vivante de la situation physique (`SituationalState`) :
```rust
pub struct SituationalState {
    pub location_context: String,          // "Bureau personnel", "Salon", "Extérieur"
    pub present_speakers: HashSet<String>, // Identifiants des locuteurs reconnus
    pub active_application: Option<String>,// Logiciel au premier plan (ex: "VS Code")
    pub ambient_noise_level_db: f32,
    pub user_present: bool,
    pub user_attentive: bool,
    pub third_parties_present: bool,
    pub people_count: usize,
    pub lighting_condition: String,
    pub user_distance_m: f32,
    pub in_meeting: bool,
    pub last_updated: Timestamp,
}
```

---

## 7. Moteur Vocal Full-Duplex et Détection d'Interruption (*Barge-In*)

Dans [`crates/annora-voice`](../crates/annora-voice), le dialogue vocal s'affranchit de l'alternance rigide des tours de parole grâce à Kyutai Moshi et au codec Mimi :

```
Microphone  ──► [ Codec Mimi (80ms) ] ──► [ Moshi Temporal Transformer ] ──► [ Codec Mimi ] ──► Haut-Parleurs
                        ▲                                                            │
                        │                                                            ▼
                        └───────── [ BargeInDetector (Interruption) ] ───────────────┘
```

### Le Détecteur de Barge-In (`BargeInDetector`)
Pendant que le système parle :
1. Le microphone continue d'ingérer l'audio sans coupure.
2. Si l'énergie vocale de l'utilisateur dépasse `speech_threshold` pendant $N$ trames consécutives (environ 80-120 ms), un événement `BargeInEvent` est émis.
3. Le flux audio de sortie est **instantanément coupé à la milliseconde**, et le texte non prononcé est renvoyé au noyau pour ajuster le contexte conversationnel.
4. L'utilisateur peut couper la parole à Annora exactement comme il le ferait avec un interlocuteur humain.
