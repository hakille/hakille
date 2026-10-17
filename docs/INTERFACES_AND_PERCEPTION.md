# Interfaces Ambiantes et Perception Multimodale

> Référence design désormais : **[AKORA_SPEC_2026-10-12.md](AKORA_SPEC_2026-10-12.md)** (Hakille v0.1) + tokens sources `tokens/` + `annora-design`.
> Ce document reste la description du pipeline perception/voix ; en cas de conflit de tokens (couleurs, rythmes), la spec + `cargo xtask tokens --check` font foi.

> *"Moins l'utilisateur voit d'interface, mieux le système fonctionne. Une présence discrète, pas une application intrusive."*

Ce document constitue la référence exhaustive des interactions utilisateur et de la perception physique d'Annora ([`crates/annora-ui`](../crates/annora-ui), [`crates/annora-perception`](../crates/annora-perception) et [`crates/annora-voice`](../crates/annora-voice)). Il détaille la philosophie de l'interface minimale, l'indicateur de présence ambiant, le HUD Spotlight éphémère, le sélecteur dynamique de canal, le budget de silence, le modèle de situation du monde et le moteur vocal full-duplex avec détection d'interruption naturelle (*Barge-In*).

---

## 1. Philosophie : L'Interface qui n'Existe Presque Pas

Annora rompt avec l'esthétique des assistants virtuels qui imposent de vastes fenêtres de discussion textuelle, des bulles de messages colorées et des avatars bavards. Les principes de design directeurs sont :

1. **Aucune fenêtre de chat par défaut** : Sur le bureau, Annora se résume à un point lumineux discret dans la barre des tâches ou le coin de l'écran.
2. **Voix d'abord, texte en secours, écran jamais forcé** : Le dialogue se déroule oralement ou par des impulsions tactiles/sonores.
3. **Le journal d'audit est le produit** : L'interface visuelle la plus riche du système est l'explorateur d'audit, navigable comme un historique Git avec annulation en un clic.
4. **Charte graphique fonctionnelle et couleur d'alerte unique** : Thème sombre absolu (`#0B0C0F`), texte clair `#ECEEF2`, et **une seule couleur d'accent vive : Braise `#FF7A2F`**. Cette teinte est strictement réservée aux demandes de confirmation humaine explicite (transactions, actions irréversibles). L'œil apprend instantanément à la repérer sans avoir à lire le texte.

---

## 2. Indicateur de Présence Ambiant à 6 États

L'indicateur de présence (`crates/annora-ui/src/presence.rs:90-99`) matérialise l'état interne du système par six comportements visuels (spec Hakille §7 : un point, six états ; repos = lueur `#F4EBDD`, braise `#FF7A2F` = décision requise uniquement) :

```
    [ Idle ]      [ Listening ]    [ Acting ]      [ Speaking ]     [ DecisionRequired ]   [ Urgent ]
     ● Fixe       ◉ Respiration    ✺ Anneau fin    21 fils verticaux  Anneau braise fixe     Double impulsion braise
     0 ms         4000 ms          1200 ms         16 ms (60 i/s)     0 ms (statique)        800 ms
```

- **Veille (`Idle`, 0 ms statique)** : Point 6 px, lueur à 40 %, 0 image/s. Seuls Silero VAD + wake-word tournent en basse consommation.
- **Écoute (`Listening`, 4000 ms)** : Point 8 px qui respire (4,0 s, opacité 60→100 %, échelle 1→1,15). Voix détectée / prise de parole.
- **Action (`Acting`, 1200 ms)** : Anneau fin autour du point, pulsation 1,2 s. Agent WASM ou System 2 en cours ; clic → journal d'audit en direct.
- **Parole (`Speaking`, 16 ms)** : 21 fils verticaux (7 sous 24 px), amplitude = enveloppe audio (`speech_wire_height_px` : 0..24 px). Barge-in : retombée < 80 ms ; 600 ms de silence → repli en point.
- **Décision requise (`DecisionRequired`, 0 ms statique)** : Anneau braise fixe, sans pulsation. Validation humaine exigée.
- **Urgence (`Urgent`, 800 ms)** : Double impulsion braise. Alarme/sécurité définie (N4).

