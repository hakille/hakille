# Exécuter les modèles localement

La commande `hakille-daemon models` permet d'inventorier, télécharger,
vérifier et exécuter les sept modèles du [catalogue local](MODELS.md), sans
démarrer le HUD, l'identité ou le journal du démon. Elle ne publie rien.

Les [résultats de validation Linux CPU](VALIDATION_INFERENCE_LOCALE_2026-11-03.md)
distinguent contrôles d'intégrité, tests de fonctionnement et preuves sur de
vraies entrées audio, texte et image.

## Préparer le binaire et les runtimes

Depuis la racine du dépôt, avec la toolchain Rust et les dépendances natives
configurées selon le [guide de développement](DEVELOPER_GUIDE.md) :

```bash
cargo build -p hakille-daemon --locked
./target/debug/hakille-daemon models --help
```

Sous Windows, le binaire est `target\debug\hakille-daemon.exe`. Les exemples
suivants utilisent `hakille-daemon` dans le `PATH` ; on peut le remplacer par
le chemin du binaire, ou par `cargo run -p hakille-daemon --locked --`.
Les moteurs ONNX nécessitent la bibliothèque native ONNX Runtime compatible
avec `ort`. Qwen et Mimi utilisent Candle CPU. UI-TARS nécessite en plus un
serveur llama.cpp natif, configuré plus bas. Python n'est pas requis pour ces
commandes.

Dans l'environnement cloud préparé de ce dépôt, les deux scripts locaux
`/workspace/.hakille-tools/env.sh` et `/workspace/.hakille-tools/onnx/env.sh`
configurent les outils et la bibliothèque ONNX. Ces chemins appartiennent à
cet environnement et ne sont pas des prérequis portables du dépôt.

## Inventorier et installer

```bash
hakille-daemon models list --models-dir ./models --json
hakille-daemon models download --models-dir ./models --model all-minilm-l6-v2
hakille-daemon models verify --models-dir ./models --model all-minilm-l6-v2
```

`--models-dir` désigne le répertoire exact des fichiers. À défaut, la CLI
utilise `HAKILLE_MODELS_DIR`, puis `./models`. Le dossier `models/` est ignoré
par Git. Un modèle inclut ses auxiliaires : tokenizer, vocabulaire ou
projecteur selon l'entrée.

Pour l'ensemble du catalogue, environ 3,69 Go de poids :

```bash
hakille-daemon models download --models-dir ./models --all
hakille-daemon models verify --models-dir ./models --all --json
```

Les sélecteurs `--model IDENTIFIANT`, `--all` et
`--tier guest|native|sovereign` sont exclusifs. Sans sélecteur, les opérations
autres que `infer` sélectionnent tout le catalogue. Les niveaux sont
cumulatifs : guest = 2 modèles, native = 6, sovereign = 7.
`infer` exige toujours un identifiant exact avec `--model`.

`list` consulte présence et taille ; il ne calcule pas tous les SHA-256 et
n'exécute aucun réseau. `verify` contrôle l'intégrité de tous les fichiers
sélectionnés. `download` revalide le cache, peut reprendre un téléchargement
partiel vérifiable, puis installe le fichier après contrôle SHA-256. Les
versions de poids inattendues sont refusées ; remplacer un modèle implique
de mettre à jour son contrat et ses tests, pas de renommer un autre fichier.

## Tester le fonctionnement des moteurs

```bash
hakille-daemon models test --models-dir ./models --model silero-vad-v5
hakille-daemon models test --models-dir ./models --model campplus-speaker-id
hakille-daemon models test --models-dir ./models --model all-minilm-l6-v2
hakille-daemon models test --models-dir ./models --model sensevoice-small-int8
hakille-daemon models test --models-dir ./models --model qwen2.5-0.5b-instruct-q4km
hakille-daemon models test --models-dir ./models --model moshi-mimi-s2s
```

Chaque commande vérifie les fichiers puis effectue un **forward réel**. Les
tests audio ONNX utilisent par défaut deux secondes de silence ; Mimi utilise
une sinusoïde. Ces signaux synthétiques sont des tests de fonctionnement,
sans évaluation de transcription, de qualité de voix ou d'identification.
Qwen reçoit une question courte et MiniLM une phrase. `test` accepte aussi
les entrées explicites des exemples ci-dessous.

UI-TARS exige un serveur et une image même pour `test` :

```bash
hakille-daemon models test --models-dir ./models --model ui-tars-2b-sft-q4km \
  --endpoint http://127.0.0.1:18081 --image capture.png \
  --text "Click the Save button" --json
```

Après démarrage de ce serveur, `test --all` doit également recevoir
`--endpoint` et `--image`. Un test global sans ces entrées échoue sur UI-TARS.
Les résultats restent détaillés par modèle, avec code de sortie non nul si
l'un d'eux échoue. `--json` émet le rapport JSON compact sur stdout ; les
diagnostics natifs peuvent être écrits sur stderr.

## Inférer sur du texte

```bash
hakille-daemon models infer --models-dir ./models \
  --model qwen2.5-0.5b-instruct-q4km \
  --text "Explique simplement ce qu'est une base de données." \
  --max-tokens 128 --timeout-seconds 120 --json

hakille-daemon models infer --models-dir ./models \
  --model all-minilm-l6-v2 --text "Le chat dort sur le canapé." --json
```

