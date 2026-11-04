# Analyse mobile Android et iOS — 4 novembre 2026

Cette analyse décrit l'état initial. Les corrections livrées et leurs preuves
sont détaillées dans le [rapport de validation](VALIDATION_MOBILE_2026-11-04.md).

## Conclusion et périmètre de preuve

Hakille possède un socle Rust réutilisable pour un client mobile : protocole
UI versionné, états d'interface, design system, logique de capabilities,
algorithmes de réplication et substrat `Guest`. **Ce socle ne constituait pas
une application Android ou iOS utilisable au début de cet audit.** Aucun
projet Gradle/Compose, projet Xcode/SwiftUI, manifeste Android, source Swift ou
Kotlin d'application, UDL ou binding UniFFI n'a été trouvé dans cet état initial.

Le chemin réaliste consiste à construire un client mobile natif qui utilise
un cœur Rust commun et un transport explicite vers le hub personnel. Le
portage automatique du renderer desktop, l'annonce d'une inférence NPU ou
l'affichage d'un état de synchronisation sans transport ne satisfont pas ce
besoin.

Cette analyse est une revue de sources et de documentation dans le workspace
partagé. Les agents travaillent simultanément sur les corrections et les
applications : les constats « initiaux » ci-dessous ne prétendent pas décrire
les corrections après validation. Aucun test Android/iOS, émulateur, appareil,
compilation native ou publication n'a été exécuté pour produire ce rapport.
Les tests présents dans les sources sont des points de départ, pas des
résultats nouvellement obtenus ici.

## Vision produit et décisions applicables

La [vision du README](../README.md) privilégie la continuité d'état, une
initiative bornée et la souveraineté. Le téléphone doit être un terminal
personnel cohérent : consultation de mémoire et journal, dialogue, décisions
explicites et reprise après déconnexion. Une présentation de chat seule ne
couvre pas cette vision ; inversement, montrer des boutons métier sans
exécutant ni accusé réel ne l'implémente pas.

Les décisions pertinentes sont :

| Source | Prescription applicable au mobile | Limite à conserver |
| --- | --- | --- |
| [ADR-001](../adr/001-rust-only-no-python.md) | Cœur métier, inférence et sécurité en Rust ; aucun runtime Python de production | Ses conséquences générales, dont le démarrage sous 50 ms, ne sont pas des mesures de cette application |
| [ADR-009](../adr/009-ui-native-winit-wgpu-accesskit.md) | Protocole typé, view-models immuables, renderer sans secrets, actions revalidées par le noyau | Le choix winit/wgpu concerne le desktop ; ADR-007/008 sont remplacées |
| [ADR-010](../adr/010-per-os-compatibility-project.md) | Mobile adapté avec **SwiftUI / Jetpack Compose via UniFFI**, partage du protocole et du design | Une compilation ne vaut pas support matériel ; pas de port automatique du HUD desktop |
| [ADR-006](../adr/006-crdt-sync-and-multi-device-handoff.md) | Continuité, tombstones, horloges et handoff | Les garanties de convergence et de fonctionnement hors ligne doivent être établies pour le code et le transport effectifs |
| [Interfaces et perception](INTERFACES_AND_PERCEPTION.md) | Présence discrète, journal lisible, coûts/provenance/confirmation explicites, repli texte | Hotkey global et overlay desktop ne sont pas un modèle d'interaction mobile ; écoute permanente en arrière-plan non démontrée |
| [Gel Slint](FREEZE_SLINT.md) | Réutiliser la frontière headless ; ne pas réintroduire le renderer retiré | Ce document n'est pas un projet mobile livré |

Les couches Swift/Kotlin natives fines sont donc cohérentes avec la décision
mobile explicite, à condition de conserver les règles métier dans Rust.
« Sans Python en production » n'interdit pas ces interfaces natives prévues
par les ADR. Une WebView n'est pas la solution retenue.

