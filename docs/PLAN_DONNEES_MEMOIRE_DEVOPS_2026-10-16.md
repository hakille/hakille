# Plan données, mémoire, Docker, CI/CD et supply chain d'Annora

**Décision proposée :** Annora doit être **local-first**. `redb` reste la base canonique d'un appareil ; le graphe versionné est la mémoire canonique ; l'index vectoriel est un index reconstruisible, jamais la vérité ; la synchronisation inter-appareils transporte des opérations chiffrées et signées, jamais une base de données partagée en clair. Docker ne sert ni à emballer le desktop ni à exécuter les modèles par défaut : il sert au développement, aux tests d'intégration et aux déploiements de hub/serveur explicitement choisis.

## 1. État actuel et diagnostic

| Domaine | Existant | Limite à fermer |
|---|---|---|
| Données locales | `redb`, triples SPO, trois index, journal d'audit persistant | schéma, migrations, sauvegarde et restauration ne sont pas un contrat unique |
| Mémoire vectorielle | trait `VectorIndex`, embeddings 384-d, store edge/cluster | implémentation actuelle brute-force O(N), pas HNSW réellement branché |
| Sync | CRDT, vector clocks, delta chiffré, tombstones | pas de transport pair/hub complet, identité pair, anti-rejeu durable ou observabilité opérationnelle |
| Cluster | abstraction `ClusterStore`, mention TiKV | pas de client TiKV réellement branché ni d'exploitation de cluster |
| Données auxiliaires | audit en `redb`, consentement et outbox en JSON | risque de mises à jour non atomiques, chiffrement/hardening inégal |
| Docker | absent | aucune image reproductible, aucun environnement de test de services |
| CI | format, check, tests, conformance, deny partiel | pas de scans, SBOM, provenance, signature, release ni politique de promotion |

Le bon choix n'est donc pas « une nouvelle base universelle ». Il faut donner à chaque type de donnée une responsabilité claire.

## 2. Architecture de données cible

```text
                   appareil Annora
 ┌──────────────────────────────────────────────────────┐
 │ redb : vérité locale                                  │
 │  ├─ graphe de mémoire + révisions + contradictions    │
 │  ├─ consentements, préférences, outbox, tâches        │
 │  ├─ journal audit append-only signé                   │
 │  └─ métadonnées d'index vectoriel                     │
 │                                                      │
 │ HNSW : index dérivé, reconstruisible                  │
 │  └─ vector_id → revision_id, modèle, dimension        │
 └──────────┬─────────────────────┬─────────────────────┘
            │ opérations CRDT chiffrées, signées, idempotentes
            ▼
 ┌──────────────────────────────────────────────────────┐
 │ hub personnel / serveur entreprise facultatif         │
 │  ├─ relais opaque et boîte d'envoi                    │
 │  ├─ registre de devices et révocations                │
 │  ├─ sauvegardes chiffrées                             │
 │  └─ stockage distribué seulement quand justifié       │
 └──────────────────────────────────────────────────────┘
```

### Invariants à écrire avant tout changement

1. Le fait source est un triplet immuable versionné ; une correction crée une révision et un lien de révocation, jamais un écrasement silencieux.
2. Le vecteur, le résumé et la réponse d'un modèle sont des **dérivés** avec provenance (`model_id`, hash d'artefact, version de pipeline, date, confidence). Ils ne deviennent jamais vérité sans validation.
3. Le journal est append-only, chaîné et signé ; son stockage et ses clés traversent les redémarrages.
4. Toute mutation atomique modifie la donnée, son index matériel et son événement d'audit dans une même unité de commit locale.
5. Toute réplication est idempotente, authentifiée, chiffrée de bout en bout et rejouable sans effet doublon.
6. Les données brutes sensibles ont une rétention explicite ; les sorties dérivées ont un droit d'effacement et de reconstruction.

## 3. Choix de base de données par étape

### Phase alpha — `redb` uniquement, pas de cluster

Conserver `redb` comme base embarquée par appareil. C'est cohérent avec le projet Rust natif et ses propriétés de transaction, MVCC et crash-safety. Ne pas introduire SQLite, Postgres ou TiKV dans l'application desktop tant qu'il n'existe pas un besoin mesuré que `redb` ne couvre pas.

