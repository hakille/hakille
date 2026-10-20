# Hakille — Architecture de l'Interface Ambiante & Perception Multimodale Native

> *"Le principe : moins l'utilisateur voit d'interface, mieux le produit fonctionne. Une AGI perçue ne s'annonce pas avec un splash screen."* — Document de vision technique (§13)

Ce document décrit l'architecture, les invariants de sécurité et l'implémentation de la couche d'interface ambiante et de perception multimodale native dans Hakille.

---

## 1. Vision Fondatrice : L'Interface qui n'existe presque pas (§13)

Hakille rompt fondamentalement avec le paradigme des assistants conversationnels actuels (boîtes de dialogue web, fenêtres de chat dédiées, clics superflus) pour adopter un design de niveau Apple/Google axé sur la discrétion absolue et la continuité cognitive.

### Invariants Clés du Design
1. **Zéro fenêtre de chat par défaut** :
   Hakille ne maintient aucune application fenêtrée permanente ouverte sur l'écran de l'utilisateur.
2. **Point de présence ambiant minimal** :
   Un indicateur discret dans la barre d'état système avec seulement trois états perceptifs :
   - **Veille (Idle)** : Statique, point gris anthracite estompé (`#787878`), aucune consommation GPU/CPU visible.
   - **Écoute (Listening)** : Pulsation lente (période de `2000 ms`), rythme respiratoire signalant l'activité vocale ou l'attention sensorielle.
   - **Action en cours (Acting)** : Pulsation rythmique rapide (`600 ms`), accès instantané en 1 clic/tap au journal d'audit append-only.
3. **Règle absolue de la couleur d'accent (Coral `#FF5722`)** :
   Le thème visuel est sombre par défaut (`#121212`, noir OLED). Une couleur d'accent unique, le Corail (`#FF5722`), est **exclusivement et strictement réservée** aux actions requérant une confirmation ou validation humaine (ex. dépense de portefeuille, envoi de communication externe, suppression irréversible). Elle n'est jamais utilisée à des fins décoratives.
4. **Le Journal d'Audit EST le produit** :
   Interface exploratoire fluide style historique Git (`git log`). Chaque entrée signée Ed25519 affiche son empreinte cryptographique, son tracé de raisonnement explicatif et un bouton d'annulation/rollback réversible en 1 clic.

---

## 2. Ephemeral HUD / QuickBar (Spotlight & Raycast Native)

Sur desktop et laptop, le raccourci global universel (`Alt+Space` sous Windows/Linux, `Cmd+Space` sous macOS) invoque une **ligne de commande éphémère** :

```text
╭─ [Hakille HUD • Alt+Space] ──────────────────────────╮
│ > Transférer 25 EUR pour déjeuner█                  │
╰─────────────────────────────────────────────────────╯
```

### Cycle de Vie Éphémère
1. **Invocation instantanée** : Apparition sans latence sous forme de barre acrylique translucide centrée.
2. **Streaming inline** : Les tokens de réponse s'écrivent directement dans la ligne, sans ouvrir de panneau supplémentaire.
3. **Puce de validation Corail `#FF5722`** :
   ```text
   ╭─ [Hakille VALIDATION REQUIRED #FF5722 CORAL] ─────────╮
   │ Action [wallet-spend-42]: Envoi 25.00 EUR vers Resto │
   │ Risk: Financial | Cost: 25.00 EUR                    │
   │ [Enter] Confirmer  |  [Esc] Annuler                  │
   ╰──────────────────────────────────────────────────────╯
   ```
4. **Auto-évaporation** : Dès que l'action est exécutée ou la réponse délivrée, la ligne s'évapore dans le vide (`1500 ms`) et disparaît complètement, ne laissant aucune fenêtre résiduelle.

---

## 3. Perception Multimodale Native & Caméras (§6)

Hakille traite en continu les flux de capteurs (caméra webcam, microphones de proximité, capteurs de présence) directement via le trait `Substrate`.

### Invariant Fondamental de Confidentialité
> **Aucun pixel brut ne quitte la mémoire vive éphémère locale.**

- Les trames capturées (`CameraFrame`) sont analysées en temps réel dans `hakille-perception::CameraPipeline`.
- Une empreinte cryptographique SHA-256 de chaque trame est calculée pour l'attestation et l'auditabilité matérielle.
- Les données de pixels bruts sont immédiatement jetées de la mémoire. Seules les métadonnées sémantiques sont conservées et transmises au `WorldModel`.

