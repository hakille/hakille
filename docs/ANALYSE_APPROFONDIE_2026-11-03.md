# Analyse approfondie et deuxième lot d'implémentation — 3 novembre 2026

Ce rapport complète l'[audit multiplateforme](ANALYSE_MULTIPLATEFORME_2026-11-03.md).
Il décrit les causes des défauts, les changements relus dans le répertoire de
travail, leurs critères de validation et les limites conservées. Il couvre les
trois axes demandés : fiabilité/sécurité, expérience utilisateur et fonctions
métier/IA. Il ne déclare pas achevés les ports natifs ni les agents métier WASM.

## 1. Le problème commun : distinguer intention, état et effet

Plusieurs défauts venaient d'une confusion entre une opération autorisée et une
opération achevée. Un jeton valide ne fait pas apparaître un pilote ; un élément
retiré d'une file n'est pas un message envoyé ; un fichier produit n'est pas
nécessairement un patch applicable ; un helper nommé « chiffré » doit résister à
une modification et à un mauvais mot de passe.

Le premier lot corrige les frontières réseau, IPC, runtime WASM et sélection
native. Le deuxième traite les effets et la persistance, puis améliore les
fonctions métier qui peuvent être réellement exercées sans connecteur externe.

| Axe | Changement utilisable | Ce qui reste distinct |
| --- | --- | --- |
| Sécurité/fiabilité | Chiffrement authentifié du helper, outbox atomique, erreurs hôtes explicites, transport borné. | Migration du repli du démon, transactions audit/outbox, connecteurs et validations matérielles. |
| Interface | Panneaux mémoire/journal paginés, navigation clavier/molette et contenu AccessKit. | Tests de lecteurs d'écran natifs, interfaces mobiles/Web et packaging. |
| Métier/IA | Patches code applicables par Git, import/export ICS plus strict, runtime WASM mieux borné. | Exécution des agents métier dans le runner, conversion des fuseaux, réseaux de messagerie/domotique et modèles absents. |

## 2. Fichiers de clés : passer d'un masquage à une authentification

### Cause initiale

Le helper historique de repli fichier utilisait un format v1 sans authentification
cryptographique des données. Sans tag vérifié, une décryption apparente n'établit
pas que le mot de passe et le contenu sont corrects. Pour des seeds d'identité,
une erreur silencieuse change l'autorité cryptographique et rend la reprise
incompatible avec les données signées auparavant.

### Implémentation relue

[`encrypted_file.rs`](../crates/hakille-keystore/src/encrypted_file.rs) définit
le format `HAKKEY02`, version 2, avec :

- un sel aléatoire de 16 octets et un nonce aléatoire de 12 octets issus de l'OS ;
- Argon2id version 0x13, mémoire 19 Mio, deux passes, une voie, clé de 32 octets ;
- ChaCha20Poly1305 avec l'en-tête complet comme données authentifiées ;
- un fichier de taille fixe 88 octets ; la lecture est plafonnée à 89 octets
  pour détecter un excès avant toute accumulation importante ;
- refus de `ANRKEY01` sans écrasement ni conversion silencieuse ;
- verrou permanent voisin pour sérialiser les appels coopérants ;
- fichier temporaire dans le même dossier, synchronisation avant remplacement,
  puis synchronisation du dossier sous Unix ; permissions de création 0600 ;
- effacement des buffers sensibles via `Zeroizing`, erreurs sans mot de passe
  ni seed et `Debug` des résultats expurgé.

La rotation authentifie d'abord l'ancien contenu, puis crée un **nouveau seed**.
Ce n'est pas un simple changement de mot de passe : elle peut invalider les
usages de l'identité précédente. Une procédure de migration doit traiter les
anciennes signatures, les clés de vérification et les données encore utiles.

### Limites conservées

