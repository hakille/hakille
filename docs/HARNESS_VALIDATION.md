# Validation du harness

Travail effectué le 9 novembre 2026 sous Linux x86_64, Rust 1.95.0. Cette
validation complète [celle de la première refonte mémoire](MEMORY_VALIDATION.md).
Le tableau initial ci-dessous décrit le travail sur la base `bbabc35`.
Une validation distincte couvre son intégration à `d8ce84d`.
Les tests longs marqués `#[ignore]` n'ont pas été lancés en debug.

## Gates exécutés avant intégration des nouveaux commits distants

| Contrôle | Résultat |
|---|---|
| `cargo fmt --all -- --check` | Réussi. |
| `xtask tokens --check` | Réussi, fichier généré à jour. |
| `cargo check --workspace --all-targets --locked` | Réussi. |
| `cargo build --workspace --all-targets --locked` | Réussi. |
| `cargo clippy --workspace --all-targets --no-deps --locked` | Réussi avec les avertissements historiques ; aucun nouveau lint sur les fichiers du harness. |
| `cargo test --workspace --locked -j 2` | **1 036 réussis, 0 échec, 10 ignorés**, 90 résultats de suites. |
| `cargo deny check bans sources advisories` | **advisories ok, bans ok, sources ok** selon la politique existante. |
| `git diff --check` | Réussi. |

Les deux tests de refus de provider réseau n'exécutent plus de modèle et ont
été déplacés des tests ignorés vers la suite rapide. Les dix tests longs
restants ne sont donc pas masqués par une nouvelle exclusion. Les tests de
conformité font partie de la commande workspace.

L'édition de liens a rencontré une limite d'espace de ce conteneur de 32 Gio.
Le nettoyage des anciennes sorties de compilation, sans toucher aux sources,
a permis de reprendre avec deux jobs et de terminer tous les contrôles.
ONNX Runtime et ALSA utilisent la même configuration locale que celle décrite
dans le rapport de validation mémoire initial.

## Contrats vérifiés

Les tests ajoutés utilisent des backends explicitement nommés fixtures et des
serveurs HTTP isolés. Aucune fixture n'est raccordée au chemin de production.
Ils vérifient notamment :

- Refus avant tout appel si l'autorisation, sa liaison à la requête, le quota
  ou la journalisation sont invalides ; contrôle répété après le commit.
- Déduplication des résultats, reprise transitoire unique, annulation des
  futures d'outils au timeout, résultat partiel et absence de page exploitable.
- Appels `Started` enregistrés avant l'effet, puis résultat réel ; sources et
  trace disponibles après fermeture et réouverture de la base.
- Conservation des sources comme archives externes, sans fait utilisateur
  automatique ; oubli qui examine également le texte des sources.
- Commande de lecture autorisée fonctionnelle même avec un provider LLM
  indisponible ; absence de génération pour une simple lecture.
- Rejet des adresses privées/réservées, IPv4 encodées et IPv6 de transition ;
  séparation de l'endpoint SearXNG configuré et des URLs publiques à lire.
- Requêtes HTTP du fournisseur sans cookies ni contexte mémoire, parsing des
  réponses, refus de redirections du fournisseur, limites de corps annoncé et
  réellement diffusé.
- Extraction HTML sans scripts/styles/templates et sans éléments `hidden` ou
  `aria-hidden=true` ; pas d'exécution JavaScript.
- Références inconnues, absentes ou mal formées refusées ; URLs libres de la
  synthèse refusées. Cela ne vérifie pas l'implication sémantique des sources.
- Délimiteurs ChatML issus des données neutralisés ; instructions communes aux
  deux modes de raisonnement.
- Propositions de plan non comptées comme exécutées, reçus dupliqués ou sans
  preuve refusés, pas de hausse de confiance liée au seul achèvement d'un outil.
- Préfill en blocs avec positions KV conservées et vérification d'échéance
  avant et après chaque bloc.

Le test Qwen réel existant a été enrichi d'une comparaison entre les logits
finaux d'un préfill complet et ceux d'un préfill découpé sur huit tokens, avec
une tolérance absolue de 0,01. Le test utilise le véritable GGUF installé et
poursuit avec la génération réelle et ses callbacks. Il imprime un `SKIP`
explicite si l'artefact manque ; il était présent dans cet environnement.

