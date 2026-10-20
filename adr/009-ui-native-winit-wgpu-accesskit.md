# ADR-009 — Shell desktop natif : winit + wgpu + AccessKit

**Date :** 2026-10-17
**Statut :** Accepté (remplace ADR-007 et ADR-008)
**Contexte :** `docs/PLAN_REFONTE_INTERFACE_2026-10-16.md`, `docs/FREEZE_SLINT.md`

## Décision

Hakille abandonne Slint comme runtime d'interface et construit son shell
desktop en Rust autour de :

| Sous-système | Choix | Version verrouillée |
|---|---|---|
| Fenêtre / événements | `winit` | 0.30 |
| GPU | `wgpu` (fallback logiciel explicite) | 30 |
| Accessibilité | `accesskit` + `accesskit_winit` | 0.24 / 0.33 |
| Scène 2D | trait interne `SceneRenderer` (POC Vello autorisé derrière) | — |
| Texte | trait interne `TextEngine` (POC Parley/cosmic-text autorisé derrière) | — |
| i18n | tables `Strings` FR/EN/AR-RTL/JA (Fluent + ICU4X hors renderer à terme) | — |

Iced **n'est pas** adopté : il reste un prototype de comparaison (spike S6).
Xilem/Vello ne deviennent pas des dépendances imposées : ils ne peuvent
accélérer que derrière nos propres traits. Tauri/Dioxus sont exclus (WebView).

## Architecture résultante

```text
hakille-core / kernel / audit / memory
              │ événements typés, commandes vérifiées
              ▼
       hakille-ui-protocol  (v1, sérialisable, golden testé)
              │
              ▼
hakille-ui + hakille-design + hakille-ui-model   (pur Rust, aucun GPU)
              │ StableSnapshot + ViewModels + UiEvent
              ▼
       hakille-ui-desktop
       ├── runtime   : winit, boucle événementielle, 0 frame en veille
       ├── renderer  : wgpu, scène Hakille, cache texte/glyphes, fallback logiciel
       ├── a11y      : AccessKit, même hiérarchie que l'affichage
       ├── platform  : hakille-ui-platform (tray/hotkeys/overlay/clipboard/window)
       └── test      : renderer tête-less, snapshots pixels + arbre a11y
```

## Règles non négociables

1. Le renderer ne reçoit que des view-models immuables ; jamais de clé,
   réseau, mémoire utilisateur ou capability.
2. Toute interaction produit un message `UiToKernel` typé ; le noyau
   revérifie chaque autorisation (`DenyAllKernel` et `RealDaemonBridge`
   prouvent le fail-closed).
3. Une action sensible a une représentation unique : carte, libellé
   AccessKit, raccourci et trace d'audit dérivent de `UiEvent`.
4. Aucun polling de rendu : fenêtre masquée = 0 frame ; animation ≤ 30 fps
   sur batterie, arrêtée si mouvement réduit.
5. Shaders, icône et animations sont des assets Hakille versionnés.

## Résultats des spikes (2026-10-17, headless outillé)

| Spike | Résultat | Preuve |
|---|---|---|
| S1 overlay/hotkey | structurel vert ; p95 ≤ 100 ms à mesurer sur matériel | `hakille-spikes/s1_tree.rs`, `runtime.rs` (1 wakeup hotkey) |
| S2 rendu 100k | fenêtre visible constante < 30 rangées, composition sous budget | `hakille-spikes/s2_render.rs` |
| S3 accessibilité | 0 interactif sans label, coût/hold/externe annoncés | `hakille-spikes/s3_a11y.rs`, `desktop/src/a11y.rs` |
| S4 i18n/RTL/IME | FR/EN/AR-RTL/JA shapeés, curseur UTF-8 sûr | `desktop/src/text.rs`, `desktop/src/i18n.rs` |
| S5 énergie | 0 frame en veille cachée, 30 fps max batterie | `hakille-spikes/s5_energy.rs` |
| S6 comparaison | grille publique 8 critères ; Iced non adopté sans échec S3/S4 | `hakille-spikes/s6_compare.rs` |

Restant labo (gates non-négociables) : NVDA/VoiceOver/Orca (S3),
IME japonais réel (S4), p95 GPU et RAM sur runner dédié (S2/S5),
packaging MSIX/WiX.

## Conséquences

- `hakille-ui`, `hakille-design`, `hakille-ui-protocol` restent le cœur
  indépendant du renderer ; `hakille-ui-model` extrait la frontière stable.
- Le renderer Slint est gelé (Étape 0) puis retiré après parité (Étape 6).
- Windows 11 est l'unique cible de la première migration ; Linux/macOS
  sont des projets de compatibilité distincts.
- La CI gagne un job `ui-desktop-quality` (a11y, snapshots, perf,
  dégradation GPU), progressivement bloquant.