Ce helper reste interdit en mode production (`HAKILLE_PROD=1/true`). Il ne
remplace pas automatiquement le repli en seeds bruts du démon. Ce dernier est
maintenant limité au premier démarrage de développement sans historique et avec
trousseau racine indisponible ; les identités brutes sont refusées en production.
Cette distinction est conservée dans [`SECURITY.md`](../SECURITY.md).

Le dossier parent doit être maîtrisé par l'utilisateur. Les contrôles de liens
symboliques et les options Unix ne constituent pas une garantie universelle
contre un acteur pouvant remplacer les répertoires parents. Sous Windows, les
ACL sont héritées ; aucune nouvelle politique ACL n'est annoncée. Un échec de
synchronisation après renommage peut survenir alors que le fichier a déjà changé :
la procédure de reprise doit relire l'état réel.

Tests requis et présents dans le lot : mauvais mot de passe, modification de
l'en-tête/sel/nonce/ciphertext/tag, tailles invalides, ancien format conservé,
créations et rotations concurrentes, permissions Unix et liens symboliques.
**Validation reçue : 21 tests keystore réussis sous Linux, dont 7 nouvelles
régressions ; aucun trousseau OS réel n’a été utilisé par ces tests.** Les contrôles `cargo check --all-targets --locked` Windows GNU et macOS
x86_64 passent également ; ils ne constituent pas des exécutions natives de
leurs trousseaux.


### Identité du démon : refuser une régénération silencieuse

Le module [`identity.rs`](../crates/hakille-daemon/src/identity.rs) centralise
le choix de l'identité au démarrage :

- Si une identité fichier existe, le mode développement la recharge sans
  changer implicitement de backend ; une paire incomplète provoque un échec.
- Le mode production refuse ces fichiers bruts et exige une migration explicite
  vers le trousseau. Dans le démon, une variable `HAKILLE_PROD` non vide autre
  que `0` active ce refus ; le helper chiffré conserve son propre contrat décrit
  ci-dessus.
- Une corruption, un fichier non régulier ou un lien symbolique n'est pas traité
  comme une absence. Les seeds lus doivent avoir exactement 32 octets ; sous
  Unix, les permissions exposées à d'autres utilisateurs sont refusées.
- La création utilise `create_new`, mode 0600 sous Unix et synchronisation du
  fichier. Un fichier partiel reste visible et provoquera un refus au prochain
  démarrage, au lieu de remplacer silencieusement l'identité.
- La présence d'un historique signé empêche le keystore de créer une moitié de
  clé manquante. Une erreur après chargement de la racine arrête le boot plutôt
  que de basculer vers un backend différent.

Ces mesures protègent la continuité de l'identité ; elles ne mettent pas en place
une transaction multi-fichier pour le premier démarrage, une migration des
anciennes clés, ni un stockage résistant à un attaquant maîtrisant le dossier
parent. Les tests couvrent stabilité, corruption conservée, paire partielle,
historique existant, permissions et refus du repli en production.

## 3. Outbox : persister avant de confirmer

### Causes initiales

Le chargement initial traitait corruption et absence de fichier comme une file
vide. La sauvegarde écrivait directement le JSON et n'informait pas l'appelant
de son échec. Une carte pouvait donc recevoir un accusé alors que son état ne
survivrait pas au redémarrage. Enfin, `:flush` retirait des actions et écrivait
une trace « transmis » sans transport métier.

### Implémentation relue

[`persistence.rs`](../crates/hakille-daemon/src/persistence.rs) rend les états
observables :

1. Seule l'absence d'un snapshot signifie une nouvelle file vide. Une erreur de
   lecture ou de parsing remonte au démarrage sans remplacer les octets d'origine.
2. La modification se fait sur une copie ; le nouveau snapshot est sérialisé
   dans un temporaire du même dossier puis synchronisé et remplacé.
3. `NotCommitted` distingue l'échec avant remplacement de `CommittedNotSynced`,
   qui indique que la publication a eu lieu mais que la durabilité après coupure
   n'est pas confirmée.
