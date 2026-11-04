# Design mobile Hakille — présence, voix et contrôle

Spécification de cette itération, 4 novembre 2026. Android Compose et iOS
SwiftUI partagent les mêmes intentions et tokens ; leurs composants restent
natifs. Ce document décrit le rendu attendu, sans déclarer disponibles les
capacités encore absentes ou non vérifiées sur appareil.

## Décisions issues des sources

La [vision technique](../Hakille%20%E2%80%94%20Document%20de%20vision%20technique.md)
a été lue intégralement : §13 et §21 imposent voix première, interface discrète,
journal accessible et canaux accessibles ; §3bis impose les limites honnêtes du
mobile ; §24e exige une utilité hors ligne ; §33a impose les nouveautés éteintes
par défaut. La [constitution](../constitution/v1.0.md) et sa
[présentation](constitution_and_ethics.md) imposent contrôle humain, absence de
capture brute exportée en continu et absence de recherche d'attachement.

La [description ambiante](ambient_multimodal_interface.md) est une intention
architecturale, pas une preuve de bindings livrés. Ses anciennes couleurs
`#121212` / `#FF5722`, ses rythmes et sa promesse de service mobile persistant
ne doivent pas être recopiés comme état actuel. Les sources graphiques sont
[tokens/color.toml](../tokens/color.toml), [type.toml](../tokens/type.toml),
[motion.toml](../tokens/motion.toml) et les modules
[brand](../crates/hakille-design/src/brand.rs),
[voice](../crates/hakille-design/src/voice.rs),
[states](../crates/hakille-design/src/states.rs).

Le code `brand.rs` décrit un signe de kora à 21 fils, mais les **logos déjà
fournis dans `logo/` montrent un arbre multicolore**. Cette livraison conserve
ces fichiers existants sans dessiner un nouveau symbole. Leurs couleurs
restent dans l'image de marque ; elles ne deviennent pas des accents d'interface.

## Composition commune

L'accueil est **Voix**, pas un historique de chat plein écran. En haut : nom
`hakille`, état réel du hub et accès à ses réglages. Au centre : signe officiel
statique, espace libre, titre court et état explicite. En dessous : une action
principale « Parler » / « Arrêter », puis « Écrire » et « Journal ». Deux modes
sont distingués : **Dictée** (défaut) remplit un brouillon relisible sans envoi ;
**Conversation** s'active après consentement explicite, envoie la transcription
finale au hub choisi et peut lire uniquement sa réponse finale corrélée. Le
consentement annonce donc la sortie du texte vers le hub et la lecture audible.
Une réponse issue du clavier ou de la dictée ne déclenche jamais de lecture
automatique. Pas d'écoute en boucle : chaque tour vocal démarre par une action.

Navigation basse : **Voix · Mémoire · Activité · Hub**. Activité rassemble
Journal et Agents ; le raccourci Journal de l'accueil ouvre directement ce
contenu. Hub rassemble connexion, statut et capacités. Les cartes de décision
et avertissements de résultat inconnu restent visibles avant l'action
principale : aucun nouvel habillage ne les cache dans un menu.

Les détails de conversation apparaissent après une demande ou par action
explicite. Afficher les vraies réponses et leurs éventuels suivis sans bulle
factice d'accueil, compteurs fictifs, suggestions qui envoient une commande au
toucher, ni métrique de « confiance » inventée. Les résultats restent accessibles
au clavier, au lecteur d'écran et avec de grandes tailles de texte.

## Matière et hiérarchie visuelle

- Sombre par défaut : surface/0 `#0B0C0F`, cartes surface/1 `#121419`, zones
  secondaires surface/2 `#1A1D24`. Mode clair complet avec les tokens associés ;
  option clair/système si proposée par l'hôte.
- Texte/1 pour l'information essentielle, texte/2 pour les précisions ; lueur
  pour le focus et le bouton vocal. **Braise uniquement** pour validation,
  progression des 600 ms et anneau « décision requise ». Jamais pour le micro,
  un onglet sélectionné, un réglage ordinaire ou une erreur générique.
- Aspect précis et sobre : surfaces superposées, liserés neutres, espace libre,
  alignements réguliers. Pas de nouvelle palette cyan/violette, de faux radar,
  de particules ni de décor qui ressemble à une mesure de capteur.
