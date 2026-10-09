# Audit approfondi d'Annora — état actuel et trajectoire production

**Date :** 8 octobre 2026  
**Périmètre :** code présent dans ce dépôt, configuration Rust, documentation, modèle de licences, chaîne d'approvisionnement des modèles et exécution locale sous Windows.  
**Niveau de confiance :** élevé sur l'état du code analysé ; limité sur les licences amont des poids et les obligations légales propres à une juridiction. Ce document est un audit technique et produit, pas un avis juridique.

## Résumé exécutif

Annora a une **vision différenciante, une décomposition Rust propre et une bonne base de spécification**, mais ce n'est **pas encore un produit prêt pour Google, Apple ou une entreprise régulée**. C'est aujourd'hui un prototype architectural convaincant et testable, dont une grande part des composants critiques est simulée ou ne relie que les métadonnées des modèles, sans effectuer l'inférence ou l'action réelle annoncée.

La recommandation est de ne pas élargir encore la surface fonctionnelle. Il faut choisir un premier produit nettement plus étroit : **assistant local Windows, texte + mémoire explicite, suggestions non autonomes, confirmation humaine obligatoire pour toute action**, puis le rendre réellement sûr et mesurable. La voix continue, la caméra, la biométrie, l'agent GUI et la synchronisation multi-appareil doivent rester désactivés en production jusqu'à ce que leurs garanties soient réelles et vérifiées.

| Dimension | État actuel | Verdict |
|---|---|---|
| Vision, architecture, documentation | Très fort | Conserver |
| Workspace Rust / compilation / tests | Bon prototype | Industrialiser |
| Inférence des modèles | Inspection et simulations, pas exécution des réseaux | Bloquant produit |
| Sécurité par capabilities | Bon concept, application absente dans les substrates | Bloquant sécurité |
| UI réelle | Machines d'état et rendu ASCII, pas d'interface graphique | Bloquant produit |
| Audit, identité et persistance | Primitives présentes, clés et journal éphémères | Bloquant conformité |
| CI/CD et gouvernance open source | Quasi absents / incohérents | Bloquant livraison |
| Licence et monétisation | Intention pertinente, mise en oeuvre juridiquement incohérente | À corriger avant publication |

**Décision proposée :** classer le dépôt en phase **prototype / pré-alpha**, sans promettre de sécurité biométrique, d'autonomie, d'inférence locale, de confidentialité de niveau entreprise ou de support multiplateforme. L'objectif atteignable est une alpha Windows en 12 à 16 semaines, sous réserve d'une équipe dédiée aux fondations d'exécution et de sécurité.

## Méthode et éléments vérifiés

- Workspace Cargo : 29 manifests, 28 membres déclarés, 177 fichiers Rust et environ 17 989 lignes Rust.
- Exécution de `cargo test --workspace` : réussite. 112 tests réussis, dont 27 tests de conformance. Les cinq binaires d'agents n'ont aucun test unitaire.
- Exécution de `cargo fmt --all -- --check` : échec ; un très grand nombre de fichiers ne respecte pas `rustfmt`.
- Exécution de `cargo clippy --workspace --all-targets -- -D warnings` : échec dès `annora-core` (`new_without_default` sur `CapabilityAuthority`).
- Exécution de `cargo deny check` : échec avant analyse, car `deny.toml` n'est pas compatible avec la version installée de l'outil.
- Exécution de `annora-daemon --help`, `models list` et vérification d'intégrité : le catalogue local est présent ; les cinq premiers artefacts avec empreinte déclarée sont vérifiés. Deux artefacts lourds n'ont pas d'empreinte dans le catalogue et ne peuvent donc pas être authentifiés cryptographiquement par ce mécanisme.
- État Git initial : une suppression non committée préexistante de `Sélection De Modèles Open Source.md` a été observée et n'a pas été modifiée.

