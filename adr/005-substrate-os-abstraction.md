# ADR 005: Découplage Matériel par les Traits Substrate et Compute

## Statut
Accepté

## Contexte
Hakille doit être capable de s'exécuter sur une multitude d'hôtes aux philosophies fondamentalement distinctes :
- Micro-noyau souverain (Redox OS) avec des schemes IPC natifs.
- Systèmes d'exploitation desktop (Linux avec seccomp, Windows avec AppContainer, macOS avec Endpoint Security).
- Systèmes d'exploitation mobiles (Android, iOS) et navigateurs (WASM).
Lier le code de délibération cognitive, de mémoire ou de perception à un OS spécifique condamnerait le projet à des forks multiples et des réécritures incessantes.

## Décision
Introduire deux traits d'abstraction distincts et orthogonaux dans le crate `hakille-substrate` :
1. **`Substrate`** : Contrat unifié d'accès aux services de l'OS hôte (fichiers virtuels, flux réseau, capture d'écran, injection d'entrées clavier/souris, processus sandboxés, attestation matérielle).
2. **`Compute`** : Contrat d'accès aux accélérateurs matériels (CUDA, Metal, Vulkan, NPU, CPU), totalement indépendant de l'OS hôte.

Chaque plateforme cible dispose de son propre crate d'implémentation (`hakille-substrate-windows`, `hakille-substrate-linux`, `hakille-substrate-redox`, etc.) et doit passer à 100% la suite de conformité `conformance/`.

## Conséquences
- **Positives** : Le cœur cognitif (`hakille-kernel`, `hakille-memory`, `hakille-agents`) ne contient aucune directive `#[cfg(target_os = "...")]`. Portabilité immédiate sur toute nouvelle cible.
- **Négatives / Défis** : Nécessite une rigueur absolue dans la modélisation des capacités de plateforme et la gestion des cas dégradés.