Qwen retourne le texte et les comptes de tokens. `--max-tokens` accepte 1 à
4096 et `--timeout-seconds` 1 à 3600. La limite de temps concerne la génération,
après chargement des poids, et est vérifiée coopérativement entre opérations ;
elle n'interrompt pas instantanément un calcul natif en cours. Le budget de
sortie ne garantit pas que tous les prompts tiennent dans le contexte.

MiniLM retourne 384 nombres normalisés L2. Le tokenizer officiel tronque à
256 tokens, tokens spéciaux compris. Cette commande ne crée pas de mémoire
persistante. Les embeddings produits par l'ancien tokenizer manuel doivent
être recalculés avant d'être comparés ou mélangés aux nouveaux.

## Inférer sur un WAV

```bash
hakille-daemon models infer --models-dir ./models \
  --model silero-vad-v5 --audio parole.wav --json

hakille-daemon models infer --models-dir ./models \
  --model sensevoice-small-int8 --audio parole.wav --json

hakille-daemon models infer --models-dir ./models \
  --model campplus-speaker-id --audio parole.wav --json
```

La CLI accepte les WAV mono ou stéréo de 8 à 192 kHz, en PCM entier
8/16/24/32 bits ou float32, de durée non nulle jusqu'à 60 secondes et au plus
64 Mio. Les échantillons doivent être finis et normalisés. Elle convertit
ces entrées en mono 16 kHz pour les trois moteurs ONNX. SenseVoice et CAM++
exigent au moins 25 ms d'audio après conversion.

Silero retourne les scores de parole, SenseVoice le texte et ses métadonnées,
CAM++ un vecteur de locuteur à 192 composantes. Le silence n'est pas un test
de qualité ASR : SenseVoice peut y halluciner du texte. La confiance ASR
n'est pas une probabilité calibrée de justesse ; les similarités CAM++ ne
constituent pas une preuve d'identité ou une autorisation.

Pour reconstruire du son avec le codec neuronal Mimi :

```bash
hakille-daemon models infer --models-dir ./models \
  --model moshi-mimi-s2s --audio voix-24khz.wav \
  --output voix-reconstruite.wav --json
```

Le WAV Mimi doit être à **24 kHz** ; la CLI mélange la stéréo en mono sans
conversion silencieuse de fréquence. Le fichier de sortie doit être nouveau
et son dossier parent exister ; un fichier existant n'est pas écrasé. Le
dernier bloc est complété pour le réseau, puis le padding est retiré afin de
conserver le nombre d'échantillons original. La sortie est un WAV mono
float32 24 kHz. Ce parcours reconstruit l'audio fourni : il ne génère pas une
réponse conversationnelle Moshi.

## Démarrer UI-TARS et proposer une action visuelle

Installer une version native de llama.cpp compatible Qwen2-VL ; la validation
Linux CPU utilise **b12074**. Le [guide UI-TARS](../crates/hakille-perception/UI_TARS_LOCAL.md)
documente la provenance, les empreintes et les limites du client.

Après `models download --model ui-tars-2b-sft-q4km`, lancer :

```bash
llama-server \
  -m models/ui-tars-2b-sft-q4_k_m.gguf \
  --mmproj models/mmproj-UI-TARS-2B-SFT-f16.gguf \
  --host 127.0.0.1 --port 18081 \
  --cors-origins http://127.0.0.1:18081 --no-cors-credentials \
  --alias ui-tars-2b-sft \
  -t 4 -tb 4 -c 4096 -np 1 -ngl 0 --no-mmproj-offload \
  --image-min-tokens 1024 --image-max-tokens 1024
```

Adapter les deux chemins si les poids sont dans un autre répertoire. Attendre
que le serveur ait chargé le modèle et son projecteur, puis dans un autre
terminal :

```bash
hakille-daemon models infer --models-dir ./models \
  --model ui-tars-2b-sft-q4km --endpoint http://127.0.0.1:18081 \
  --image capture.png --text "Click the Save button" --json
```

Les images PNG/JPEG sont limitées à 8 Mio et 2048 × 2048 pixels par le client,
les instructions à 8192 octets. L'endpoint doit utiliser une IP littérale de
boucle locale et un port explicite. Le serveur doit être de confiance : la
vérification de l'alias ne prouve pas quels poids il a chargés.

La réponse contient une seule proposition validée parmi clic, double clic,
clic droit, glisser, saisie, raccourci, défilement, attente ou fin. Les
coordonnées sont normalisées **0 à 1000**, et non exprimées en pixels. Cette
commande n'effectue aucune action dans le système. Une action incorrecte
reste possible ; `finished` ne prouve pas l'accomplissement d'une tâche.

## Interpréter un échec

- Fichier absent ou SHA incorrect : vérifier le répertoire sélectionné puis
  relancer `download` pour cet identifiant exact.
- Bibliothèque ONNX absente ou incompatible : vérifier l'installation native
  et le chemin de chargement avant de relancer le forward.
- UI-TARS inaccessible ou alias incorrect : vérifier le serveur, ses deux
  fichiers et son alias. Aucun moteur de secours ne masque cet échec.
- Entrée refusée : respecter le format, les bornes et la fréquence du moteur.
  Une erreur est un résultat explicite, pas une inférence réussie.

Ces commandes rendent les moteurs testables localement. Les preuves actuelles
de forward CPU Linux ne valident pas à elles seules l'installation ou les
performances natives sur toutes les autres plateformes.
