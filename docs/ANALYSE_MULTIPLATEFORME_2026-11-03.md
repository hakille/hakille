# Analyse technique multiplateforme — 3 novembre 2026

> Ce document conserve l'analyse de l'état initial et du premier lot. Le
> [deuxième lot](ANALYSE_APPROFONDIE_2026-11-03.md) corrige ensuite les faux
> effets audio/capture/injection, la persistance de l'outbox, le helper de clés
> chiffrées et les fonctions métier calendrier/code. Lire les deux états
> séparément : les limites initiales ne décrivent pas toutes le code final.

## Périmètre et niveau de preuve

Cette analyse porte sur le code du dépôt `hakille/hakille`, base Git
`bbabc352b609e66f42561068d76bf85f1706ece6`, et distingue l'état initial des
corrections développées dans la même session. Elle couvre le cœur Rust, le
runtime d'agents, l'interface, les six substrats et la chaîne de validation.
Il s'agit d'une revue de code et d'architecture : les déclarations de CI ne
prouvent pas qu'un workflow distant a réussi, et les tests exécutés sous Linux
ne prouvent pas le fonctionnement matériel de Windows, macOS ou d'un navigateur.
Les résultats d'intégration du lot courant doivent être lus dans le compte rendu
de validation associé, et non déduits de cette revue.

Le produit est un prototype substantiel : crypto, mémoire, protocole UI,
exécution WASM de référence et plusieurs chemins natifs existent. Son principal
risque est l'écart entre une interface Rust disponible et un effet réellement
réalisé par le système hôte. Déclarer « toutes les fonctionnalités sur toutes les
plateformes » serait actuellement inexact.

## 1. Architecture et frontières utiles

| Couche | Code principal | Fonction réelle et point d'attention |
| --- | --- | --- |
| Confiance | `hakille-core`, `hakille-constitution`, `hakille-auth` | Primitives Ed25519/Biscuit, atténuation, règles et consentements. La vivacité vocale reste heuristique ; elle ne constitue pas une authentification forte autonome. |
| Mémoire | `hakille-memory` | Graphe SPO, index, stockage redb, snapshots, requêtes et recherche vectorielle. Une persistance locale vérifiée n'établit pas une réplication distribuée opérationnelle. |
| Synchronisation | `hakille-sync` | Horloges, fusion CRDT, enveloppes de réplication, handoff et outbox. Le transport entre appareils, son authentification et l'exploitation d'un cluster doivent être prouvés séparément. |
| Inférence | `hakille-inference`, `hakille-perception`, `hakille-voice` | Chargement de modèles, Qwen/Candle, ONNX et traitement audio. La présence d'un moteur n'établit pas celle du modèle, du périphérique ni du binding hôte. |
| Agents | `hakille-agents`, `agents/*`, `hakille-daemon/src/agent_runtime.rs` | Exécution réelle de deux agents WASM de référence ; fonctions natives des cinq agents métier et manifests chargés, sans chaîne d'exécution métier WASM complète. |
| Interface | `hakille-ui-protocol`, `hakille-ui-model`, `hakille-ui-desktop` | Frontière typée noyau/UI, machine à états, rendu et accessibilité. Conserver les contrôles d'autorisation et d'audit côté noyau. |
| Plateformes | `hakille-substrate` et six implémentations | Contrat commun utile, mais les garanties des méthodes ne sont pas encore homogènes. |

La séparation entre le modèle UI pur et les effets exécutés par le démon est
une bonne base pour les tests multiplateformes. Les correctifs dans les couches
partagées ont un rendement élevé : ils améliorent plusieurs hôtes sans multiplier
les implémentations. En revanche, copier un adaptateur factice ne réalise pas un
portage natif.

## 2. Matrice des plateformes à l'état initial

« Présent » signifie ici visible dans le code ; « natif » exige encore la
validation sur l'OS et les périphériques concernés.