Deux références de plan citées par les ADR sont absentes de cet état du
dépôt : `docs/PLAN_REFONTE_INTERFACE_2026-10-16.md` et
`docs/PLAN_REFONTE_SUIVI.md`. En revanche, `FREEZE_SLINT.md` et
`SPIKES_LAB.md` existent. Il faut s'appuyer sur les sources et décisions
disponibles, sans inventer le contenu des plans manquants.

## Matrice de l'existant initial

| Domaine | Réalité vérifiée | Android / iOS restant à raccorder |
| --- | --- | --- |
| Application native | Aucun shell mobile trouvé au début de l'audit | Navigation Compose/SwiftUI, cycle de vie, configuration de build, packaging |
| Protocole UI | `hakille-ui-protocol` v1, DTO et messages sérialisables, tests de golden | Adaptateur mobile complet, bornes d'entrée, corrélation et états d'erreur |
| Modèle d'interface | `hakille-ui-model`, `StableSnapshot`, événements et view-models sans GPU | Snapshot mobile complet, conservation des réponses de consultation, géométrie adaptative |
| Design | `tokens/*.toml`, générateurs Swift/Kotlin dans `xtask/src/tokens.rs` | Génération et consommation dans les apps, thèmes système, Dynamic Type, RTL |
| Transport | `hakille-ipc` : socket Unix ou named pipe Windows, jeton local, JSON borné | Liaison téléphone-hub authentifiée, transport sécurisé, appairage, révocation, reconnexion |
| Substrat mobile | Rust `Tier::Guest`, vérifications Biscuit, résolution de chemins, logique batterie | Bindings Android/iOS, répertoire réel fourni par l'hôte, permissions et mesures |
| Stockage mobile | Au départ, `read_file`/`write_file` utilisent une map shadow en RAM | Persistance durable réelle, erreurs de disque, protection contre liens et corruption |
| Synchronisation shadow | Queue et machine d'état locales | Transport, ACK vérifié, rétention des changements tant qu'ils ne sont pas acquittés |
| Réplication du graphe | Horloges, tombstones, deltas chiffrés et fusion dans `hakille-sync` | Appairage, stockage, livraison, reprise, tests de convergence et conflit complet |
| Handoff | Signature Ed25519, destinataire, expiration et cache anti-rejeu en mémoire | Identité du pair approuvée, clé durable, rejeu après redémarrage, transport |
| Caméra, audio, écran, injection | Méthodes mobiles refusant explicitement l'effet absent | Camera2/AVFoundation, capture audio, consentements ; pas d'injection générale promise sur iOS |
| Agents | `spawn_sandboxed` mobile refuse l'exécution non raccordée | Aucun `.wasm` métier mobile ou runtime host mobile validé |
| Inférence mobile | Manifeste Guest déclaratif ; moteurs desktop distincts | Modèles mobiles réels, runtime ARM64, CoreML/NNAPI si effectivement intégrés et mesurés |
| Livraison | Pas de pipeline Android/iOS identifié dans la CI initiale | APK/AAB, application iOS, architectures, signatures et essais matériels |

Les sources principales sont
[`hakille-substrate-mobile`](../crates/hakille-substrate-mobile/src/lib.rs),
[`hakille-ui-protocol`](../crates/hakille-ui-protocol/src/lib.rs),
[`hakille-ipc`](../crates/hakille-ipc/src/transport.rs),
[`hakille-ui-model`](../crates/hakille-ui-model/src/snapshot.rs),
[`hakille-sync`](../crates/hakille-sync/src/lib.rs) et
[`manifests/guest.toml`](../manifests/guest.toml).

## Écarts critiques relevés

### 1. Faux état de connexion et changements supprimés sans ACK

Dans l'état initial de [shadow.rs](../crates/hakille-substrate-mobile/src/shadow.rs),
`connect()` passe de `Connecting` à `SyncingState`, puis `ActiveStreaming`,
sans socket ni handshake. `sync_deltas()` vide la queue locale et renvoie
son ancienne longueur ; `wake_and_resync()` procède de même. Aucun accusé
réseau ne justifie cette suppression. Le test historique vérifie cette
machine locale, pas une connexion TiKV.

