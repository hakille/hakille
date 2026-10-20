# ADR-010 — Compatibilité Linux/macOS : projets distincts, support prouvé par OS

**Date :** 2026-10-17
**Statut :** Accepté
**Contexte :** `docs/PLAN_REFONTE_INTERFACE_2026-10-16.md` §7 (Étape 7) et §10, `adr/009-ui-native-winit-wgpu-accesskit.md`, `docs/SPIKES_LAB.md`, `docs/FREEZE_SLINT.md`, `docs/PLAN_REFONTE_SUIVI.md`

## Contexte

ADR-009 retient Windows 11 comme unique cible de la première migration et
qualifie Linux/macOS de « projets de compatibilité distincts ». Le plan
(§7, §10) liste ce qui n'est pas portable par magie : hotkey global, chrome
de fenêtre, tray, lecteurs d'écran, IME, packaging et tests matériels. Le
gel Slint est levé et le shell `hakille-ui-desktop` remplace l'ancien
renderer ; toutefois les gates labo Windows (NVDA, IME japonais réel, p95
GPU/RAM, MSIX/WiX) restent ouverts. Élargir sans méthode rejouerait ces
gates en double et transformerait une compilation verte en promesse de
support non tenue.

## Décision

Linux puis macOS sont traités comme **deux projets de compatibilité
distincts et séquencés**, jamais comme une extension automatique du shell
Windows. Un OS n'est déclaré « supporté » qu'après preuve labo par OS.
Les axes de preuve, par OS :

| Axe | Linux | macOS |
|---|---|---|
| Hotkey global | X11 / Wayland (`wlr-layer-shell`) | `CGEventTap` + permission Accessibilité |
| Chrome de fenêtre | X11 et Wayland, HUD sans décor | `NSPanel` |
| Tray | StatusNotifier ; hôte absent → erreur explicite | `NSStatusItem` |
| Lecteur d'écran | Orca | VoiceOver |
| IME | IBus / Fcitx5 | Kotoeri (IMK) |
| Packaging | format à trancher (deb/AppImage ou équivalent) | DMG et signature à trancher |
| Tests matériels | station X11 + station Wayland | Mac Intel et Apple Silicon, permissions accordées puis refusées |

**Phase 1 — préparation (en cours au 2026-10-17)**, sans engager le
support :

- compilation cross-target : Linux est déjà couverte par la CI
  (`ubuntu-latest`, `cargo check --workspace --all-targets --locked`) ;
  le cross-check de crates pures pour `x86_64-unknown-redox` est le
  précédent ; macOS est à couvrir par un job dédié ;
- chemins OS réels sous `cfg`, jamais de stub silencieux :
  `PlatformKind::auto` (`crates/hakille-ui-platform/src/window.rs`),
  `HotkeySpec::default_for` (`Cmd+Space` sur macOS),
  `TraySpec` pure (`tray.rs`) ;
- parseurs testables host-indépendants, déjà couverts par `conformance`
  (`SeccompProfile::simulate_bpf`, `MacosSandboxProfile::validate_safety`,
  `EndpointSecurityClient`) ;
- CI non bloquante : `ui-desktop-quality` est `continue-on-error` ; **la
  CI macOS reste `continue-on-error` tant que le labo n'a pas validé
  l'OS** — une compilation verte n'est pas une preuve de support.

**Phase 2 — mesures labo** : dérouler les grilles par OS de
`docs/SPIKES_LAB.md` (Orca, IBus/Fcitx5, X11 vs `wlr-layer-shell`,
StatusNotifier, p95 hotkey ; VoiceOver, Kotoeri/IMK, `NSPanel`,
`NSStatusItem`, `CGEventTap` + permissions) et statuer OS par OS.

## Non-goals

- Mobile (SwiftUI/Compose via UniFFI) et Web (client de consultation) ne
  sont **pas** des ports automatiques du shell : ils consomment le même
  protocole et le même design system, avec une UI native adaptée (plan §5).
- Aucun second renderer : un échec de labo corrige la couche desktop
  existante (`docs/FREEZE_SLINT.md`).

## Alternatives

| Alternative | Pourquoi écartée |
|---|---|
| Portage multi-OS monolithique | surface ingérable ; gates labo rejouées en parallèle et non attribuables (plan §10) |
| « Supporté dès que la CI compile » | compilation ≠ hotkey, chrome, tray, IME, lecteurs d'écran, packaging ; faux succès |
| Support via toolkit externe (Tauri/WebView) | WebView et multi-langage, contraires ADR-001 et ADR-009 |
| Mobile/Web comme ports automatiques | UI native adaptée requise ; seul le protocole est partagé |

## Conséquences

- Un OS ne peut pas devenir « supporté » par accident : la décision est
  datée, par OS, adossée aux grilles de `SPIKES_LAB.md`.
- La phase 1 n'engage rien et reste non bloquante ; macOS en CI ne bloque
  pas le développement tant que le labo n'a pas validé.
- Coût : matériel et campagnes labo par OS (stations X11/Wayland, Mac
  Intel et Apple Silicon), packaging par OS, gestion des permissions
  (`CGEventTap`, Accessibilité).
- Windows reste la cible de référence : l'Étape 7 ne modifie pas ses
  gates (NVDA, IME réel, p95 GPU/RAM, MSIX/WiX).