| Plateforme | Éléments présents | Limites constatées | Preuve nécessaire avant support annoncé |
| --- | --- | --- | --- |
| Windows | Capture GDI, injection `SendInput`, caméra MediaFoundation sous `cfg(windows)`, pipe local, intégration desktop/hotkey et fond transparent spécifique Windows. | `Substrate::start_audio_capture` produit un identifiant sans démarrer CPAL ; `play_audio` accepte sans jouer. `spawn_sandboxed` ne lance initialement aucun WASM. | Capture/caméra autorisées et refusées, périphérique retiré, redémarrage du démon, NVDA, IME, DPI, installation signée et mises à jour. |
| Linux | Lecture `/proc`/`/sys`, analyse cgroups, profils Seccomp, injection `/dev/uinput`, socket IPC, primitives desktop. | Capture écran synthétique dans `display.rs` ; caméra V4L2 refusée explicitement ; profils Seccomp ne prouvent pas une isolation appliquée aux agents. | Sessions X11 et Wayland séparées, portail de capture et consentement, uinput refusé/autorisé, Orca, D-Bus/tray et persistance du trousseau. |
| macOS | Requêtes hôte dans `host_query.rs`/`mach_iokit.rs`, profils Seatbelt, modèles de fenêtre/hotkey, Keychain. | Capture synthétique explicitement commentée ; `inject_input` retourne initialement `Ok(())` sans injection ; caméra AVFoundation absente. | Machine Intel et Apple Silicon, autorisations Accessibilité/Screen Recording/caméra, VoiceOver, IME et distribution signée. |
| Redox | Schémas/paquets et règles de capabilities modélisés, types du cœur compilables sur certaines cibles. | Capture synthétique, injection sans effet, caméra absente, alimentation par défaut ; runner WASM non câblé dans le substrat. | Boot réel/VM Redox, I/O via schemes, erreurs d'accès, exécution isolée et reprise de données. |
| Android/iOS | Stockage cloisonné modélisé, shadow sync, états de conservation d'énergie, contrats partagés. | Pas de binding Camera2/AVFoundation ; capture synthétique ; injection sans effet ; batterie fixe à 85 % dans `power_state`. | Applications natives, lifecycle suspend/reprise, permissions révoquées, stockage sécurisé, IME/accessibilité, synchronisation hors ligne sur appareils. |
| Web | Opérations OPFS simulées en mémoire, structures audio/canvas/WebGPU, politique HTTP commune lors des tests natifs. | Pas de binding `getUserMedia`, stockage volatil en mémoire, capture de démonstration, injection sans effet. IPC desktop restreint à Windows/Unix. | Build navigateur réel, persistance OPFS, permission utilisateur, CORS, contraintes WASM, fermeture/rechargement d'onglet et tests navigateur. |

Sources principales :

- [`substrat Windows`](../crates/hakille-substrate-windows/src/substrate_impl.rs),
  [`caméra Windows`](../crates/hakille-substrate-windows/src/camera_mediafoundation.rs).
- [`capture Linux`](../crates/hakille-substrate-linux/src/display.rs),
  [`télémétrie Linux`](../crates/hakille-substrate-linux/src/telemetry.rs).
- [`capture macOS`](../crates/hakille-substrate-macos/src/display.rs),
  [`requêtes hôte macOS`](../crates/hakille-substrate-macos/src/host_query.rs).
- [`Redox`](../crates/hakille-substrate-redox/src/substrate_impl.rs),
  [`mobile`](../crates/hakille-substrate-mobile/src/substrate_impl.rs),
  [`Web`](../crates/hakille-substrate-web/src/substrate_impl.rs).

L'audio mérite une distinction : [`hakille-audio`](../crates/hakille-audio/src/lib.rs)
contient des chemins CPAL réels. Cela ne rend pas réelles les méthodes audio des
substrats qui ne les appellent pas. De même, une capture de pixels de test peut
servir au renderer mais ne doit jamais être interprétée comme une observation du
bureau de l'utilisateur.

## 3. Constats prioritaires et implications

### P0 — Contrat réseau commun à durcir

