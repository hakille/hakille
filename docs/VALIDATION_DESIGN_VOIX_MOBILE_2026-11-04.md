# Validation — design, voix et fonctions natives mobiles

Date : 2026-11-04. Cette troisième itération suit la version `22510d6`.
Le [design](DESIGN_MOBILE_2026-11-04.md) décrit les décisions visuelles et
le [guide mobile](../mobile/README.md) précise les fonctions et leurs limites.

## Périmètre livré

- Interfaces Compose/SwiftUI : Voix, Mémoire, Activité, Hub ; logo officiel,
  Inter sous licence OFL, thèmes sombre/clair/système, tokens partagés.
  Les icônes des barres système Android suivent le thème de l’application.
- Conversation activée explicitement : capture locale → texte envoyé au hub
  → réponse finale corrélée lue par une voix locale installée. Aucun retour
  automatique en écoute ; cartes de décision toujours approuvées séparément.
- Caméra, fichier sélectionné, position ponctuelle et destinations autorisées
  via API natives. Aperçus temporaires ; aucun envoi automatique au hub.
- Invalidation des callbacks après navigation/interruption, bornes des données,
  annulation des lectures, diagnostics de permission et de voix indisponible.
- CI Apple exécutée sur macOS : le cross-check Linux précédent échouait car
  `ring` compile du C pendant `cargo check` et nécessite la toolchain Apple.
  ONNX Runtime 1.31 ne publie plus de binaire macOS Intel. Seule l’étape
  `cargo check` Intel utilise `ORT_SKIP_DOWNLOAD=1`, mécanisme officiel
  `ort-sys` qui conserve les types et reporte l’erreur à une vraie liaison
  via un symbole non résolu. Ce contrôle ne prouve ni la liaison ni l’exécution
  ONNX sur Intel. L’étape ARM et le job macOS natif gardent le runtime normal.
- Test HTTP Windows : l’envoi d’un corps surdimensionné pouvait rencontrer
  une fermeture de socket avant de lire le rejet 413. Le test exige maintenant
  ce rejet dès les en-têtes, sans cette course ; la limite réelle en chunked
  reste testée séparément. Les seize tests passerelle repassent sur Linux.

## Contrôles locaux

| Contrôle | Résultat |
| --- | --- |
| Format Rust et exports des tokens | Réussite |
| `cargo check --workspace --all-targets --locked` | Réussite |
| Clippy workspace/all-targets | Réussite, avertissements existants conservés |
| Tests workspace | **1 306 réussis, 22 ignorés**, aucun échec |
| Tests du cœur mobile avec FFI | **40 réussis** |
| Conformance rapide | **66 réussis, 7 ignorés** |
| `cargo deny check bans sources advisories` | Réussite |
| Bibliothèques Rust Android arm64-v8a/x86_64 | Construites avec alignement ELF 16 Kio |
| APK debug Android | Construction réussie |
| Tests JVM Android | **31 réussis**, aucun échec |
| Lint Android | **0 erreur, 19 avertissements** |
| Inspection APK | Signature, ZIP/ELF 16 Kio, TLS, logo et Inter vérifiés |

APK local SHA-256 :
`9cffa963452ce7cc29d094b228e943f291a77322e099fc05b87cda539d31c2f9`.
Les APK CI utilisent leur propre signature debug et leur propre empreinte.

Les tests de décodage Qwen longs ignorés ne sont pas relancés pour cette
itération d’interface. Les tests unitaires de politique vocale vérifient
l’identité et l’annulation ; ils ne mesurent pas une capture microphone.

## Référence native déjà exécutée

La [CI Mobile de `22510d6`](https://github.com/hakille/hakille/actions/runs/41777888530)
a terminé avec succès sur les deux plateformes avant la refonte :

- Android API 35 x86_64 : sept assertions de parcours (pont Rust réel,
  brouillon, rotation, arrière-plan, portrait, restauration après arrêt,
  connexion), sans crash enregistré.
- iOS : seize XCTest réussis sur simulateur, dont le pont UniFFI réel.

Ces résultats portent sur cette révision précise.

## Nouvelle interface — première exécution CI

La [CI Mobile de `42bbc82`](https://github.com/hakille/hakille/actions/runs/41780927806)
a réussi son job Android : **31 tests JVM** et **26 étapes de parcours**
sur Android API 35 x86_64 (1080 × 1920, densité 420), en 82,95 secondes pour
le parcours. Il inclut le pont Rust réel, brouillon/rotation/restauration,
les quatre onglets, le sélecteur système de fichier annulé, la confirmation
d’ouverture des réglages annulée puis acceptée, et le retour à Hakille.
Aucune permission sensible accordée au lancement ni après ce scénario.
APK CI SHA-256 : `a8374e64119ec2494b48b655d07afebe00e0945f0b1431f992cf97acd3ade8c0`.

Les captures `voice_home.png` et `phone_capabilities.png` de l’artefact
`hakille-android-validation` ont été inspectées : logo, Inter, hiérarchie,
cartes et navigation sont rendus. Elles ont confirmé des icônes système trop
sombres lorsque le système est clair ; le correctif qui suit fait dépendre
les barres du thème effectif de Hakille. Le moteur ASR absent de cet émulateur
est annoncé comme indisponible, sans résultat vocal simulé.

Le job iOS de cette même révision a réussi **26 XCTest**, sans échec ni test
ignoré, sur iPhone Air simulé / iOS 26.5 ARM64. Les tests incluent les politiques
vocales, fichiers/annulation, destinations exactes, position approximative,
le pont UniFFI et le chargement réel de la fonte et de l’image de marque.
La capture `screenshots/home-settled.png` a été inspectée : accueil Voix,
logo, commandes et navigation sont présents. Le libellé de marque de la barre
supérieure était tronqué sur iOS 26 ; un correctif ciblé suit cette mesure.

## Captures corrigées et contrôle de contraste

La [CI Mobile de `c20254f`](https://github.com/hakille/hakille/actions/runs/41782237912)
est entièrement réussie : **31 tests JVM**, **26 étapes Android en 91,59 s**,
**26 XCTest iOS**, sans échec. APK CI SHA-256 :
`3a34275d8a544c47ad5c123151f8f0491d27818721adde36e34c4f31fd46828a`.
Les captures confirment la marque iOS sans troncature et la barre d’état Android
lisible. Les contrôles Apple Intel/ARM et macOS natif de cette révision passent.

Une revue complémentaire a mesuré seulement 1,97:1 de contraste pour les trois
boutons de navigation système Android. Le style sombre explicite désactivait
la protection de contraste Android. La retouche suivante conserve le thème
réel de Hakille via `SystemBarStyle.auto` et réactive cette protection, sans
API privée ni remplacement des boutons de l’OS. Les diagnostics de fenêtre et
de SystemUI sont désormais conservés par le smoke, avec un budget facultatif
strictement inclus dans les vingt secondes de nettoyage. Une nouvelle capture
est nécessaire pour qualifier le résultat de cette dernière retouche.

## Vérifications matérielles encore nécessaires

Reconnaissance et voix installées pour chaque langue ciblée, comportement
hors ligne des fournisseurs OS, microphone/haut-parleur physiques, caméra,
localisation, fournisseurs de fichiers tiers, TalkBack/VoiceOver et endurance.
Les API refusent explicitement une langue ou un moteur absent ; le projet ne
prétend pas fournir tous les modèles Hakille sur le téléphone. L’ouverture
d’une application ne fournit pas un contrôle autonome de ses écrans.
