# Conversation vocale et fonctions du téléphone Android

Ces fonctions nécessitent une action visible dans l'application. Elles ne
permettent pas au texte produit par un modèle de piloter le téléphone. Les
contrôles de cartes et le journal du hub restent ceux du cœur Rust.

## Trois usages distincts

- Le mode texte conserve l'édition et le bouton **Envoyer**.
- La **dictée dans le brouillon** utilise la reconnaissance locale Android et
  ajoute uniquement le résultat final ; elle n'envoie rien.
- Le **mode conversation**, activé après une explication explicite, autorise
  l'envoi du résultat final de chaque tour lancé par **Parler** au hub configuré,
  puis la lecture locale de sa réponse. Il n'existe pas de boucle d'écoute :
  la fin de la lecture ne rallume jamais le microphone.

La conversation exige un hub connecté, une session confirmée, aucun brouillon
à écraser, aucune carte ni opération en attente et aucun avertissement
incertain non pris en compte. Le cœur émet un contexte à usage unique,
valable 120 secondes pour la capture. L'ASR native reste bornée à 30 secondes.
`submit_voice` consomme ce contexte et suit le checkpoint durable habituel
avant HTTP. Les callbacks d'une ancienne capture, session ou génération ne
peuvent pas soumettre un nouveau message. Le microphone natif ne démarre que
sur action explicite et au premier plan.

Android 12/API 31 et un moteur ASR sur l'appareil disponible sont requis.
Aucun moteur réseau de remplacement n'est créé. La première demande de
permission peut mettre l'activité en pause : le contexte et le mode sont
alors annulés. Après autorisation, un message demande de réactiver la
conversation et d'appuyer à nouveau sur **Parler** ; aucun démarrage automatique.

La lecture utilise Android `TextToSpeech`, uniquement une voix de la langue
choisie qui ne nécessite pas le réseau et dont les données sont installées.
Il n'y a pas d'installation ou de téléchargement automatique. Sans voix
compatible, la réponse demeure accessible à l'écran et une erreur est visible.
Le statut « lecture » provient de `onStart`, pas de la simple demande de synthèse.
L'initialisation est limitée à 10 secondes et la lecture à 120 secondes.
L'absence ou la perte du focus audio arrête la lecture. Toute capture suivante
arrête d'abord la synthèse. Les textes dépassant la borne du contexte vocal
Rust (8192 octets UTF-8) ou celle du moteur TTS ne sont pas lus ; l'interface
l'indique sans tronquer ou exécuter leur contenu.

Seule `snapshot.voice_reply`, liée aux identifiants du contexte et de la
requête courante, autorise une lecture, une seule fois. Les deltas et les
anciens messages du chat ne sont jamais utilisés pour décider quoi prononcer.
Une carte reçue invalide le contexte vocal : son approbation reste séparée.
La navigation, la pause et la déconnexion arrêtent capture et lecture ; le
passage en arrière-plan désactive le mode. Une interruption d'un transport
vocal déjà commencé conserve une issue inconnue, sans relance automatique,
et ne prouve pas l'annulation du calcul ou d'un effet sur le hub.

## Panneau « Ce téléphone »

Ces résultats restent dans l'application ; aucun chemin ne les attache à une
requête au hub. Quitter le panneau, annuler ou déconnecter efface les aperçus.
Aucun fichier temporaire photo/document n'est créé par Hakille.

### Photo

Le bouton ouvre la caméra native avec `TakePicturePreview`. Il récupère une
**miniature** en mémoire, dont les dimensions sont affichées ; ce n'est pas
une capture pleine résolution. Hakille ne demande pas la permission CAMERA,
car la capture est déléguée à l'application caméra choisie. Celle-ci peut
appliquer sa propre politique de stockage. Annulation, caméra absente et
résultats périmés sont traités explicitement. Aucun accès caméra permanent.

### Fichier

Le sélecteur SAF utilise `ACTION_OPEN_DOCUMENT` avec `EXTRA_LOCAL_ONLY`.
Le respect de cette préférence dépend du fournisseur Android. Aucun droit
persistant n'est demandé et aucune copie durable n'est conservée. La lecture
vérifie les octets effectivement reçus, avec une limite de **1 Mio**, même
si la taille annoncée par le fournisseur est incorrecte. L'aperçu textuel est
limité aux premiers **4 Kio** ; il est rendu comme texte non fiable, sans
interprétation HTML, JavaScript ou commande. Les autres fichiers affichent
uniquement leurs métadonnées et l'empreinte SHA-256 du contenu lu.