À l'état initial, [`egress.rs`](../crates/hakille-substrate/src/egress.rs)
vérifie l'URL de départ puis utilise le comportement de redirection de reqwest.
Une destination autorisée peut ainsi rediriger vers un hôte que la politique
initiale n'autorisait pas. Le même code appelle `response.bytes()` avant de
vérifier la taille finale : un flux sans `Content-Length` fiable peut dépasser
la limite mémoire annoncée avant son refus.

Critères de correction : aucune requête implicite vers une destination non
validée ; contrôle de taille pendant la lecture ; conservation de TLS et des
timeouts ; tests avec serveurs locaux et réponses fractionnées. Ces corrections
sont prises en charge dans le lot réseau décrit en section 8.

### P0 — Le flux IPC doit préserver toutes les trames

À l'état initial, [`transport.rs`](../crates/hakille-ipc/src/transport.rs)
lit par blocs et retourne dès le premier saut de ligne, en abandonnant les octets
suivants déjà lus. Deux trames reçues dans un seul bloc peuvent donc devenir une
seule trame visible. Cela touche notamment le streaming et les accusés de décision.

L'écoute Unix tentait aussi de supprimer un chemin déjà occupé sans d'abord
établir qu'il s'agissait d'un socket périmé. Le correctif doit préserver un
serveur actif et un fichier ordinaire. Le lot IPC est pris en charge séparément ;
il doit être vérifié par des tests de messages groupés, fragmentés, trop longs,
EOF partiel et collision d'endpoint.

### P0 — Aucune réussite de processus sans exécution

Les six `spawn_sandboxed` ignorent initialement `_wasm` puis renvoient un
`ProcessHandle` issu d'un compteur après vérification des capabilities. Cela
n'exécute aucun processus. Une autorisation cryptographique valide ne constitue
pas une preuve d'exécution. Le lot substrats de cette session remplace ce faux
succès par un refus explicite tant que le runner n'est pas relié.

Le runtime [`wasm_runner.rs`](../crates/hakille-agents/src/wasm_runner.rs) est
une autre voie, réelle : compilation Wasmtime, ABI, fuel, interruption par epoch,
`ResourceLimiter`, vérification des artefacts et absence d'import hôte. Ne pas
confondre l'existence de ce runner avec son intégration à chaque substrat ou avec
l'exécution des cinq agents métier.

### P1 — Sélection de plateforme du démon

Le démarrage initial de [`main.rs`](../crates/hakille-daemon/src/main.rs)
instancie `WindowsSubstrate` et `WindowsComputeBackend` sans sélection selon
l'OS, même lorsque le binaire est exécuté sur Linux. Compiler une implémentation
Linux dans le workspace ne la sélectionne pas automatiquement au boot.

Correction attendue : fabrique d'hôte explicite, erreurs pour hôtes non couverts,
manifest cohérent avec l'OS réel, mêmes racines de confiance sur chaque voie et
tests du mapping. Les chemins `:win-*` doivent rester spécifiques à Windows et
ne pas produire de diagnostic matériel trompeur ailleurs.

### P1 — Persistance, clés et identité

Le démon utilise un registre de clés OS avec relecture ; l'identité n'est donc
plus simplement régénérée à chaque démarrage. Cependant :

- Le backend Linux configuré `keyring/linux-native` est documenté comme volatil
  par session dans [`hakille-keystore`](../crates/hakille-keystore/src/lib.rs).
- Le démon possède encore un repli vers des seeds bruts dans `keys/*.bin`.
  L'existence d'un helper de fichier chiffré ne prouve pas son utilisation au boot.
- Le répertoire de données par défaut est encore fondé sur `LOCALAPPDATA`, puis
  le dossier courant `./hakille_data`, et non une politique native pour chaque OS.
- L'outbox écrit du JSON avec `std::fs::write` : tester interruption d'écriture,
  fichier tronqué et reprise. Une écriture atomique et une politique de migration
  sont nécessaires avant de promettre une continuité robuste.

