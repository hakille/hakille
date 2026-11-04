# ADR-011 — Clients mobiles natifs et noyau sur le hub personnel

Date : 2026-11-04. Statut : accepté pour le prototype ; validation matérielle requise.

## Contexte

ADR-010 prévoit des interfaces SwiftUI et Compose qui partagent le protocole
avec le noyau Rust. Le dépôt avait un substrate mobile, mais aucun projet
d'application Android/iOS. Plusieurs valeurs de matériel et transitions de
synchronisation étaient simulées. L'[audit mobile](../docs/ANALYSE_MOBILE_2026-11-04.md)
décrit cet état initial, les écarts de confiance et les exigences de validation.

## Décision

- `hakille-mobile` porte les états et décisions du client en Rust. UniFFI expose
  les mêmes opérations à Kotlin et Swift. Les interfaces natives prennent en
  charge le rendu, le cycle de vie, le transport et les trousseaux système.
- Le premier chemin utilisable rejoint le **hub personnel** : les modèles
  Hakille s'exécutent là où tourne `hakille-daemon`. Aucun poids Hakille ni
  pilote NPU Hakille n'est embarqué sur le téléphone. Une dictée native
  optionnelle utilise uniquement les API de reconnaissance locale de l'OS :
  Android API 31+ avec moteur sur l'appareil disponible, iOS 16+ avec support
  local de la langue choisie et requête imposant ce mode. Sans ce support,
  elle refuse l'écoute ; aucun repli cloud n'est configuré.
  Les permissions sont demandées sur action explicite, la capture est bornée
  à 30 secondes ; en mode dictée, le résultat final complète seulement le brouillon.
  La dictée ne déclenche ni commande, ni approbation, ni réveil permanent.
- Le mode conversation vocale est explicitement activé et chaque tour démarre
  sur pression. Il soumet le texte final au hub après le checkpoint durable,
  puis lit sa seule réponse finale corrélée par contexte et requête. Les cartes
  de décision restent manuelles. La navigation et l’arrière-plan invalident
  le contexte ; aucune boucle de réactivation du microphone n’est installée.
- Les hôtes fournissent caméra, sélection de fichier bornée, localisation
  ponctuelle et ouverture confirmée de destinations fixes. Les aperçus restent
  locaux et temporaires ; aucun texte de modèle n’est interprété comme une
  commande du téléphone. La lecture est bornée à 1 Mio sur Android, 8 Mio sur
  iOS ; son annulation reste coopérative vis-à-vis du fournisseur. Android
  efface les aperçus en quittant le panneau ou à la pause, tout en acceptant le
  retour d'un sélecteur/caméra externe lancé explicitement. iOS les conserve
  entre onglets jusqu'à effacement explicite ou passage en arrière-plan.
  Les restrictions de l’OS et permissions s’appliquent.
- `hakille-mobile-gateway` est un processus séparé, lancé explicitement. Il
  écoute sur IPv4 loopback et traduit les échanges HTTP authentifiés vers
  l'IPC local existant. Le démon conserve ses vérifications de capacités et
  son journal d'audit. Le protocole IPC v1 reste inchangé.
- Le jeton de la passerelle est distinct du jeton IPC. C'est un secret donnant
  accès aux opérations exposées du hub, à conserver dans le trousseau natif.
  La connexion distante nécessite une terminaison HTTPS de confiance ; la
  passerelle seule ne fournit ni TLS, ni découverte, ni appairage automatique.
- Les réponses sont corrélées, bornées et liées à une session. Un résultat
  incertain après une coupure n'est jamais présenté comme réussi ni rejoué
  automatiquement. Les cartes d'une ancienne session sont invalidées.
- Le checkpoint mobile **v2**, distinct du protocole IPC v1, conserve le
  brouillon, le compteur de requêtes et les marqueurs d'incertitude. L'hôte
  attend sa sauvegarde atomique synchronisée avant HTTP. Un échec connu avant
  émission est déclaré via `fail_before_send` ; ce chemin est interdit dès
  que le transport a commencé. À la restauration, une mutation en cours
  devient `unknown`, sans rejeu ni session ou carte réutilisable. Un fichier
  corrompu est refusé sans écrasement. Le statut et le journal restent
  consultables ; les mutations exigent la prise en compte explicite des
  avertissements. Archiver un avertissement ne confirme ni n'annule l'effet
  métier. Les hôtes préservent ces marqueurs lors d'une déconnexion ou d'un
  changement de hub.