4. La RAM suit l'état réellement publié, y compris lors d'un échec de
   synchronisation postérieur au remplacement.
5. L'IPC ne renvoie pas un accusé de décision durable en cas d'échec de
   persistance. La carte reste disponible pour réessai.
6. Un échec d'audit ne supprime pas un doublon qui existait avant la décision ;
   seul l'ajout de cette décision peut être restauré, et les erreurs de
   restauration restent visibles.
7. `:defer` et `:revalidate` traitent les erreurs ; `:flush` refuse l'envoi et
   conserve la file tant qu'aucun exécuteur métier n'est branché.

Le test avec un dossier occupant `outbox.json` force un échec même sous un runner
privilégié. Les tests couvrent aussi corruption, recharge, absence de temporaire
résiduel et échec simulé de synchronisation après publication.

### Risque résiduel important

Le snapshot JSON et le journal redb restent deux ressources distinctes. Le lot
n'en fait pas une transaction atomique commune. Un arrêt entre insertion durable
et audit peut laisser une action en file sans la trace attendue ; un futur
exécuteur devra réconcilier ces états avant tout effet. Le refus actuel de
`:flush` évite de transformer cette file en promesse d'exécution automatique.
La synchronisation du dossier est Unix ; le contrat Windows ne promet pas un
équivalent qui n'a pas été exécuté. La garantie de remplacement atomique concerne
le parcours du démon ; `hakille-sync::Outbox::save_to_file` reste une API distincte
avec écriture directe, à ne pas présenter comme équivalente.

## 4. Effets hôtes : arrêter les réussites synthétiques

La matrice finale détaillée est dans [SUBSTRATES_EFFECTS.md](SUBSTRATES_EFFECTS.md).
Les contrôles Biscuit restent prioritaires : un appel sans droit retourne un refus
d'autorisation, même lorsque le backend matériel manque.

| Opération | Après correction |
| --- | --- |
| Capture audio et lecture via les six substrats | Refus explicite tant qu'aucun flux natif n'est géré ; plus de handle obtenu par simple compteur ni de succès sans son joué. |
| Capture écran Linux/macOS/mobile/Web/Redox | Refus du binding absent ; plus de pixels synthétiques présentés par l'API de perception hôte. |
| Injection macOS/mobile/Web/Redox | Refus explicite ; plus de `Ok(())` sans événement natif. |
| Windows GDI/SendInput/MediaFoundation | Chemins natifs conservés ; validation sur Windows et permissions physiques toujours distincte. |
| Linux uinput | Chemin existant conservé ; refus du périphérique indisponible. |
| `spawn_sandboxed` des six substrats | Refus explicite sans runner raccordé, après contrôle des droits. |

Les générateurs d'images de démonstration et la validation de pixels fournis par
l'appelant restent utilisables pour les tests. Leur existence est maintenant
séparée de l'acquisition d'un écran réel. De même, `hakille-audio` possède son
propre chemin CPAL : le lot ne le désactive pas et ne prétend pas que les six
substrats pilotent déjà son cycle de vie.

**Preuve reçue : 205 tests des six crates de substrats réussissent sous Linux,
dont 21 nouvelles régressions sur les effets indisponibles après le premier lot.**
Ce total ne représente pas six campagnes matérielles natives.

## 5. Agent code : produire un patch que Git peut appliquer

L'ancienne construction utilisait `str::lines()`, qui perd la présence du saut
de ligne final et simplifie les terminaisons. Deux contenus différents pouvaient
donc produire un patch qui ne reproduisait pas exactement les octets attendus.

[`CodePatchEngine`](../agents/code/src/lib.rs) utilise maintenant des lignes
avec terminaison conservée, produit un hunk complet déterministe, ajoute
`\\ No newline at end of file` lorsque nécessaire et respecte les plages de
fichiers vides. Un contenu inchangé produit un patch vide. L'échappement des
chemins empêche un nom contenant un saut de ligne d'injecter un second en-tête.

