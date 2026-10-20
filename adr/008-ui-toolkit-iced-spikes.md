# ADR 008: Toolkit d'interface — iced cible (spec Hakille §10), Slint conservé, Xilem plan B

## Statut
Remplacé par ADR-009 (2026-10-17) — winit + wgpu + AccessKit retenus,
Iced reste un prototype de comparaison (spike S6). Conservé pour l'historique.
Accepté (2026-10-12) — migration conditionnée aux spikes, pas au calendrier.

## Contexte
La spec Hakille §10 tranche : bureau/web = **iced + wgpu** (Elm,
AccessKit, cosmic-text, Fluent + ICU4X), mobile = SwiftUI/Compose + UniFFI,
protocole `hakille-ui-protocol`. Écartés : Slint (à trancher juridiquement), Tauri (vue web, anti tout-Rust),
Qt Bridge (bêta, Redox à valider), Flutter (Dart). Xilem = plan B suivi et
contribué.

L'ADR-007 avait choisi Slint (natif, <300 KiB, Redox viable, tray 1.17).
Les deux visions partagent l'essentiel : headless-first, pas de WebView,
tokens uniques, journal = produit. Le désaccord porte sur UN point :
le moteur de rendu bureau.

## Décision
1. **Cible : iced.** Tout le nouveau code UI est toolkit-agnostique
   (`hakille-ui` machines d'état, `hakille-design` tokens/composants,
   `hakille-ui-protocol` messages, `hakille-ui-shell` VMs) : il alimentera
   iced sans réécriture, comme la spec l'exige (« l'interface en est une
   projection »).
2. **Gate par spikes, pas par date** (spec §10, Phase 0) :
   - S1 accessibilité : iced + AccessKit sur macOS/Windows/Linux (3 lecteurs
     parcourent Carte d'action et Journal) — sinon Xilem/Masonry ;
   - S2 Redox : surimpression + raccourci global sous Orbital ≤ 100 ms —
     sinon shell Rust minimal sur framebuffer ;
   - S3 saisie composée : IME ZH/JA/KO/AR dans la ligne de commande —
     sinon champ natif par plateforme ;
   - S4 événementiel : 0 réveil/s en Veille — sinon boucle suspendue par l'OS.
3. **En attendant : Slint conservé** comme renderer de travail (il compile,
   il tourne, `shell_demo` le prouve). Aucune logique métier n'y est
   enfermée : le jour du basculement iced, seul `ui/hakille.slint` +
   `render.rs` sont remplacés.

## Conséquences
- Positif : cap iced sans parier la Phase 0 dessus ; spikes mesurables ;
  iced apporte AccessKit/cosmic-text/Fluent prêts pour §12.
- Négatif : deux renderers cohabitent transitoirement ; le portage iced
  (fenêtre, tray, overlay, IME) reste à chiffrer après S1–S4.
