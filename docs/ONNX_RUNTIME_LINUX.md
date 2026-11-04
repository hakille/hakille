# ONNX Runtime CPU sous Linux : installation vérifiée

Le crate `hakille-inference` utilise `ort = 2.0.0-rc.14`, dont les fonctionnalités
par défaut demandent l'API ONNX Runtime **31**. Le téléchargement automatique
référence une distribution ONNX Runtime **1.31.0** sur `cdn.pyke.io`.

Le 3 novembre 2026, ce CDN retourne HTTP 403 dans l'environnement cloud : la
connexion au proxy réussit, puis la réponse du CDN indique `Your request was
blocked.` La politique réseau effective est `unrestricted/enforced` ; ajouter
une seconde fois le domaine à une liste d'autorisations ne résout pas ce cas.

Le mécanisme supporté `ORT_LIB_LOCATION` avec `ORT_PREFER_DYNAMIC_LINK=1`
permet d'utiliser la distribution CPU officielle Microsoft de la **même version**.
Il ne nécessite aucun changement de `Cargo.toml`, de `Cargo.lock` ni des sources
des dépendances. Les contrôles TLS et SHA-256 restent actifs.

## Source et intégrité

- [Release officielle Microsoft v1.31.0](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0), publiée le 9 octobre 2026.
- [Métadonnées officielles de la release](https://api.github.com/repos/microsoft/onnxruntime/releases/tags/v1.31.0).
- Asset CPU : `onnxruntime-linux-x64-1.31.0.tgz` (11 408 003 octets).
- SHA-256 publié dans le champ `digest` de cet asset et vérifié sur le téléchargement :
  `cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57`.

Ce checksum correspond à l'archive **Microsoft**, pas à l'archive statique du
CDN Pyke. Il ne faut pas remplacer le checksum embarqué dans `ort-sys` : la
sélection de bibliothèque externe est le mécanisme prévu par son build script.

## Installation reproductible

Exécuter dans un shell Bash sous Linux x86_64. Le chemin choisi est extérieur
au dépôt ; conserver les paramètres proxy et certificats fournis par l'hôte.

```bash
set -euo pipefail
onnx_runtime_root=/workspace/.hakille-tools/onnx
mkdir -p "$onnx_runtime_root"
cd "$onnx_runtime_root"
curl --fail --location --max-time 120 \
  https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-1.31.0.tgz \
  -o onnxruntime-linux-x64-1.31.0.tgz
printf '%s\n' \
  'cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57  onnxruntime-linux-x64-1.31.0.tgz' \
  | sha256sum --check --strict
tar --extract --gzip --file onnxruntime-linux-x64-1.31.0.tgz --no-same-owner
export ORT_LIB_LOCATION="$onnx_runtime_root/onnxruntime-linux-x64-1.31.0/lib"
export ORT_PREFER_DYNAMIC_LINK=1
export LD_LIBRARY_PATH="$ORT_LIB_LOCATION${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Un échec du checksum interrompt l'installation. Ne pas extraire ni utiliser une
archive dont l'intégrité n'a pas été vérifiée. La distribution conserve ses
fichiers de licence et de notices dans le répertoire extrait.

Les trois variables doivent être présentes **pendant la compilation et
l'exécution**. `copy-dylibs` peut matérialiser des liens dans `target`, mais le
répertoire de bibliothèque ci-dessus reste la référence. Dans l'environnement
cloud préparé, les commandes d'activation sont :

```bash
. /workspace/.hakille-tools/env.sh
. /workspace/.hakille-tools/onnx/env.sh
cd /workspace/hakille
```

Le helper local est conservé hors du dépôt ; cette préparation ne modifie ni
ne publie les paramètres d'environnement cloud. Les instructions d'installation
ci-dessus restent utilisables si le helper n'existe pas dans une nouvelle machine.

## Validation et limites

La bibliothèque téléchargée a été réellement chargée dans l'environnement
Linux. `OrtGetApiBase()->GetVersionString()` renvoie **1.31.0** et
`OrtGetApiBase()->GetApi(31)` renvoie une table d'API non nulle. Les bibliothèques
système nécessaires sont résolues par `ldd`.

Après activation, les contrôles du projet restent nécessaires :

```bash
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo run --locked -p xtask -- conformance
cargo run --locked -p hakille-daemon -- models verify
```

Ces commandes constituent une procédure de validation, pas l'affirmation qu'elles
ont toutes réussi. Le chargement de l'API ne valide pas l'inférence d'un modèle.
Les artefacts de modèles doivent être provisionnés séparément et leurs checksums
vérifiés. Les tests ignorés d'inférence réelle conservent leurs prérequis.

Cette archive cible uniquement le **CPU Linux x86_64**. Ne pas réutiliser son
chemin `ORT_LIB_LOCATION` pour une compilation Windows, macOS, ARM, Android ou
WebAssembly. Ces plateformes nécessitent leurs propres binaires et validations.
