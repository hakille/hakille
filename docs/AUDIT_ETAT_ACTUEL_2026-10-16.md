# Audit approfondi d'Annora — état actuel et trajectoire « enterprise-grade »

**Date :** 16 octobre 2026  
**Périmètre :** état du répertoire de travail (y compris modifications non validées), Rust workspace, CI, documentation, modèles et interface.  
**Nature :** audit technique et produit ; les recommandations juridiques doivent être validées par un conseil qualifié avant publication ou vente.

## Conclusion en une phrase

Annora est un **prototype Rust ambitieux, remarquablement bien décomposé et en voie de devenir réel sur plusieurs moteurs d'inférence**, mais il ne faut pas encore le présenter comme un système de production comparable aux exigences de Google, Apple ou d'une entreprise régulée : l'exécution réelle n'est pas systématiquement livrée/testée, la surface plateforme est trop large, les contrôles d'autorisation sont inégaux, et la gouvernance de release/compliance est incomplète.

La bonne stratégie n'est pas de réécrire : c'est de **réduire le produit initial**, transformer les promesses en contrats testables, et industrialiser chaque couche avant d'élargir la surface.

## 1. Méthode, niveau de preuve et limites

Éléments examinés : manifests Cargo et lockfile, 31 membres de workspace, code de chaque couche, ADR, CI GitHub Actions, catalogues de modèles, sécurité, interface Slint, politique de sécurité et état Git.

Vérifications exécutées le 16 octobre :

- `cargo test --workspace --locked --offline` a terminé avec succès dans cet environnement ; il a néanmoins signalé des usages dépréciés de la vérification de capability non épinglée dans les substrates Linux, macOS, mobile, Web et Redox.
- La suite `cargo xtask conformance` a été lancée mais n'a pas produit son résultat final, car Cargo attendait un verrou d'artefacts dans l'environnement d'audit. Ce n'est donc **pas** une conformité attestée par cet audit.
- `cargo fmt --all -- --check` n'a pas été évalué de façon fiable ici : la sandbox a refusé la canonicalisation du chemin Windows. La CI doit rester la source de vérité pour ce gate.
- Les gros poids ne sont pas versionnés dans `models/` (seul `.gitkeep` est présent). Les tests d'inférence réelle dépendant de fichiers locaux ne prouvent donc pas, à eux seuls, qu'une release propre les exécutera.

Les tests démontrent que le code testé respecte ses contrats ; ils ne démontrent pas à eux seuls la sécurité d'un poste hôte, la conformité RGPD, la qualité d'un modèle ou la résilience d'une flotte de production.

## 2. Scorecard honnête

| Domaine | État | Verdict |
|---|---|---|
| Vision, architecture, ADR, Rust natif | Très solide | Différenciant à conserver |
| Découpage par crates et contrats typés | Solide | Bonne base d'industrialisation |
| Tests de logique / conformance | Bon | À compléter par des tests de vrais backends et de redémarrage |
| Modèles | Mixte | Certains forwards réels, mais couverture de livraison et d'évaluation insuffisante |
| Capabilities / audit / consentement | Bonne intention, application inégale | Bloquant avant autonomie ou données sensibles |
| UI | Prototype réel en Slint, encore non qualifié | À concentrer sur desktop Windows |
| Multi-plateforme | Architecture seulement | Réduire officiellement la matrice supportée |
| CI / supply chain | Première base utile | Doit devenir une chaîne de release signée |
| Enterprise readiness | Pré-alpha | Pas prêt pour secrets, biométrie, paiements ni SLA |

## 3. Ce qui est réellement fort

### Architecture et gouvernance

Le dépôt évite l'anti-pattern du « chatbot monolithique ». Les limites entre `annora-core`, substrates, mémoire, kernel, agents, audit, auth, inférence, voix et interface sont sensées. Les ADR fixent des décisions importantes (Rust-only, capabilities, mémoire, CRDT, interface). Les types Rust et les erreurs explicites rendent une évolution contrôlée possible.

Les concepts de moindre privilège, délégation atténuée, journal d'audit chaîné, consentement, simulation avant action irréversible, budget de silence et distinction du contenu non fiable sont exactement les protections à prévoir dans un assistant pouvant agir sur un système.

### Évolution des modèles : progrès réel, à ne pas sous-estimer

L'ancien audit du 8 octobre n'est plus entièrement à jour. Le code courant comporte de vrais chemins d'exécution :