La configuration par défaut contient également un endpoint TiKV et un token
APNs d'exemple. Ces valeurs ne prouvent ni un serveur joignable ni une
inscription push. Une app doit rester hors ligne tant que le transport
authentifié n'a pas confirmé la connexion. Une écriture en attente doit être
préservée jusqu'à un ACK lié à sa version, sans confondre « retirée de la
queue » et « répliquée ».

### 2. Matériel annoncé sans mesure

Le manifeste initial annonce `has_secure_enclave=true`, et
[compute_impl.rs](../crates/hakille-substrate-mobile/src/compute_impl.rs)
retourne NPU préféré et 8 Gio de RAM sans binding de détection. Le substrat
retourne aussi une batterie à 85 %. Le budget de 4 Gio du manifeste est une
constante de profil, pas la mémoire disponible sur le téléphone.

`attest()` signe avec une clé générée localement et construit un
`manifest_hash` à partir de la longueur des données. Ce n'est pas un hash
SHA-256 du manifeste ni une preuve Secure Enclave/Android Keystore. Ces
champs doivent annoncer l'inconnu ou une politique explicite ; une capacité
déclarée ne signifie pas que son binding, sa permission et son appareil sont
opérationnels.

### 3. Stockage scopé logique, sans persistance initiale

[scoped_storage.rs](../crates/hakille-substrate-mobile/src/scoped_storage.rs)
contrôle des chaînes de chemin et mémorise des URI autorisées. Il ne lance
pas SAF, Photo Picker ou un sélecteur iOS. Sa racine par défaut est un chemin
Android codé en dur, inadapté au conteneur iOS.

Dans le substrat initial, le chemin résolu est ignoré après validation : les
opérations lisent ou modifient le shadow en mémoire. Les tests de chemins
ne démontrent donc ni écriture durable, ni résistance aux liens symboliques,
ni accès réel au trousseau. La racine doit être fournie par l'application
native ; un scope `SecureKeystore` ne transforme pas un fichier en clé
matérielle.

### 4. Signature du handoff sans identité de pair approuvée

[handoff.rs](../crates/hakille-sync/src/handoff.rs) vérifie la signature avec
la clé publique contenue dans le message. Cette vérification protège
l'intégrité par rapport à cette clé, mais ne lie pas celle-ci à un appareil
préalablement appairé : un autre émetteur peut créer sa clé et déclarer un
identifiant source. Le cache de signatures vues est volatil ; le helper de
création accepte aussi une expiration personnalisée.

Avant toute exposition réseau, vérifier le lien entre identifiant et clé de
pair approuvée, borner la validité, traiter la révocation et le rejeu durable.
Un handoff contenant une action en attente n'approuve jamais cette action.

### 5. Convergence non démontrée sur les égalités

Le shadow arbitre par version puis timestamp, avec priorité à une tombstone.
À version et timestamp égaux, deux valeurs différentes non supprimées sont
départagées en conservant l'existant local. Deux pairs recevant les mêmes
valeurs dans l'ordre inverse peuvent conserver des résultats différents.

La même difficulté existe dans
[`CrdtSync::merge`](../crates/hakille-sync/src/crdt.rs), qui garde l'existant
à `last_activated` égal ; le commentaire affirmant la commutativité de ce
choix ne suffit pas et est incorrect pour des valeurs concurrentes
différentes. Un ordre total stable ou une représentation multivaleur est
nécessaire. Le différentiel temporel de
[replication.rs](../crates/hakille-sync/src/replication.rs), fondé sur
`created_at`, doit aussi être testé face aux modifications d'un fait ancien
et aux horloges décalées.

### 6. Modèle UI ancien incomplet pour les réponses actuelles

Dans [ui-model/events.rs](../crates/hakille-ui-model/src/events.rs), la
conversion `KernelToUi → KernelEvent` transforme les réponses journal,
mémoire, statut, agents et accusés de carte en simples toasts. Les données
complètes disparaissent de cette conversion. Un client mobile doit traiter
les DTO complets, pas se contenter d'afficher « mémoire : N lignes ».