Critère d'acceptation : deux processus successifs et un redémarrage de session
conservent la même identité et relisent le journal ; corruption ou indisponibilité
produit une erreur claire sans création silencieuse d'une identité concurrente.

### P1 — Vérité des capacités et mesures

Certains manifests accordent des capacités dont le binding est absent. Le mobile
annonce notamment `has_secure_enclave: true` sans détection associée dans son
constructeur. Les mesures inconnues doivent être distinctes des valeurs de
référence : `Unknown` existe pour le thermique, mais la batterie et les captures
ne portent pas toutes la même provenance explicite.

Il faut rendre impossible au consommateur de confondre « interface prévue »,
« binding disponible », « autorisation accordée » et « effet réalisé ». Une carte
approuvée, un scope déclaré, un handle fabriqué et une trace d'audit sont quatre
faits différents.

## 4. Agents métier : les fonctions présentes et le chemin restant

| Agent | Ce que le code natif sait faire | Ce qu'il ne faut pas annoncer |
| --- | --- | --- |
| Mail | Parser/ingérer des messages, chercher, gérer des brouillons, archiver localement un envoi confirmé. | Un mail transmis : `SendReceipt.status` précise `archived-locally (no SMTP transport configured)`. |
| Calendrier | Import/export ICS, événements et conflits, synchronisation vers le graphe. | Synchronisation CalDAV/provider ou conversion complète des zones TZID ; le parseur signale encore ce travail. |
| Code | Indexer du contenu, extraire des symboles, construire/appliquer une transformation en mémoire. | Exécution d'un agent isolé écrivant dans le dépôt de l'utilisateur avec permissions métier effectives. |
| Maison | Registre d'appareils et transformation d'états locaux. | Commande physique confirmée par Matter/MQTT ou un autre transport. |
| Portefeuille | Lecture/analyse de comptes de démonstration et refus des mutations financières. | Connexion bancaire ou transfert réel. |

Sources : [`agents/`](../agents/) et
[`runtime du démon`](../crates/hakille-daemon/src/agent_runtime.rs).

Avant le branchement d'un premier agent métier, définir son contrat d'entrée et
de sortie, son artefact exécutable, les imports hôtes minimaux et leur médiation
Biscuit, les quotas, le refus après révocation et l'idempotence après reconnexion.
Un premier scénario de lecture seule vérifiable est préférable à cinq connecteurs
qui renverraient un succès local sans effet distant.

## 5. Interface et validation utilisateur

Le modèle de cartes, le maintien d'Entrée, les accusés d'audit et la séparation
rendu/effets sont présents. Le chemin souris existe dans le desktop actuel :
certains passages de `ARCHITECTURE.md` qui disent le contraire sont périmés.
Le même niveau de prudence s'applique à l'accessibilité : un arbre AccessKit bien
formé n'est pas une preuve d'utilisation avec NVDA, VoiceOver ou Orca.

Priorités : rendre les états déconnecté/occupé/échec visibles et non ambigus,
préserver les saisies et décisions lors des reconnexions, empêcher les doubles
soumissions et faire correspondre le focus clavier, la souris et les actions
AccessKit. Le lot UI de cette session apporte ses tests ciblés ; leur résultat et les
limites de validation sont distingués en section 8.

## 6. Tests, CI et dette documentaire

Le workflow actuel déclare des tests workspace Windows/Ubuntu, des contrôles
cross-target, un job macOS et un job qualité UI. Les deux derniers restent
`continue-on-error: true`. Les tests d'inférence lente sont séparés, ignorés par
le parcours normal, et exigent les artefacts Qwen. Les filtres du job UI ne
sélectionnent que certains noms : un nouveau test ne sera pas nécessairement
exécuté par ce job spécialisé.

Ces fichiers décrivent la politique souhaitée ; cette revue n'a pas consulté
l'historique des exécutions distantes. Les badges statiques « passing » ne
constituent pas une preuve de validation de la révision courante.

La documentation mélange plusieurs générations d'implémentation :

- `SECURITY.md` décrit des clés éphémères alors que le démon utilise désormais
  un keystore et un repli fichier.