- Grille 4 ; espacements 8/12/16/24/32 selon les tokens ; rayons 8 pour les
  contrôles, 16 pour les cartes. Cibles ≥48 dp Android, ≥44 pt iOS. Le texte
  agrandi peut augmenter les hauteurs : pas de découpe du contenu pour tenir
  une composition fixe.
- Respecter les styles typographiques générés. **Inter Variable est désormais
  embarquée**, fichier officiel inchangé, licence OFL 1.1 ; provenance et
  SHA-256 dans [Android](../mobile/android/app/src/main/res/raw/inter_provenance.txt)
  et [iOS](../mobile/ios/Hakille/Fonts/PROVENANCE.txt). Android utilise
  `R.font.inter_variable`, iOS le nom PostScript `Inter-Regular` après
  enregistrement UIAppFonts. Les scripts absents gardent les substitutions
  Unicode natives. Alignements début/fin compatibles RTL ; aucune icône seule
  ne remplace un libellé d'action important.

## Présence : chaque état doit correspondre à une observation

| État réel | Texte visible indicatif | Signe et commandes |
|---|---|---|
| Aucune capture | « Le micro est arrêté » | Logo immobile ; Parler et Écrire disponibles selon capacités |
| Permission / préparation | « Préparation du micro » | Aucun voyant « écoute » anticipé ; Annuler accessible |
| Callback natif d'écoute reçu | « Je t'écoute » | Anneau lueur distinct du logo ; Arrêter et Annuler |
| Reconnaissance en cours | « Transcription sur ce téléphone » | Micro présenté arrêté seulement après son arrêt réel ; aucun volume inventé |
| Dictée obtenue | « Relis avant d'envoyer » | Texte éditable ; Envoyer explicite, aucune reprise automatique |
| Conversation : transcription finale | « Texte envoyé au hub » seulement après émission | Consentement préalable obligatoire ; préserver un brouillon déjà présent |
| Conversation : réponse finale corrélée | « Lecture de la réponse » seulement au callback natif | Arrêter accessible ; réponse d'un ancien tour/texte ignorée, aucune parole après interruption |
| Requête au hub en vol | « Le hub traite ta demande » | Annuler le transport ; ne pas prétendre arrêter un effet distant |
| Hub déconnecté | « Hub non connecté » | Brouillon conservé ; fonctions locales disponibles séparément |
| Résultat incertain | « Le résultat n'est pas confirmé » | Journal et prise de connaissance ; jamais « annulé » ou « réussi » par déduction |
| Décision reçue | Action, raison, coût et destination réels | Braise autorisée ; maintien Rust 600 ms, refus immédiat |

Le logo n'est jamais animé. Un anneau d'état peut évoluer après un événement
réel, mais ne pulse pas en veille ou pour simuler une écoute permanente. Aucun
spectrogramme/volume sans données audio natives. Réduction des animations :
états statiques lisibles, sans perte de fonctionnalité. Sous 400 ms éviter le
spinner ; après 2 s afficher l'attente réelle et une action d'interruption,
sans reprendre littéralement « Je cherche dans ta mémoire » pour toute opération.

L'accessibilité de l'approbation conserve les deux activations annoncées et le
délai Rust ; une animation de progression ne constitue pas l'autorisation.
Arrière-plan, changement de contexte ou geste annulé retirent la confirmation.
La dictée ne reprend jamais automatiquement au retour de l'application.

## Capacités du téléphone et du hub

La section Capacités sépare **sur ce téléphone** et **sur ton hub**. Pour chaque
fonction : disponible, permission nécessaire, refusée, indisponible ou inconnue,
avec sa conséquence pratique. Une disponibilité n'est pas une permission acquise.

Les capacités attribuées à cette itération ont les contrats suivants. Cette
liste fixe l'interface attendue ; elle ne certifie pas leur validation matérielle.

| Capacité native | Présentation et garde-fou |
|---|---|
| Dictée OS locale | Langue et moteur disponibles réels ; mode Dictée vers brouillon, mode Conversation vers hub après consentement explicite |
| Lecture vocale | Android : voix hors ligne seulement ; iOS : AVSpeechSynthesizer ; lecture manuelle ou réponse finale corrélée du mode Conversation opt-in, Arrêter accessible |
| Photo manuelle | Permission à l'action ; aperçu local temporaire et suppression accessible ; aucun upload automatique |
| Fichier choisi | Sélecteur système, accès limité au choix et bornes réelles ; nom/taille utiles, sans lecture globale du téléphone |
| Position ponctuelle | Demande au premier plan ; âge et précision affichés, refus/absence clairement signalés |
| Ouvrir une application ou une URL | Destination lisible, liste autorisée et confirmation ; ouverture système, aucun contrôle de l'autre application |

