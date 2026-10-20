# Spikes Phase 0 — protocole labo (spec Hakille §10)

> Ce qui est prouvé en CI (outillé) vs ce qui exige du matériel humain.
> Gate : aucun engagement iced/Redox/IME/événementiel avant sortie de spike.

## Déjà prouvé en CI (`hakille-spikes`, `hakille-ui-shell`)

| Spike | Preuve automatisée | Fichier |
| --- | --- | --- |
| S1-données | Arbre AccessKit réel depuis VMs : 0 interactif sans label, coût dans Valider, focus défini, carte pastilles/provenance/hold, pouvoir verrouillé sans Click | `crates/hakille-spikes/src/s1_tree.rs` |
| S4-données | `shell_needs_frame()` : Veille + HUD masqué = aucun travail ; animés = frames ; braise fixe = statique | `crates/hakille-spikes/src/s4_quiescence.rs` |
| Perf §11 | Filtre + tri + pagination 100 000 entrées < 2 s (debug) | `crates/hakille-ui-shell/src/views.rs` |
| Tokens | `cargo xtask tokens --check` : TOML source unique → Rust | `tokens/`, `xtask/src/tokens.rs` |
| Contraste | Texte ≈17/16, braise AA deux thèmes | `crates/hakille-design/src/color.rs` |
| Pseudo-loc | Expansion ~40 %, crochets, accents | `crates/hakille-design/src/typo.rs` |

## Reste au labo humain
### S1 — Accessibilité (critère : 3 lecteurs parcourent Carte + Journal)
- **Matériel :** macOS + VoiceOver, Windows + NVDA, Linux + Orca.
- **Procédure :** lancer `hakille-desktop` (`cargo run -p hakille-ui-desktop
  --bin hakille-desktop`), tabuler jusqu'à Valider (label entendu
  avec coût ?), parcourir 5 entrées journal (action + réversibilité
  annoncées ?), activer un pouvoir verrouillé (refus annoncé, pas de clic ?).
- **Sortie :** grille passage/échec par lecteur. **Plan B :** Xilem/Masonry.

### S2 — Redox (critère : surimpression + hotkey ≤ 100 ms sous Orbital)
- **Prouvé en CI (2026) :** `hakille-design`, `hakille-ui-protocol`,
  `hakille-ui`, `hakille-core`, `hakille-memory` (redb inclus) et
  `hakille-ui-shell` (baseline headless, sans renderer) compilent pour
  `x86_64-unknown-redox` — job CI `redox compat`. Le noyau UI est donc
  portable ; restent le renderer (pas de winit/Orbital vérifié) et la mesure.
- **Matériel restant :** station Redox OS (schemes `orbital:`, `chan:`).
- **Procédure :** `hakille-desktop` (ou `hakille-ui-shell` headless pour la
  mesure noyau) via `hakille-substrate-redox`, mesurer
  hotkey → overlay visible (cible p95 ≤ 100 ms).
- **Sortie :** mesure + capture. **Plan B :** shell Rust minimal sur framebuffer.

### S3 — Saisie composée (critère : ZH/JA/KO/AR corrects dans la ligne de commande)
- **Matériel :** Windows (IME MS), macOS (Kotoeri), Linux (IBus/Fcitx5).
- **Procédure :** activer IME CJK puis arabe, taper 20 caractères avec
  pré-édition + validation, vérifier curseur UTF-8 et texte final.
- **Sortie :** texte final exact par langue. **Plan B :** champ natif par plateforme.
- **Déjà couvert en CI :** curseur/édition UTF-8 (CJK, arabe, émojis) dans
  `hakille-ui::EphemeralHud` — voir tests `test_hud_*_utf8`.

### S4 — Événementiel (critère : 0 réveil/s en Veille)
- **Matériel :** laptop batterie, Windows (ETW), macOS (Instruments), Linux (powertop).
- **Procédure :** shell en Veille 60 s, compter réveils/s ; HUD ouvert puis
  évaporé, vérifier retour à 0.
- **Sortie :** traces. **Plan B :** boucle suspendue par l'OS.

### Grille Étape 7 — Linux (ADR-010, projet distinct)

Compilation verte ≠ support : chaque ligne se mesure sur matériel réel ;
un échec documenté vaut mieux qu'un succès supposé.

| Domaine | Critère mesurable | Matériel | Sortie attendue |
| --- | --- | --- | --- |
| Lecteur d'écran | Orca parcourt Carte + Journal : 0 interactif sans label, coût/action/annulation annoncés | station Linux | grille passage/échec |
| IME IBus | 20 caractères ja + ar avec pré-édition, curseur UTF-8 sûr | station Linux | texte final exact |
| IME Fcitx5 | même critère qu'IBus, moteur distinct | station Linux | texte final exact |
| Overlay X11 | HUD 720×64 sans décor + `Alt+Space` → visible | station X11 | capture + trace 1 wakeup |
| Overlay Wayland | idem sous `wlr-layer-shell` (compositeur nommé) | station Wayland | capture ou échec documenté |
| Tray StatusNotifier | icône + menu 5 items ; hôte absent → `NotifierMissing` explicite | bureau GNOME/KDE | capture + trace d'erreur |
| p95 hotkey | p50/p95 sur ≥ 30 appuis, cible p95 ≤ 100 ms | stations X11 + Wayland | journal de mesures |

### Grille Étape 7 — macOS (ADR-010, projet distinct)

| Domaine | Critère mesurable | Matériel | Sortie attendue |
| --- | --- | --- | --- |
| Lecteur d'écran | VoiceOver parcourt Carte + Journal : 0 interactif sans label | Mac Intel + Apple Silicon | grille passage/échec |
| IME | Kotoeri/IMK : pré-édition ja, validation, curseur UTF-8 sûr | Mac | texte final exact |
| Chrome de fenêtre | `NSPanel` : HUD sans décor, topmost ; journal standard | Mac | captures |
| Tray | `NSStatusItem` + menu 5 items | Mac | capture |
| Hotkey CGEventTap | `Cmd+Space` → HUD visible ; permission Accessibilité accordée puis refusée (erreur explicite, jamais de hotkey silencieux) | Mac Intel + Apple Silicon | mesures + trace sur refus |

**Plan B (les deux OS) :** un échec corrige la couche desktop par OS ;
jamais un second renderer (`docs/FREEZE_SLINT.md`). Le support n'est
prononcé qu'une fois toutes les lignes d'un OS vertes (ADR-010).

## Annexe WASM — Tier invité web (vérifié 2026)

`hakille-design` + `hakille-ui-protocol` compilent pour
`wasm32-unknown-unknown` (CI : job à ajouter si la matrice le permet).
Correctifs appliqués : `uuid/js`, `getrandom 0.2/js` (via `hakille-core`),
suppression des `tokio = full` inutilisés (`hakille-core`, `hakille-ui`,
`hakille-audit`, `hakille-memory`).

**Blocage upstream honnête :** `biscuit-auth 5.0.0` (latest) ne compile pas
pour `wasm32-unknown-unknown` (`performance_now`, imports `u128`). Donc tout
ce qui dépend des capabilities (`hakille-core`, graphe, shell) ne passe pas
en WASM aujourd'hui. Voies : (a) correctif upstream biscuit-auth, (b)
abstraction crypto du noyau avec backend WASM, (c — recommandée, conforme
spec §10) : le client web PWA ne porte QUE protocole + design + vues
(ligne de commande + journal lecture) et parle au noyau local via
`hakille-ui-protocol` — jamais besoin du Biscuit côté navigateur.