- L'absence de `StoreLimits` nommément ne signifie pas une absence de limites :
  le runtime possède un `ResourceLimiter` personnalisé.
- Les descriptions d'OS mentionnent parfois un protocole natif alors que le
  chemin d'effet est encore synthétique.
- Des compteurs de tests et des mesures de latence sont historiques ; ils ne
  doivent pas être présentés comme résultats de la session courante.

La règle de maintenance proposée est de documenter chaque fonction par son point
d'entrée, le backend exécuté, le test associé et la limite connue. Les tests qui
nécessitent un modèle ou un périphérique doivent distinguer réussite, omission
explicite et échec ; zéro test exécuté n'est pas une validation.

## 7. Feuille de route avec critères de sortie

| Ordre | Lot | Critère de sortie vérifiable |
| --- | --- | --- |
| P0 | Réseau partagé | Aucune redirection hors politique ; refus de réponse trop grande pendant la lecture ; tests locaux négatifs et positifs. |
| P0 | IPC et décisions | Aucun message perdu en rafale ; serveur actif protégé ; reconnexion et décision auditée sans double effet. |
| P0 | Vérité d'exécution | `spawn_sandboxed` refuse explicitement sans runner ; aucun succès d'agent métier déclaré comme exécuté sans artefact/effet. |
| P1 | Hôte et données | Backend choisi selon l'OS, chemins natifs documentés, identité et état conservés au redémarrage. |
| P1 | Runtime d'agents | Bornes d'entrée/sortie/mémoire/temps vérifiées avec modules hostiles et cas limites ; refus observable. |
| P1 | UX accessible | Saisie et focus cohérents clavier/souris/AccessKit ; scénarios d'erreur et reconnexion testés. |
| P2 | Premier agent métier | Fonction de lecture seule exécutée dans le runner, données réelles injectées explicitement, permissions et audit prouvés. |
| P2 | Capture/audio réels | Branchements natifs par OS ou erreur explicite ; aucune image synthétique présentée comme perception réelle. |
| P2 | Linux/macOS natifs | Campagnes matérielles distinctes selon ADR-010, incluant permissions refusées et accessoires absents. |
| P3 | Mobile/Web | Applications et bindings natifs/navigateur, persistance, lifecycle, transport authentifié et synchronisation réels. |
| P3 | Redox/distribution | VM Redox, schemes effectifs, packaging et stratégie de mise à jour avec restauration. |

Chaque lot doit rester révisable et avoir des preuves propres. Le succès des
couches partagées profite aux ports existants ; il ne dispense pas de construire
et valider les adaptateurs propres à chaque plateforme.

## 8. Corrections du lot courant et état de validation

Cette section décrit les modifications présentes dans le répertoire de travail,
indépendamment de l'état initial analysé ci-dessus. « En attente » signifie que
la correction n'est pas encore revendiquée comme validée dans cette revue.

