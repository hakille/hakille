# Harness : diagnostic et plan de réalisation

La deuxième étape prolonge la mémoire continue. Le chemin existant possède des
contrôles constitutionnels et des capabilities, mais pas de recherche web
connectée. S2 découpe le texte en phrases, appelle Qwen local même lorsqu'un
provider différent est sélectionné, facture un coût estimé et considère ses
étapes proposées comme exécutées. Son timeout externe ne transmet pas
l'échéance au décodeur. Une autocritique échouée peut devenir une autorisation.

Le travail porte sur quatre contrats vérifiables :

1. Instructions système communes à S1/S2 ; citations externes séparées des
   instructions ; neutralisation des délimiteurs ChatML provenant des données.
2. Raisonnement honnête : provider réellement exécuté, échéance de décodage,
   coût local explicite, réflexion fondée sur des reçus et échec de critique
   qui ne devient pas un succès.
3. Recherche bornée : requête explicite, capability réseau, recherche puis
   lecture des pages, déduplication, reprise limitée des erreurs transitoires,
   quotas et délais, sources horodatées et résultats partiels explicites.
4. Traçabilité : enregistrement durable avant/après chaque outil, conservation
   des extraits et références dans l'épisode, sans promotion en fait utilisateur.

Le réseau sera déclenché par `:research <requête>` ou `:read-web <URL>`,
accessibles au REPL et par le champ de saisie IPC authentifié. Ces commandes
constituent la demande explicite de sortie réseau ; aucun souvenir privé n'est
ajouté aux requêtes envoyées. Le moteur de recherche sera configurable (Brave
ou SearXNG), sans fournisseur silencieusement choisi. Les pages publiques
seront lues en HTTPS avec restrictions d'adresses, redirections contrôlées et
réponses bornées. Les sources ne pourront pas choisir un nouvel outil.

Cette première boucle de recherche sera déterministe et inspectable. Elle ne
prétend pas être un navigateur JavaScript, un planificateur universel ou une
preuve automatique de vérité des affirmations du modèle.

## Réalisation

Les quatre contrats sont intégrés au chemin du démon. Les essais ont également
conduit à contrôler l'échéance du préfill entre blocs de tokens et à rendre
la lecture d'URL indépendante du modèle. Les mises à jour de dépendances
corrigent les dix alertes initiales, sans nouvelle exception. Voir
[le contrat effectif](HARNESS_END_TO_END.md) et
[les vérifications exécutées](HARNESS_VALIDATION.md).