### Analyse de Présence & Attention en Temps Réel
`CameraPipeline` analyse :
- **Présence utilisateur** : `UserPresence::Present`, `Absent`, `Distracted` (regard détourné), `MultiplePeopleDetected { count }`.
- **Estimation de distance** : Calcul de la distance utilisateur (`estimated_distance_m`).
- **Attention au dispositif** : Vecteur de regard de l'utilisateur (`user_gaze_focused_on_device`).
- **Condition d'éclairage** : `Daylight`, `IndoorAdequate`, `Dim`, `Dark`.

---

## 4. Le Sélecteur de Canal Dynamique (`ChannelSelector`) (§21a)

> *"Parler à voix haute dans un train, en réunion ou tard le soir est une mauvaise idée."*

Le `ChannelSelector` arbitre en temps réel le canal de communication idéal en fonction de la situation ambiante :

| Contexte Ambiant | Détection Capteurs | Canal Sélectionné | Justification |
| :--- | :--- | :--- | :--- |
| **Seul, calme (< 50 dB)** | Caméra = Présent, Diarisation = 1 | `OutputChannel::VoiceNormal` | Expérience mains-libres naturelle |
| **Nuit / Pièce silencieuse (< 32 dB)** | Heure nocturne ou ambiance feutrée | `OutputChannel::VoiceWhisper` | Voix chuchotée sans déranger l'entourage |
| **En réunion (Meeting actif)** | Statut réunion / Micro conf | `OutputChannel::EphemeralHud` | Discrétion absolue, aucun son émis |
| **Présence de tiers / Bureau ouvert** | Caméra > 1 personne ou diarisation multi-voix | `OutputChannel::EphemeralHud` | Protection de la vie privée et secret des données |
| **Tiers présents + Casque branché** | Écouteurs détectés | `OutputChannel::VoiceNormal` | Le son est confiné au casque de l'utilisateur |
| **Bruit ambiant élevé (> 72 dB)** | Bruit de fond mesuré > 72 dB | `OutputChannel::EphemeralHud` | La voix serait inaudible ou étouffée |
| **Tâche de fond terminée (Wearable)** | Montre / anneau connecté | `OutputChannel::Haptic` | Micro-vibration tactile sans coupure visuelle |
| **Statut passif en arrière-plan** | Synchronisation terminée | `OutputChannel::AmbientPulse` | Simple clignotement du point de présence |

---

## 5. Le Budget de Silence (`SilenceBudgetTracker`) (§21d)

> *"Le budget de silence — l'initiative proactive se rationne."*

Pour éviter le travers des assistants intrusifs qui dérangent l'utilisateur à tout moment :
1. **Rappels programmés par l'utilisateur** : Coûtent **0 crédit**. Toujours délivrés.
2. **Suggestions inférées par le système** : Coûtent **1 crédit d'interruption** sur un quota quotidien strict (par défaut 5 crédits / jour).
3. **Condition de confiance requise** : Une suggestion proactive n'est autorisée que si la confiance cognitive de System 2 dépasse `0.85`.
4. **Épuisement du budget** : Lorsque les 5 crédits sont consommés, Hakille bascule en silence complet pour le reste de la journée, attendant une sollicitation explicite de l'utilisateur.

---

## 6. Intégration Multi-Plateforme (Toutes les OS)

L'adaptateur `PlatformAdapter` mappe nativement cette expérience sur chaque environnement hôte :

- **Windows** : Icône de barre des tâches `Shell_NotifyIcon`, composition acrylique DirectComposition, enregistrement `RegisterHotKey(Alt+Space)`.
- **Linux** : D-Bus `StatusNotifierItem` (KStatusNotifier), superposition Wayland `wlr-layer-shell` / X11 overlay, portails `xdg-desktop-portal`.
- **macOS** : `NSStatusItem` dans la barre de menu, panneau flottant `NSPanel` avec `NSVisualEffectView`, écoute globale `CGEventTap(Cmd+Space)`.
- **Redox OS** : Schéma d'affichage natif orbital, IPC scheme pour les tampons éphémères.
- **Mobile (Android / iOS)** : Pilule de statut persistante Foreground Service, retour haptique haptics engine, geste de balayage sur le bord.
- **Web (WASM)** : API Web Audio, Notifications Web, canvas HTML5 flottant discret.
