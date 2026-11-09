# ADR 023 — Harness de recherche borné et preuves d'exécution

Statut : implémenté. Complète ADR 022 (mémoire cognitive continue).

## Contexte

La mémoire ne suffit pas à établir la vérité d'une réponse. Le noyau doit
séparer demande, plan proposé, outil réellement appelé, résultat obtenu et
synthèse. S2 marquait ses étapes textuelles comme réussies et son provider
configuré ne correspondait pas toujours au moteur local exécuté. Aucun outil
de recherche web n'était relié à ce chemin.

## Décision

Conserver le noyau Rust et son autorisation Biscuit. Introduire un
`ResearchBackend` asynchrone avec deux opérations typées, recherche et lecture,
un ordonnanceur déterministe à budgets et reprises bornés, et une trace
durable dans les épisodes existants. Le démon implémente Brave/SearXNG et les
lectures HTTPS avec reqwest ; `scraper` fournit le parsing HTML maintenu.

La demande réseau est explicite et liée par SHA-256 au contexte autorisé.
Les pages sont des données, ne sélectionnent aucun outil et ne reçoivent pas
les souvenirs privés. Les étapes de raisonnement ne sont réussies qu'avec des
reçus ; les références citées sont vérifiées avant présentation. L'échéance
de décodage et les instructions système sont communes à S1/S2.

Les APIs tierces ne sont pas émulées par le moteur local. Une collecte partielle,
une erreur de modèle ou une absence de configuration restent visibles.

## Conséquences

La boucle est inspectable, testable sans modèle ni fournisseur payant, et
utilisable avec les mêmes commandes via le REPL et l'IPC authentifié. Les
traces partagent la durabilité, les partitions et l'oubli des épisodes.
Les anciennes bases restent compatibles via un champ optionnel.

Le périmètre est volontairement limité aux lectures publiques. La vérification
des citations ne prouve pas l'implication sémantique. Le proxy explicite devient
une frontière de confiance pour la résolution finale. Les budgets LLM et de
collecte sont distincts ; les limites sont détaillées dans
[HARNESS_END_TO_END](../docs/HARNESS_END_TO_END.md).