- sessions ONNX Runtime pour Silero VAD, SenseVoice, CAM++ et MiniLM ;
- tests golden VAD qui ouvrent le modèle, appellent `Session::run`, testent silence, signal voisé, bruit, carry et latence ;
- moteur Qwen Candle CPU qui charge un GGUF Q4_K_M, un tokenizer BPE, construit le modèle et génère avec KV-cache ;
- RVQ/Mimi et synthèse formantique dont les sources de poids sont explicitement distinguées de données synthétiques ;
- shell Slint natif derrière une feature `render`, sans WebView.

Le projet a donc franchi un cap : **il ne faut plus dire que toute l'inférence est simulée.** En revanche, « le chemin existe dans le code » n'est pas encore « le produit est prêt à distribuer et à opérer ».

### Préparation produit

Le design system, les tokens générés, les écrans, les règles de présence/silence, l'accessibilité modélisée et l'onboarding orienté limites avant capacités constituent une base plus mûre qu'une simple maquette. Le keystore introduit un contrat utile : secrets de 32 octets, accès OS, rotation versionnée, zéroisation et erreur explicite plutôt qu'une régénération silencieuse.

## 4. Les écarts majeurs à fermer

### P0 — ne pas élargir l'autonomie avant l'autorisation au point d'effet

Le build actuel émet des avertissements sur `verify_capability_unpinned`. Cette API accepte un jeton auto-signé et est marquée dépréciée ; elle est encore appelée dans plusieurs substrates. Cela invalide le modèle de confiance « capability vérifiée contre une racine durable » quand ce chemin est employé.

**Décision :** aucun effet externe (fichier, réseau, caméra, micro, écran, input, processus, maison connectée, paiement) ne doit être accessible autrement qu'au travers d'un `AuthorizationContext` épinglé, vérifié au point d'effet. Il doit porter au minimum identité/audience, expiration, action, ressource normalisée, quota, nonce et chaîne de délégation.

**Critère de sortie P0 :** un test d'intégration tente chaque effet I/O avec jeton absent, expiré, auto-signé, hors-scope, hors-ressource, puis révoqué ; tous échouent. Un seul chemin de production vérifié est autorisé.

### P0 — modèles : séparer statut d'artefact, statut de runtime et qualité

Le catalogue associe URL, taille, checksum optionnel, RAM/VRAM et rôle. C'est un départ correct. Mais il ne conserve pas encore, par artefact, le commit/révision immuable, le fournisseur, la notice, les conditions d'usage commercial, la date de contrôle, la signature de manifeste, ni un niveau d'évaluation.

De plus, tous les moteurs ne sont pas homogènes : `gguf_runtime` conserve un générateur heuristique explicitement `SIMULÉ` pour l'inspection-only ; UI-TARS reste inspecté sans forward VLM ; les modèles réels exigent des fichiers qui ne sont pas dans une release de test. C'est honnête dans une partie du code, mais il faut l'imposer à toutes les surfaces CLI/UI.

Introduire un état structuré, jamais une chaîne libre :

`absent` → `downloaded` → `sha256_verified` → `signature_verified` → `loadable` → `forward_verified` → `quality_verified` → `approved_for_product`.

Le dernier état doit être signé par une évaluation reproductible. Les métriques minimales : WER par langue/bruit pour ASR, FPR/FNR VAD, FAR/FRR et biais pour speaker-ID (jamais authentification forte sans étude dédiée), similarité/rappel pour embeddings, exactitude et latence pour Qwen, taux de réussite/erreur sûre pour GUI, p50/p95/RAM/VRAM/énergie pour tous.

### P0 — sécurité, données et identité

Le keystore est une amélioration, mais il faut prouver que le démon l'emploie pour toute racine Biscuit et clé de journal en production ; sinon une clé éphémère compromet révocation, audit inter-redémarrages et investigations. Les backends keychain doivent être validés par OS, notamment leur persistance réelle et la politique de récupération.

La biométrie vocale, caméra et traces d'activité sont des données à haut risque. Elles doivent rester désactivées par défaut dans le produit initial. Avant activation : registre de traitements, minimisation, rétention et purge vérifiables, export/effacement, contrôle d'accès, DPIA, modèle de menace, tests de spoof/biais et validation DPO/juridique.

