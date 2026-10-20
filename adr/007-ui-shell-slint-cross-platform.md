# ADR 007: Coquille UI cross-platform — Slint natif (pas de WebView)

## Statut
Remplacé par ADR-009 (2026-10-17) — le renderer Slint a été retiré à
l'Étape 6 (winit + wgpu + AccessKit). Conservé pour l'historique.
Accepté (2026-10-11).

## Contexte
Vision §13 (interface qui n'existe presque pas) + §21 (sélecteur de canal,
barge-in, budget de silence, accessibilité) exige :
- point de présence 3 états toujours allumé, < 1% CPU, zéro WebView
- HUD éphémère `Alt+Space` (overlay acrylique 720x64→280, blur 30, radius 12,
  Coral #FF5722 réservé validation), auto-évaporation 1500ms
- journal d'audit = le produit (scroll Git-like, annulation 1 geste)
- onboarding "limites avant capacités" (manifeste Tier)
- compteur coût System 2 temps réel, budget silence 5/jour, sélecteur canal
- cibles : Windows 11, Linux (X11+Wayland wlr-layer-shell), macOS (NSPanel),
  Android/iOS (Tier invité), Web WASM (démo), Redox OS (Tier souverain)
- contrainte ADR-001 : Rust pur, pas de runtime Python/JS obligé en prod

Options évaluées (oct. 2026) :
- **Tauri v2** : meilleur packaging desktop (MSI/DMG/deb), plugins mûrs,
  mais WebView+IPC obligatoires (WebKitGTK), binaire lourd, incompatible
  Redox, casse "un seul langage".
- **Dioxus 0.7** : React-like Rust, tests VirtualDom excellents, mais desktop
  = wry/WebView (mêmes dettes WebKitGTK/GTK), bug titlebar Wayland constaté,
  packaging jeune, 287 crates / ~98MiB idle.
- **Iced/egui** : pur Rust GPU, mais pas de tray/menu natif mûr (tray #124
  depuis 2019 pour iced), widgets riches à reconstruire.
- **Slint 1.17-1.18** : déclaratif `.slint` + Rust, rendu natif (Skia/femtovg/
  software, GPU/DMA2D/fb), <300KiB runtime, desktop+mobile+embedded+WASM,
  `SystemTrayIcon`+`MenuBar` en DSL (1.17), pas de WebView/Chromium,
  compatible Redox/ressources contraintes et adapté au desktop/mobile/web.

## Décision
- `hakille-ui` reste **headless / testable** : state machines pures
  (Presence, EphemeralHud, ChannelSelector, SilenceBudget, CostCounter,
  JournalView, Onboarding, Accessibility) + `to_compositor_payload()` JSON.
- Nouveau crate `hakille-ui-shell` : pont fin vers **Slint** :
  `ui/hakille.slint` (design tokens #0D1117/#E0E0E0/#FF5722, PresenceDot,
  HudBar, ValidationChip, JournalView, OnboardingSheet) + `shell.rs`
  (bridge Rust↔Slint, event-loop non bloquant via `invoke_from_event_loop`,
  tray `tray-icon`+`muda`+`global-hotkey`, overlay par OS : DWM / NSPanel /
  wlr-layer-shell, fallback fenêtre centrée).
- Pas de JS/TS obligatoire : tout le comportement en Rust. Web = build WASM
  Slint (canvas) pour démo uniquement.
- Accessibilité : rôles/arbre Slint + `to_compositor_payload` pour lecteurs.

## Conséquences
- Positif : 1 codebase `.slint`+Rust pour 6 OS, binaire natif sans Chromium,
  point présence low-footprint, HUD <16ms, Redox viable, tests headless via
  `i-slint-backend-testing` + tests Rust existants (27/27 conformance gardés).
- Négatif : widgets riches (date-picker, dnd cross-app) à construire ;
  packaging (MSI/DMG/deb/AppImage) à posséder (Tauri faisait mieux) ;
- Travail nuit : shell → presence → HUD → journal → onboarding → coût/silence/
  canal → inférence honnête → agents → validation (voir PLAN_NUIT_UI.md).