Créer un crate `annora-store` qui possède tous les noms de tables, codecs, versions de schéma, migrations et sauvegardes. Les autres crates passent par des traits de domaine (`MemoryRepository`, `AuditRepository`, `ConsentRepository`, `OutboxRepository`) ; elles n'ouvrent jamais une table `redb` directement.

Tables initiales :

| Groupe | Tables logiques | Notes |
|---|---|---|
| Mémoire | `facts`, `revisions`, `edges`, `context`, `contradictions` | clés UUIDv7, index SPO/POS/OSP, provenance obligatoire |
| Recherche | `vector_meta`, `vector_payload`, `index_generation` | index dérivé, reconstruction versionnée |
| Sécurité | `consents`, `capability_revocations`, `device_keys` | écriture auditée et dates d'expiration |
| Opérations | `tasks`, `outbox`, `sync_cursor`, `sync_inbox` | clé d'idempotence et statut de retry |
| Audit | `audit_entries`, `audit_order`, `audit_checkpoints` | fichier séparé autorisé pour isolation et restauration contrôlée |
| Gestion | `schema_version`, `migration_log`, `backup_manifest` | migration forward-only et recovery testée |

**Seuil de sortie :** migration testée depuis chaque version supportée ; corruption simulée ; sauvegarde chiffrée → restauration sur une machine neuve → vérification du journal et du graphe.

### Phase beta — synchronisation, pas « cluster magique »

Livrer d'abord un **hub de synchronisation Rust** très étroit : authentification des appareils, boîte d'envoi, accusés de réception, limite de taille, rate limit, révocation, health checks et blobs chiffrés opaques. Le hub ne peut pas lire le graphe ou les vecteurs.

Le protocole doit définir :

- `device_id`, clé publique, attestation d'enrôlement et révocation ;
- enveloppe signée avec `operation_id`, `parent_clock`, timestamp logique, hash de payload et nonce anti-rejeu ;
- delta CRDT compacté, snapshot signé et stratégie de tombstones ;
- reprise après perte réseau, ordre indifférent, duplication et conflit ;
- limites de rétention du hub, export et effacement.

Le CRDT actuel est une base d'algorithme ; ne pas l'exposer sur Internet avant cette couche.

### Phase entreprise/sovereign — TiKV seulement avec un besoin prouvé

TiKV devient pertinent lorsque le hub doit stocker durablement beaucoup de boîtes chiffrées, assurer haute disponibilité multi-nœud, ou servir plusieurs milliers d'appareils. Il n'est pas nécessaire à l'alpha ni à un hub personnel mono-machine. Il implique PD, nœuds TiKV, Raft, sauvegardes, upgrades, monitoring et réponse à incident.

Avant adoption : prototype d'un vrai client Rust, authentification mTLS, clés de partition stables (`tenant/device/object`), tests de partition réseau et preuve de restauration. Le cluster ne stocke que des données chiffrées côté client ; les clés restent sur les appareils ou dans un KMS entreprise.

**Décision de coût :** pas de TiKV avant d'avoir une métrique démontrant que `redb + hub mono-instance` ne tient plus la charge ou la disponibilité cible.

## 4. Plan mémoire cognitive

### 4.1 Modèle de mémoire à cinq couches

| Couche | Durée | Support | Règle |
|---|---|---|---|
| Contexte de travail | secondes/minutes | mémoire RAM chiffrée si nécessaire | purgé à la fin de tâche sauf distillation explicite |
| Épisodes | jours/années | triples + révisions | « qui/quoi/quand/pourquoi » avec source et consentement |
| Sémantique | durable | graphe SPO + index | assertions distinguées des hypothèses et sorties IA |
| Procédurale | durable mais révocable | tâches, plans, politiques | jamais automatiquement promue depuis une conversation externe |
| Audit | durée réglementaire choisie | journal séparé signé | preuve de décision, pas mémoire conversationnelle de confort |

### 4.2 Écriture mémoire : pipeline obligatoire

