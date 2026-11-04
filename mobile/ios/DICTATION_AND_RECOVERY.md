# Dictée locale et reprise iOS

La dictée est un service natif iOS distinct du cœur Rust. Elle se déclenche
uniquement par **Dicter dans le brouillon**, dans l'onglet Chat. Elle n'exige pas
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

Dans cet environnement Linux, Xcode et les frameworks Apple ne sont pas
présents : **ni compilation iOS, ni XCTest, ni dictée physique n'ont été
exécutés ici**. Le résultat de la nouvelle CI reste à observer.

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