Les agents WASM ont des quotas/fuel et un modèle de capabilities ; il faut vérifier en intégration les limites mémoire/temps, WASI, préouvertures, egress, taille de module, annulation et kill switch. Ne pas présenter un bac à sable comme hermétique avant ces preuves.

### P1 — la cible supportée est trop large

Windows, Linux, macOS, Redox, mobile et Web sont présents. C'est excellent comme architecture de recherche, mais pas comme promesse produit. Chaque OS implique sécurité, permissions, signature, packaging, accessibilité, télémétrie, support et incidents propres.

**Recommandation :**

1. **Alpha : Windows 11 x64 uniquement**, assistant local textuel + mémoire explicite + suggestions, pas d'action autonome.
2. **Beta :** Windows avec UI native, modèles locaux évalués, import/export, actions confirmées et journal durable.
3. **Après preuve :** Linux, puis macOS (signature, notarisation et entitlements). Mobile/Web deviennent des clients limités ; Redox reste R&D.

### P1 — interface et accessibilité

Slint est cohérent avec le choix Rust et le shell actuel constitue une vraie direction. La feature `render` est toutefois optionnelle, donc la CI ordinaire vérifie surtout le bridge et les modèles d'écran. Il faut des captures de référence, tests de navigation clavier/screen-reader, scale/DPI, contrastes, permissions OS, scénarios d'erreur et tests manuels sur matériel.

Le dossier de design mentionne également Iced alors que l'implémentation utilise Slint : choisir un toolkit cible dans une ADR de remplacement ou de confirmation et supprimer la contradiction documentaire.

## 5. CI/CD et supply chain : cible obligatoire

Le workflow actuel est un bon début : Windows + Ubuntu, format, tokens, check, Clippy non bloquant, tests, conformance et `cargo deny`. Mais il ne suffit pas à garantir une release.

| Gate | État actuel | À rendre obligatoire |
|---|---|---|
| Format | Défini | `cargo fmt --check`, sans exception |
| Compilation/tests | Défini | mêmes versions Rust verrouillées + `--locked` |
| Clippy | Exécute sans `-D warnings` | budget d'avertissements à zéro, puis `-D warnings` |
| Conformance | Défini | séparer fast contracts et real-inference golden, publier résultats |
| OS | Windows/Ubuntu | macOS avant annonce support, hardware runners pour périphériques |
| Dépendances | `cargo deny` | `cargo audit`, Dependabot/Renovate, politique de mise à jour |
| Sécurité code | Absent | CodeQL, `cargo geiger`, secret scanning, review obligatoire |
| SBOM/provenance | Absent | SBOM, SLSA provenance, artefacts signés, checksums, attestations |
| Releases | Absent | versioning semver, changelog, signature Windows, notarisation macOS, rollback |
| UI | compilation feature render | snapshots, tests a11y et parcours end-to-end |

À terme, chaque release doit être reproductible à partir d'un tag immuable : toolchain verrouillée, lockfile, modèles référencés par manifest signé, résultats d'évaluation, SBOM, binaire signé, hashes et notes de sécurité.

## 6. Architecture cible du premier produit vendable

```text
Utilisateur
  │ texte + confirmations explicites
  ▼
Slint desktop shell (Windows)
  │ protocole versionné + journal UI
  ▼
Kernel : planification, mémoire locale, simulation, politiques
  │ AuthorizationContext vérifié au point d'effet
  ├── Model gateway : local-only, états de preuve, métriques
  ├── Audit durable : clé OS, chaînage, export vérifiable
  └── Connecteurs : aucun effet sans confirmation + capability scoped
```

**Inclusion alpha :** chat texte local, mémoire explicitement consentie, Qwen local si matériel compatible, import/export chiffré, audit lisible, simulateur et cartes de confirmation.

**Exclusion alpha :** voix toujours à l'écoute, caméra, biométrie d'identité, agents GUI autonomes, domotique, e-mail sortant, paiements, sync cloud automatique, mobile/web comme systèmes égaux.

Cette contrainte n'est pas un recul : elle crée la première promesse qui peut être démontrée, supportée et vendue.

## 7. Feuille de route priorisée avec critères de sortie

### 0–30 jours : rendre le dépôt publiable sans surpromesse

- CI verte et stricte ; correction des warnings `verify_capability_unpinned`; publication de badges reliés à un vrai workflow.
- Inventaire de modèles signé, checksums obligatoires, conditions des poids, statut d'exécution structuré.
- Réconciliation README/docs/CLI : seule une capacité avec preuve se dit « réelle » ; UI-TARS reste experimental/inspection-only.
- Définition de Windows 11 x64 comme unique plateforme alpha.