Les tests attestent que le code testé respecte ses propres attentes. Ils ne démontrent ni l'inférence réelle, ni la sécurité du système hôte, ni le comportement sur des appareils physiques, ni la conformité réglementaire.

## Ce qui est bien construit

### 1. Une vision produit cohérente et distinctive

Le positionnement « assistant souverain, local et continu » est plus précis qu'un simple chatbot. Les documents d'architecture, d'ADR, de modèle de menace et de constitution donnent un vocabulaire commun rare à ce stade. La séparation entre mémoire, cognition, perception, agents, audit et substrate est saine.

### 2. Le choix de Rust est pertinent

Le workspace est entièrement Rust, avec édition 2024, lockfile, erreurs typées et un découpage par crates. Pour un produit qui manipule fichiers, voix, caméra, agents et cryptographie, ce choix réduit une grande classe de problèmes mémoire. Les primitives ChaCha20-Poly1305, Ed25519, Biscuit, redb, Wasmtime et des tests de propriétés sont de bons points de départ.

### 3. Les intentions de sûreté sont explicites

Les concepts de moindre privilège, atténuation des capabilities, journal chaîné, consentement, dry-run, budget de silence et séparation des contenus non fiables sont les bonnes protections à concevoir dès le départ. Le fait qu'ils soient représentés dans le code et testés est préférable à une promesse documentaire seule.

### 4. La chaîne de téléchargement de modèles a des briques utiles

Le téléchargement est repris, écrit dans un fichier partiel puis promu, et peut vérifier SHA-256. Les modèles ne sont pas committés, ce qui évite d'alourdir le dépôt. Les parseurs GGUF et Safetensors imposent déjà des limites simples sur les en-têtes.

### 5. La documentation facilite l'onboarding

Le README, les guides de déploiement et les ADR donnent une bonne base à un nouvel ingénieur. La documentation est toutefois aujourd'hui plus avancée que l'implémentation ; elle doit devenir contractuelle et être réconciliée avec les capacités réelles.

## Écarts critiques entre promesse et exécution

### Inférence et modèles : le système ne fait pas encore tourner les modèles annoncés

C'est le point le plus important de l'audit.

- `OnnxSession::run` renvoie le premier tenseur d'entrée comme sortie ; le commentaire indique explicitement une « simulated forward projection » ([onnx_runtime.rs](../crates/annora-inference/src/onnx_runtime.rs)). Il n'y a pas d'appel à ONNX Runtime malgré la documentation.
- `SenseVoiceSmall::transcribe` choisit des IDs de tokens fixes `[3, 6]` selon l'énergie du signal ; il ne calcule pas une transcription ASR ([asr.rs](../crates/annora-perception/src/audio/asr.rs)).
- `MimiCodec` convertit l'énergie de segments audio en huit nombres et reconstitue des sinusoïdes. Il lit éventuellement les métadonnées Safetensors, mais n'exécute pas le codec Mimi ([codec.rs](../crates/annora-voice/src/codec.rs)).
- `CosyVoice2Engine` génère une sinusoïde à 440 Hz ; ce n'est pas une synthèse vocale ([tts.rs](../crates/annora-voice/src/tts.rs)).
- `UiTarsAgent` prend une décision par recherche de mots-clés dans une phrase (« click », « type », « scroll ») et ne fait aucune inférence vision-langage ([gui_agent.rs](../crates/annora-agents/src/gui_agent.rs)).
- Les lecteurs GGUF, Safetensors et ONNX valident ou inspectent principalement le format. Les poids ne sont pas reliés à une boucle de décodage, un tokenizer complet, des KV caches, un scheduler, ni à une exécution GPU/CPU reproductible.

**Conclusion :** le catalogue prouve la présence et partiellement l'intégrité des fichiers, pas leur utilisation fonctionnelle. Les messages « Connected & Verified » et « Neural Fabric Online » sont trompeurs tant qu'ils signifient seulement « fichier lu / métadonnées reconnues ».