## Essais du démon

Les essais utilisent des répertoires de données temporaires isolés, jamais la
mémoire personnelle de l'utilisateur. Le proxy de cet environnement et son
bundle CA ont été déclarés via `HAKILLE_RESEARCH_PROXY` et
`HAKILLE_RESEARCH_CA_BUNDLE`, avec TLS vérifié.

Une lecture réelle de `https://example.org/` a produit le titre **Example
Domain**, un extrait effectivement reçu, une heure de consultation et une
empreinte SHA-256. Après arrêt puis redémarrage, `:episodes` et
`:research-trace <UUID>` ont retrouvé la source, le résultat de lecture et la
limite de synthèse constatée. Le graphe contenait toujours **zéro fait** :
la page n'avait pas été promue en déclaration utilisateur.

Cet essai a révélé le coût du préfill d'un long prompt en debug. Le harness a
été corrigé pour contrôler l'échéance entre blocs de 16 tokens, puis pour
séparer `:read-web` (lecture seule, aucun modèle) de `:research` (collecte puis
synthèse). Un calcul matriciel déjà commencé et le chargement du modèle
restent non préemptibles ; le timeout ne constitue pas une borne stricte de
latence du processus.

Sur la version finale, le même parcours de lecture seule a terminé en
**5.86 secondes** dans cet environnement, avec un budget LLM
réglé à 1 ms pour vérifier son indépendance de la génération. La trace restaurée
après deux nouveaux démarrages contient un seul appel `read`, `Succeeded`, une
source et un état `Complete`. L'empreinte du corps reçu est
`25ddf2c883e0d1958ea971d279a7e4f0fd446724ee3db7db19dadabd4a62e484`.
Cette durée inclut le démarrage et ne constitue pas un benchmark général.

Deux essais négatifs du démon ont également retourné un code de sortie 1 :
lecture de `https://127.0.0.1/metadata` refusée avant connexion, et recherche
sans Brave/SearXNG refusée avec une indication de configuration. La session
REPL reste disponible après un échec ; le mode d'évaluation signale l'échec.

Les adaptateurs Brave et SearXNG sont vérifiés par contrat et par HTTP local.
Aucune clé Brave ni instance SearXNG de production n'était fournie : la qualité
et la disponibilité d'un fournisseur de recherche public n'ont pas été
mesurées. La lecture HTTPS publique a, elle, été effectuée réellement.

## Correctifs de dépendances

Les dix alertes bloquantes relevées lors de la première étape mémoire ont été
traitées par mises à jour compatibles et vérifiées du lockfile :

| Dépendance | Avant | Après |
|---|---|---|
| Tokio | 1.53.2 | 1.53.3 |
| Wasmtime / WASI et famille associée | 48.0.7 | 48.0.8 |
| zstd | 0.13.3 | 0.13.4 |
| zstd-safe | 7.3.0 | 7.4.0 |
| zstd-sys | 2.1.0 | 2.1.2 |
| Candle core / nn / transformers | 0.11.0 | 0.11.1 |
| zip, via Candle | 8.6.0 | 9.5.0 |

Le parseur HTML est `scraper` 0.29.0. La version initialement évaluée utilisait
un `fxhash` non maintenu ; elle a été remplacée, sans ajouter d'exception.
`cargo deny check bans sources advisories` passe avec la politique existante
`deny.toml`. Aucune exception de sécurité supplémentaire n'a été ajoutée.
Les avertissements non bloquants de la politique et les lints historiques du
workspace restent distingués des erreurs.

## Limites de validation

Pas de certification de toutes les plateformes, de toutes les features ou des
dix tests longs. Les timings CPU dépendent du build et de la charge de la
machine ; ils ne sont pas des objectifs garantis. Les sources externes peuvent
être erronées ou malveillantes, et une citation syntaxiquement valide ne prouve
pas la vérité d'une affirmation. Le proxy explicitement configuré reste une
frontière de confiance réseau. Les traces mémoire restent locales et non
chiffrées, avec un oubli logique.

Les détails du contrat et les commandes sont dans
[HARNESS_END_TO_END.md](HARNESS_END_TO_END.md).