---

## 3. Le HUD Spotlight Éphémère (`EphemeralHud`)

Invoqué par le raccourci global universel **`Ctrl + Alt + Space`** (`Cmd + Space` sur macOS, alias historique `Alt + Space` accepté, spec §8/§29, géré nativement par `annora-substrate-windows`, `linux` ou `macos`) :

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
1. **Apparition Instantanée** : L'appui sur `Ctrl + Alt + Space` (alias `Alt + Space` accepté) projette une ligne de saisie épurée au centre supérieur de l'écran, sans contour de fenêtre ni bordure de titre.
2. **Streaming Inline** : La réponse commence à s'écrire au fur et à mesure que les tokens sont générés par le modèle local Système 1.
3. **Puce de Validation Braise** : Si l'action comporte un risque (ex: virement Wallet, suppression de fichier), le HUD affiche un badge Braise `#FF7A2F` avec le coût estimé et les options `[Entrée: Valider]` ou `[Échap: Annuler]`.
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
| **Bruit ambiant élevé (> 65 dB, seuil unique `ChannelSelector::NOISY_MIN_DB`)** | `EphemeralHud` (ou `VoiceNormal` si casque) | La voix serait inaudible ou forcerait à crier. Écart honnête : `WorldModel::should_suppress_loud_audio` coupe encore à 68,0 dB — TODO aligné sur 65 dB (code `channel_selector.rs:85` fait foi). |
| **Tiers présents dans la pièce** | `EphemeralHud` / `VoiceWhisper` | Préservation stricte de la confidentialité privée. |
| **Réunion détectée (Calendrier / Zoom)** | `EphemeralHud` / `AmbientPulse` | Interdiction formelle d'interrompre la réunion oralement. |
| **Nuit tombée (> 22h00)** | `VoiceWhisper` ou `EphemeralHud` | Ne pas réveiller les autres occupants du foyer. |
| **Appareil porté (Montre / Wearable)** | `Haptic` (Vibration double tap) | Information discrète au poignet sans écran. |
| **Urgence faible et utilisateur non attentif** | `Defer` | Rétention de l'information jusqu'au prochain regard sur l'écran. |

---

## 5. Le Budget de Silence et l'Initiative Bornée

L'autonomie proactive ne doit pas virer au harcèlement de notifications. Annora applique un **budget de silence quotidien** (`crates/annora-ui/src/silence_budget.rs`) :

$$\text{Crédits Quotidiens} = 3 \text{ interventions autonomes par jour (spec §18/§29)}$$

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

---

## 8. Navigation écrans (spec §8 — 4 destinations, classes Slint équipe render)

1. **Journal** (`JournalRow` + `filter_journal`/`page_journal`, 20 lignes/page, virtualisé).
2. **Mémoire** (`MemoryScreenVM` : `[frais] texte (part, conf%)`, filtre `Tout/Personal/Professional/Shared`, jamais de fusion).
3. **Pouvoirs** (`PowersView` + `PowerRow` : périmètre, quota, dernier usage, expiration, « Tout couper »).
4. **Réglages** (`SettingsState` : fournisseurs `[Local/Cloud]`, voix, canaux, langues, a11y, sauvegarde, foyer, simulation).
- Feuilles : **Onboarding** (`OnboardingSheetVM`, `étape 1/5 : manifeste`), **Carte action** (`ActionCardStrings`, 1 décision à la fois, appui long 600 ms), **Pourquoi** (`WhyStrings`, 3 niveaux).

---

## 19. File d'envoi visible (spec §19 — jamais d'envoi surprise)

- `OutboxPanelVM::from_entries` : `"[clé] verbe objet — statut"` (`envoyable` ou `revalidation requise (> 15 min)`).
- Reprise sûre : clé d'idempotence (rejeu sans doublon), FIFO, persistance JSON, tracée au Journal (`:flush`).
- Règle : > 15 min d'attente ⇒ revalidation explicite (`:revalidate <clé>`), sinon l'entrée ne part pas.