- Une approbation de carte exige un maintien de 600 ms mesuré par le client
  Rust ; le passage en arrière-plan annule ce maintien. Ce contrôle local
  n'est pas une preuve cryptographique de présence humaine pour le serveur.
  L'accusé du noyau indique l'effet réel, notamment `outbox_enqueued`, qui
  signifie une mise en file durable et non une exécution métier.
- Le substrate mobile expose les fonctionnalités installées par l'hôte. Un
  pilote absent ne donne pas de permission, d'attestation matérielle, de
  mesure de batterie ou de capacité NPU supposée. Le conteneur de fichiers
  est réel ; les fonctions SAF et trousseau exigent leurs fournisseurs natifs.
- La synchronisation d'ombre conserve les opérations jusqu'à un accusé
  explicite, avec identifiants persistants et génération de connexion. Ses
  conflits restent observables. Ce composant n'établit pas à lui seul une
  réplication réseau authentifiée, un service APNs/FCM ou un cluster TiKV.
- Le handoff signé de `hakille-sync` utilise son propre protocole **v2** et
  exige une paire identité d'appareil/clé publique explicitement approuvée
  par un canal de confiance. Une clé reçue dans un message n'est jamais une
  autorisation d'appairage. Le v1 est refusé ; rotation et révocation sont
  explicites. Le jeton de la passerelle n'établit pas cette identité.

## Conséquences et limites

Le démon sert actuellement une seule connexion IPC à la fois. La passerelle
occupe cette connexion : le HUD desktop doit être déconnecté pendant son
utilisation. Ajouter plusieurs clients exige une évolution explicite de la
gestion des cartes et de l'orchestrateur, et non plusieurs connexions ouvertes
en supposant une concurrence inexistante.

Les brouillons locaux sont séparés des secrets et des effets à transmettre.
Le checkpoint contient du texte personnel, mais pas de charge utile rejouable,
de jeton ou d'URL de hub. Les archives d'incertitude sont bornées et ne
remplacent pas le journal du hub ; changer de hub n'efface pas l'incertitude
sur le précédent. Android utilise son stockage privé sans sauvegarde système,
iOS un fichier protégé exclu des sauvegardes cloud. La dernière frappe peut
précéder sa sauvegarde asynchrone ; l'envoi HTTP attend toujours le checkpoint.
Une réponse de modèle n'est pas une commande système à exécuter sur le téléphone.
Les politiques Rust de veille et de température produisent des recommandations ;
leur exécution par un service OS doit être raccordée et mesurée avant toute
promesse d'écoute permanente ou de travail en arrière-plan.

La fusion CRDT retient une version déterministe par identifiant, avec priorité
aux tombstones ; elle n'est pas une fusion sémantique sans perte. L'horloge
vectorielle globale n'établit pas une causalité par élément. Les tombstones
ne sont plus supprimés sur leur seul âge : leur accumulation reste une limite.
Les échanges actuels transportent l'état complet ; l'application au graphe
ne propage pas encore les suppressions. Les hôtes doivent approuver les pairs,
protéger les clés et persister le cache antirejeu avant utilisation du contexte
reçu, puis le restaurer avant réception après redémarrage. Cette persistance
et le transport réseau ne sont pas raccordés automatiquement aux applications.
Voir le [contrat de synchronisation](../crates/hakille-sync/README.md).

La compilation Android, la compilation iOS sur macOS et les tests Rust sont des
preuves distinctes. Les essais TalkBack/VoiceOver, verrouillage, rotation,
mémoire, réseau intermittent et batterie sur appareils réels restent requis
avant d'annoncer un support mobile de production. Aucun runtime Python n'est
ajouté au produit.

Les scripts `verify-android-apk.py`, `test-android-runtime.py` et
`test-ios-simulator.sh` distinguent inspection du paquet, parcours natif sur
émulateur dédié et exécution XCTest sur Mac. `cargo xtask tokens --check`
contrôle aussi les exports Kotlin et Swift versionnés. Leur présence ne vaut
pas preuve d'exécution : les résultats acquis et les vérifications encore
nécessaires sont dans le
[rapport de seconde itération](../docs/VALIDATION_MOBILE_SUITE_2026-11-04.md).
