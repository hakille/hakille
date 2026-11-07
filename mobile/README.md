# Applications natives Hakille

`android/` utilise Kotlin et Jetpack Compose ; `ios/` utilise Swift et SwiftUI.
Les deux applications appellent le même cœur Rust
[`hakille-mobile`](../crates/hakille-mobile) par UniFFI. Elles ne contiennent
pas de WebView et ne chargent aucun modèle Hakille sur le téléphone. La
reconnaissance et la synthèse vocales optionnelles utilisent les moteurs locaux
fournis par le système mobile.

Les résultats mesurés et les limites sont consignés dans le
[rapport de validation de la seconde itération](../docs/VALIDATION_MOBILE_SUITE_2026-11-04.md).

La navigation propose **Voix, Mémoire, Activité et Hub**. Le thème sombre par
défaut, les variantes clair/système, le logo officiel et la police Inter
reprennent les [principes du design mobile](../docs/DESIGN_MOBILE_2026-11-04.md).
L’écran Voix privilégie la parole ; le clavier et les détails restent accessibles.

Les écrans exposent la conversation, le statut du hub, la recherche et
l'enregistrement de mémoire, le journal, les agents réellement disponibles
et les cartes d'autorisation. Les données proviennent du hub choisi : un
écran vide indique qu'aucune donnée n'a encore été reçue. Les résultats
`confirmed`, `failed` et `unknown` restent ceux du cœur ; une mise en file
d'action n'est pas présentée comme une exécution métier.

Dans **Voix**, le mode **Dictée** demande les permissions au lancement
(**Dicter dans le brouillon** sur Android, **Parler** sur iOS). Android exige
Android 12/API 31 et un moteur de reconnaissance
sur l'appareil disponible ; les versions API 26–30 restent utilisables au
clavier. Sur iOS 16 ou supérieur, la langue choisie doit prendre en charge
`supportsOnDeviceRecognition` et la requête impose
`requiresOnDeviceRecognition`. L'absence de moteur ou de ressource linguistique
produit une indisponibilité explicite, sans repli vers un service cloud.

Seul le résultat final rejoint le brouillon ; il faut ensuite appuyer sur
**Envoyer**. La dictée ne soumet aucune commande et n'approuve aucune carte.
La capture est limitée à 30 secondes et s'arrête notamment au passage en
arrière-plan ou à la déconnexion. Sur Android, une boîte de permission qui
met l'activité en pause invalide le démarrage : un nouvel appui est nécessaire.
Sur iOS, cette inactivité de permission est distinguée de l'arrière-plan ;
la capture autorisée attend le retour actif avant de démarrer. Il n'y a ni
réveil permanent « Hakille »,
ni Whisper embarqué. Le champ `microphone_enabled` du cœur Rust reste distinct
de l'état de cette capture native. Les contraintes et essais manuels sont
décrits dans les guides [Android](android/DICTATION.md) et
[iOS](ios/DICTATION_AND_RECOVERY.md). La qualité et le fonctionnement hors ligne
des moteurs installés restent à vérifier sur les appareils ciblés.

## Conversation vocale et fonctions du téléphone

Le mode **Conversation vocale** est une option distincte de la dictée du
brouillon. Après activation et connexion au hub, chaque pression lance un
tour : reconnaissance locale → transcription finale envoyée au hub → lecture
locale de sa réponse. Le cœur Rust attribue un contexte unique, expire les
contextes non soumis et ne permet de lire que la réponse finale correspondante.
Une carte de décision suspend cette lecture ; elle exige toujours sa propre
approbation. Le microphone ne redémarre pas automatiquement après la réponse.
Une navigation, une interruption ou un passage en arrière-plan invalide le tour.
Le hub reçoit le texte reconnu, pas l’enregistrement audio. Les voix/langues
absentes ou réponses trop longues produisent un diagnostic ; le texte reste
accessible. Ce mode exige un brouillon vide et aucune opération incertaine.

Dans **Hub → Ce téléphone**, les permissions sont demandées à l’action :

| Fonction | Android | iOS |
| --- | --- | --- |
| Caméra | Miniature renvoyée par l’application caméra, conservée en mémoire | Capture native, aperçu de 1 536 pixels au plus sur le côté long, JPEG ≤ 8 Mio, en mémoire |
| Fichier choisi | Sélecteur SAF, lecture ≤ 1 Mio, aperçu texte ≤ 4 Kio | Sélecteur système, fichier régulier ≤ 8 Mio, aperçu ≤ 4 000 caractères |
| Localisation | Mesure ponctuelle, âge et précision affichés | Mesure ponctuelle, âge et précision affichés ; autorisation approximative acceptée et signalée |
| Ouvrir une application | Réglages, Horloge, Calendrier, Cartes, documentation | Réglages Hakille, Calendrier, Cartes, documentation |

