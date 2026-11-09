# ADR 022 : mémoire continue, épisodes durables et rappel avant inférence

## Statut

Accepté — implémenté dans `hakille-memory`, `hakille-kernel` et `hakille-daemon`.

## Problème

Le graphe persisté ne représentait qu'une partie de l'état cognitif : les
conflits et le contexte disparaissaient, les conversations étaient tronquées
et promues en faits certains, S1 n'utilisait pas les souvenirs avant de
générer. La présence d'un store vectoriel en bibliothèque ne corrigeait pas
ces ruptures dans le chemin effectif du démon.

## Décision

1. Utiliser redb pour un checkpoint cohérent (graphe, libellés, conflits,
   corrections, contexte court, partition active) et une table d'épisodes.
   Écrire l'entrée avant l'inférence, son résultat après, et propager les
   erreurs de commit. Importer le JSON historique une seule fois.
2. Garder les épisodes intégraux séparés des faits. Les échanges courts peuvent
   être compactés ; les archives restent consultables. Les réponses générées
   n'augmentent pas la confiance par auto-corroboration.
3. Injecter avant S1 et S2 un contexte borné de preuves sourcées et d'extraits
   historiques. Signaler les contradictions sans choisir silencieusement.
4. Combiner BM25 et embeddings réels optionnels par fusion de rangs. Utiliser
   le HNSW existant au-delà de 1 000 vecteurs et borner l'indexation froide.
   Conserver un mode lexical explicite, utilisable sans modèle ni réseau.
5. Auditer les identifiants réellement rappelés. Permettre la résolution des
   conflits et l'oubli logique, y compris dans les archives associées.

## Alternatives et conséquences

Un nouveau service vectoriel distant n'améliorerait ni la conservation des
conflits ni la continuité locale et imposerait une dépendance opérationnelle.
Un plus grand LLM ne récupère pas des échanges supprimés. On conserve donc
Rust/redb/MiniLM/instant-distance, sans nouvelle dépendance Cargo.

Le checkpoint complet est O(N), les recherches d'archives sont linéaires et
les embeddings sont un cache en RAM reconstructible. Ce sont des limites
mesurables, pas des performances distribuées revendiquées. Les APIs historiques
EdgeStore/MemoryRuntime restent disponibles mais ne constituent pas une seconde
source durable du démon.

La base reste en clair (0600 sur Unix). L'oubli retire des données de la mémoire
active et des tables logiques ; les sauvegardes, pages anciennes du moteur et
l'audit indépendant ne font pas l'objet d'un effacement physique garanti.
La recherche des références textuelles ne reconnaît pas toutes les paraphrases.

Contrats, commandes et tests : [mémoire de bout en bout](../docs/MEMORY_END_TO_END.md).
