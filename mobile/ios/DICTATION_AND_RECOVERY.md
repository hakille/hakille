# Dictée locale et reprise iOS

La dictée est un service natif iOS distinct du cœur Rust. Elle se déclenche
par le bouton **Parler**, dans l’onglet **Voix**, en mode **Dictée** par défaut.
Elle n’exige pas
un hub connecté. Le texte final complète le brouillon ; l'envoi exige toujours
une action explicite sur **Envoyer**. Il n'y a ni mot d'éveil permanent, ni
Whisper embarqué, ni démarrage automatique du microphone.

## Branchement et limites

- `NativeDictationController` utilise `SFSpeechRecognizer` et `AVAudioEngine`.
  Il exige `supportsOnDeviceRecognition` et configure chaque requête avec
  `requiresOnDeviceRecognition = true`. Aucun repli vers la reconnaissance
  réseau n'est prévu. La disponibilité dépend de l'appareil, de la langue,
  des ressources de reconnaissance installées et des restrictions iOS.
- La langue de l'appareil est proposée par défaut ; français et anglais sont
  également sélectionnables. Les permissions Speech et Microphone sont
  demandées seulement après le bouton utilisateur. Les clés Info.plist et
  les messages sont traduits en français et anglais.
- La capture s'arrête après 30 secondes ou au bouton d'arrêt. La finalisation
  dispose de 8 secondes supplémentaires, microphone arrêté. Sans buffer audio
  après 3 secondes, l'opération échoue explicitement. Aucun résultat partiel
  n'est ajouté au brouillon ; un résultat vide ou trop long est refusé.
- Une interruption audio, un périphérique d'entrée retiré, un retour à
  l'arrière-plan ou une déconnexion invalide les callbacks et ferme le tap,
  le moteur et la requête Speech. L'inactivité des dialogues de permission
  est distinguée de l'arrière-plan : elle ne permet jamais de démarrer
  l'audio avant le retour au premier plan. Une dictée déjà en cours est
  interrompue dès l'inactivité.
- Le verrou du destinataire audio protège la fermeture contre un buffer
  tardif. Les callbacks AVFoundation/Speech n'accèdent pas directement à
  SwiftUI. Le texte final est appliqué une fois, puis sa sauvegarde est
  attendue sans bloquer MainActor. Aucun audio/transcript n'est journalisé.
- `snapshot.microphone_enabled` reste **false** : ce champ décrit le service
  vocal du cœur, qui n'est pas raccordé. `MobileModel.dictationStatus` décrit
  séparément la dictée native.

## Checkpoint avant transport

`MobileStateStore` conserve le seul export `MobileBridge.draftJson()` dans
Application Support/MobileRecovery, exclu des sauvegardes cloud et protégé
par la protection complète des fichiers iOS. Il contient le brouillon, le
compteur d'ID et les marqueurs d'opérations incertaines/lues ; jamais le token,
la session HTTP, les commandes ou l'adresse du hub. Les credentials restent
au trousseau. Les fichiers sont limités à 64 Kio ; le schéma v1 est accepté
à la lecture et le cœur produit le schéma v2.

Chaque écriture crée un fichier privé, synchronise son contenu, le remplace
atomiquement puis synchronise le répertoire. `MobileStateIO` effectue ces I/O
sur un acteur distinct de MainActor. Une chaîne de tâches conserve leur ordre.
Le transport attend sa sauvegarde et revérifie génération et ID pending.
Un échec certain avant émission utilise `fail_before_send`, sans inventer un
effet inconnu ; un échec après le début du transport reste conservateur.

Le checkpoint est également sauvegardé après une réponse, un changement de
brouillon ou de cycle de vie. Une panne de stockage bloque l'émission ; le
bouton de reprise réessaie le stockage sans renvoyer automatiquement de
requête. Un fichier corrompu n'est pas écrasé. Si la protection iOS interdit
l'accès lorsque l'appareil est verrouillé, l'erreur est visible au retour.

Une reconnexion interrompt le cœur avant d'exporter et restaurer son état.
Effacer les credentials ne supprime donc pas les avertissements antérieurs.
Après relancement, les opérations en vol deviennent inconnues, sans rejeu.
Leur avertissement est visible même hors ligne. **J'ai lu cet avertissement**
archive la prise de connaissance ; cela ne confirme ni n'annule aucun effet.
Le cœur borne séparément les marqueurs inconnus et les archives à 32 entrées ;
le journal du hub reste la source à consulter pour les effets métier.

## Vérification

La CI macOS est configurée pour construire puis exécuter XCTest sur un
simulateur iPhone via `scripts/test-ios-simulator.sh`. Les nouveaux tests
vérifient la politique de dictée, les callbacks périmés, les checkpoints et
la liaison/exécution réelle de `MobileBridge`. Ils ne simulent pas un succès
de reconnaissance et ne remplacent pas un essai de microphone physique.

La CI macOS du commit `22510d6` a compilé, lié et exécuté les **16 XCTest
de la version précédente avec succès**. La présente refonte ajoute un mode
conversation et des capacités natives, avec **10 tests supplémentaires :
26 XCTest sont attendus au total**. La compilation et les résultats de ces
26 tests doivent encore être vérifiés dans la nouvelle CI ; les 16 succès
précédents ne valident pas ce nouveau lot. Aucun microphone physique n’a été
validé ici.
L’environnement de développement Linux n’exécute pas les frameworks Apple.

Sur un iPhone compatible :

