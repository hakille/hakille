# Dictée Android et état local

La dictée se lance uniquement avec **Dicter dans le brouillon**. Elle emploie
`SpeechRecognizer.createOnDeviceSpeechRecognizer` sur Android 12 / API 31 ou
ultérieur, après vérification du moteur sur appareil. Aucun appel au constructeur
réseau, aucun repli cloud. Android 8 à 11 reste compatible avec le reste de l’app,
mais la dictée y est explicitement indisponible. La disponibilité du moteur ne
garantit pas qu’une langue soit installée : ses erreurs restent visibles.

La permission microphone est demandée uniquement par ce bouton. L’état affiche
« préparation » jusqu’au callback `onReadyForSpeech`, puis écoute, fin demandée ou
reconnaissance. Le résultat **final** est ajouté au brouillon existant ; il ne
soumet rien et n’approuve aucune carte. Les résultats partiels ne sont pas utilisés.
La langue du téléphone, le français et l’anglais sont sélectionnables. Le texte
Unicode est préservé ; un brouillon dépassant 8192 octets UTF-8 est refusé sans
troncature.

Une session dure au maximum 30 secondes. Annulation, résultat final, erreur,
changement de brouillon, déconnexion, rotation, changement d’onglet ou passage en
arrière-plan annulent et détruisent le moteur. Les callbacks et permissions d’une
ancienne génération sont ignorés. Si la boîte de permission met l’activité en
pause, il faut appuyer à nouveau après l’autorisation ; aucune reprise automatique
ne démarre le microphone. Le bouton de fin appelle réellement `stopListening`,
puis attend le résultat final avec la même échéance.

Il s’agit du moteur de dictée fourni par l’appareil, **pas de Whisper embarqué ni
d’une écoute permanente du mot Hakille**. Son état natif est séparé du champ
`microphone_enabled` du cœur Rust, dont le moteur microphone reste indisponible.
Les tests JVM utilisent des callbacks injectés pour vérifier ces décisions ; ils
ne prouvent ni la qualité de transcription ni l’accès à un microphone physique.
À vérifier sur matériel compatible : moteur installé, français/anglais, permission
refusée/révoquée, indicateur microphone système et arrêt effectif après rotation.
Un émulateur AOSP sans moteur doit afficher son indisponibilité.

## Sauvegarde et reprise

Le brouillon et les marqueurs v2 du cœur sont stockés dans le dossier privé de
l’app (`mobile-draft-v2.json`), exclu des sauvegardes Android. Aucun jeton ni contenu
de requête n’est ajouté à ces marqueurs ; le brouillon contient naturellement le
texte saisi ou dicté. Le jeton et son endpoint restent dans le stockage chiffré
Android Keystore séparé.

Chaque requête attend l’écriture atomique de son état de reprise : synchronisation
du fichier, promotion `AtomicFile`, lecture de contrôle et synchronisation du
répertoire. Une erreur avant l’envoi produit un résultat **non envoyé** via
`fail_before_send`. Après lancement HTTP, une erreur conserve un résultat inconnu,
sans relance automatique. Les écritures sont sérialisées. Une déconnexion préserve
les marqueurs lors du remplacement du cœur ; elle n’efface pas le brouillon.

Au redémarrage, le fichier est lu et validé avant d’autoriser les actions. Un état
illisible bloque les requêtes avec un message visible au lieu d’être écrasé. Une
opération qui était en cours est présentée comme inconnue : consulter le journal
du hub avant de réessayer. L’action « J’ai lu cet avertissement — l’archiver » ne
constitue ni une annulation ni une confirmation d’effet. Aucun identifiant de hub
n’est persisté dans le marqueur ; l’avertissement précise qu’il peut concerner un
autre hub. Le compteur des requêtes est restauré par le cœur.

Les tests de barrière vérifient qu’échec/annulation d’écriture empêchent l’appel au
transport ; ils ne remplacent pas un essai d’interruption brutale ou de disque
plein sur téléphone. La dernière frappe peut être perdue si le processus est tué
avant sa sauvegarde asynchrone ; **l’envoi HTTP**, lui, attend la sauvegarde.