`entrée → provenance/trust → extraction candidate → déduplication → contradiction → consentement → révision → audit → indexation asynchrone`.

Toute entrée externe reste non fiable. Une proposition de mémoire est visible, explicable et refusée par défaut pour les catégories sensibles. La mise à jour du HNSW est asynchrone mais possède un `index_generation` ; une requête peut donc annoncer « index en cours de mise à jour » plutôt que prétendre à une exhaustivité inexistante.

### 4.3 Recherche hybride

1. Filtrer d'abord par contexte, propriétaire, droits, période et confiance.
2. Exécuter recherche exacte/graphe pour faits et relations.
3. Exécuter ANN vectoriel sur le sous-ensemble autorisé.
4. Fusionner les résultats avec une formule versionnée ; afficher provenance, fraîcheur et incertitude.
5. Ne jamais laisser la similarité vectorielle déclencher une action ou modifier un fait.

### 4.4 Vectoriel : décision pragmatique

Le trait `VectorIndex` est la bonne frontière ; l'implémentation actuelle doit être nommée `BruteForceVectorIndex` sans ambiguïté. Ajouter deux spikes derrière le trait : HNSW pur Rust et brute-force SIMD. Choisir sur rappel@10, p95, RAM, temps de rebuild, qualité de suppression et support de dimension 384.

Critères de sortie HNSW : rappel@10 ≥ 0,98 contre brute-force sur corpus de référence ; suppression/tombstone vérifiée ; rebuild atomique ; mémoire plafonnée ; tests à 10k, 100k et 1M vecteurs. Si l'index ne satisfait pas ces critères, rester brute-force pour les petites collections plutôt que simuler une recherche rapide.

## 5. Docker : usage contrôlé, jamais imposé au poste utilisateur

### Ce qui entre dans Docker

- hub de synchronisation, workers de maintenance et API d'administration ;
- dépendances d'intégration (TiKV quand/si adopté, observabilité, proxy de tests) ;
- environnements de développement reproductibles et tests de chaos ;
- images de release serveur, pas le shell Windows/macOS de l'utilisateur.

### Ce qui n'entre pas dans Docker

- application desktop native, hotkeys, tray et permissions OS ;
- données personnelles locales, clés utilisateur et cache de modèles par défaut ;
- modèle en poids téléchargé au build ; ces artefacts suivent un manifeste séparé et ne doivent pas gonfler l'image.

### Structure cible

```text
deploy/
  docker/
    Dockerfile.hub          # multi-stage Rust, runtime minimal non-root
    Dockerfile.test         # outils d'intégration seulement
  compose/
    compose.dev.yml         # hub + services test, volumes nommés
    compose.chaos.yml       # partitions/fautes, jamais production
  k8s/                      # seulement après le pilote entreprise
  scripts/                  # health, backup-restore, smoke tests
```

Exigences image : builds multi-stage, image de base épinglée par digest, utilisateur non-root, système de fichiers read-only, `tmpfs` explicite, capabilities Linux retirées, port et egress minimaux, healthcheck, labels de commit/version, zéro secret dans ARG/ENV/layer. Les images sont construites avec SBOM et provenance, puis signées et vérifiées avant déploiement.

## 6. CI/CD — pipeline cible obligatoire

### Pull request : rapide et bloquant

| Gate | Commande/type | But |
|---|---|---|
| Reproductibilité | toolchain + `--locked` | aucun lockfile ou Rust implicite |
| Style | `cargo fmt --check` | format déterministe |
| Qualité | Clippy sans warnings, tests unitaires/propriétés | dette zéro ou budget dégressif explicite |
| Contrats | protocole, migrations, graph/audit/CRDT | compatibilité et invariants |
| Sécurité | tests de refus de capability, secret scanning, SAST | empêcher les effets non autorisés |
| Dépendances | `cargo deny` bans/sources, `cargo audit` | vulnérabilités et sources inattendues |
| UI | snapshots headless + AccessKit | pas de régression visuelle/a11y |

Les tests d'inférence à poids réels sont un job séparé, déclenché sur runner contrôlé et manifestes d'artefacts vérifiés. Ils ne doivent jamais télécharger un modèle non épinglé pendant une PR ordinaire.

