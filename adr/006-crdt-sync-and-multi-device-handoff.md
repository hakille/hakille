# ADR 006: Réconciliation Multi-Appareils sans Conflit par CRDT et Handoff Continu

## Statut
Accepté

## Contexte
L'utilisateur d'Hakille utilise plusieurs appareils au cours de sa journée : son smartphone en déplacement, son ordinateur de travail, et son hub domestique le soir.
Deux écueils majeurs guettent ce mode de fonctionnement :
1. **Les conflits d'écriture hors-ligne** : Des heuristiques de fusion classiques de type "last-write-wins" basées sur des horloges système entraînent inévitablement des pertes destructives de données mémorielles.
2. **La rupture de session au changement d'écran** : Devoir réexpliquer à son ordinateur ce que l'on vient de dicter sur son téléphone brise le premier critère cardinal d'Hakille : la **continuité d'état**.

## Décision
Adopter une réconciliation mathématique décentralisée basée sur les types de données répliqués sans conflit (**CRDT**) au sein du crate `hakille-sync` :
- Utilisation de **Grow-Only Sets avec tombstones** pour l'ajout et l'invalidation de faits dans le graphe.
- Horloges vectorielles pour ordonner causalement les événements sans dépendance à une synchronisation d'horloge NTP centrale.
- Implémentation d'un coordinateur de continuité (**`HandoffCoordinator`**) qui synchronise l'état de l'échange en cours (dernière phrase prononcée, action en attente d'approbation) entre le hub et les terminaux clients.

## Conséquences
- **Positives** : Garantie mathématique de convergence éventuelle sans perte de données, fonctionnement hors-ligne complet avec rattrapage fluide à la reconnexion, passage d'un appareil à l'autre sans perte de contexte.
- **Négatives / Défis** : Légère augmentation de la taille du journal local due à la rétention des tombstones jusqu'au cycle de compaction garbage-collection.