Le délai de lecture demandé est de 10 secondes. Sur Android 11/API 30 et
supérieur, les lectures de descripteur utilisent le mode non bloquant via
`Os.fcntlInt` et `poll` par tranches bornées. Android 8–10/API 26–29 ne fournit
pas cette opération dans le SDK public : seuls les fichiers ordinaires
identifiés par `Os.fstat` sont acceptés ; les pipes/sockets des fournisseurs
SAF sont refusés avec une explication FR/EN, sans appel d’API cachée. Annulation
et expiration ferment aussi le descripteur actif et sollicitent
`CancellationSignal`. Un mutex borne à un seul travail de lecture effectif.
Android ne garantit pas qu'un fournisseur externe interrompe un appel Binder
`query`/`open` ou une opération noyau qui ignore l'annulation : dans ce cas,
le travail peut encore retenir son emplacement jusqu'au retour du fournisseur.
L'interface ne reste pas occupée et les résultats tardifs sont rejetés ;
aucun arrêt forcé du processus du fournisseur n'est revendiqué.

### Position

Le bouton demande les permissions de localisation au moment de l'utilisation.
Après leur octroi, un second appui lance la mesure. Une permission approximative
reste acceptable si un fournisseur réseau est disponible ; GPS exige la
permission précise. Il n'y a ni permission de localisation en arrière-plan,
ni suivi continu. Le service de localisation de l'OS peut employer ses propres
réseaux : ceci est distinct d'un envoi au hub, qui n'a pas lieu.

La recherche s'arrête à la première mesure valide ou après **20 secondes**.
Les coordonnées, la précision annoncée et l'âge croissant de la mesure sont
affichés. Une mesure vieille de plus de 60 secondes, datée dans le futur ou
contenant des valeurs impossibles est refusée. Les positions marquées simulées
par Android sont signalées ; elles ne sont pas présentées comme une preuve GPS
réelle. La pause, le changement de panneau ou la déconnexion arrêtent la mesure.

### Applications

La liste est fixe : réglages Android, alarmes, calendrier, cartes et documentation
Hakille sur GitHub. Chaque bouton
annonce sa destination et ouvre une confirmation avant de passer à l'application.
Après résolution par Android, l'intent utilise le composant explicite choisi.
Les cartes sont ouvertes sans coordonnées ni itinéraire. La documentation
ouvre uniquement `https://github.com/hakille/hakille` dans le navigateur ;
la confirmation annonce ce contact avec GitHub et les paramètres propres
au navigateur. Les autres applications peuvent aussi utiliser leurs propres
réseaux, comptes ou permissions, sans pièce jointe fournie par Hakille.
Aucun URL fourni par le modèle, appel téléphonique, message, événement ou
alarme n'est créé. Pas de `QUERY_ALL_PACKAGES`, de service d'accessibilité
caché ou de manipulation automatique d'autres applications. Le succès de
`startActivity` signifie seulement qu'Android a reçu la demande d'ouverture.

## Validation et limites

Les tests JVM vérifient l'opt-in, la consommation unique des réponses, les
réponses périmées, le choix des voix hors ligne installées, les limites de
lecture et la validation des mesures de position. Les tests de dictée
injectent des callbacks pour vérifier l'invalidation du contexte vocal.
Ces tests ne constituent pas une reconnaissance, une synthèse, une photo
ou une mesure GPS réellement effectuée sur un téléphone.

Le parcours automatisé Android peut vérifier la navigation, l'ouverture puis
l'annulation du sélecteur et la confirmation d'ouverture des réglages sans
accorder de permission sensible. Les résultats de compilation, de CI et de
ce parcours sont consignés séparément par le rapport du lot. Restent à
vérifier sur matériel : ASR/TTS hors ligne en français et anglais avec les
ressources installées, permissions refusées/révoquées, appel et interruption
audio, écouteurs débranchés, caméra réelle, fournisseurs SAF, localisation
approximative/précise et accessibilité. Les API configurées pour un traitement
local ne constituent pas un audit indépendant de chaque moteur OS fournisseur.