**Action :** remplacer les faux runtimes par une interface explicite : `ModelInspector` (métadonnées uniquement) d'un côté, `InferenceEngine` (exécution réelle et métriques) de l'autre. Une capacité ne doit être annoncée « disponible » qu'après un test d'inférence avec un jeu de référence, versionné et mesuré.

### Sécurité des capabilities : concept correct, point d'application absent

`CapabilityAuthority::verify` sait vérifier un scope et une action ([capability.rs](../crates/annora-core/src/capability.rs)). Mais les méthodes du `Substrate` Windows acceptent presque toutes un `_cap` sans l'utiliser : lecture/écriture/suppression de fichiers, réseau, audio, caméra, capture d'écran, injection d'entrées et création de processus ([substrate_impl.rs](../crates/annora-substrate-windows/src/substrate_impl.rs)). Une capability volée, périmée, sans droit, ou même non pertinente n'est donc pas bloquée au point où l'effet réel arrive.

Risques précis :

1. **Escalade de privilèges fonctionnelle :** une route applicative qui atteint le substrate peut agir sans vérification effective du jeton.
2. **Sortie du bac à sable par chemin :** `resolve_path` retire seulement le préfixe `/` ou `\\` et réalise un `join`; il ne normalise ni ne refuse `..`. Un chemin relatif malveillant peut viser hors de `VirtualFS`.
3. **Réseau fictif :** `http_request` retourne une réponse synthétique à 200 et ne met en oeuvre ni egress policy, ni DNS/URL allowlist, ni TLS pinning.
4. **Attestation non matérielle :** l'attestation est une signature locale d'une chaîne formatée, pas une preuve TPM/VBS vérifiée par un serveur de confiance.
5. **Clés éphémères :** la racine de capabilities est générée au démarrage. Il n'y a pas de rotation, de révocation durable, d'identité utilisateur, de stockage TPM/DPAPI/Keychain, ni de journal des délégations.

**Action immédiate :** aucun appel I/O ne doit être possible sans un `AuthorizationContext` validé au point d'effet. Le jeton doit porter identité, audience, expiration, action, ressource exacte (chemin normalisé ou host/port/méthode), quota, nonce et chaîne de délégation. Centraliser la vérification dans un middleware qui renvoie un handle autorisé, jamais un booléen ignoré.

### Isolation des agents WASM : limites déclarées, non imposées

`SandboxLimits` possède des champs de mémoire, durée, réseau et système de fichiers, mais `WasmAgentRuntime` ne branche que le carburant Wasmtime ([sandbox.rs](../crates/annora-agents/src/sandbox.rs), [runtime.rs](../crates/annora-agents/src/runtime.rs)). Pas de `StoreLimits`, pas d'interruption/timeout, pas de linker WASI configuré, pas de préouverture de répertoire, pas de réseau contrôlé, pas de mapping capabilities vers les imports, pas de limite sur la taille du module.

En outre, `spawn_sandboxed` dans le substrate Windows retourne uniquement un compteur. Il ne lance pas le bytecode. Ne pas commercialiser la promesse « agents WASM sandboxés » avant d'avoir une vraie chaîne : manifeste signé → vérification → Wasmtime/WASI Preview 2 → limites strictes → journaux → kill switch.

### Données, audit et vie privée : primitives, pas garanties produit

- Le démon crée son journal dans `tempfile::NamedTempFile` ([main.rs](../crates/annora-daemon/src/main.rs)) : le journal d'audit est détruit à l'arrêt. Cela contredit l'idée de continuité et de traçabilité durable.
- La clé de signature du journal est également générée au démarrage ; la vérification ne forme pas une chaîne de confiance durable entre deux sessions.
- Le chiffrement de blobs est correct comme primitive AEAD, mais la gestion de clés, la protection matérielle, la rotation, la sauvegarde, l'effacement et la récupération ne sont pas définis.
- La synchronisation CRDT est en mémoire ; l'implémentation de réplication sérialise puis chiffre, sans authentification des pairs, anti-rejeu, résolution de conflit métier ni client TiKV opérationnel.
- La biométrie vocale et le liveness sont des heuristiques RMS/ZCR/jitter. Elles ne sont pas un modèle anti-spoofing évalué. Elles ne doivent jamais autoriser une dépense ou remplacer une authentification forte.
- La caméra fabrique une image RGB synthétique de 1920×1080 dans plusieurs chemins. La documentation de « zéro pixel stocké » n'est pas une propriété vérifiée ni une politique d'effacement.