Les aperçus et coordonnées restent locaux, sans ajout automatique au brouillon
ni envoi au hub. Sur Android, quitter le panneau ou mettre l'activité en pause
efface les aperçus ; une caméra ou un sélecteur externe explicitement ouvert
peut encore répondre après cette pause. Sur iOS, les aperçus restent en mémoire
en changeant d'onglet ; **Effacer les aperçus locaux** ou le passage en
arrière-plan les retire. Aucun des deux hôtes ne crée de copie durable de ces
aperçus. Les ouvertures exigent confirmation et utilisent une liste fixe ;
la documentation ouvre
GitHub dans le navigateur. Les applications externes conservent leurs propres
permissions et règles de stockage. Le texte d’un modèle ne peut pas lancer
une URL, lire un fichier ou commander une autre application. Ces accès passent
par les API autorisées par Android/iOS : ils ne donnent pas un contrôle général
du téléphone. Les lectures de fichiers vérifient les octets reçus et rejettent
les résultats périmés. Sur Android API 26–29, seuls les descripteurs de fichiers
ordinaires sont acceptés ; les pipes et sockets de fournisseurs sont refusés.
Leur annulation et leur échéance de 10 secondes ne
peuvent pas garantir l'arrêt immédiat d'un fournisseur ou d'un appel noyau
bloqué ; le contrôle iOS est coopératif entre les blocs lus. Voir le
[contrat Android détaillé](android/VOICE_AND_PHONE.md).

## Construire Android

Prérequis : JDK complet 21, Android SDK plateforme 35, build-tools 35.0.0,
NDK **27.2.12479018**, toolchain Rust du dépôt et accès aux dépendances.
`ANDROID_HOME` ou `ANDROID_SDK_ROOT` doit pointer vers le SDK. Le NDK est
recherché dans son sous-dossier `ndk/27.2.12479018`. Consulter le `--help`
du script Rust pour les options.

Depuis la racine :

```bash
rustup target add aarch64-linux-android x86_64-linux-android
scripts/build-mobile-rust.sh android
cd mobile/android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

Le script produit les bindings Kotlin et les bibliothèques Rust pour
`arm64-v8a` et `x86_64`. Ne pas remplacer ces bibliothèques par des stubs pour
obtenir un APK. Gradle 8.13 est épinglé avec vérification SHA-256 de sa
distribution ; AGP 8.9.2 et Kotlin/Compose 2.1.20 sont définis dans le projet.
Le wrapper JAR 8.13 est issu du dépôt officiel Gradle, SHA-256 :
`81a82aaea5abcc8ff68b3dfcb58b3c3c429378efd98e7433460610fecd7ae45f`.

L'APK de développement est
`mobile/android/app/build/outputs/apk/debug/app-debug.apk`. Il cible Android
API 26 ou supérieur et n'est pas une publication sur un magasin. Les
bibliothèques générées, caches et APK restent hors Git.

Depuis la racine, inspecter l'APK puis exécuter le parcours sur un **émulateur
dédié** déjà démarré :

```bash
python3 scripts/verify-android-apk.py mobile/android/app/build/outputs/apk/debug/app-debug.apk \
  --report target/mobile-validation/android/apk-report.json
python3 scripts/test-android-runtime.py mobile/android/app/build/outputs/apk/debug/app-debug.apk \
  --serial emulator-5554
```

L'inspection vérifie notamment la signature, l'alignement ZIP/ELF des
bibliothèques natives et la politique réseau empaquetée. Le test runtime
**efface les données Hakille de l'émulateur** ; il vérifie le chargement natif,
le brouillon, la rotation, le retour d'arrière-plan et la restauration après
arrêt du processus. Il ne demande aucune permission vocale, ne se connecte
pas à un hub et n'envoie aucun message. Ses résultats et captures sont placés
dans `target/mobile-validation/android/runtime/`.

## Construire iOS sur Mac

Prérequis : macOS, Xcode avec SDK iOS et simulateur, toolchain Rust du dépôt,
et [XcodeGen](https://github.com/yonaskolb/XcodeGen). Le projet cible iOS 16
ou supérieur. Depuis la racine :

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-mobile-rust.sh ios
cd mobile/ios
xcodegen generate
xcodebuild -project Hakille.xcodeproj -scheme Hakille \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath build CODE_SIGNING_ALLOWED=NO build-for-testing
```