1. Vérifier l'absence de dialogue et d'indicateur micro au lancement ; dicter
   en français puis en anglais, mode avion activé après installation des
   ressources de langue. Vérifier que seul le brouillon change.
2. Refuser chaque permission, puis la réactiver dans Réglages. Vérifier les
   messages d'erreur et l'absence d'écoute après un refus.
3. Arrêter, annuler, dépasser 30 secondes, verrouiller l'appareil, recevoir
   un appel et retirer un casque pendant la dictée. L'indicateur micro doit
   disparaître et aucun résultat tardif ne doit modifier un nouveau brouillon.
4. Vérifier VoiceOver, tailles de texte, français/anglais, mode sombre et RTL.
5. Interrompre l'app après l'émission d'une mutation au hub, puis la relancer.
   Vérifier l'avertissement inconnu sans rejeu, y compris après changement de
   hub ou effacement des credentials. Confirmer les effets dans le journal.


## Conversation vocale explicite et interface

L'accueil Voix utilise le logo officiel statique, Inter et les tokens partagés.
Quatre destinations donnent accès à Voix, Mémoire, Activité (Journal/Agents)
et Hub (Connexion/Statut/Téléphone). Les cartes de décision, les avertissements
de résultat inconnu et le maintien Rust de 600 ms restent visibles. Le thème
sombre est proposé par défaut ; clair, système et RTL sont disponibles. Le
signe n'affiche aucun niveau sonore fictif et ne pulse jamais en veille.

Le mode **Conversation** nécessite une confirmation : chaque appui sur Parler
transcrit localement un tour, puis transmet son texte final au hub choisi.
Il ne peut démarrer avec un brouillon existant, une décision en attente ou
une session non confirmée. Les permissions sont obtenues avant `begin_voice`,
appelé immédiatement avant la capture réelle ; `submit_voice` utilise ensuite
le contexte monotone et mono-usage du cœur. Le checkpoint précède toujours HTTP.

Seul `voice_reply`, corrélé par contexte et ID de requête, peut déclencher
`AVSpeechSynthesizer`. Le dernier message de chat n'est jamais utilisé comme
substitut. Les réponses trop longues ou sans version vocale restent lisibles
à l'écran. La synthèse utilise une voix retournée par la liste système des
voix installées, sans service de synthèse réseau ajouté par l'application.
L'installation des ressources vocales relève d'iOS. Le début de lecture est
annoncé au callback natif, avec arrêt immédiat et limite de deux minutes.
Une voix qui ne démarre pas sous huit secondes est signalée indisponible.

Ni le clavier ni le mode Dictée ne déclenchent une lecture automatique.
Chaque nouveau tour exige un nouvel appui : aucun réarmement de microphone
à la fin de la réponse. Quitter l'onglet Voix, désactiver le mode, passer en
arrière-plan ou changer de contexte arrête la voix et invalide les callbacks.
Aucune transcription n'approuve une carte et aucune réponse du modèle ne
commande les capacités du téléphone.

## Capacités locales ponctuelles

L'onglet Hub/Téléphone propose quatre opérations séparées du transport hub :

- **Caméra** : sélecteur caméra UIKit natif, permission au bouton, aperçu
  rééchantillonné à 1536 pixels maximum et réencodé sans reprise des métadonnées
  d'origine, limite 8 Mio. L'image reste en mémoire, sans écriture dans Photos
  et sans upload. L'absence de caméra est explicite sur simulateur.
- **Document** : un fichier texte/PDF/image choisi dans le sélecteur système,
  scope temporaire, lecture d'un fichier ordinaire sans suivre les symlinks,
  maximum 8 Mio. Lecture sur acteur séparé, contrôles d'annulation et délai
  monotone de 10 s entre blocs. Un appel système bloqué ne peut pas être
  préempté par ce délai ; aucun résultat tardif n'est publié après annulation.
  Le fournisseur Fichiers choisi peut télécharger son document. Aucun accès
  global, bookmark persistant ou envoi au hub n'est créé. L'aperçu texte est
  limité à 4000 caractères ; les formats binaires affichent nom et taille.
- **Position** : permission When In Use, demande unique au premier plan,
  délai de 20 s après autorisation. Les positions de plus de 15 s, coordonnées
  invalides, précision négative ou non finie sont refusées. La localisation
  approximative reste autorisée (notamment 5000 m) et explicitement présentée
  avec précision réelle et âge qui progresse ; aucun suivi en arrière-plan.
- **Ouverture** : uniquement Plans, Calendrier, réglages de cette application
  et URL fixe de documentation GitHub. La destination exacte est affichée
  avant confirmation. Aucune URL issue du modèle, aucun pilotage d'application
  tierce, aucun transfert de coordonnées implicite.

Les aperçus sont supprimables et effacés au passage en arrière-plan. Aucun
secret, audio ou document n'est journalisé. Les tests supplémentaires portent
sur les générations vocales, les réponses corrélées, les URI, la précision
approximative, les fichiers bornés/annulés, le chargement effectif du logo et
d'Inter, et le refus d'armer la voix sans hub confirmé.

Sur appareil, vérifier en plus les langues/voix installées, les cas permission
refusée, la caméra sans enregistrement Photos, les fournisseurs Fichiers,
la position approximative, l'interruption audio, VoiceOver, les grandes polices,
la sortie d'onglet pendant capture et l'ouverture confirmée de chaque destination.
Ces essais matériels restent à effectuer ; un résultat XCTest n'en tient pas lieu.