En Europe, les données biométriques utilisées pour identifier une personne bénéficient d'une protection particulière sous le RGPD. Avant de les activer : analyse d'impact (DPIA), finalité très stricte, minimisation, consentement explicite lorsque pertinent, rétention courte, effacement, contrôle d'accès et tests de biais. Voir les explications de la [Commission européenne](https://commission.europa.eu/law/law-topic/data-protection/information-individuals_en). L'avis d'un DPO et d'un juriste est requis avant commercialisation.

### UI : bonne direction produit, aucune interface déployée

`annora-ui` contient des modèles d'état, un rendu ASCII de HUD, des règles de canal et une palette ; il n'a aucune dépendance de toolkit graphique et aucune fenêtre, barre système, hotkey, journal visuel, onboarding ou accessibilité réelle. Les integrations mentionnées dans les chaînes de caractères ne constituent pas une UI.

Il n'est donc pas possible de tester l'accessibilité, les permissions OS, les confirmations haut-risque, l'explicabilité ou le comportement lors d'une erreur. C'est un blocage majeur car Annora est une application d'interaction système.

## Qualité, livraison et dette d'ingénierie

### CI/CD inexistant

Le dépôt ne contient pas de workflow GitHub Actions, de pipeline GitLab, de hooks, de stratégie de release, de signature, de SBOM, de politique de vulnérabilités, de `CODEOWNERS`, `CONTRIBUTING`, `SECURITY.md` ou `NOTICE`.

La badge « build passing » du README n'est reliée à aucun workflow connu. Il faut la supprimer ou la relier à une exécution publique réelle.

### Outils de qualité non bloquants

- `rustfmt` échoue sur un grand nombre de fichiers. Exécuter `cargo fmt --all` une fois dans une PR dédiée, puis exiger `cargo fmt --all -- --check`.
- Clippy strict échoue immédiatement. Corriger d'abord `Default` sur `CapabilityAuthority`, puis lever toutes les alertes ou documenter des exceptions minimales.
- `deny.toml` utilise un schéma invalide pour l'outil installé. Réécrire le fichier depuis le modèle de la version verrouillée puis activer réellement advisories, licences, sources et bans.
- Le projet manque de versioning sémantique au niveau package, de métadonnées Cargo (`license`, `repository`, `description`, `authors`, `rust-version`) et de politique MSRV.

### Tests : quantité utile, couverture insuffisante des risques

Les 112 tests sont une bonne base de logique pure. Ils n'exercent pas le vrai chemin critique : périphériques audio/caméra, modèle ONNX/GGUF/Mimi réellement exécuté, accès réseau, injection UI, sandbox WASM, stockage permanent, redémarrage, corruption du journal, mise à jour signée, migration, charge mémoire ou compatibilité OS.

Les tests de conformance donnent un faux sentiment de conformité car beaucoup testent des stubs cohérents. Renommer la suite actuelle `architecture_conformance` ou `contract_tests` jusqu'à l'ajout de tests d'intégration sur les backends réels.

### Portabilité affichée trop large

Windows, Linux, macOS, Web, mobile et Redox sont tous présents dans le graphe Cargo. Aucun produit pré-alpha ne doit prétendre à cette matrice sans CI par OS et tests sur matériel réel. Certaines implémentations sont inévitablement conditionnelles, simulées ou non exécutables hors de leur environnement.