**Sortie :** clone neuf → build/test/release candidate reproductible ; aucune promesse contredite par le code ; aucune I/O sans test de refus.

### 31–90 jours : alpha locale sûre

- Brancher et tester durablement keystore, journal, révocation et recovery.
- Finaliser le chemin Qwen/VAD/embeddings avec artefacts de test approuvés et golden tests séparés.
- UI Slint utilisable : onboarding, permissions, journal, cartes de confirmation, erreurs, a11y, snapshots.
- Tests de redémarrage/corruption/concurrence, fuite de secrets, capacités, chemins, egress et annulation d'agent.
- Installer signature Windows, auto-update désactivé par défaut ou signé avec rollback.

**Sortie :** 20–50 utilisateurs internes peuvent accomplir une tâche texte/mémoire sans perte d'audit et sans action non confirmée.

### 91–180 jours : beta entreprise

- Politiques administrables, RBAC/SSO seulement si le modèle d'identité est réellement complet, export d'audit, proxy de modèles et configuration offline.
- Mesure de fiabilité, confidentialité, latence, crash-free, support et incidents. Évaluations de modèles versionnées.
- DPIA et revue externe de sécurité avant biométrie ou connecteurs sensibles.
- Packaging entreprise, observabilité minimisée, SLA/support pilote et contrats.

**Sortie :** pilote rémunéré, avec périmètre fonctionnel écrit, supportable, mesuré et réversible.

## 8. Indicateurs à suivre par la direction

- **Sécurité :** pourcentage de points d'effet testés refusant une capability invalide ; vulnérabilités critiques ouvertes ; MTTR ; intégrité de journal vérifiée.
- **Qualité :** taux de CI verte, temps de build, warnings Clippy, couverture des chemins de refus, crash-free sessions.
- **Modèles :** pourcentage d'artefacts avec hash + forward + évaluation ; p95 latence/RAM ; taux de réponses « modèle indisponible » honnêtes.
- **Produit :** taux de confirmations acceptées/annulées, taux de correction de suggestions, rétention sans collecte excessive, tickets support par utilisateur.
- **Entreprise :** temps de déploiement offline, temps d'audit export, taux de mise à jour signée, temps de restauration.

## 9. Décisions à prendre maintenant

1. Valider **Windows-only alpha** et le périmètre texte/mémoire/confirmation.
2. Geler les nouveaux pouvoirs autonomes jusqu'à la fermeture des P0 authorization + modèles + key management.
3. Affecter un propriétaire à chaque gate de release : sécurité, modèles, UI/a11y et plateforme.

## Annexe A — lecture des modèles, composant par composant

| Composant | Verdict actuel | Condition de mise en produit |
|---|---|---|
| Silero VAD | Forward ONNX réel et golden tests présents | artefact signé + tests dans CI capable de le fournir + bruit/langues réels |
| SenseVoice | Chemin ONNX réel présent | WER multilingue, pré/post-traitement validé, politique données audio |
| CAM++ | Chemin ONNX réel présent | ne pas l'employer comme authentification forte sans FAR/FRR, anti-spoof et DPIA |
| MiniLM | Chemin ONNX réel présent | benchmark rappel, confidentialité embeddings, migrations d'index |
| Qwen 2.5 0.5B | Génération Candle CPU réelle présente | evals de tâche/sûreté, tokenizer et poids hashés, limites ressources ; GPU non disponible en v1 |
| UI-TARS | Poids inspectés, pas de forward VLM ; grounding CV | laboratoire seulement, actions toujours confirmées et ancrées visuellement |
| Mimi/Moshi/TTS | RVQ / formants avec distinctions explicites | évaluation qualité bout-en-bout et politique voix avant promesse conversationnelle |

## Annexe B — état Git et discipline de livraison

Le répertoire de travail contient de nombreuses modifications suivies et de nombreux nouveaux fichiers non suivis, touchant pratiquement toutes les couches. Cet audit ne les a pas modifiés. Avant une release, créer une série de PR/commits thématiques (sécurité, modèles, UI, docs, CI) avec propriétaire, revue, test et changelog ; une grosse validation unique masquerait les régressions et rendrait l'audit impossible.

