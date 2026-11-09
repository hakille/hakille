# Validation de la mémoire continue — première étape

> Mise à jour : les dix alertes décrites dans ce rapport historique ont été
> traitées pendant la refonte du harness. Voir [HARNESS_VALIDATION.md](HARNESS_VALIDATION.md)
> pour l'état courant et les versions corrigées.

Validation effectuée le 9 novembre 2026 sous Linux x86_64, Rust 1.95.0.
Le graphe Cargo et les exceptions de sécurité existantes n'ont pas été modifiés.

## Vérifications exécutées

| Vérification | Résultat |
|---|---|
| `cargo fmt --all -- --check` | Réussi. |
| `xtask tokens --check` | Réussi ; tokens générés à jour. |
| `cargo check --workspace --all-targets --locked` | Réussi. |
| `cargo build --workspace --all-targets --locked -j 4` | Réussi. |
| `cargo clippy --workspace --all-targets --no-deps --locked` | Réussi avec les avertissements existants ; pas de `-D warnings`. |
| `cargo test --workspace --locked -j 4` | **1 016 tests réussis, 0 échec, 12 ignorés**, sur 90 résultats de suites. |
| `cargo deny check bans sources advisories` | `bans` et `sources` réussis ; **10 alertes bloquantes de dépendances préexistantes**. |

Les tests de conformité font partie de la suite workspace. Les 12 tests longs
déjà marqués `#[ignore]` ne sont pas exécutés en debug, conformément aux
instructions du dépôt. La réussite des tests n'est donc pas une certification
de tous les parcours système, de toutes les plateformes ou de toutes les
combinaisons de features.

La couverture ajoutée vérifie notamment :

- Migration JSON non destructive, redémarrage et restauration des libellés,
  du contexte, des partitions, des conflits et des corrections.
- Refus des bases corrompues ou de schéma incompatible, métadonnées d'import
  invalides et identifiants dupliqués.
- Conservation intégrale des échanges, tours échoués et état `Started`, rappel
  d'un détail ancien après compaction du contexte court.
- Rappel lexical, partitions, provenance, budget d'encodage, cache vectoriel,
  erreurs du vrai chemin d'encodage et budgets de contexte.
- Absence de promotion des réponses du modèle en faits utilisateur certains,
  extraction prudente et séparation des invités/contenus externes.
- Résolution durable, annotation des épisodes anciens avec la valeur corrigée,
  oubli du fait avec toute sa chaîne de corrections et les épisodes associés.
- Intégration du contexte avant génération pour S1/S2 et maintien des contrôles
  de sécurité et d'exécution existants.

## Essais avec modèles réels

Les artefacts MiniLM et Qwen déjà prévus dans le catalogue du projet ont été
téléchargés depuis leurs sources officielles, vérifiés contre leurs SHA-256
attendus et laissés dans `models/`, hors Git. Aucun serveur de modèle externe
n'a été ajouté et aucun embedding de substitution n'est utilisé en production.

Le test `memory_real_minilm_retrieves_paraphrase_with_provenance` a effectivement
exécuté MiniLM : « Who practices medicine? » retrouve le fait concernant Alice,
médecin, face à un autre fait parlant de course automobile. Il ne s'agit pas
d'une évaluation générale du rappel sémantique en français.

Un essai du démon avec un répertoire de données temporaire isolé a suivi ce
parcours :

1. Premier processus : `:remember MissionAtlas identifiant ORION73`, puis
   deux notes indépendantes « jazz » et « café ». Trois faits sauvegardés,
   sans faux conflit entre les notes. Arrêt propre du processus.
2. Nouveau processus : prompt « Quel est l'identifiant de MissionAtlas ? ».
   S1, avec Qwen2.5 0.5B local, répond effectivement :
   **« L'identifiant de MissionAtlas est : ORION73 »**.
3. Troisième processus : `:episodes MissionAtlas` retrouve le prompt et cette
   réponse avec le statut `Completed`. Le graphe contient toujours trois faits ;
   la réponse générée n'a pas été ajoutée comme une nouvelle vérité.

Le tour Qwen a pris environ **44 secondes** sur le CPU de cet environnement.
Ce résultat valide le parcours après redémarrage, pas une promesse de latence
interactive ni de qualité générale du petit modèle.

Pour compiler ici, ONNX Runtime 1.31.0 officiel a été fourni via `ORT_LIB_PATH`
et `ORT_PREFER_DYNAMIC_LINK=1`, puis via `LD_LIBRARY_PATH` pour lancer le démon.
Les en-têtes ALSA du paquet Debian officiel ont été extraits localement et
exposés avec `PKG_CONFIG_PATH` / `PKG_CONFIG_SYSROOT_DIR`. Cela contourne le CDN
ONNX inaccessible et l'absence de droits d'installation système sans modifier
le code ou le lockfile du projet.

## Audit de dépendances restant ouvert

Les alertes suivantes concernent des versions déjà présentes dans `Cargo.lock`.
Elles restent visibles : aucune exception supplémentaire n'a été introduite.

| Dépendance verrouillée | Alerte(s) RustSec | Version corrigée indiquée par l'audit |
|---|---|---|
| tokio 1.53.2 | RUSTSEC-2026-0386 | 1.53.3 |
| wasmtime 48.0.7 | RUSTSEC-2026-0422, 0423, 0424 | 48.0.8 |
| wasmtime-wasi 48.0.7 | RUSTSEC-2026-0420 | 48.0.8 |
| zip 8.6.0 | RUSTSEC-2026-0429 | 9.0.0 |
| zstd 0.13.3 | RUSTSEC-2026-0391 | 0.13.4 |
| zstd-safe 7.3.0 | RUSTSEC-2026-0398, 0403 | 7.4.0 |
| zstd-sys 2.1.0+zstd.1.5.7 | RUSTSEC-2026-0410 | 2.1.2 |

Une mise à niveau coordonnée reste nécessaire, notamment pour `zip`, dont la
version corrigée change de version majeure sous une dépendance de Candle.
L'audit relève aussi la version retirée du registre d'`async-executor` et des
avertissements de doublons et de dépendances locales wildcard. Le contrôle de
sécurité global du dépôt ne peut donc pas être déclaré entièrement réussi.

## Limites fonctionnelles conservées

La conservation durable ne garantit ni une vérité parfaite ni un rappel
exhaustif. Le stockage reste en clair, l'oubli est logique, la purge textuelle
ne détecte pas toutes les paraphrases, le checkpoint et la recherche d'épisodes
restent linéaires. Le cache vectoriel se reconstruit progressivement en RAM et
MiniLM reste surtout anglophone. Les détails, commandes et périmètres sont
décrits dans [MEMORY_END_TO_END.md](MEMORY_END_TO_END.md).