**Décision :** Windows 11 x64 comme seule cible alpha. Linux ensuite ; macOS après signature/notarisation et entitlements ; mobile/web comme clients très limités, pas comme substrates équivalents. Redox peut rester une piste de R&D, pas un engagement de roadmap commerciale.

## Modèles : recommandation de validation et de positionnement

### Évaluation par composant

| Composant annoncé | Situation dans le code | Décision |
|---|---|---|
| Silero VAD | Fichier inspecté, détection applicative heuristique | Intégrer un vrai runtime et mesurer FPR/FNR/latence |
| SenseVoice | Tokenizer lu, transcription simulée | Bloqué tant que pré/post-traitement ONNX réel n'est pas disponible |
| CAM++ | Fichier inspecté, comparaison possible | Ne pas utiliser comme auth ; benchmark anti-spoofing indépendant requis |
| MiniLM | Fichier inspecté | Intégrer embeddings réels ou remplacer par un service local vérifié |
| Qwen GGUF | GGUF parsé, pas de génération | Choisir un moteur mature sous licence compatible et établir evals / guardrails |
| UI-TARS | Métadonnées et mots-clés, pas vision/action fermée | Garder en laboratoire ; confirmation humaine sur chaque action |
| Mimi/Moshi/CosyVoice | Métadonnées / synthèse sinusoïdale | Ne pas annoncer le dialogue vocal jusqu'à une intégration et mesure bout-en-bout |

### Pipeline de modèles à imposer

1. Un manifeste versionné par artefact : fournisseur, URL immuable ou commit, licence du **code** et des **poids**, SHA-256 obligatoire, taille, dates, usage commercial, régions et notice.
2. Une signature de manifeste maintenue par Annora ; aucune URL ou empreinte codée en dur sans revue.
3. Un téléchargement désactivé par défaut en entreprise, avec proxy/miroir approuvé et allowlist.
4. Un test de santé qui exécute une vraie inférence avec entrées et sorties dorées ; distinguer `present`, `hash_verified`, `loadable`, `runnable`, `quality_verified`.
5. Des benchmarks reproductibles : WER par langue et bruit, DER, taux de rejet/acceptation biométrique, taux de succès GUI, latence p50/p95, RAM/VRAM, consommation et échecs sûrs.
6. Un registre de risques : jailbreak/prompt injection, exfiltration, biais, données personnelles, dérive, régression d'un modèle et retrait rapide (« kill switch » local).

Pour l'alpha, réduire le catalogue à **un LLM texte local réellement exécutable + un modèle d'embeddings réellement exécutable**. Tout le reste est expérimental et opt-in.

## Interface recommandée

### Choix : Tauri 2 + Leptos, avec noyau et daemon Rust

Pour le premier client de bureau, je recommande **Tauri 2** comme shell Windows et **Leptos** comme interface Rust/WASM. Cela préserve un langage applicatif majoritairement Rust, fournit une fenêtre, une barre système, un auto-updater, l'accès aux permissions OS et une UI moderne avec un coût bien moindre qu'une UI Win32/DirectComposition entièrement maison. Le WebView2 est une dépendance système assumée, mais l'exécution du moteur cognitif reste hors de la webview.

Alternative : **Slint** si l'objectif principal est une UI rendue en natif et très contrôlée. C'est préférable pour une appliance, mais moins rapide à produire pour un journal complexe, des réglages entreprise et une accessibilité web mature. Ne pas démarrer par les deux.

### Architecture UI cible

```text
Tauri / Leptos desktop
  - tableau de bord, mémoire explicite, consentements, journal, paramètres
  - notifications et confirmations pour actions à risque
  - accessibilité clavier/lecteur d'écran, localisations
                 |
   IPC local authentifié (named pipe Windows, protocole versionné)
                 |
Annora daemon (service utilisateur séparé)
  - policy engine, audit durable, modèles, mémoire, orchestration
  - aucune action sans décision de policy + capability ressource-spécifique
                 |
Workers isolés
  - inference worker, connecteurs, WASM agents, chacun avec privilèges minimaux
```