Le choix d'un hunk complet privilégie la fidélité et une implémentation simple ;
il peut produire des patches plus volumineux qu'un algorithme de diff minimal.
Le changement ne donne pas à l'agent l'autorisation d'écrire dans un dépôt :
l'application métier reste séparée de la génération du texte de patch.

Les tests invoquent réellement `git apply` dans un dépôt temporaire et comparent
les octets : LF, CRLF, dernier saut de ligne absent, ajout/suppression du contenu,
texte UTF-8, chemins avec espaces et rejet de l'injection d'en-tête. Ce type de
preuve est plus fort qu'un test qui vérifierait seulement la présence de `@@`.

## 6. Agent calendrier : préserver les données et refuser l'ambiguïté

Le contrat métier distingue maintenant temps UTC, temps flottant, journée
entière et TZID. Les noms de fuseau sont conservés dans les événements plutôt
que supprimés au parsing. Leur conservation n'est pas une conversion correcte
vers UTC : elle ne nécessite pas de faire semblant de résoudre les changements
d'heure ou les règles historiques d'une zone.

`CalendarManager::find_conflicts` retourne `Result` et refuse les bases de temps
incompatibles ou les fuseaux non résolus. Les événements annulés et les instants sans durée ne bloquent pas
le créneau ; l'intervalle est traité comme `[début, fin)`. Les fractions de seconde
non nulles et secondes intercalaires sont refusées plutôt que tronquées.
L'export retourne aussi
`Result` pour refuser les valeurs invalides, échappe les champs TEXT et préserve
les lignes UTF-8 lors du repliement. Il émet `DTSTAMP` à l'instant réel UTC
d'export, sans prétendre conserver l'estampille du fichier source.

Ces signatures constituent une évolution d'API Rust : les appelants doivent
traiter l'erreur. Les nouveaux champs de fuseau ont une valeur par défaut pour
la désérialisation, ce qui ne supprime pas l'obligation de mettre à jour les
constructions littérales de `CalendarEvent` dans du code Rust externe.

Le parseur conserve RRULE sans expansion et refuse la planification des
événements récurrents tant qu'un moteur adapté n'existe pas. Les composants
VALARM/VTIMEZONE et les exceptions de récurrence sont refusés explicitement,
ainsi que `TRANSP:TRANSPARENT` et les méthodes iTIP hors publication.
DURATION est pris en charge pour les durées positives sans TZID ; la fin absente
d'une journée entière est le lendemain exclusif. Le gestionnaire vide par
défaut distingue désormais les données utilisateur des exemples de démonstration.

Le lot ne réalise ni synchronisation CalDAV, ni moteur complet de récurrence,
ni conversion TZID. Les propriétés non représentées dans `CalendarEvent`
(organisateur, participants ou extensions fournisseurs, notamment) ne sont pas
conservées : il ne s'agit pas d'un roundtrip complet d'un calendrier arbitraire.
Les API fallibles doivent être utilisées pour la planification. **Les 18 tests
du calendrier réussissent**, avec import du lot sans modification partielle en
cas d’erreur, recherche des journées couvertes et refus des ambiguïtés décrites
ci-dessus.

## 7. ONNX Runtime : lever le blocage sans masquer la provenance

Le refus du CDN n'était plus un refus de la politique cloud une fois le nouvel
environnement publié. Une archive officielle Microsoft ONNX Runtime **1.31.0
CPU Linux x86_64** a été provisionnée avec vérification de son SHA-256, puis
chargée réellement : l'API 31 est disponible. La procédure reproductible et les
variables de liaison sont dans [ONNX_RUNTIME_LINUX.md](ONNX_RUNTIME_LINUX.md).

Cela rend possibles la liaison et les tests complets, mais ne constitue pas
leur résultat. Il faut distinguer :

