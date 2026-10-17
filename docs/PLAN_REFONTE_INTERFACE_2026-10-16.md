# Plan de refonte de l'interface Annora — fondation native, durable et futuriste

**Décision proposée :** abandonner Slint comme runtime d'interface, mais ne pas le remplacer mécaniquement par Iced. Construire un shell desktop Annora en Rust autour de **winit + wgpu + AccessKit**, avec une couche de scène/typographie interne et des composants Annora. Évaluer Vello/Parley derrière cette couche, sans rendre le produit dépendant de leurs API expérimentales. Garder `annora-ui`, `annora-design` et `annora-ui-protocol` comme cœur indépendant du renderer.

Cette voie maximise le contrôle visuel, les performances, l'accessibilité testable et la longévité. Elle coûte plus de travail qu'un toolkit complet, mais l'interface est le produit d'Annora : ce coût est justifié pour les quelques surfaces réellement nécessaires.

## 1. Verdict sur l'état actuel

### Ce qui doit survivre

| Élément actuel | Décision | Raison |
|---|---|---|
| `annora-ui` | Conserver | Machines d'état pures : présence, HUD, silence, canal, onboarding, accessibilité |
| `annora-design` et tokens TOML | Conserver | Source unique de l'identité visuelle et des constantes multi-plateformes |
| `annora-ui-protocol` | Conserver et étendre | Contrat versionné UI ↔ noyau ; aucun secret ni décision dans le renderer |
| `annora-ui-shell/src/bridge.rs` | Conserver après renommage | `ShellSnapshot` est la bonne frontière de données headless |
| `annora-spikes/src/s1_tree.rs` | Promouvoir | Arbre AccessKit déjà construit depuis les view-models |
| tray, hotkeys, placement d'overlay | Extraire | Ce sont des adaptateurs OS, pas des concepts Slint |

### Ce qui doit disparaître

Le renderer Slint représente environ 1 000 lignes de DSL et près de 450 lignes de glue de rendu, plus build script, feature `render`, démos et documentation. Il contient des callbacks de démonstration qui tracent des intentions mais ne les connectent pas au noyau. C'est acceptable comme prototype ; ce n'est pas une architecture d'interface de production.

À supprimer après remplacement fonctionnel :

- dépendances `slint` et `slint-build`, build script et feature `render` ;
- `ui/*.slint`, `render.rs`, la démo Web Slint et les mentions de Slint ;
- les callbacks simulés ;
- les tests qui ne vérifient que la projection Slint.

Ne jamais supprimer d'abord `ShellSnapshot`, le protocole, les tests de view-model ou l'arbre d'accessibilité. C'est le filet de sécurité de la migration.

## 2. Pourquoi ne pas adopter Iced directement

Iced reste une option intéressante : Rust natif, modèle Elm, Windows/macOS/Linux/Web et widgets usuels. Il correspond bien à des écrans de réglages et à la projection d'un état. Toutefois, Annora ne doit pas choisir une fondation en fonction de l'esthétique d'une démo : les critères durs sont l'accessibilité, le texte international, les overlays sans polling, les raccourcis globaux et le comportement multi-fenêtre sur les OS réellement supportés.

Le projet ne possède pas encore la preuve que ces exigences sont satisfaites par Iced. Son support AccessKit, la saisie IME et les particularités Wayland doivent donc être évalués, pas présumés. Iced peut rester un **prototype de comparaison** ; il ne doit pas être une dépendance centrale avant les résultats des spikes ci-dessous.

Xilem/Masonry/Vello est attrayant pour une interface expressive et réactive, mais encore trop jeune pour devenir une dépendance imposée au produit. Il peut accélérer le renderer seulement derrière nos propres traits. Egui est excellent pour un outil développeur, pas pour l'identité produit ; Tauri/Dioxus introduisent une WebView, contraire à l'objectif de shell natif et de faible consommation.

## 3. Architecture cible

