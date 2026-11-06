# Livraison mobile — validation du 4 novembre 2026

> Rapport historique de la première livraison. Les mesures et l'empreinte
> ci-dessous concernent ce premier APK. La [seconde itération](VALIDATION_MOBILE_SUITE_2026-11-04.md)
> ajoute la dictée native locale, la reprise durable et de nouvelles validations ;
> elle indique le paquet courant et les preuves obtenues depuis.

Cette livraison ajoute des clients natifs Android/Compose et iOS/SwiftUI, un
cœur Rust partagé par UniFFI et une passerelle vers le hub personnel. Elle
corrige aussi le stockage, les capacités annoncées et la synchronisation du
substrat mobile. L'[audit initial](ANALYSE_MOBILE_2026-11-04.md) décrit les
problèmes de départ ; [ADR-011](../adr/011-mobile-native-client-and-hub.md)
fixe le périmètre architectural et le [guide mobile](../mobile/README.md)
donne les commandes de compilation et de connexion.

## Fonctionnalités et frontières

- Conversation, statut, recherche/enregistrement de mémoire, journal, liste
  et exécution des agents disponibles, consultation et décision des cartes.
- Brouillons locaux bornés, états de connexion explicites, résultats confirmés,
  échoués ou incertains ; aucune relance automatique des mutations.
- Secrets de connexion liés à leur URL : chiffrement Android Keystore et
  stockage iOS Keychain. HTTPS obligatoire hors `127.0.0.1`, redirections
  refusées et requêtes/réponses bornées. Les snapshots Rust ne portent pas de jeton.
- Interfaces en français/anglais, disposition RTL, cartes avec coût/devise,
  destination des données et provenance lisibles. Le maintien de confirmation
  est contrôlé par Rust ; une alternative accessible à deux activations existe.
- Substrat avec conteneur de fichiers réel, vérifications de capacités au point
  d'effet, observations matérielles inconnues sans pilote, budgets conservateurs.
- File de synchronisation conservée jusqu'à un ACK valide, sessions invalidées
  après interruption, snapshots et conflits bornés, identité locale unique.

Les modèles tournent sur le **hub**, pas dans l'application du téléphone.
Le microphone et le réveil vocal mobile ne sont pas raccordés. Les pilotes NPU,
CoreML, SAF et d'attestation matérielle, ainsi qu'APNs/FCM et la réplication TiKV,
ne sont pas annoncés opérationnels. Une approbation de carte peut mettre une
action dans l'outbox ; elle ne prouve pas une exécution métier.

## Vérifications réalisées dans cet environnement Linux

| Vérification | Résultat et portée |
|---|---|
| `cargo fmt --all -- --check` | Conforme |
| `cargo run -p xtask -- tokens --check` | Sources de tokens et génération conformes |
| `cargo check --workspace --all-targets --locked` | Réussi |
| `cargo clippy --workspace --all-targets --no-deps --locked` | Réussi ; avertissements historiques du workspace conservés |
| `cargo test --workspace --locked --no-fail-fast` | **1 271 tests réussis**, zéro échec, **22 ignorés** |
| `cargo run -p xtask -- conformance` | **66 réussis**, 7 ignorés, après relance isolée décrite ci-dessous |
| `cargo deny check bans sources advisories` | Trois contrôles réussis |
| Substrat mobile | **51 tests** inclus dans le workspace, dont les ACK, la reprise et le confinement |
| Cœur mobile avec `--features ffi` | **20 tests réussis**, Clippy toutes fonctionnalités sans avertissement |
| Passerelle HTTP/IPC | **10 tests d'intégration réussis**, Clippy ciblé sans avertissement |
| `scripts/build-mobile-rust.sh android` | Bibliothèques ARM64 et x86_64 liées, bindings Kotlin régénérés |
| Substrat Android ARM64 | `cargo check --target aarch64-linux-android` réussi |
| Cœur Rust iOS ARM64 avec FFI | `cargo check --target aarch64-apple-ios` réussi ; aucune liaison Xcode ni compilation Swift impliquée |
| Android `testDebugUnitTest` | **5 tests JVM réussis**, zéro échec/ignoré : frontières réseau et présentation des décisions |
| Android `lintDebug` et `assembleDebug` | Réussis après activation du générateur UniFFI Android, compatible API 26 |