Le script génère `Frameworks/HakilleMobile.xcframework` (bibliothèques
statiques appareil et simulateur), les headers/modulemap et
`Generated/HakilleMobile.swift`. Le module C s'appelle `HakilleMobileFFI`.
Pour un simulateur Intel, ajouter `--ios-x86_64-sim` au script Rust. Les
options `--release` et `--offline` sont décrites par le script.

Pour exécuter les tests, revenir à la racine après la génération du projet :

```bash
scripts/test-ios-simulator.sh
```

Ce script choisit un iPhone simulateur disponible, compile puis exécute les
XCTest et exige des tests réellement réussis ; il conserve le rapport
`.xcresult` dans `target/mobile-validation/ios/`. Il nécessite macOS/Xcode.
Les exécutions CI macOS sont consignées dans les rapports de validation.
Une installation sur iPhone/iPad demande une équipe de signature Xcode et un
appareil disponible. Les sources et une compilation CI ne constituent pas
une validation sur appareil, avec microphone ou avec VoiceOver.

## Connecter son hub

Démarrer le démon IPC et la
[passerelle mobile](../crates/hakille-mobile-gateway/README.md) selon son guide.
La passerelle écoute par défaut `127.0.0.1:17840` et utilise un jeton mobile
distinct du jeton IPC du démon. Elle conserve une seule session IPC : fermer
la connexion du client desktop pendant l'utilisation de cette passerelle.

Pour un appareil Android ou un émulateur relié par ADB au poste du hub :

```bash
adb reverse tcp:17840 tcp:17840
adb install -r mobile/android/app/build/outputs/apk/debug/app-debug.apk
```

Dans **Hub → Connexion**, saisir `http://127.0.0.1:17840` et le jeton mobile,
puis **Connecter et enregistrer de façon sécurisée**. Le transport ADB doit
être mis en place explicitement ; `10.0.2.2` et `localhost` ne sont pas des
exceptions HTTP acceptées. Sur téléphone distant, employer une URL **HTTPS**
avec certificat valide et reverse proxy/tunnel explicitement configuré.
L'application ne désactive pas la validation TLS et ne suit pas les
redirections.

L'URL est une base sans chemin, identifiant, mot de passe, paramètres ni
fragment. Le client ajoute `/v1/request`. Le jeton et cette URL sont
conservés ensemble : Android les chiffre via une clé Android Keystore,
iOS les stocke dans le Keychain avec l'attribut `ThisDeviceOnly`, sans
synchronisation iCloud. Les snapshots Rust et journaux de l'application ne
contiennent pas ce jeton. Une nouvelle URL efface la saisie du jeton ; la
reconnexion enregistrée réutilise seulement la paire d'origine.

**Déconnecter et effacer les identifiants** annule les échanges locaux,
réinitialise les vues et supprime les identifiants enregistrés. Une réponse
arrivée d'une ancienne connexion ne peut pas repeupler la session actuelle.
La connexion enregistrée n'est jamais utilisée automatiquement au lancement.
La déconnexion ne supprime pas les avertissements sur des opérations dont
l'issue reste inconnue.

## Brouillon durable et reprise après interruption

Le checkpoint local **v2** contient le brouillon, le prochain identifiant de
requête et des marqueurs bornés d'opérations en cours, incertaines ou déjà
prises en compte. Il ne contient ni jeton, ni session, ni carte, ni charge
utile à rejouer. Le brouillon peut contenir du texte personnel : Android le
conserve dans son stockage privé sans sauvegarde système ; iOS utilise un
fichier protégé, exclu de la sauvegarde cloud. Les secrets restent dans leurs
trousseaux distincts.

Les hôtes attendent l'écriture atomique et synchronisée du checkpoint **avant
tout envoi HTTP**. Un échec de stockage avant émission rend la demande
« non envoyée ». Après le début du transport, une interruption laisse une
issue potentiellement inconnue. Au redémarrage, les marqueurs en cours sont
restaurés en `unknown`, sans requête en attente ni relance automatique ; un
checkpoint corrompu est refusé et conservé pour diagnostic.