Règles UX non négociables : indicateur persistant lorsque micro/caméra/écran sont actifs ; historique des consentements ; prévisualisation et annulation des actions ; bouton d'arrêt global ; mode privé ; aucune action silencieuse par défaut ; explication séparant les faits récupérés, l'inférence du modèle et l'action demandée.

## Licence, open source et modèle économique

### Constats

Le README annonce Apache-2.0 pour l'edge/client et AGPLv3 pour le serveur/cluster, alors que le dépôt ne contient que `LICENSE-APACHE`. Les manifests Cargo n'exposent pas de champ `license` ou `license-file`. Il n'existe pas de fichier `LICENSE-AGPL`, de `NOTICE`, de copyright, de tableau SPDX par crate, ni de frontière de dépendance prouvant que le code Apache et le code AGPL sont séparables.

C'est juridiquement et commercialement trop ambigu pour accepter des contributions, distribuer des binaires ou vendre à des entreprises. Une licence ne se résume pas à un badge. Apache-2.0 permet explicitement la redistribution, les produits dérivés et la vente, tout en fournissant une concession de brevet sous conditions ; voir le texte de l'[OSI](https://opensource.org/license/apache-2.0). L'AGPLv3 ajoute une obligation de proposer le source aux utilisateurs interagissant avec une version modifiée sur un réseau ; voir l'[AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html).

### Modèle recommandé : open core Apache-2.0 propre

**Recommandation principale :** publier le noyau local sous **Apache-2.0**, et vendre des produits et services autour de lui. C'est le modèle le plus compatible avec l'adoption entreprise : elles peuvent auditer, modifier et intégrer le core, tandis qu'Annora monétise la valeur opérationnelle qui ne se limite pas au code source.

Open source Apache-2.0 :

- `annora-core`, protocole, SDK, runtime local de base, UI communautaire, format d'audit, connecteurs de démonstration, documentation et outils de migration.

Offre payante Annora Enterprise / Cloud (dépôts et packages séparés, licence commerciale) :

- administration centralisée, SSO SAML/OIDC/SCIM, RBAC/ABAC, politiques d'entreprise, coffre de secrets et BYOK/KMS ;
- connecteurs certifiés (Microsoft 365, Google Workspace, Slack, SAP…), DLP, egress proxy et journal SIEM ;
- synchronisation managée, sauvegarde chiffrée, fleet management, mises à jour signées, observabilité, support, SLA et conformité ;
- distribution de modèles approuvés, évaluations, red teaming, rapports d'audit et assistance au déploiement ;
- version appliance/on-premise avec support et maintenance.

Les entreprises ne paient pas pour empêcher l'accès au code ; elles paient pour réduire le risque, obtenir des intégrations, des garanties, une exploitation et un support. L'édition communautaire doit rester véritablement utilisable, sans composants fictivement ouverts qui seraient indispensables mais fermés.

### Si l'AGPL est souhaitée malgré tout

Ne l'utiliser que pour un **serveur réseau autonome** dans un dépôt/package séparé, sans lier statiquement ni mélanger ses sources avec les crates Apache. Prévoir alors : `LICENSE-AGPL-3.0-or-later`, notices d'interface source, mode de mise à disposition du source pour les utilisateurs réseau, politique de double licence commerciale, CLA/DCO et revue juridique. Cette voie protège davantage contre un SaaS concurrent, mais augmente fortement la friction procurement et la complexité de conformité. Elle n'est pas recommandée pour le premier lancement entreprise d'Annora.

### Checklist licence avant toute publication

1. Choisir le détenteur de copyright (société ou personnes) et faire signer les cessions/CLA ou imposer DCO.
2. Ajouter `LICENSE`, `NOTICE`, `COPYRIGHT`, `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` et une politique de marques.
3. Déclarer les identifiants SPDX par crate dans chaque `Cargo.toml` et harmoniser README, headers et distribution.
4. Générer une SBOM et un fichier de notices de dépendances à chaque release.
5. Créer un registre séparé pour les licences des modèles, datasets, tokenizer et poids. Ne jamais déduire qu'une licence du code couvre les poids.
6. Protéger la marque « Annora » par une politique de trademark : Apache autorise la réutilisation de code, pas une confusion de provenance.
7. Faire relire le choix final par un avocat open source / software licensing avant la première release ou contrat.

## CI/CD cible

### Garde-fous obligatoires sur chaque pull request

1. `cargo fmt --all -- --check`.
2. `cargo clippy --workspace --all-targets -- -D warnings`.
3. `cargo test --workspace` sous Windows et Linux ; macOS dès que le support devient officiel.
4. Tests d'intégration marqués séparément : aucun téléchargement de modèle, matériel ou clé secrète dans les tests de PR par défaut.
5. `cargo deny check` avec une configuration corrigée, puis `cargo audit` ou équivalent de vulnérabilités.
6. Analyse SAST Rust (CodeQL ou Semgrep), détection de secrets, vérification des licences/SBOM.
7. Compilation `--locked`, génération de la doc et vérification des liens.

### À chaque release

- Build reproductible et artefacts signés (Windows Authenticode, macOS notarisation lorsqu'applicable).
- Attestation de provenance (SLSA), SBOM CycloneDX/SPDX, checksum, notes de version et matrice de compatibilité.
- Mise à jour de sécurité signée avec rollback et canary.
- Tests de migration de données et de redémarrage ; test de non-régression des politiques de consentement.
- Evaluation de modèles versionnée : accepter/rejeter une mise à jour de modèle sur des seuils définis.

### Exemple de structure de workflows

```text
.github/workflows/
  ci.yml              # fmt, clippy, unit tests, docs, deny
  integration.yml     # OS matrix, feature flags, hardware simulé
  security.yml        # audit, CodeQL, secrets, SBOM
  release.yml         # tag signé, packages, provenance, publication
```

Les credentials de publication, de signature et de cloud ne doivent jamais être accessibles aux forks ou aux tests de PR.

## Roadmap de correction priorisée

### P0 — avant toute alpha publique (0–4 semaines)

- Réduire les promesses produit dans README et CLI : distinguer clairement `simulated`, `metadata-only`, `experimental` et `production`.
- Corriger `rustfmt`, Clippy, `deny.toml`, métadonnées Cargo, badge CI et dépendances. Mettre la CI en protection de branche.
- Remplacer le journal temporaire par un stockage durable chiffré, clés sécurisées par OS, rotation et export contrôlé.
- Vérifier toutes les capabilities au point d'effet ; introduire des droits ressource-spécifiques et refuser les chemins non canoniques / `..`.
- Désactiver caméra, biométrie, injection UI et action agent par défaut ; aucun fallback synthétique ne doit se faire passer pour une capture réelle.
- Écrire le plan de licences, ajouter les fichiers nécessaires et faire la revue juridique.

**Critère de sortie P0 :** aucune fonctionnalité mise en avant n'est simulée sans indication ; une action I/O non autorisée échoue dans un test d'intégration ; le pipeline CI vert est public et reproductible.

### P1 — alpha Windows utile et sûre (4–12 semaines)

- Construire l'UI Tauri/Leptos et l'IPC daemon authentifié.
- Implémenter un seul moteur LLM local réel et un moteur d'embeddings réel, isolés dans un worker avec limites de mémoire/temps.
- Fournir mémoire explicite, recherche locale, import/export, journal durable, consentement et confirmations d'action.
- Implémenter Wasmtime avec limites réellement appliquées et un connecteur à faible risque (lecture de fichiers dans un répertoire approuvé).
- Ajouter tests E2E sur Windows, métriques p50/p95, crash reporting opt-in et télémétrie sans contenu.

**Critère de sortie P1 :** un utilisateur Windows peut installer, comprendre les permissions, converser avec un modèle local réel, récupérer/supprimer ses données et auditer toute action, sans réseau obligatoire.

### P2 — bêta entreprise (3–6 mois)

- SSO/RBAC, politiques d'organisation, DLP, SIEM, proxy de modèles, mises à jour signées, support et documentation admin.
- Connecteurs certifiés avec consentements, scopes minimes et tests de régression.
- Synchronisation chiffrée authentifiée avec résolution de conflit, backup/restore et audits de sécurité externes.
- Pentest, threat modeling mis à jour, exercices de réponse incident, programme de disclosure.
- Évaluation indépendante de l'accessibilité, confidentialité et modèles.

### P3 — fonctionnalités sensibles (après preuves)

- Voix toujours active, caméra, biométrie, GUI autonomy, paiement et domotique uniquement après évaluations techniques et juridiques dédiées.
- Pour chaque domaine : mode simulation, opt-in explicite, action preview, double confirmation, kill switch et métriques de sécurité.

## Risques principaux et arbitrages

| Priorité | Risque | Effet | Traitement |
|---|---|---|---|
| P0 | Fonctionnalités IA présentées comme actives mais simulées | Perte de confiance, risques contractuels | Repositionner et implémenter une verticalité réelle |
| P0 | Capabilities ignorées par les substrates | Fuite / action non autorisée | Enforcement central au point d'effet |
| P0 | Journal et clés temporaires | Audit non fiable | Persistance, stockage de clés OS, rotation |
| P0 | Licence Apache/AGPL incohérente | Blocage légal et enterprise | Choisir une stratégie, fichiers et séparation |
| P0 | Pas de CI / supply-chain | Régression et dépendance compromise | CI protégée, SBOM, signatures, scanning |
| P1 | Surface multi-OS trop grande | Retard et qualité médiocre | Windows seul en alpha |
| P1 | Biometrie et caméra | Risque vie privée / conformité | Désactivation par défaut, DPIA, tests indépendants |
| P1 | Agents GUI | Actions erronées et exfiltration | Preview, sandbox, permission par ressource |

## Indicateurs de maturité à suivre

Ne pas mesurer seulement le nombre de crates ou de modèles. Piloter les indicateurs suivants :

- taux de chemins d'action couverts par test d'intégration réel ;
- pourcentage de méthodes sensibles avec capability effectivement vérifiée ;
- ratio de code simulé explicitement étiqueté ;
- vulnérabilités critiques ouvertes, délai de correction et fraîcheur des dépendances ;
- latence p50/p95, crash-free sessions, mémoire/VRAM et consommation ;
- exactitude de l'ASR/embeddings/LLM sur jeux de tests par langue et matériel ;
- taux de confirmation, annulation et rollback des actions ;
- demandes d'accès aux données, délai d'export/effacement, incidents de confidentialité ;
- adoption communautaire : contributions externes, délai de revue, documentation exécutée en CI.

## Conclusion

Annora a les éléments d'un excellent **programme de recherche et de produit** : un angle clair, une architecture découplée et des principes de confiance bien choisis. Sa faiblesse n'est pas l'ambition ; c'est l'écart actuel entre les garanties annoncées et les mécanismes réellement exécutés.

Le chemin vers une crédibilité de niveau grande plateforme n'est pas d'ajouter davantage de modèles ou d'OS. C'est de construire une première tranche étroite, mesurée, durable et honnête : Windows, modèle texte local réel, mémoire locale maîtrisée, UI accessible, permissions effectives, audit persistant, CI stricte et stratégie Apache open-core sans ambiguïté. Une fois cette fondation prouvée, l'entreprise peut vendre du support, de l'administration, de la conformité et des intégrations, tout en gardant un noyau open source réellement attractif.
