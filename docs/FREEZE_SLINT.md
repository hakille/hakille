# Gel du renderer Slint — Étape 0 → Étape 6 (appliquée)

**Date :** 2026-10-17
**Statut :** gel levé — renderer Slint retiré (Étape 6 exécutée, ADR-009)
**Références :** `docs/PLAN_REFONTE_INTERFACE_2026-10-16.md`, `adr/009-ui-native-winit-wgpu-accesskit.md`

## Historique du gel

Le DSL `.slint` a été gelé à l'Étape 0 : plus aucune fonctionnalité,
seuls les correctifs de sécurité ou de build. Le 2026-10-17, par décision
explicite, l'Étape 6 a été exécutée : le renderer Slint a été retiré
(dépendances, build script, feature `render`, DSL, démos, callbacks simulés,
étape CI) sans attendre les gates labo S3/S4, à la demande du projet.

## Baseline conservée (filet de sécurité)

| Élément | Fichier | Rôle |
|---|---|---|
| Snapshot headless | `crates/hakille-ui-shell/src/bridge.rs` | frontière de données, parité testée |
| View-models | `crates/hakille-ui-shell/src/views.rs` | journal, mémoire, pouvoirs, réglages |
| Actions headless | `crates/hakille-ui-shell/src/actions.rs` | mapping honnête → noyau ou `DaemonError` |
| Specs plateforme | `crates/hakille-ui-shell/src/{overlay,shell_config}.rs` | géométrie HUD, wiring par OS |
| Arbre AccessKit v1 | `crates/hakille-spikes/src/s1_tree.rs` | accessibilité prouvée |
| Protocole v1 | `crates/hakille-ui-protocol/src/lib.rs` | contrat versionné |
| Tokens | `tokens/*.toml` → `hakille-design/src/generated.rs` | identité visuelle |

Ces éléments ne sont **jamais** supprimés avant que le nouveau shell ne les
remplace fonctionnellement (parité testée dans `crates/hakille-ui-model` et
`crates/hakille-ui-desktop`, e2e dans `conformance::desktop_shell`).

## Ce qui a été supprimé (Étape 6)

- dépendances `slint` et `slint-build`, build script, feature `render` ;
- `crates/hakille-ui-shell/ui/*.slint` (DSL), `src/render.rs`, `src/hotkey.rs`,
  `src/tray.rs`, binaires `shell-demo` et `gallery` ;
- démo Web WASM (`crates/hakille-ui-shell/web/`, commande `xtask shell-web`) ;
- étape CI `render (windows only)` et mentions actives dans README/guide/daemon.

Vérification : `rg -i slint` ne retourne plus aucune dépendance, cible de
build ni mention active ; seuls les documents historiques datés (plan,
audit, spécification, ADR-007/008 marqués « Remplacé par ADR-009 ») en
gardent trace.

## Parité prouvée avant retrait

- `StableSnapshot` ≡ `ShellSnapshot` (test `parity_with_shell_snapshot_all_fields_and_states`).
- Arbre AccessKit du nouveau shell ≡ S1 (0 interactif sans label).
- Parcours « demande → confirmation → journal → annulation » vert dans
  `conformance::desktop_shell`.
- Spikes S1–S6 verts en headless outillé ; gates labo (NVDA/VoiceOver/Orca,
  IME japonais réel) restent à dérouler sur matériel — un échec corrige la
  couche desktop, jamais un second renderer.