`StableSnapshot` contient en outre une géométrie et des représentations
historiques du HUD desktop. Réutiliser les règles et tokens ne signifie pas
imposer un champ 720 × 64 au téléphone ou tronquer les identifiants `u64`
dans un type natif signé plus petit.

## Contrats attendus du client mobile

### État et transport

Le cœur Rust devrait exposer une création de session, une soumission
d'intention, l'ingestion de réponses et un snapshot immuable. Les résultats
doivent distinguer au minimum : déconnecté, connexion en cours, connecté
authentifié, incompatible, occupé et erreur. Les transitions doivent découler
d'événements réels ; un écran de démo ne devient pas un état connecté.

Les enveloppes [IPC](../crates/hakille-ipc/src/frame.rs) possèdent déjà un
identifiant de requête, contrairement aux enveloppes simples du protocole
UI. Le mobile doit corréler chaque réponse, rejeter les anciennes sessions
et gérer timeout, annulation et reconnexion. Les deltas de tokens sont
provisoires ; `HudReply` remplace la réponse affichée. Borner trames, texte,
files, historiques et travail de désérialisation ; `MAX_FRAME_BYTES` vaut
1 Mio dans l'IPC local actuel, mais ne borne pas automatiquement une API FFI
ou un transport mobile ajouté.

Le token local du démon n'est pas un mécanisme d'appairage réseau. Une
passerelle téléphone-hub doit avoir son propre contrat de transport sûr,
identité serveur et révocation. Ne pas exposer le socket ou son secret sur
le réseau sans ce contrat. L'offline doit conserver les données et afficher
leur ancienneté ; les actions sensibles ne sont pas rejouées aveuglément.

### Fonctions visibles et critères d'acceptation

| Fonction | Contrat attendu |
| --- | --- |
| Dialogue | Saisie native, état occupé, réponse finale, erreurs lisibles ; aucune réponse inventée hors ligne |
| Cartes | Objet, raison, coût, sortie de données, provenance et réversibilité conservés ; refus possible ; état conservé jusqu'à ACK audité |
| Mémoire | DTO complets, recherche et liste bornées ; mémorisation confirmée par ID réel ; suppression non simulée |
| Journal | Résumés lisibles, dates, audit ID ; « annuler » uniquement si l'action serveur existe |
| Agents | Distinguer manifest déclaratif et artefact exécutable ; résultat lié à la requête et à l'audit |
| Statut | Connectivité et capacités réellement disponibles ; absence de modèle ou de permission visible |
| Réglages | Options effectivement persistées et appliquées ; contrôle microphone et données sortantes explicites |
| Permissions | Demande contextuelle native, refus/révocation gérés, aucun placeholder de permission accordée |
| Arrière-plan | Arrêt ou suspension conforme au cycle de vie ; notification push distincte d'une synchronisation réussie |

Une approbation de carte signifie aujourd'hui une mise en file durable sur
le hub lorsque le noyau l'accepte, pas une exécution métier. Le commentaire
du protocole relatif à l'appui long doit également être lu avec le code :
le chemin `Approve` de `hakille-daemon/src/ipc.rs` appelle `decide_card`
directement ; il ne transporte pas une preuve d'appui mesurée par le noyau.
Le desktop mesure son maintien localement. Une UI mobile doit traiter
appui/annulation/accessibilité explicitement sans présenter ce geste comme
une preuve cryptographique distante. L'autorisation reste côté noyau.

### Frontière Rust et hôtes natifs

- **Rust** : validation des messages, limites, états, idempotence,
  snapshots, règles de consentement et traitements métier partageables.
- **Compose** : navigation Android, champs IME, sémantique TalkBack,
  Activity lifecycle, permissions et accès aux API Android.
- **SwiftUI** : navigation iOS, champs et Dynamic Type, VoiceOver,
  Scene lifecycle, permissions et accès aux API Apple.
- **UniFFI** : frontière versionnée et générée, ownership explicite,
  destruction des ressources, erreurs typées et callbacks hors du thread UI.
  Un wrapper C/JNI transitoire doit documenter sa divergence de l'ADR plutôt
  que se présenter comme UniFFI livré.