```text
annora-core / kernel / audit / memory
              │ événements typés, commandes vérifiées
              ▼
       annora-ui-protocol  (versionné, sérialisable)
              │
              ▼
annora-ui + annora-design  (pur Rust, aucun GPU, testable)
              │ ShellSnapshot + ViewModels + AccessKit tree
              ▼
       annora-ui-desktop
       ├── runtime : winit + event loop pilotée par événements
       ├── renderer : wgpu, scène Annora, cache de texte et de glyphes
       ├── a11y : AccessKit, même hiérarchie que l'affichage
       ├── platform : window / tray / hotkeys / overlay / clipboard
       └── test : renderer tête-less, snapshots pixels + arbre a11y
```

### Règles non négociables

1. Le renderer ne reçoit que des **view-models immuables**. Il n'accède ni aux clés, ni au réseau, ni à la mémoire utilisateur brute, ni aux capabilities.
2. Toute interaction produit un message `UiToKernel` typé. Le noyau valide à nouveau chaque autorisation ; aucun bouton ne produit un effet système directement.
3. Une action sensible a une représentation unique : carte visuelle, libellé AccessKit, raccourci clavier et trace d'audit dérivent de la même donnée.
4. Aucun polling de rendu : fenêtre masquée = aucun frame ; animation = planifiée à 30 fps maximum sur batterie et arrêtée si mouvement réduit.
5. Les shaders, l'icône et les animations sont des assets Annora versionnés, pas des comportements implicites d'un toolkit.

## 4. Direction visuelle : « calme, matériel, explicable »

Le futurisme d'Annora ne doit pas être néon, verre permanent ou animation gratuite. Le langage visuel proposé :

- fond très sombre chaud, surfaces opaques et profondeur légère ; le flou n'est employé que pour un overlay court ;
- une seule couleur d'attention réservée aux confirmations et erreurs actionnables ; jamais pour décorer ;
- présence persistante minimaliste : point/anneau dont la forme, le rythme et le texte expliquent l'état ;
- HUD compact au raccourci, transformable en « espace de décision » plutôt qu'en fenêtre de chat ;
- journal chronologique comme surface principale : action, pourquoi, portée de données, coût, réversibilité, annulation ;
- cartes d'action sans ambiguïté : un verbe, un objet, l'origine, la conséquence, puis Valider / Modifier / Refuser ;
- mouvement utilitaire : 120–220 ms, spring discret, aucune animation infinie hors présence ;
- thème clair/sombre réel, contraste élevé, densité confortable/compacte et texte jusqu'à 200 % dès la première version.

L'esthétique sera cohérente si les tokens restent la source unique et si chaque nouveau composant doit exister en cinq états : normal, focus clavier, chargement, erreur et mouvement réduit/contraste élevé.

## 5. Choix technique précis

| Sous-système | Choix | Justification |
|---|---|---|
| Fenêtre et événements | `winit` | Contrôle natif de la boucle, DPI, surface GPU, fenêtres et événements OS |
| GPU | `wgpu` | Backends natifs DX12, Metal, Vulkan et solution WebGPU/WebGL pour une cible Web ultérieure |
| Scène 2D | Trait `SceneRenderer` Annora ; POC Vello autorisé | Évite de lier les concepts métier à une bibliothèque graphique changeante |
| Texte | Trait `TextEngine`; POC Parley/cosmic-text | Internationalisation, fallback de polices, shaping et cache doivent être mesurés |
| Accessibilité | AccessKit, intégré dès le premier écran | Le renderer et l'arbre assistif doivent être testés ensemble |
| État et navigation | state machines existantes + messages Elm-like | Réduction des états impossibles et tests purs |
| i18n | Fluent + ICU4X, hors renderer | Traductions, pluriels et formats sans chaînes UI codées en dur |
| Desktop OS | adaptateurs par plateforme | Hotkeys, tray, overlay, clipboard et permissions ne sont pas portables par magie |
| Mobile | SwiftUI/Compose via protocole/UniFFI, plus tard | Une interface mobile native ne doit pas être une copie réduite du shell desktop |
| Web | client de consultation limité, plus tard | Ne pas promettre aujourd'hui l'équivalence d'un système local avec un navigateur |

