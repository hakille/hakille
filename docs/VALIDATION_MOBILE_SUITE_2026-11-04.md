# Mobile — seconde itération du 4 novembre 2026

Cette itération poursuit la [première livraison](VALIDATION_MOBILE_2026-11-04.md)
avec six agents répartis entre Android, iOS, cœur Rust, passerelle, synchronisation
et validation native. Le [guide mobile](../mobile/README.md) décrit la construction
et la connexion. Ce rapport décrit les mesures locales avant l'envoi du code sur
GitHub ; les résultats CI doivent être consultés séparément dans la
[pull request](https://github.com/hakille/hakille/pull/7). Aucun environnement
ni magasin d'applications n'a été publié.

## Changements livrés

### Dictée native, explicite et locale

Android utilise exclusivement `createOnDeviceSpeechRecognizer`, à partir
d'Android 12/API 31 avec un moteur local installé. Le reste de l'application
conserve son minimum Android 8/API 26. iOS utilise Speech/AVAudioEngine avec
`requiresOnDeviceRecognition = true` et vérifie la disponibilité du moteur
pour la langue choisie. Un moteur ou une langue indisponible produit un message
explicite. Aucun repli vers une reconnaissance distante n'est ajouté.

Le bouton de dictée demande les permissions, lance une session bornée et ajoute
la transcription au brouillon. Il n'envoie pas de message. Annulation, interruption,
changement de connexion et callbacks périmés sont traités ; une modification
manuelle du brouillon empêche son remplacement par une ancienne transcription.
La capture est limitée à 30 secondes. Android borne la session complète ; iOS
accorde ensuite au plus huit secondes à la finalisation locale.

Les guides [Android](../mobile/android/DICTATION.md) et
[iOS](../mobile/ios/DICTATION_AND_RECOVERY.md) détaillent les états et limites.
Les libellés existent en français et anglais ; les langues de reconnaissance
dépendent du moteur installé. Dire « Hakille » ne constitue pas un réveil vocal
permanent sur téléphone. Les modèles neuronaux Hakille restent sur le hub.

### Reprise durable sans répétition automatique

Le cœur exporte un checkpoint v2 : brouillon, compteur d'identifiants, marqueurs
des mutations en cours ou inconnues et avertissements déjà pris en compte.
Il n'y stocke ni jeton, ni session, ni commande à rejouer. Les anciennes sauvegardes
v1 restent lisibles. Une restauration ne déclenche aucun réseau.

Les hôtes natifs publient atomiquement le checkpoint et synchronisent le fichier
et son répertoire **avant l'envoi HTTP**. Les écritures sont ordonnées hors du
thread d'interface ; une sauvegarde impossible bloque l'envoi. Un échec certain
de préparation avant émission est distingué d'un résultat incertain après émission. Après
redémarrage, les mutations interrompues restent inconnues. Leur prise en compte
explicite débloque les nouvelles mutations sans inventer un succès ou un rollback.
Une réponse de statut ne résout pas l'incertitude historique.

La sauvegarde des frappes est asynchrone : une mort immédiate du processus peut
perdre les dernières frappes non encore enregistrées. La barrière avant émission
des requêtes reste obligatoire. Un fichier corrompu est signalé et conservé,
sans écrasement silencieux.

### Passerelle et synchronisation

Les tests de passerelle relient le vrai cœur mobile à un serveur HTTP réel et
à un noyau IPC contrôlé : cartes, confirmation, expiration, redémarrage, rotation
de jeton, session périmée et absence de rejeu. Le chemin du jeton est mieux
protégé contre les alias de fichier sous Windows. Un exemple `mobile_probe`
permet de lire le statut d'une passerelle explicitement sélectionnée sans
démarrer de service ni soumettre de mutation.

Le protocole de synchronisation passe à **v2** : clé de pair préalablement
approuvée, signature canonique stricte, destinataire et durée contrôlés, cache
anti-rejeu borné exportable. La fusion converge lors d'égalités de dates ; les
suppressions sont conservées plutôt que purgées sur leur seul âge. La réplication
transmet un état complet borné, les anciens filtres pouvant perdre des mises à jour.

Cette évolution rompt la compatibilité des messages sync v1, désormais refusés,
et certaines API Rust retournent `Result`. Elle ne modifie pas le protocole UI/IPC v1.
Le [contrat sync](../crates/hakille-sync/README.md) précise les limites : fusion LWW
avec perte possible des versions concurrentes, croissance des tombstones,
authentification du transport et confiance des données importées à gérer par l'hôte.
Aucun transport sync de production, appairage automatique ou réplication TiKV
n'est annoncé opérationnel.

## Validation

| Contrôle | Résultat et portée |
|---|---|
| `cargo fmt --all -- --check` | Réussi |
| `cargo run -p xtask -- tokens --check` | Réussi ; vérifie aussi les exports Kotlin/Swift réellement inclus dans les apps |
| `cargo check --workspace --all-targets --locked` | Réussi |
| `cargo clippy --workspace --all-targets --no-deps --locked` | Réussi ; avertissements historiques conservés |
| `cargo test --workspace --locked --no-fail-fast` | **1 296 réussis**, zéro échec, **22 ignorés** |
| `cargo run -p xtask -- conformance` | **66 réussis**, zéro échec, 7 ignorés |
| `cargo deny check bans sources advisories` | Les trois contrôles passent |
| Cœur mobile | 30 tests dans le workspace ; **30 réussis également avec FFI activée** |
| Passerelle, cible `--all-targets` | **17 réussis** : 15 HTTP/IPC, un contrôle de chemin et un contrôle du probe |
| Substrat mobile et sync | Respectivement **51** et **30** tests réussis dans le workspace |
| Android Rust + UniFFI | Reconstruction ARM64 et x86_64 réussie avec alignement 16 Kio |
| Android Gradle | **19 tests JVM réussis**, zéro échec/ignoré ; `lintDebug` et `assembleDebug` réussis |
| APK final | Signature, alignement ZIP et segments ELF des six bibliothèques, politique TLS vérifiés |
| Rust iOS ARM64, `--features ffi` | `cargo check` réussi ; cela ne compile ni ne lie Swift/Xcode |
| Bibliothèque gateway Windows GNU x86_64 | `cargo check --lib` réussi ; aucun test d'exécution Windows réalisé ici |

Les suites workspace et conformance ont utilisé `RUST_TEST_THREADS=1` pendant
les essais d'émulation : mêmes tests et budgets, exécution séquentielle. Elles
ont réussi dès cette exécution, sans désactivation de test ni changement de
budget. Les tests d'inférence lente ignorés n'ont pas été lancés en debug.
Les 19 tests JVM couvrent la dictée (10), la barrière de sauvegarde (4), le
transport (4) et la présentation des cartes (1). Le lint rapporte zéro erreur
et 11 avertissements, sans baseline masquant des erreurs.

Outils : Rust 1.95, UniFFI 0.32.2, JDK Temurin 21.0.13, SDK/build-tools Android
35/35.0.0, NDK 27.2.12479018, Gradle 8.13, AGP 8.9.2 et Kotlin 2.1.20.

### APK courant

`mobile/android/app/build/outputs/apk/debug/app-debug.apk`, **17 793 621 octets**.
Empreinte SHA-256 :

```text
f47edaef0ff415fe2d1f71deef75aa23b311f76ec16433d85921e41665b50989
```

Le rapport machine est `target/mobile-validation/android/apk-verification.json`.
Les bibliothèques Rust, JNA et AndroidX sont présentes pour les deux ABI et leurs
segments LOAD sont alignés sur 16 Kio. Le manifeste empaqueté désactive les
sauvegardes Android et impose HTTPS hors de l'exception explicite `127.0.0.1`.
Ces inspections n'établissent pas à elles seules le fonctionnement sur un noyau
Android utilisant des pages de 16 Kio.

### Exécution Android

**Preuve partielle, parcours complet non validé.** L'APK courant a été installé et
lancé sur Android API 35, x86_64, pages noyau de 4 Kio. Le premier passage a validé
`native_bridge_loaded` : le véritable bridge JNA/UniFFI/Rust a initialisé l'interface.
La capture et le XML montrent le champ de saisie et l'indisponibilité explicite du
moteur vocal AOSP. La capture a aussi été inspectée visuellement. Aucun test de
reconnaissance vocale ou de connexion réseau n'est revendiqué.

L'environnement ne dispose pas de `/dev/kvm`. L'émulateur officiel 37.2.13 utilise
l'émulation CPU logicielle et SwiftShader. Les archives officielles de l'émulateur
et de l'image AOSP 35 ont été vérifiées contre les sommes de contrôle du fournisseur.
Le premier démarrage a duré 471,1 s ; le redémarrage ayant servi aux essais a pris
275,7 s. Ces durées décrivent l'environnement de test, pas la latence de Hakille.

- Premier passage : chargement natif réussi, puis échéance ADB de 45 s dépassée
  pendant la saisie du texte de test. Résultat global **échoué**, conservé sous
  `target/mobile-validation/android/runtime-attempt1/` avec capture, XML et journal.
- Deuxième passage : délais multipliés par quatre et réutilisation de l'APK installé
  seulement après vérification de son SHA-256. Le système affiche « Process system
  isn't responding », confirmé par le XML du package Android. Un clic explicite
  sur « Wait », la réduction de résolution à 540×960/densité 240 et l'arrêt du
  Bluetooth du seul émulateur n'ont pas permis de terminer le parcours. Le dernier
  essai a expiré avec une erreur de lecture XML. Résultat global **échoué**, sous
  `target/mobile-validation/android/runtime-final/`, avec les réglages et le dialogue
  système conservés. Le dernier dump brut montre encore ce dialogue système.

Les journaux collectés ne montrent pas de crash Hakille ; ils contiennent des
crashs du service Bluetooth Android. Cela ne transforme pas les échecs du parcours
en succès. **Saisie automatisée, rotation, retour d'arrière-plan, restauration après
arrêt forcé et écran de connexion ne sont donc pas validés par ce smoke.**

Le script conserve désormais le dernier XML et les erreurs de chaque tentative,
refuse de masquer la mort du processus et offre des délais bornés configurables.
La CI Android exige KVM et garde les mêmes assertions. Aucun résultat de ce
workflow sur GitHub n'était disponible au moment des mesures locales ; consulter
les contrôles de la pull request pour son état actuel. Le script macOS exige
également de vrais résultats XCTest,
plutôt qu'une simple compilation des tests.

## Limites de la preuve

- Xcode n'est pas disponible sur cette machine Linux : la compilation Swift,
  les XCTest et l'exécution iOS ne sont pas attestés. Les sources ont été relues,
  leur syntaxe analysée, le projet et les traductions contrôlés. Le workflow macOS
  exécute désormais réellement XCTest et exige un résultat non vide ; il n'a pas
  été lancé sur GitHub pour ces changements locaux.
- Aucun microphone physique ni moteur ASR mobile n'a été validé ici. Les tests
  de politique avec moteurs injectés ne prouvent pas une transcription réelle.
- Le paquet est signé pour le développement. Les essais sur appareils physiques,
  TalkBack/VoiceOver, batterie, verrouillage et réseau intermittent restent à mener.
- L'essai Qwen réel décrit dans le premier rapport n'a pas été répété : ces
  changements n'ajoutent pas de moteur neuronal embarqué au téléphone.
- Les sept modèles ne sont pas déclarés tous opérationnels sur toutes les plateformes.
  La disponibilité réelle des artefacts, pilotes et accélérateurs demeure déterminante.