Éviter d'embarquer tous les moteurs desktop par transitivité dans le cœur
mobile de consultation. Les dépendances minimales permettent de compiler
et tester les règles partagées sans Wasmtime, ONNX, GPU ou capture audio.
Une politique thermique ou batterie reçoit des mesures natives ; elle ne
les invente pas. Aucun thread d'inférence ou réseau ne bloque le thread UI.

## Priorités d'implémentation et validation

1. **P0 — contrats honnêtes et protection des données.** Corriger les faux
   états shadow, préserver les deltas, retirer les mesures matérielles
   inventées, raccorder une racine de stockage réelle. Tester absence de
   backend, refus de permission, corruption et conservation après échec.
2. **P0 — cœur mobile pur et complet.** Adapter directement `KernelToUi`,
   snapshots complets, erreurs, limites et session/corrélation. Tests sur
   versions incompatibles, réponses anciennes, troncature, Unicode et ACK.
3. **P1 — deux hôtes natifs.** Livrer des projets reproductibles Android et
   iOS consommant le même cœur et les mêmes tokens ; prouver les parcours
   texte, consultation et carte, sans boutons factices.
4. **P1 — liaison authentifiée au hub.** Appairage explicite, transport,
   révocation, négociation de version, reconnexion. Une consultation locale
   importée est utile si clairement nommée, mais n'est pas un hub connecté.
5. **P1 — durabilité et continuité.** Reprise après arrêt brutal, ACK
   idempotent, clés persistantes, conflits égaux, tombstones et déconnexion
   pendant une décision. Deux appareils doivent converger dans des ordres
   d'arrivée différents.
6. **P2 — capacités natives prouvées.** Micro/caméra/document picker,
   arrière-plan, notifications et inférence légère seulement après binding,
   permission et mesure par OS. Le manifeste Guest mentionne Moonshine,
   NeuTTS-Air et CAM++-Edge ; ces noms ne provisionnent aucun modèle mobile.

### Matrice de preuves avant annonce de support

| Niveau | Preuves attendues | Ce que cela ne prouve pas |
| --- | --- | --- |
| Rust host | Unitaires, golden protocole, limites, lifecycle et concurrence | Compilation ARM ou UI native |
| Android build | Rust ARM64, bibliothèque chargée, Gradle/Compose, APK debug | Micro réel, batterie, OEM et TalkBack |
| iOS build | Rust device/simulator, framework, SwiftUI/Xcode, app lancée | App Store, signatures de distribution ou appareil réel |
| Simulation UI | Navigation, annulation, erreurs, clavier, reconnexion | Permissions réelles, consommation, accessibilité humaine |
| Appareil Android | Permission accordée/refusée/révoquée, Doze, reprise, TalkBack, réseau | Tous les constructeurs et versions Android |
| Appareil iOS | Scene/background, interruptions audio, VoiceOver, sandbox, réseau | Exécution arbitraire hors sandbox ou écoute illimitée |
| Interop hub | Pairage, refus d'un mauvais pair, ACK, audit, offline/reprise | Exécution métier encore absente du hub |

Dans l'environnement d'audit, Rust 1.95.0 est disponible via
`/workspace/.hakille-tools/env.sh`. Java est présent ; `gradle`, `adb`,
`swift` et `xcodebuild` ne sont pas trouvés dans le PATH lors de cette revue.
Ce constat ne préjuge pas des installations ultérieures réalisées par les
agents. Xcode et les campagnes sur appareil nécessitent un environnement
adapté ; un check Rust Linux n'est pas une compilation iOS.

## Suivi des lots parallèles

Les constats P0 ont été transmis au coordinateur et aux agents concernés.
Un lot substrat prend la persistance sous racine fournie par l'hôte et les
déclarations matérielles. Les corrections de shadow, les applications et
leur cœur/transport sont à qualifier sur leurs diffs et résultats finaux.
Ce rapport ne leur attribue aucun succès anticipé. Il doit servir de base
de comparaison entre l'état initial décrit et les preuves effectivement
obtenues à la fin du lot.
