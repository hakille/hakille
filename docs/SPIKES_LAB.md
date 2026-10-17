# Spikes Phase 0 — protocole labo (spec Hakille §10)

> Ce qui est prouvé en CI (outillé) vs ce qui exige du matériel humain.
> Gate : aucun engagement iced/Redox/IME/événementiel avant sortie de spike.

## Déjà prouvé en CI (`annora-spikes`, `annora-ui-shell`)

| Spike | Preuve automatisée | Fichier |
| --- | --- | --- |
| S1-données | Arbre AccessKit réel depuis VMs : 0 interactif sans label, coût dans Valider, focus défini, carte pastilles/provenance/hold, pouvoir verrouillé sans Click | `crates/annora-spikes/src/s1_tree.rs` |
| S4-données | `shell_needs_frame()` : Veille + HUD masqué = aucun travail ; animés = frames ; braise fixe = statique | `crates/annora-spikes/src/s4_quiescence.rs` |
| Perf §11 | Filtre + tri + pagination 100 000 entrées < 2 s (debug) | `crates/annora-ui-shell/src/views.rs` |
| Tokens | `cargo xtask tokens --check` : TOML source unique → Rust | `tokens/`, `xtask/src/tokens.rs` |
| Contraste | Texte ≈17/16, braise AA deux thèmes | `crates/annora-design/src/color.rs` |
| Pseudo-loc | Expansion ~40 %, crochets, accents | `crates/annora-design/src/typo.rs` |

## Reste au labo humain
### S1 — Accessibilité (critère : 3 lecteurs parcourent Carte + Journal)
- **Matériel :** macOS + VoiceOver, Windows + NVDA, Linux + Orca.
- **Procédure :** lancer `shell_demo`, tabuler jusqu'à Valider (label entendu
  avec coût ?), parcourir 5 entrées journal (action + réversibilité
  annoncées ?), activer un pouvoir verrouillé (refus annoncé, pas de clic ?).
- **Sortie :** grille passage/échec par lecteur. **Plan B :** Xilem/Masonry.

### S2 — Redox (critère : surimpression + hotkey ≤ 100 ms sous Orbital)
- **Prouvé en CI (2026) :** `annora-design`, `annora-ui-protocol`,
  `annora-ui`, `annora-core`, `annora-memory` (redb inclus) et
  `annora-ui-shell` (sans `render`) compilent pour
  `x86_64-unknown-redox` — job CI `redox compat`. Le noyau UI est donc
  portable ; restent le renderer (pas de winit/Orbital vérifié) et la mesure.
- **Matériel restant :** station Redox OS (schemes `orbital:`, `chan:`).
- **Procédure :** `annora-ui-shell` via `annora-substrate-redox`, mesurer
  hotkey → overlay visible (cible p95 ≤ 100 ms).
- **Sortie :** mesure + capture. **Plan B :** shell Rust minimal sur framebuffer.

### S3 — Saisie composée (critère : ZH/JA/KO/AR corrects dans la ligne de commande)
- **Matériel :** Windows (IME MS), macOS (Kotoeri), Linux (IBus/Fcitx5).
- **Procédure :** activer IME CJK puis arabe, taper 20 caractères avec
  pré-édition + validation, vérifier curseur UTF-8 et texte final.
- **Sortie :** texte final exact par langue. **Plan B :** champ natif par plateforme.
- **Déjà couvert en CI :** curseur/édition UTF-8 (CJK, arabe, émojis) dans
  `annora-ui::EphemeralHud` — voir tests `test_hud_*_utf8`.

### S4 — Événementiel (critère : 0 réveil/s en Veille)
- **Matériel :** laptop batterie, Windows (ETW), macOS (Instruments), Linux (powertop).
- **Procédure :** shell en Veille 60 s, compter réveils/s ; HUD ouvert puis
  évaporé, vérifier retour à 0.
- **Sortie :** traces. **Plan B :** boucle suspendue par l'OS.

## Annexe WASM — Tier invité web (vérifié 2026)

`annora-design` + `annora-ui-protocol` compilent pour
`wasm32-unknown-unknown` (CI : job à ajouter si la matrice le permet).
Correctifs appliqués : `uuid/js`, `getrandom 0.2/js` (via `annora-core`),
suppression des `tokio = full` inutilisés (`annora-core`, `annora-ui`,
`annora-audit`, `annora-memory`).

**Blocage upstream honnête :** `biscuit-auth 5.0.0` (latest) ne compile pas
pour `wasm32-unknown-unknown` (`performance_now`, imports `u128`). Donc tout
ce qui dépend des capabilities (`annora-core`, graphe, shell) ne passe pas
en WASM aujourd'hui. Voies : (a) correctif upstream biscuit-auth, (b)
abstraction crypto du noyau avec backend WASM, (c — recommandée, conforme
spec §10) : le client web PWA ne porte QUE protocole + design + vues
(ligne de commande + journal lecture) et parle au noyau local via
`annora-ui-protocol` — jamais besoin du Biscuit côté navigateur.
