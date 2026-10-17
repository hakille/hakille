# PLAN NUIT — Annora UI futuriste + produit A-Z (Slint natif)

> Objectif : au réveil, un produit qui tourne de A à Z sur Windows/Linux/macOS
> (+ Web démo, + Redox viable), honnête sur l'inférence, Apple-grade sans
> WebView. Voir ADR-007.

## 0. Choix techno verrouillé : Slint 1.x
- Pourquoi pas Tauri/Dioxus : WebView+IPC (WebKitGTK), 98MiB idle, bug
  titlebar Wayland, incompatible Redox, casse ADR-001 « un seul langage ».
- Pourquoi Slint : `.slint` déclaratif + Rust, rendu natif Skia/femtovg/
  software, <300KiB runtime, GPU/DMA2D/fb, desktop+mobile+embedded+WASM,
  `SystemTrayIcon`+`MenuBar` en DSL (1.17), pas de Chromium.
- Architecture : `annora-ui` = state machines pures testées (27/27) +
  `to_compositor_payload()` ; `annora-ui-shell` = pont fin Rust↔Slint
  (event-loop non bloquant `invoke_from_event_loop`, tray `tray-icon`+`muda`+
  `global-hotkey`, overlays OS : DWM / NSPanel / wlr-layer-shell).

## 1. Vision → périmètre A-Z (tout doit exister au réveil)
### 1.1 §13 — l'interface qui n'existe presque pas
- [ ] PresenceDot toujours allumé : Idle statique 30% / Listening sinus 2000ms
  / Acting rythmé 600ms (doc dit 500ms → trancher 600ms code, noter écart).
  Clic Acting → Journal. Tokens : fond `#0B0C0F`, texte `#ECEEF2`, Braise
  `#FF7A2F` STRICT validation. Acrylic 720px, top-offset 120, blur 30,
  radius 12, opacités 0.88/0.92/0.95, évaporation 1500ms.
- [ ] Pas de chat par défaut. Voix d'abord, texte secours, écran jamais forcé.
- [ ] Journal = le produit : scroll Git-like, cliquable, annulation 1 geste,
  `pourquoi` avec reasoning_trace + nœuds graphe.
- [ ] Onboarding : limites AVANT capacités, `acknowledge` bloquant, manifeste
  Tier injecté (§3ter), pas de statique seule.
- [ ] Palette sombre + 1 accent Coral validation uniquement.