- le premier contrôle global des types réussi avec `ORT_SKIP_DOWNLOAD=1` ;
- le chargement réel de la bibliothèque officielle ;
- les contrôles et tests ultérieurs avec la bibliothèque activée ;
- l'inférence réelle, qui exige encore les modèles et leurs checksums.

Aucune archive Linux ne doit être réutilisée comme dépendance native Windows,
macOS, ARM, mobile ou Web. Aucun changement d'environnement n'est publié par
cette préparation locale.

## 8. Revue et conditions de livraison

La revue de ce deuxième lot est une lecture des diffs et des tests, pas un audit
cryptographique externe. Les résultats chiffrés ne doivent être ajoutés qu'après
la fin du runner et en excluant les sous-suites déjà comptées ailleurs.

Conditions de sortie du lot :

- Formatage, compilation et tests pertinents passent avec le lockfile final.
- Les migrations incompatibles sont explicites : fichier de clés v1 refusé,
  signatures calendrier fallibles et erreurs d'effets désormais propagées.
- Une erreur ne génère ni seed de remplacement silencieux, ni file vide
  silencieuse, ni accusé de transmission, ni mesure synthétique prise pour réelle.
- Les tests de conformance continuent à distinguer absence d'artefact, refus
  attendu et effet matériel réellement exercé.
- Les limites natives, le repli brut du démon, la transaction audit/outbox et
  l'absence de connecteurs métier restent visibles dans la documentation.

Le résultat attendu est un socle plus fiable et des fonctions métier réellement
améliorées, avec un périmètre de preuve identifiable. Une implémentation intégrale
de chaque pilote, application mobile, connecteur et modèle nécessitera les lots
spécifiques et les validations natives de la feuille de route initiale.

## 9. Dépendances et preuves d'intégration

Une première exécution normale de `cargo test --workspace --locked`, avec ONNX
Runtime officiel activé, a terminé avec **1 081 tests réussis, aucun échec et
12 tests ignorés**. Ce résultat précède la mise à jour de sécurité des dépendances
décrite ci-dessous : il ne doit pas être présenté comme celui du lockfile final.
Les modèles n'étant pas provisionnés, il ne prouve pas les scénarios d'inférence
réelle dépendant d'artefacts, même lorsqu'un test rapide termine correctement.

Le gate `cargo deny check bans sources advisories` a ensuite révélé dix alertes
nécessitant des versions corrigées. Le lockfile et les pins Candle ont été mis à
jour sans ajouter de nouvelle exception de sécurité :

| Dépendance | Version corrigée retenue |
| --- | --- |
| Tokio | 1.53.3 |
| Wasmtime / WASI et composants associés | 48.0.8 |
| Cranelift associé | 0.135.8 |
| zstd / zstd-safe / zstd-sys | 0.13.4 / 7.4.0 / 2.1.2+zstd.1.5.7 |
| Candle core / nn / transformers | 0.11.1, versions alignées |
| zip via Candle | 9.3.0 |

**Résultat confirmé : `advisories ok, bans ok, sources ok`**, sans exception
ajoutée. Les avertissements de politique préexistants restent distincts d'un
échec du gate. Ce succès est limité à la base d'avis et au graphe de dépendances
contrôlés au moment de cette exécution ; il ne certifie pas l'absence de toute
vulnérabilité.

### Validation avec le lockfile corrigé

La nouvelle suite complète `cargo test --workspace --locked` a terminé avec
**1 092 tests réussis, aucun échec et 12 tests ignorés**. Le compte a été vérifié
à partir des résultats des runners. Les 18 tests calendrier et les sous-suites
décrites précédemment sont inclus lorsqu'elles appartiennent à cette exécution ;
ils ne s'additionnent pas une seconde fois au total.