Les permissions sont demandées à l'action concernée, sans activation collective
à l'accueil. Aucun AccessibilityService, contrôle d'interface tierce ou envoi
implicite de capteur. Une disponibilité n'est pas une permission acquise. Les
aperçus et brouillons ne deviennent ni messages au hub ni souvenirs sans choix
explicite. Partage, presse-papiers et batterie ne sont pas ajoutés à cette
itération ; ne pas créer de tuiles laissant croire ces fonctions livrées.

La dictée locale déjà intégrée reste conditionnée au moteur natif disponible et
à la langue ; ne promettre ni écoute du mot « Hakille » en arrière-plan, ni
transcription locale universelle. Une mesure absente reste « inconnue », jamais
0 %, 100 % ou « sécurisé » inventé. L'état du hub ne devient pas l'état du
téléphone : modèle hébergé et accès mémoire distants sont présentés comme tels.
Hors ligne, afficher ce qui reste utilisable ; ne promettre ni moteur neuronal
local absent ni file de mutations rejouée au retour du réseau.

## Assets livrés et branchement

Les six PNG sources ont été inspectés visuellement avant sélection. Les signes
intra-app ci-dessous sont des copies **octet pour octet** ; seule la mise aux
formats launcher/AppIcon est mécanique, sans redessin, teinte ni nouvelle couleur.

| Destination | Source et format | Utilisation |
|---|---|---|
| Android `drawable-nodpi/hakille_mark_light.png` | `logo/hakille-logo.png`, 640×640 RGBA | Logo sur thème clair, sans ColorFilter |
| Android `drawable-nodpi/hakille_mark_dark.png` | `logo/hakille-logo-dark.png`, 640×640 RGBA | Logo sur thème sombre, sans ColorFilter |
| Android `mipmap-*/ic_hakille_launcher.png` | `logo/hakille-app-icon.png`, redimensionné 48/72/96/144/192 | `android:icon="@mipmap/ic_hakille_launcher"` |
| iOS `HakilleMark.imageset` | Même paire 640×640, apparence luminosity/dark | `Image("HakilleMark").renderingMode(.original)` |
| iOS `AppIcon.appiconset` | `logo/hakille-logo-square.png`, RGB opaque | `ASSETCATALOG_COMPILER_APPICON_NAME: AppIcon` |

Le catalogue iOS fournit les 18 emplacements iPhone/iPad/marketing : 20, 29,
40, 60, 76 et 83,5 points aux échelles requises, plus 1024×1024. La variante
carrée opaque existante est utilisée car App Store refuse l'alpha. Les pixels
du marketing 1024 sont identiques à la source après retrait du canal alpha
uniformément opaque. Les assets Android conservent la transparence existante.
L'OS applique son masque d'icône ; aucun masque de marque supplémentaire n'est
inventé. Le logotype textuel est du texte natif `hakille`, jamais du faux texte
raster ajouté à l'image.

## Vérification de cette livraison

Dimensions des 5 densités Android et des 18 slots iOS vérifiées ; toutes les
icônes iOS sont RGB sans alpha ; les 4 copies intra-app sont identiques à leurs
sources. Contrôle visuel effectué sur les exports Android 192 et iOS 180.
Aucune compilation lourde lancée pour ce lot d'assets.

Contraste des tokens vérifié par calcul WCAG : texte/1, /2, /3 sur les trois
surfaces ≥4,81:1 en sombre et ≥4,82:1 en clair ; texte sur braise 7,52:1 et
5,18:1 respectivement. Ces mesures ne remplacent pas une revue d'écran natif
(états désactivés, superpositions, lecteur d'écran et texte agrandi). La revue
source a aussi fait corriger les rôles Material hérités hors palette et les
styles typographiques oubliés ; la présence d'Inter dans le bundle construit
reste à contrôler lors des gates natifs.

La validation des écrans appartient aux hôtes : capture réelle en clair/sombre,
texte agrandi, RTL, navigation Journal, micro arrêté puis autorisé/refusé,
interruption, restauration incertaine et carte à 600 ms. Un écran réussi dans
un émulateur ne prouve pas la dictée, la synthèse ou les permissions sur tous
les appareils. La présence d'un asset ne prouve son branchement qu'après
configuration du manifeste/catalogue et inspection de l'application construite.
