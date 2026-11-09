# Mémoire Hakille de bout en bout

Le démon utilise désormais `<data_dir>/memory.redb` comme source durable.
Les faits, leurs libellés, les conflits en attente, les corrections et le
contexte court sont conservés dans un checkpoint transactionnel redb. Les
échanges complets sont archivés séparément, sans troncature du texte stocké.

## Parcours effectif

```text
REPL / IPC → autorisation → épisode Started (commit)
                         → extraction prudente des déclarations User
                         → checkpoint faits + conflits (commit avant inférence)
                         → BM25 + MiniLM optionnel + fusion par rang
                         → faits sourcés + épisodes récents/pertinents bornés
                         → S1 ou S2 reçoit ce contexte avant génération
                         → épisode Completed / Failed + checkpoint (commit)
                         → audit : ID épisode + IDs des faits rappelés
```

Un crash pendant l'inférence laisse un épisode `Started`, et non une réponse
inventée. Une erreur de persistance remonte à l'appelant. REPL et IPC confirment
`:remember` / `MemoryRemember` après sauvegarde ; les conflits IPC sont eux
aussi sauvegardés. Le journal d'audit reste distinct, signé et append-only.

Au premier démarrage de cette version, `memory.json` sous `data_dir`, ou à
défaut `./hakille-memory.json`, est importé dans le checkpoint. L'original est
conservé intact. Un JSON invalide bloque la migration ; une base corrompue ou
une version inconnue bloque l'ouverture. Après migration, un ancien JSON ne
peut pas remplacer une base déjà initialisée.

## Trois formes de mémoire

- **Faits symboliques** : sujet, prédicat, objet, source, confiance, partition.
  Les identifiants de contenu distinguent la casse ; les noms d'entités restent
  normalisés. Des faits de partitions différentes ne se contredisent pas.
- **Épisodes** : prompt intégral, réponse effectivement obtenue ou erreur,
  statut, date, partition, niveau de confiance de la source et IDs rappelés.
  Le contexte court peut être compacté sans effacer ces archives.
- **Travail et corrections** : focus/objectifs/contexte court restaurés,
  contradictions dédupliquées et persistantes, historique des résolutions.
  Les anciennes valeurs restent consultables sans devenir des faits actuels.
  Lorsqu'un ancien épisode rappelle une valeur corrigée, le contexte indique
  aussi la valeur actuelle, même si la recherche ne trouvait que l'épisode.

Les textes du modèle et le journal du tour ne sont plus insérés comme des faits
utilisateur certains. Une distillation conserve sa confiance et sa source
`Inferred`, avec `TrustLevel::Untrusted` : elle est inspectable mais exclue du
rappel automatique. Les invités et les contenus externes ne peuvent pas créer
une déclaration `User` par extraction. L'extraction est une heuristique limitée
aux déclarations directes, pas un parseur linguistique universel.

## Rappel et contexte

`HybridRetriever` travaille sur le triplet complet, y compris les libellés.
Le classement lexical BM25 normalise les accents français et retire une liste
explicite de mots courants. Les scores lexicaux et cosinus sont fusionnés par
rang (RRF), puis départagés de façon déterministe. Un score de rappel mesure
la pertinence, pas la probabilité qu'un fait soit vrai.

Quand MiniLM est présent, le démon branche son véritable forward ONNX :

```bash
cargo run -p hakille-daemon -- models download --model all-minilm-l6-v2
cargo run -p hakille-daemon -- models verify
```

Sans modèle, le rappel lexical reste disponible et le mode est annoncé au
boot. Un échec d'encodage est exposé dans le rapport, les logs et l'audit.
Aucun vecteur simulé n'est substitué au modèle. Le cache encode chaque contenu
une seule fois tant qu'il reste inchangé ; les suppressions et les changements
de modèle/partition invalident les données concernées. L'index HNSW est construit
en un lot au-delà de 1 000 vecteurs ; en dessous, recherche cosinus exacte.

Valeurs par défaut : 12 faits, 6 000 octets de preuves, au plus 32 nouveaux
encodages par recherche. Le retard d'indexation est signalé ; la recherche
lexicale continue de couvrir tous les faits visibles. Le contexte combiné
est borné à 10 000 octets avant sérialisation : trois épisodes récents et
quatre épisodes pertinents au maximum, avec déduplication. Les extraits sont
centrés sur les termes recherchés quand ils apparaissent loin dans le texte.
Les archives complètes restent disponibles via `:episodes`.