### Main : intégration et plateformes

- matrice Windows, Ubuntu et macOS ; Redox/Web seulement quand une cible est réellement maintenue ;
- tests intégration Docker du hub, reprise de sync, coupure réseau, backup/restore ;
- benchmark non bloquant au début, puis seuils p95 bloquants une fois une baseline stabilisée ;
- génération SBOM CycloneDX/SPDX ; archivage des rapports de tests, couverture et golden images ;
- analyse de dérive du schéma et compatibilité de protocole N-1.

### Tag release : promotion, pas recompilation locale

1. tag signé et changelog validé ;
2. build hermétique depuis le tag ;
3. binaires desktop et images serveur versionnés par commit SHA ;
4. hashes, SBOM, provenance SLSA/in-toto, attestations Docker et signature Sigstore/cosign ;
5. smoke test d'installation dans une VM neuve ;
6. publication dans un canal `canary`, puis `stable` après approbation humaine ;
7. rollback vers l'artefact signé précédent, jamais « latest » mutable.

## 7. Supply chain — règles de confiance

1. Actions GitHub épinglées par SHA, permissions minimales par job, pas de secrets dans les forks.
2. `Cargo.lock` obligatoire ; mise à jour de dépendance dans une PR isolée avec rapport de diff.
3. Registre de provenance des modèles distinct du code : URL/révision, hash obligatoire, taille, pipeline, évaluation et approbation.
4. Toutes les images et binaires portent version, commit, date de build et SBOM ; provenance vérifiée avant déploiement.
5. Les secrets CI utilisent OIDC quand disponible ; pas de clés longues durées dans GitHub Actions.
6. `main` et tags protégés : review obligatoire, status checks, accès mainteneur minimal, aucune publication depuis un poste développeur.
7. Procédure d'incident : désactivation d'une release, révocation de signature/jeton, notification, rotation et post-mortem.

## 8. Feuille de route 180 jours

### Jours 0–30 — fondations locales et CI

- créer `annora-store`, versionner schéma/migrations/backup manifest ;
- déplacer consentement et outbox JSON vers repositories transactionnels ;
- renommer le vectoriel O(N), définir corpus et métriques ;
- rendre CI stricte sur format, check, tests, Clippy et sécurité ;
- ajouter Dockerfile de test et compose de développement, sans production.

**Sortie :** restauration locale vérifiée, zéro fichier JSON métier isolé, CI déterministe et verte.

### Jours 31–90 — mémoire et hub de synchronisation

- spikes HNSW et corpus d'évaluation ;
- implémenter opérations sync signées, inbox/outbox durables, anti-rejeu et tests de partition ;
- hub mono-instance Dockerisé avec healthcheck, limites et stockage opaque ;
- backups chiffrés, drill de restauration et métriques de sync.

**Sortie :** deux appareils synchronisent après perte réseau sans doublon, conflit silencieux ou perte de journal.

### Jours 91–180 — beta entreprise et promotion release

- SSO/RBAC et stockage serveur seulement si nécessaires au pilote ;
- évaluer TiKV sur charge et panne, sans migration par défaut ;
- signer binaires/images, SBOM/provenance, canary/stable, rollback ;
- tests de chaos Docker et runbooks d'incident/restauration.

**Sortie :** un pilote peut déployer, sauvegarder, restaurer, vérifier l'origine d'un artefact et revenir à une release antérieure sans intervention d'ingénierie.

## 9. Décisions à valider maintenant

1. `redb` reste la vérité locale alpha/beta ; aucun serveur de base de données pour le desktop.
2. L'index vectoriel est dérivé et soumis à benchmark avant HNSW.
3. Le premier hub est Rust, mono-instance, opaque et Dockerisé ; TiKV est reporté à un besoin de disponibilité/scalabilité mesuré.
4. Docker est limité au serveur, aux tests et au développement, jamais obligatoire pour l'utilisateur desktop.
5. Toute release doit fournir tests, SBOM, provenance, signature et plan de rollback avant d'être stable.