| Contrôle final | Résultat et périmètre |
| --- | --- |
| `cargo check --workspace --all-targets --locked` | Réussi, avec ONNX Runtime officiel activé. |
| Clippy workspace sans `-D warnings` | Réussi ; des avertissements préexistants restent visibles. |
| `cargo build --workspace --locked` | Réussi, liaison des binaires incluse. |
| `cargo fmt --all -- --check` et `xtask tokens --check` | Réussis ; tokens générés à jour. |
| Tests workspace avec dépendances corrigées | 1 092 réussis, 0 échec, 12 ignorés. |
| `xtask conformance` | Réussi : 66 tests rapides, 7 tests lents ignorés, déjà inclus dans le total workspace. |
| Tests ciblés Silero VAD avec `--nocapture` | 8 réussis avec le modèle réellement chargé, aucun saut pour artefact absent. |
| `cargo deny check bans sources advisories` | Trois contrôles réussis, sans ajout d'exception. |
| Compilation croisée IPC et keystore, lockfile final | Réussie pour Windows GNU x86_64 et macOS x86_64 ; aucune exécution native de ces OS n'est revendiquée. |
| Scénario réel du démon Linux | Réussi : démarrage Linux, exécution WASM de `echo-agent`, mémoire et outbox retrouvées après redémarrage, `:flush` refuse un envoi fictif. |

Les traces de validation de cette session sont dans
`/tmp/hakille-workspace-secured-tests.log`,
`/tmp/hakille-final2-check.log`, `/tmp/hakille-final2-clippy.log`,
`/tmp/hakille-final2-build.log` et `/tmp/hakille-deny-secured.log`.
Le scénario du démon a été exécuté par
`/workspace/.hakille-tools/smoke-daemon.py` ; ses journaux de premier démarrage
et de redémarrage se trouvent dans `/tmp/hakille-daemon-smoke-9jm7ewzt`.
Ces chemins sont des artefacts locaux de vérification, pas des résultats de CI
distante ni une preuve de restauration dans une autre machine.

Le modèle Silero a été téléchargé, son checksum du catalogue vérifié
(`1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3`), et il
était présent pour la suite finale. La commande ciblée
`cargo test -p hakille-inference ort_vad::real_model_tests::golden --locked -- --nocapture`
a exécuté **8 tests réussis**, avec chargement du graphe réel et sans saut pour
modèle absent. Le journal `/tmp/hakille-final-vad.log` confirme les entrées
`input/state/sr` et sorties `output/stateN`, le rejet du silence et des tons,
la détection du signal vocal de test et la conservation des échantillons.
La latence observée par bloc CPU est p50 = 0,163 ms et p95 = 1,596 ms dans ce
runner ; elle ne constitue pas une garantie sur un autre appareil ni une
mesure de la chaîne microphone/transcription/synthèse complète.

Qwen et les autres modèles ne sont pas provisionnés. Les 12 tests lents ignorés
n'ont pas été exécutés, conformément à leurs prérequis et aux consignes
`AGENTS.md`. Certains tests rapides d'autres moteurs peuvent signaler un saut
pour artefact absent : le total workspace ne prouve pas leur inférence réelle.
Les commandes distinctes `xtask tokens --check` et `xtask conformance` ont
également réussi ; journaux `/tmp/hakille-final2-tokens.log` et
`/tmp/hakille-final2-conformance.log`.

Après deux nettoyages de lints sans changement de comportement (condition de
rollback et imports de tests conditionnels Windows), la revalidation ciblée
du démon et du substrat Windows sous Linux a réussi : **35 + 19 tests**,
aucun échec. Le formatage et Clippy workspace ont été revérifiés avec succès ;
preuves `/tmp/hakille-final3-fmt.log`, `/tmp/hakille-final3-touched-tests.log`
et `/tmp/hakille-final3-clippy.log`. Ces tests répétés ne s'ajoutent pas au
total workspace. Aucun commit, push ni publication d'environnement n'a été
effectué.