**Important :** aucun des composants de rendu n'est un moteur de décision. `wgpu` sert l'interface, jamais le traitement des modèles ; les budgets GPU UI et IA sont isolés.

## 6. Spikes de décision — quatre semaines, aucune migration massive avant preuve

| Spike | But | Mesure de sortie | Décision |
|---|---|---|---|
| S1 : overlay | HUD transparent, hotkey, focus, multi-DPI Windows 11 | p95 hotkey → visible ≤ 100 ms ; 0 frame en veille | valide `winit` + platform layer |
| S2 : rendu | Présence, carte, journal virtualisé à 100 000 lignes | ≥ 120 fps défilement sur machine de référence, frame p95 ≤ 8 ms | valide scène/texte/cache |
| S3 : accessibilité | Carte + journal au clavier et lecteur d'écran | NVDA Windows, VoiceOver macOS, Orca Linux passent les mêmes parcours | gate bloquant, pas une démo |
| S4 : international | Français, anglais, arabe RTL, japonais IME | aucun caractère coupé, focus cohérent, saisie/annulation correctes | valide texte et i18n |
| S5 : énergie | fenêtre cachée et présence | 0 réveil périodique ; mesures CPU/GPU/batterie archivées | valide boucle événementielle |
| S6 : comparaison | même mini-shell en Iced et Vello/Xilem | grille publique : a11y, IME, RAM, p95, packaging | confirme ou infirme le renderer interne |

Les spikes produisent un rapport, des enregistrements de test, des métriques et un verdict. Aucun score esthétique ne compense un échec S3 ou S4.

## 7. Plan de migration en sept étapes

### Étape 0 — geler l'ancien renderer

Ne plus ajouter de fonctionnalité dans `.slint`. Corriger uniquement les défauts de sécurité ou build. Définir une baseline de captures, de `ShellSnapshot`, d'arbre AccessKit et des parcours d'acceptation actuels.

### Étape 1 — extraire les frontières stables

- Renommer conceptuellement `annora-ui-shell` en trois modules : `annora-ui-model`, `annora-ui-platform`, `annora-ui-desktop`.
- Déplacer `ShellSnapshot`, `JournalRow`, `MemoryRow`, `PowersView`, préférences et view-models hors de toute référence Slint.
- Remplacer les chaînes de callbacks de démo par des événements `UiToKernel` exhaustifs, testés et versionnés.
- Définir un trait `DesktopHost` pour fenêtre, tray, hotkey, overlay et notifications.

**Sortie :** tous les tests de modèles UI passent sans compiler un renderer.

### Étape 2 — construire le runtime desktop minimal

- Créer `annora-ui-desktop` avec fenêtre winit, surface wgpu, gestion DPI, boucle événementielle et fermeture propre.
- Afficher uniquement la présence et le HUD ; connecter le hotkey et l'overlay Windows.
- Ajouter un backend de rendu logiciel ou une erreur explicite pour GPU indisponible ; ne jamais afficher une fenêtre vide silencieusement.

**Sortie :** S1 et S5 réussis sur Windows 11.

### Étape 3 — bibliothèque de composants Annora

Construire 12 primitives, et pas davantage : `Surface`, `Text`, `Icon`, `Button`, `Toggle`, `FocusRing`, `List`, `TextInput`, `ActionCard`, `Pastille`, `Presence`, `Sheet`. Elles utilisent tokens, thèmes, scale de texte, états d'erreur et réduction de mouvement.

Ajouter la virtualisation au niveau `List`, pas dans `JournalScreen`. Les composants métier sont des compositions de primitives, jamais des widgets génériques imposés par un framework.

**Sortie :** S2 réussi, captures de référence stables, pas de fuite GPU détectée lors d'un test de 30 minutes.

### Étape 4 — accessibilité et texte avant les écrans secondaires

- Générer l'arbre AccessKit dans le même passage que la scène.
- Relier focus, activation, valeurs texte, annonces et sélection à `UiToKernel`.
- Faire les tests NVDA/VoiceOver/Orca et les tests IME/RTL avant de développer les animations avancées.

