# Mémoire cognitive : diagnostic et plan d'intégration

## Lecture du système avant intervention

Hakille est un système cognitif local en Rust : les substrats exposent les
capacités matérielles, la perception produit des observations, le noyau route
vers S1/S2, les plans passent les contrôles constitutionnels et Biscuit, puis le
démon expose le résultat par REPL/IPC. L'audit et l'outbox restent distincts de
la mémoire : mémoriser une intention ne signifie pas l'avoir exécutée.

Le graphe SPO, les partitions, redb, les embeddings MiniLM réels et HNSW
existent déjà. Cependant, le chemin du démon utilise un snapshot JSON et le
rappel du noyau réencode chaque objet à chaque appel, répété pour chaque mot
du prompt. S1 reçoit uniquement le prompt ; les souvenirs sont ajoutés au texte
de réponse après l'inférence. Le contexte court supprime les anciens échanges
au profit d'un compteur. Les conflits sont volatils, les journaux tronqués à
140 caractères deviennent des faits utilisateur de confiance 1.0, et deux
« retiens que » indépendants se disputent le même sujet/prédicat.

## Plan

1. **Conservation** : journal épisodique redb, texte intégral et état du tour
   (en cours, terminé, échoué), checkpoint transactionnel du graphe, de ses
   libellés, du contexte de travail et des contradictions. Migration JSON
   non destructive ; base invalide refusée, jamais remplacée par une mémoire vide.
2. **Rappel** : recherche lexicale sur le triplet complet, fusion avec les
   vecteurs réels, cache invalidé sur changement de contenu ou de modèle,
   sélection déterministe et bornée avec provenance. Pas d'embedding simulé.
3. **Continuité** : échanges récents et archives pertinentes disponibles pour
   S1 et S2 avant génération ; les réponses du modèle restent des épisodes,
   jamais des preuves indépendantes de vérité. Le contexte court peut être
   compacté sans supprimer les archives durables.
4. **Cohérence** : conflits persistants et dédupliqués, résolution explicite,
   extraction prudente des déclarations utilisateur, confiance de distillation
   conservée. Oubli explicite des épisodes et des faits.
5. **Vérification** : redémarrage, migration, corruption, suppression,
   confidentialité des partitions, cache vectoriel, budgets du contexte,
   parcours du noyau sans modèle et gates du workspace.

## Choix techniques et limites

Conserver Rust, redb, MiniLM/ONNX et HNSW évite d'ajouter un service distant
ou un modèle obligatoire pour corriger des pertes structurelles. MiniLM
all-MiniLM-L6-v2 reste optionnel et surtout anglophone ; sa qualité en français
devra être évaluée sur des requêtes réelles avant de choisir un encodeur
multilingue et de reconstruire son index. Aucun téléchargement de plusieurs Go
ne remplace les tests de persistance et de rappel.

Ce travail ne constitue pas une AGI ni une garantie de rappel parfait. La
conservation durable, la pertinence d'une recherche et la vérité d'une
information sont trois propriétés différentes, à tester séparément.

## Réalisation

Les cinq étapes sont implémentées dans le chemin du démon et du noyau, sans
nouvelle dépendance Cargo. Les détails du contrat effectif et ses limites sont
dans [MEMORY_END_TO_END.md](MEMORY_END_TO_END.md), la décision dans
[ADR 022](../adr/022-continuous-cognitive-memory.md), et les résultats des tests
et de l'essai avec les modèles réels dans
[MEMORY_VALIDATION.md](MEMORY_VALIDATION.md).