S1 et S2 utilisent la même enveloppe de citations JSON échappées. Les faits
ambigus sont signalés comme conflits ; les épisodes et conflits ne comptent
pas comme corroborations indépendantes dans S2. Cette discipline réduit la
confusion entre données et instructions, sans prétendre rendre un LLM immunisé
contre toute injection. Les contrôles Biscuit et constitutionnels restent
nécessaires avant tout effet.

Le démon accepte `HAKILLE_MEMORY_BUDGET_MS` (défaut 5 000 ms par étape mémoire,
mesuré et vérifié après l'étape). Ce budget inclut les vrais encodages et les
commits disque ; il ne garantit pas une interruption au milieu d'un forward.
Les budgets de bibliothèque restent inchangés.

## Commandes

| Commande | Effet réel |
|---|---|
| `:remember Alice ville Paris` | Insère un fait, ou conserve un conflit sans écrasement ; accuse après commit. |
| `retiens que le projet Atlas termine mardi` | Une note indépendante et idempotente ; ne concurrence pas les autres notes. |
| `:recall échéance Atlas` | Rappel hybride des faits de la partition active, Default et Shared. |
| `:episodes [texte]` | 20 échanges récents ou pertinents, avec texte complet et état réel. |
| `:contradictions` | Paires en attente avec UUID de l'entrant. |
| `:resolve <uuid-entrant> existing\|incoming` | Décision explicite, checkpoint et historique ; rollback RAM si commit échoue. |
| `:memory-history` | Anciennes et nouvelles valeurs des corrections. |
| `:memory-scope [personal\|professional\|shared\|default]` | Sélection persistante du contexte ; Shared n'ouvre pas les partitions privées. |
| `:forget <préfixe\|uuid>` | Retire le fait, ses conflits/révisions et les épisodes référençant le fait ou ses anciennes valeurs, ou contenant leurs objets exacts ; vide le cache associé. |
| `:forget-episode <uuid>` | Retire un échange et ses copies du contexte court, sans effacer les faits indépendants. |
| `:export` / `:import` | Export/import du graphe uniquement ; ne constitue pas une sauvegarde complète des épisodes. |

La recherche automatique exclut Guest/Untrusted ; les épisodes correspondants
restent conservés dans la base avec leur statut pour inspection explicite par
l'API `MemoryJournal::episode(id)`.

## Limites et choix assumés

- Ce n'est pas une AGI et aucune garantie de rappel parfait n'est revendiquée.
- Le checkpoint sérialise le graphe complet en O(N). La recherche d'archives
  parcourt le disque en O(N), avec une sélection bornée. Un index textuel durable
  et des checkpoints différentiels sont les prochaines optimisations à mesurer.
- Le cache vectoriel du noyau est reconstructible, en RAM ; un redémarrage le
  reconstruit progressivement. Le modèle MiniLM existant est surtout anglophone,
  son seuil 0,40 n'est pas calibré sur un corpus utilisateur francophone.
- Les textes sont stockés localement **sans chiffrement** dans cette base.
  L'oubli est logique : il ne garantit pas l'effacement physique des anciennes
  pages redb, des exports, des JSON legacy ni du journal d'audit indépendant.
  La purge textuelle détecte les occurrences exactes, pas toutes les paraphrases.
- Il n'y a ni réplication réseau des épisodes ni synthèse neuronale des archives.
  Le compteur `archived_summary` du contexte court reste un compteur honnête.
- `MemoryManager`, `MemoryRuntime` et `EdgeStore` restent des APIs de bibliothèque
  historiques. Le démon utilise `MemoryJournal` et le graphe de l'orchestrateur ;
  il ne maintient pas en parallèle une copie JSON ou une seconde base vectorielle.

## Vérifications

```bash
cargo test -p hakille-memory --locked
cargo test -p hakille-kernel --locked
cargo test -p hakille-daemon --locked
cargo test --workspace --locked
```

Les tests couvrent les redémarrages, migrations, corruptions, textes longs,
épisodes interrompus/échoués, conflits/résolutions, provenance, partitions,
cache, limites de contexte et absence de résurrection après oubli. Le test
`memory_real_minilm_retrieves_paraphrase_with_provenance` exécute réellement
MiniLM si son artefact est présent ; sinon il imprime un `SKIP` explicite.
Les résultats de la validation complète et les limites de l'environnement
sont consignés dans [MEMORY_VALIDATION.md](MEMORY_VALIDATION.md).

## Sources issues d'outils

Le champ optionnel `tool_trace` des épisodes conserve les recherches, lectures,
extraits et échecs. Ces sources peuvent être rappelées comme archives externes
datées et sont incluses dans l'oubli d'un épisode ou d'un fait associé. Elles
ne deviennent jamais automatiquement des déclarations utilisateur. Voir
[HARNESS_END_TO_END.md](HARNESS_END_TO_END.md).
