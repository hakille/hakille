# Mémoire Hakille de bout en bout (Agent E — intégration)

Cycle complet : `:remember` → graphe RAM → snapshot → export/import → `:recall` → `:forget`.

## Diagramme du cycle

```
┌─────────────┐    extraction auto (insensible casse, trim .,!?;:)   ┌──────────────────┐
│  REPL / prompt│ ── "je m'appelle…", "retiens que…" ──────────────▶ │ CognitiveOrch.   │
│  hakille-daemon│    :remember S P O ──▶ orchestrator.remember() ───▶ │  memory_graph    │
└─────────────┘                                                     │  (RAM, Knowledge │
       ▲                                                            │   Graph + SPO)     │
       │ :recall ──▶ orchestrator.recall() ── étages GraphQuery      └────────┬─────────┘
       │            (poids→confiance, dédupliqué) ◀── search_text ◀──┤        │
       │                                                            │        │ insert (routage
       │ :forget <préfixe-8|uuid> ── révocation + reçu crypto        │        │  contradiction :
       │                                                            │        │  AskUser ⇒ warn +
       ▼                                                            │        │  pending_resolutions,
       │                                                     ┌──────┴────────┴──┐  jamais écrasé)
┌──────────────┐  save_to_file/load_from_file  │ snapshot ./hakille-memory.json │◀─────────────┘
│ :export <fic>│ ── export_graph(Json) ──▶ fichier ── :import ──▶ import_graph + import_into_graph
│ :memory      │    (affiche taille + compte + snapshot auto)   (labels→registres, insert, warn si conflit)
│ :memory-view │    UUID court 8 chars + texte (:forget utilisable)
│ :embed-test  │    [DEMO] hash non sémantique — PAS un vrai embedding
│ :contrad.    │    scan_contradictions (boîte, décision utilisateur)
└──────────────┘
       │ CRDT (sans réseau) : knowledge_graph_to_crdt() / apply_crdt_to_graph() (hakille-sync)
       ▼ redb : EdgeStore (persist_to / hydrate_from) — tier edge, distinct du snapshot JSON
```

## Commandes REPL

| Commande | Effet |
|---|---|
| `:remember Marie aime le jazz` (ou 1–2 mots → `Fait est <texte>`) | `orchestrator.remember()` → `Inserted` / `ContradictionResolved` / `NeedsUserResolution` (rien écrasé, mis en `pending_resolutions` + `warn`). Usage affiché si vide. |
| `:recall jazz` | `orchestrator.recall()` : GraphQuery (tri poids→confiance) + registre SPO + fallback substring + embeddings réels si branchés. Dédupliqué. Usage si vide. |
| `:memory` | Liste `UUID-court (8)` + texte ; hint `:forget`. Vide → 1 phrase + 3 exemples (spec §19). |
| `:memory-view` / `:mem2` / `:contradictions` | Inspection riche (poids, confiance, partitions) / boîte à contradictions. |
| `:forget <préfixe-8 \| uuid-complet>` | Préfixe unique accepté (ambigu ⇒ erreur + conseil `:memory-view`) ; reçu crypto (`Droit à l'oubli`). |
| `:export [fichier]` | `export_graph(Json)` : taille + compte affichés ; écrit le fichier si donné ; **snapshot RAM auto** (`./hakille-memory.json`). |
| `:import <fichier>` | `import_graph` (JSON `:export` v`"1"`, refus explicite sinon) + `import_into_graph` (labels→registres, `insert`, `warn` si conflit) ; **snapshot RAM auto**. |
| `:embed-test <txt>` | **[DEMO]** hash stable non sémantique, norme affichée non mesurée — brancher le vrai MiniLM `ort` pour un vecteur réel. |
| `:exit` / `:quit` (ou EOF) | **Sauvegarde auto** du snapshot avant arrêt. |
| Démarrage daemon | `load_from_file(./hakille-memory.json)` si présent (compte affiché), sinon graphe vide. |

## Fichiers touchés (Agent E uniquement)

- `crates/hakille-kernel/src/orchestrator.rs` — champ `pending_resolutions`, `warn`+stockage dans `remember`, extraction auto insensible à la casse + trim ponctuation + accusé réception (recall déjà trié/dédupliqué : vérifié, inchangé).
- `crates/hakille-daemon/src/main.rs` — restore/save snapshot, `:import` + `import_into_graph()`, `:memory` UUID court, `:embed-test [DEMO]`, gardes `:remember`/`:recall`, `:forget` préfixe.
- `crates/hakille-sync/src/crdt.rs` — ajout seul : `knowledge_graph_to_crdt()` / `apply_crdt_to_graph()` + test roundtrip.
- `docs/MEMORY_END_TO_END.md` — ce fichier.

## Limites honnêtes

- **Volatilité résiduelle** : la RAM reste la source de vérité en session ; seul le snapshot `./hakille-memory.json` (+ `:export`) survit au redémarrage. Crash entre deux sauvegardes = faits perdus. Pas de fsync/dir-sync garanti, pas de journal d'écriture.
- **Snapshot local au CWD** : `./hakille-memory.json` dépend du répertoire de lancement (contrairement à `audit.redb`/`outbox.json` sous `LOCALAPPDATA/Hakille`). Ne pas lancer depuis deux CWD en espérant partager la mémoire.
- **Embeddings optionnels** : `:embed-test` = hash démo ; `recall` sémantique réel seulement si `embedding_engine` (MiniLM `ort`) branché, seuil cosinus 0.40 hérité à recalibrer avec le vrai modèle.
- **Sync sans réseau** : le pont CRDT est une conversion locale (`Triplet` <-> élément) ; `merge`/tombstones/LWW existent mais aucun transport, aucune découverte de pairs, aucun chiffrement de canal.
- **redb ≠ snapshot** : `EdgeStore` (`persist_to`/`hydrate_from`) est un tier edge distinct, non branché au cycle REPL ci-dessus (pas d'auto-persist vers redb au `:exit`).
- **Contradictions** : `AskUser` ne résout rien seul — les paires s'accumulent dans `pending_resolutions` / `:contradictions` jusqu'à décision humaine (`:forget` ou ré-insertion).