Les consultations du statut et du journal restent possibles pour vérifier
ce qui s'est passé sur le hub. Les nouvelles mutations restent bloquées tant
que les avertissements n'ont pas été explicitement pris en compte. Cette
action locale archive l'avertissement : **elle ne confirme pas la réussite,
n'annule aucun effet et ne rejoue rien**. Les archives sont bornées et ne
remplacent pas le journal du hub. Les marqueurs ne sont pas liés à une URL :
après changement de hub, vérifier aussi le journal du hub concerné.

Le format v1 reste lisible, sans pouvoir reconstruire les anciennes opérations
jamais enregistrées. La dernière frappe peut être perdue si le processus est
tué avant sa sauvegarde asynchrone ; le transport, lui, attend son checkpoint.
Voir le [contrat Rust de persistance](../crates/hakille-mobile/README.md).

## Autorisations, interruption et accessibilité

Chaque carte affiche le motif, le coût et sa devise, la destination des
données, la provenance et la réversibilité. Au toucher, maintenir le bouton
pendant au moins 600 ms puis relâcher. Le cœur Rust mesure cette durée avec
une horloge monotone ; l'application ne transmet pas une durée déclarative.
Le refus est une demande explicite adressée au hub.

Pour les technologies d'assistance, l'action accessible utilise **deux
activations explicites** : commencer la confirmation, puis confirmer après
600 ms. Cette alternative est annoncée comme deux étapes. Elle ne prétend
pas être un maintien physique. Changer de carte, annuler le geste ou placer
l'application en arrière-plan annule la confirmation.

Le passage en arrière-plan et le bouton d'annulation arrêtent le transport
local. Ils ne prouvent pas l'arrêt du calcul ou l'annulation d'un effet sur
le hub. Une requête ambiguë reste `unknown`, sans relance automatique.
**Actualiser** dans **Statut** rétablit l'état de connexion, mais ne lève pas
à lui seul le blocage lié aux opérations incertaines décrit ci-dessus.

Les libellés existent en français et anglais. Les contrôles natifs utilisent
le sens de disposition du système ; le réglage **Disposition de droite à
gauche** permet aussi de vérifier une présentation RTL. Les tailles de texte
reprennent les tokens avec la mise à l’échelle du système, les listes défilent
et les boutons disposent de libellés accessibles. La vérification pratique avec TalkBack/VoiceOver,
grandes polices, clavier externe et interruption réseau reste nécessaire sur
les appareils visés.

## Source des couleurs et tests

Les fichiers `HakilleTokens.kt` et `HakilleTokens.swift` sont des exports des
mêmes `tokens/*.toml` que le desktop. Pour les régénérer depuis la racine :

```bash
cargo xtask tokens export-kotlin
cargo xtask tokens export-swift
cp target/tokens/HakilleTokens.kt mobile/android/app/src/main/java/hakille/tokens/HakilleTokens.kt
cp target/tokens/HakilleTokens.swift mobile/ios/Hakille/HakilleTokens.swift
cargo xtask tokens --check
```

`tokens --check` compare aussi les exports Kotlin et Swift versionnés à leur
source TOML et échoue s'ils divergent, sans les réécrire.

La braise est réservée aux cartes de décision. Les suites JVM/XCTest couvrent
notamment les frontières d'URL, les identifiants UInt64 et la présentation
des montants. Les tests JVM de transport utilisent des serveurs HTTP locaux
explicitement factices pour vérifier refus de redirection, absence de fuite
du jeton et lecture bornée à 1 Mio. Ils ne démontrent pas une connexion à un
hub réel ni le comportement d'un appareil physique.

## Synchronisation entre appareils : périmètre

La bibliothèque [`hakille-sync`](../crates/hakille-sync/README.md) exige,
pour le handoff signé **v2**, une paire identité d'appareil/clé publique
explicitement approuvée par un canal de confiance. Le jeton HTTP du hub ne
constitue pas cet appairage. La bibliothèque ne fournit ni découverte,
transport réseau, ni écran d'appairage intégré aux applications.

Sa fusion CRDT choisit une version déterministe par identifiant et conserve
les tombstones de suppression ; elle ne conserve pas toutes les variantes
d'un conflit. Les échanges actuels portent l'état complet, et l'application
au graphe ne propage pas encore les suppressions. Le cache antirejeu peut
être exporté/restauré, mais sa persistance protégée avant utilisation reste
une responsabilité de l'hôte à raccorder. Ces primitives ne démontrent pas
une synchronisation mobile réseau déployée.