**Sortie :** S3 et S4 réussis. En cas d'échec, corriger la couche desktop ; ne pas la contourner avec un second renderer.

### Étape 5 — connecter les vrais flux noyau

Brancher dans cet ordre : onboarding → présence/HUD → carte de décision → journal → mémoire → pouvoirs/réglages → outbox. Chaque flux garde son test de refus d'autorisation côté noyau. Les données de démonstration de `render.rs` disparaissent.

**Sortie :** parcours complet « demande → simulation → confirmation → journal → annulation » sans stub UI.

### Étape 6 — retirer Slint proprement

Seulement après parité des parcours et succès des spikes : supprimer DSL, build script, dépendances, feature, démos et références dans README/daemon/ADR/CI. Mettre à jour les tests vers le nouveau renderer tête-less et ajouter les captures GPU au CI Windows.

**Sortie :** `rg -i slint` ne retourne aucune dépendance ou mention active ; `cargo test --workspace --locked` et CI passent.

### Étape 7 — élargir prudemment

Linux puis macOS sont des projets de compatibilité distincts. Ajouter un OS uniquement après : hotkey, window chrome, tray, lecteurs d'écran, IME, packaging et tests matériels. Mobile et Web ne sont pas des ports automatiques ; ils consomment le même protocole et design system, avec leur UI native adaptée.

## 8. CI et qualité de rendu

Ajouter ces jobs, progressivement bloquants :

1. test pur des view-models, messages et règles de sécurité ;
2. test AccessKit : tout contrôle interactif a un nom, un rôle et une action ;
3. snapshots du graphe de scène et golden images à taille/DPI/thème déterministes ;
4. tests de performance sur runner Windows réservé, avec p50/p95, RAM et wakeups ;
5. tests e2e clavier et lecteur d'écran sur matériel réel ;
6. test de dégradation GPU : adapter absent, reset de surface, DPI et changement d'écran ;
7. audit de dépendances et mise à jour contrôlée de `wgpu`/texte/rendering.

Les métriques de sortie : démarrage à froid ≤ 800 ms, hotkey → HUD p95 ≤ 100 ms, 0 frame en veille, 120 i/s de défilement, mémoire UI ≤ 80 Mo hors modèles, zéro contrôle inaccessible au clavier, WCAG 2.2 AA minimum.

## 9. Décisions à prendre maintenant

1. Valider **Windows 11 desktop comme unique cible d'interface de la première migration**.
2. Valider `winit + wgpu + AccessKit` comme fondation et `SceneRenderer`/`TextEngine` comme frontières internes.
3. Interdire toute nouvelle fonctionnalité Slint ; lancer S1–S6.
4. Affecter un responsable UI système, un responsable accessibilité et un responsable performance ; ces trois responsabilités ne peuvent pas être absorbées par le noyau IA.
5. Après les spikes, faire une ADR finale qui remplace ADR-007 et ADR-008, avec résultats et versions effectivement retenues.

## 10. Risques et garde-fous

| Risque | Garde-fou |
|---|---|
| Construire un toolkit sans fin | Limiter les primitives, livrer d'abord le HUD/journal/carte |
| Dépendances graphiques mouvantes | Traits Annora stables, versions verrouillées, spike avant adoption |
| Accessibilité décorative | S3 est bloquant et testé par lecteurs réels |
| Interface spectaculaire mais lente | Budget de frame, d'énergie et de mémoire dans CI |
| Surface multiplateforme ingérable | Windows-only jusqu'à preuve par OS |
| Renderer qui contourne la sécurité | Protocole unique, noyau seul détenteur des effets et des capabilities |

## Conclusion

Annora ne doit pas choisir un toolkit « beau ». Elle doit posséder un système d'interface où la beauté résulte d'états cohérents, de typographie de qualité, de mouvement sobre, d'un rendu fluide et de décisions compréhensibles. La bonne refonte est donc une migration vers un shell Rust natif contrôlé, avec un socle GPU et accessibilité stable, et non un simple échange de `.slint` contre des widgets Iced.