### 1.2 §21 — interaction au-delà de la voix
- [ ] ChannelSelector debug live : 8 entrées contexte → `channel/rationale/
  haptic`. Table : <45dB seul→Voice, >65dB (code 72dB → harmoniser) →HUD,
  tiers→HUD/Whisper, réunion→HUD/AmbientPulse, nuit→Whisper/HUD, wearable→
  Haptic double-tap, faible+inattentif→Defer (file d'attente à créer).
- [ ] Barge-in norme : VAD actif pendant TTS, coupure <120ms, texte restant
  renvoyé contexte. UI : onde + badge « interruption ».
- [ ] Haptique/lumière urgence : `ShortPulse/DoubleTap/SubtleVibe/UrgentBuzz`,
  couleur/rythme urgence (§21c). Kill-switch canal à part.
- [ ] Budget silence : 3/jour (spec §18/§29), UserScheduled 0, SystemInferred 1 si conf≥0.85.
  Jauge x/3, historique filtrable, reward +1, rollover jour, métrique Nord
  « temps attention ↓ ».
- [ ] Continuité inter-appareils : CRDT + ombre cloud, dernière phrase/action
  en attente, jamais répéter contexte.
- [ ] Multi-locuteurs : diarisation live → permissions/wallet à la bonne
  personne. Accessibilité = reconfiguration (sourd→TextFirst/Haptic, LSF
  vision), jamais dégradé. Ton = incertitude réelle (hésiter si conf basse).

### 1.3 §24 — coût, dry-run, offline
- [ ] Coût System2 compteur taxi live : `total/daily_budget (500c EUR)`,
  états normal/near≥80%/exceeded, agrégation HUD+Journal.
- [ ] Dry-run avant irréversible : simulation locale + « voici ce que ça
  donnerait ». UI ValidationChip avec cost/risk.
- [ ] Offline-first : edge redb + System1 local couvrent quotidien sans réseau.
  UI badge « hors-ligne, continuité locale ».

## 2. Honnêteté inférence (règle UI globale)
Afficher `Inspecté / Heuristique locale / Démo` + chiffres réels
(tensor_count, vocab 25055, file_size). INTERDIT : `Opérationnel / Vérifié /
Neural / sub-1ms` sans suffixe `(inspection seule, inférence simulée)`.
- Qwen 0.5B GGUF : header réel, génération keywords → « gabarits locaux ».
- UI-TARS 2B : header réel, intentions keywords → « heuristiques ».
- Mimi : JSON header réel, codec énergie/sine → « démo 24kHz ».
- SenseVoice : tokens.txt réel, transcription énergie→ids 3/6/8/5, conf 0.94
  fixe → « heuristique ».
- CAM++ : 192-dim hash sine → « empreinte déterministe, pas biométrie ».
- MiniLM : 384-dim hash n-gram → « vecteur démo HNSW ».
- Silero VAD : RMS+ZCR, hangover 3 → « heuristique seuil 0.5 ».
- Moshi : tours déterministes, interrupted=false → « démo duplex ».
- CosyVoice : sine 440Hz → « aperçu, voix neurale non chargée ».
- Grounding : 2 boîtes dures Submit/SearchInput, scène DesktopGUI/12 →
  « plan simulé, exécution Substrate réelle si autorisée ».
- 5 agents : CRUD in-memory réels, seeds fictifs, SMTP/CalDAV/MQTT/banque
  simulés, wallet lecture seule (transfer toujours Err). Badges
  « Démo locale / validation requise ».

## 3. Découpage nuit (agents parallèles)
- **Agent A — Shell Slint** : `crates/annora-ui-shell` : `ui/annora.slint`
  (tokens, PresenceDot, HudBar, ValidationChip, JournalView, OnboardingSheet,
  CostBar, SilenceGauge, ChannelDebug), `src/bridge.rs` (Rust→Slint props),
  `src/tray.rs` (tray-icon+muda), `src/hotkey.rs` (global-hotkey Ctrl+Alt+Space /
  Cmd+Space mac, alias Alt+Space accepté), `src/overlay.rs` (DWM/NSPanel/layer-shell/fallback).
- **Agent B — Design system** : tokens `#0B0C0F/#ECEEF2/#FF7A2F`, blur 30,
  radius 12, 720px/y120, anims sinus 2000/600, fade 1500, focus/curseur UTF-8,
  high-contrast/screen-reader/subtitles, responsive 720→360 mobile.
- **Agent C — Inférence honnête** : `InferenceStatus` unifié
  (`Inspected/Simulated/Real`), bandeau `connect_all_models` → « X artefacts
  inspectés », `run_real()` erreur, `:inspect-models` enrichi, tests.
- **Agent D — Agents métier** : 5 agents câblés au Journal (validation Coral
  pour send/unlock, wallet bloqué), seeds marqués DÉMO, ICS/RFC5322 réels
  mis en avant.
- **Agent E — Validation** : `cargo fmt/check/test/conformance 27/27`,
  `deny bans/sources`, screenshots/headless Slint testing.

## 4. Ordre d'exécution (jusqu'au matin)
1. ADR-007 ✅ + ce plan ✅
2. `annora-ui-shell` scaffold + `.slint` + bridge + tests (ce tour)
3. Presence → HUD → ValidationChip → Journal/Mémoire/Consentements
4. Onboarding + Cost + Silence + ChannelDebug + Accessibilité
5. InferenceStatus + bandeaux honnêtes + `:inspect` enrichi
6. 5 agents → Journal + chips validation
7. Tray+hotkey+overlay par OS (Win d'abord, Linux/macOS fallback fenêtre)
8. Web WASM démo (canvas), Redox orbital fallback texte
9. `cargo fmt/check/test/conformance`, deny, docs, screenshots
10. Commit « feat(ui-shell): Slint ambient shell A-Z honnête »

## 5. Critères d'acceptation (Google/Apple-grade)
- `Ctrl+Alt+Space` (alias `Alt+Space` accepté) <100ms, HUD 720px blur 30 radius 12, streaming inline,
  chip Braise avec cost/risk, évaporation 1500ms sans clic.
- Presence 3 états à 60fps, <1% CPU Idle, tap Acting→Journal.
- Journal scroll 1000 entrées, signature badge, annulation 1 geste,
  `pourquoi` complet (pas steps[0] seul).
- Onboarding bloque sans acknowledge, manifeste Tier réel injecté.
- Coût live par appel, near/exceeded à 80%/100%, silence x/3 + historique.
- ChannelDebug affiche les 8 signaux + décision + rationale en direct.
- Aucun libellé mensonger (grep `Opérationnel|Vérifié|Neural` sans
  `inspecté/simulé` = 0). `fmt/check/test/conformance` verts.
- Accessibilité : clavier seul, contraste AA, screen-reader labels sur tous
  les contrôles, mode TextFirst fonctionnel.

## 6. Écarts spec assumés (honnêteté > casse)
- Géométrie commande : HUD actuel 720×64 r12 vs spec §8 640×56 r16. Changement
  gelé (trop risqué ce soir) — voir `// TODO(spec §8)` dans `overlay.rs`.
  La ligne de commande reste à 720 px, top-offset 120, blur 30.
- Hotkey : défaut spec ⌃⌥Espace = `Ctrl+Alt+Space` (`Cmd+Space` macOS),
  modifiable dès l'onboarding ; `Alt+Space` reste un alias accepté.
- Quota silence : 3/jour partout (spec §18/§29).
- Tokens : nouveaux tokens seuls dans le périmètre UI (legacy purgés) ;
  `tray_and_hotkey.rs` hors périmètre signalé, à purger par l'équipe Substrate.

## 7. Risques assumés
- Widgets riches (date-picker, dnd cross-app) custom, pas natifs Slint.
- Packaging MSI/DMG/deb/AppImage à posséder (Tauri faisait mieux).
- Wayland tray/hotkey : StatusNotifier + portal, X11 fallback, échec bruyant
  (pas de succès silencieux). Voir recherche 2026-08-08.
- Vraie inférence neurale (`ort`/Candle forward, tokenizers complets, KV
  cache, GPU) = semaines, PAS cette nuit — on livre inspection honnête +
  architecture prête (`InferenceEngine` trait + `run_real`).

## 8. État écrans (ÉQUIPE MODEL — VMs pures, testées, aucun import Slint)

| Écran | VM `annora-ui-shell` (pur, `[string]` Slint-ready) | Slint (équipe render) | Daemon |
| :--- | :--- | :--- | :--- |
| Mémoire | `MemoryScreenVM::from_rows(&[MemoryRow], partition, contra_count)` → `{visible: "[frais] texte (part, conf%)", partitions: [Tout, Personal, Professional, Shared], contra_label: "X contradiction(s) — Garder A / B / les deux"}` | `screens.slint` + filtre partitions | `:memory-view` existant ; `:mem2` (snippet livré, ancre `":memory-view"`) |
| Réglages | `SettingsState::from_settings(&SettingsView)` → `{rows: "Fournisseur : nom [Local/Cloud : …]", voix, canaux, langues, a11y, sauvegarde, foyer, simulation}` | `screens.slint` | `:settings` existant |
| Onboarding | `OnboardingSheetVM::from_view(&OnboardingView)` → `{limit_lines: "• t [NON-NÉGOCIABLE] — d", cap_lines: "+ c", progress: "étape 1/5 : manifeste"}` | `components.slint` sheet | `:onboarding` existant |
| File d'envoi | `OutboxPanelVM::from_entries(&[(clé, "verbe objet", statut)])` → `"[clé] verbe objet — statut"` (`annora-sync` hors dépendance ui-shell : mapping daemon via `sendable`/`waiting_minutes`) | `screens.slint` | `:outbox` existant |
| Carte + Pourquoi | `ActionCardStrings::from_parts(verb, object, …, pastilles [coût, données, réversibilité], validate "Valider (appui long 600 ms)" si hold, badge `Some("externe")`)` ; `WhyStrings::from_parts(sentence, facts "label — kind, date, source", …, confidence "haute (0.92)")` (miroirs sans dépendance `annora-design`, seuils `<0.5/<0.85` recopiés) | `components.slint` carte + sheet 3 niveaux | `:journal-why` existant ; `:why2 <n\|id>` (snippet livré, pattern `:contradictions` + `memory_graph.all_triplets()`) |

Reste (équipe render / intégrateur) : câbler `ui/components.slint` + `ui/screens.slint`
aux VMs ci-dessus ; déclarer `pub mod screens_model_tests;` dans
`conformance/src/lib.rs` (fichier e2e livré, 5/5 verts hors workspace, prouvé
car `conformance` ne compile pas ce soir : `annora-agents` + `annora-kernel`
cassés par d'autres équipes) ; coller `:mem2` / `:why2` après leurs ancres.