La seconde exécution de conformance, lancée pendant la compilation native et
le décodage Qwen, a rencontré un dépassement de budget de simulation
(`50 ms > 40 ms`) dans un test existant. La suite du workspace était déjà verte.
La relance isolée de conformance a réussi en 5,40 s, sans modifier ni ignorer le
test et sans augmenter les budgets du noyau. Les tests d'inférence lente ignorés
n'ont pas été lancés en debug.

Outils : Rust 1.95, UniFFI 0.32.2, JDK Temurin 21.0.13, Android SDK/build-tools
35/35.0.0, NDK 27.2.12479018, Gradle 8.13, AGP 8.9.2, Kotlin 2.1.20.
Le SDK/NDK et le JDK ont été téléchargés depuis leurs fournisseurs officiels
et leurs empreintes vérifiées. Aucun de ces outils ou modèle n'est ajouté au dépôt.

## Paquet Android vérifié

Fichier : `mobile/android/app/build/outputs/apk/debug/app-debug.apk`
(17 995 581 octets). Empreinte SHA-256 du paquet de cette session :

```text
dfde589872bb99d70310dfcc58c41a026dcd920a5eca865b2a1ea9bdf34589bd
```

`apksigner verify` valide la signature de développement ; `zipalign -c -P 16`
valide l'alignement du paquet. Les segments ELF des **six bibliothèques**
embarquées — Rust, JNA et AndroidX, pour ARM64 et x86_64 — ont tous un alignement
d'au moins 16 Kio. Cela vérifie le paquet, pas son lancement sur un appareil.
Le lint final rapporte **zéro erreur et dix avertissements**, notamment des
versions de dépendances plus récentes, attributs ignorés sur les anciens Android
et suggestions Kotlin ; aucune erreur n'est cachée par une baseline.

## Essai de bout en bout avec le vrai noyau

Un conteneur de données temporaire indépendant, un vrai `hakille-daemon`, la
passerelle compilée et le cœur natif Linux chargé via UniFFI ont été utilisés.
Le pilote de validation est extérieur au produit ; aucun runtime Python n'est
nécessaire aux applications livrées. Les six vérifications suivantes ont réussi :

1. Statut reçu à travers **UniFFI → HTTP → IPC → démon**.
2. Mémoire contenant du français, de l'arabe et du japonais, accusée avec son
   identifiant réel, persistée et relue dans le snapshot du client.
3. Journal reçu dans le client.
4. Agent `echo-agent` présent et réellement exécuté dans Wasmtime, sortie vérifiée.
5. Réponse non vide du modèle Qwen local, répercutée dans la conversation ;
   **41,78 s** pour le tour mesuré avec une limite de 64 tokens et un budget
   d'inférence de 120 s. Le temps inclut le traitement du hub, pas seulement le modèle.
6. Redémarrage du démon et de la passerelle, puis relecture de la même mémoire
   persistée avec son identifiant initial.

La fixture a retiré uniquement son propre socket Unix après avoir attendu
l'arrêt de ses processus. Le démon refuse volontairement un socket périmé
sans nettoyage explicite ; ce comportement n'a pas été contourné dans le produit.
La durée mesurée n'est pas un engagement de latence sur téléphone ou réseau distant.

## Preuves encore nécessaires

- **iOS natif** : sources, signatures Swift/UniFFI, disponibilité des API iOS 16
  et configuration XCFramework relues ; compilation Swift/Xcode et exécution
  des quatre tests XCTest non réalisées ici, faute de macOS/Xcode.
- Le [workflow mobile](../.github/workflows/mobile.yml) ajoute la compilation
  Android et `build-for-testing` iOS. Il n'a pas été exécuté sur GitHub pour ce
  lot local ; une configuration CI ne constitue pas un résultat CI.
- Aucun lancement de l'APK sur émulateur ou appareil physique dans cette session.
  TalkBack/VoiceOver, gestes réels, rotation, verrouillage, réseau intermittent,
  mémoire et batterie doivent être éprouvés sur téléphones.
- La passerelle occupe la connexion IPC exclusive du démon. Le HUD desktop doit
  être déconnecté. Une publication distante demande un proxy/tunnel HTTPS
  explicitement configuré ; aucun serveur distant n'a été déployé ici.
- Le stockage de snapshot du composant shadow, l'authentification de son pair
  et la déduplication des opérations restent des responsabilités de l'hôte.
  Les interfaces natives ne prétendent pas activer une réplication TiKV.

L'APK est un paquet de **développement** signé pour les essais locaux. Les
bibliothèques, bindings générés, clés de signature et APK restent hors Git.
Aucun environnement, magasin d'applications ou service distant n'a été publié.
