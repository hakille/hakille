# Validation — design, voix et fonctions natives mobiles

Date : 2026-11-04. Cette troisième itération suit la version `22510d6`.
Le [design](DESIGN_MOBILE_2026-11-04.md) décrit les décisions visuelles et
le [guide mobile](../mobile/README.md) précise les fonctions et leurs limites.

## Périmètre livré

- Interfaces Compose/SwiftUI : Voix, Mémoire, Activité, Hub ; logo officiel,
  Inter sous licence OFL, thèmes sombre/clair/système, tokens partagés.
- Conversation activée explicitement : capture locale → texte envoyé au hub
  → réponse finale corrélée lue par une voix locale installée. Aucun retour
  automatique en écoute ; cartes de décision toujours approuvées séparément.
- Caméra, fichier sélectionné, position ponctuelle et destinations autorisées
  via API natives. Aperçus temporaires ; aucun envoi automatique au hub.
- Invalidation des callbacks après navigation/interruption, bornes des données,
  annulation des lectures, diagnostics de permission et de voix indisponible.
- CI Apple exécutée sur macOS : le cross-check Linux précédent échouait car
  `ring` compile du C pendant `cargo check` et nécessite la toolchain Apple.
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
`cb0cb1478253cc0ae4f0129784c2b5ef1df204a05bc92b5f7b9e581bae2d4af9`.
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

Ces résultats portent sur cette révision précise. La nouvelle interface et
les nouveaux tests nécessitent une nouvelle exécution CI ; leurs résultats
seront consignés ci-dessous après exécution.

## Vérifications matérielles encore nécessaires

Reconnaissance et voix installées pour chaque langue ciblée, comportement
hors ligne des fournisseurs OS, microphone/haut-parleur physiques, caméra,
localisation, fournisseurs de fichiers tiers, TalkBack/VoiceOver et endurance.
Les API refusent explicitement une langue ou un moteur absent ; le projet ne
prétend pas fournir tous les modèles Hakille sur le téléphone. L’ouverture
d’une application ne fournit pas un contrôle autonome de ses écrans.