| Lot | Modification réalisée | Validation constatée dans cette session |
| --- | --- | --- |
| Réseau commun | Vérification de chaque redirection, refus de dégradation HTTPS vers HTTP, limite du corps pendant la lecture, canonicalisation IPv6 et masquage des identifiants d'URL dans les erreurs. | 10 tests unitaires et 8 tests de régression passés, incluant HTTP local, confidentialité des erreurs et réponse de taille exactement égale au plafond. Aucun appel à un service externe requis par ces tests. |
| IPC | Lecture bufferisée préservant les trames groupées ; refus d'un endpoint occupé au lieu d'une suppression aveugle. | 18 tests passés sous Linux ; vérification de compilation pour Windows GNU et macOS x86_64. Les échanges sur named pipe Windows et socket macOS n’ont pas été exécutés sur ces OS. |
| Runtime WASM | Tables et E/S JSON bornées, validation avant exécution, refus d'ABI incompatibles, erreurs de démarrage explicites. | 21 tests passés dans un harness incluant le fichier source exact du runtime ; cela ne borne pas toute la mémoire du compilateur/JIT et ne crée pas de nouveaux agents métier. |
| Substrats | Les six méthodes `spawn_sandboxed` refusent explicitement sans runner après les vérifications de capabilities ; suppression des compteurs de processus fictifs. | 184 tests des six crates exécutés sous Linux, dont 18 nouvelles régressions ; aucun échec ni test ignoré. Ce résultat ne valide pas les périphériques des six OS. |
| Démarrage natif | `hakille-daemon/src/host.rs` sélectionne Linux/macOS/Windows avec racine de confiance ; aliases `:ram`/`:power` natifs ; refus des commandes GUI Windows ailleurs. | 2 tests passés dans un harness incluant le module exact : choix Linux/CPU et refus d’une racine étrangère avant effet fichier. Les branches macOS/Windows de cette fabrique ne sont pas exécutées ici. La branche Unsupported n'établit pas que toutes les dépendances du démon compilent sur mobile, Web ou Redox. |
| Interface | Panneaux mémoire/journal paginés (8 lignes, clavier et molette), contenu complet des réponses exposé à AccessKit, conservation des longues chaînes UTF-8. | 94 tests bibliothèque et 2 tests binaire passés sous Linux sans interface graphique, dont la conservation des fractions de molette/trackpad ; aucun échec ni test ignoré. Lecteurs d’écran et sessions graphiques natifs restent à vérifier. Voir [détail UI](UI_NAVIGATION_PANNEAUX.md). |

`SECURITY.md` a également été corrigé pour refléter le limiteur WASM et le
keystore existants, en conservant les réserves sur le repli en clair et le
backend Linux volatile. Les captures synthétiques, la médiation métier, les
bindings mobiles/navigateur et les limites restantes de la section 3 ne sont
pas résolus par ces corrections.

### Limite de validation globale

Le contrôle global des types du workspace a réussi avec `ORT_SKIP_DOWNLOAD=1`
pour distinguer les erreurs Rust des problèmes de provisionnement du binaire
ONNX Runtime. Cette option de contrôle ne prouve ni la liaison de l'application,
ni son démarrage, ni l'inférence ONNX. Les harness utilisent les fichiers source
réels et les dépendances correspondantes ; leurs succès ne sont pas présentés
comme un passage de `cargo test --workspace` ou des tests de conformance complets.
Les trois tests de `ipc_client` déjà inclus dans la bibliothèque UI ne doivent
pas être additionnés une seconde fois à son total.

### Bilan final des vérifications locales

- **339 tests réussis** dans les suites ciblées distinctes : réseau partagé 18,
  IPC 18, desktop 96, six substrats 184, runtime WASM 21 et fabrique native 2.
  Les deux derniers lots ont été exécutés dans des harness locaux incluant les
  fichiers source exacts, afin d'isoler la dépendance ONNX indisponible.
- `cargo fmt --all -- --check` et `git diff --check` réussis.
- `ORT_SKIP_DOWNLOAD=1 cargo check --workspace --all-targets --locked` réussi.
- `ORT_SKIP_DOWNLOAD=1 cargo clippy --workspace --all-targets --locked --no-deps`
  réussi, avec avertissements dans les substrats Linux/Windows hors des lignes
  modifiées par ce lot.
- Vérifications IPC Windows GNU et macOS x86_64 réussies ; exécution native
  de ces deux plateformes non réalisée.
- Compilation normale encore bloquée par HTTP 403 du CDN ONNX après connexion
  au proxy. Aucun contournement de TLS ni de checksum n'a été utilisé.
  `ORT_SKIP_DOWNLOAD` est utilisé uniquement pour l'analyse des types et Clippy,
  jamais comme preuve d'un binaire fonctionnel.
- Tests workspace complets, conformance, génération de tokens en mode contrôle,
  démarrage complet, modèles et `cargo-deny` non validés dans ce lot. Les commandes
  dépendant d'ONNX doivent être reprises une fois le binaire accessible.

Le dépôt contient les corrections et leurs tests ; aucune publication ni aucun
commit n'a été effectué pendant ce travail.
