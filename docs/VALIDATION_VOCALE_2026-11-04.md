# Validation vocale locale — 4 novembre 2026

## Résultat et limites

Whisper tiny multilingue exécute une vraie transcription locale avec Candle
0.11.1 sur CPU Linux. L'optimisation du calcul matriciel ramène les essais courts
à quelques secondes. L'indice lexical natif « Hakille », associé à une langue
explicitement choisie, permet de reconnaître le nom dans les exemples
synthétiques testés.

**La qualité de transcription n'est pas validée comme exacte.** Deux contrôles
stricts ont échoué : une phrase française humaine est mal retranscrite et une
phrase anglaise comporte une négation absente de la référence upstream. La
commande française synthétique reste incorrecte après le nom. Ces échecs ne
sont pas remplacés par les succès des tests de silence, de fragments partiels
ou d'activation.

Les 99 codes de langue correspondent au vocabulaire de Whisper multilingue ;
seuls le français et l'anglais ont été essayés ici. L'autodétection choisit à
tort le japonais sur un énoncé synthétique français court. Le moteur n'est pas
un détecteur acoustique spécialisé de mot d'activation. Les essais sans faux
réveil ne constituent pas une garantie générale contre les hallucinations.

## Périmètre exécuté

- Conteneur Linux x86_64, quota de quatre CPU, Rust 1.95, profil de développement.
- Candle CPU et modèle FP32 tiny multilingue, aucun service ASR distant ni
  runtime Python dans le chemin produit.
- PCM mono normalisé 16 kHz, de 100 ms à 30 secondes. Les valeurs non finies,
  hors de `[-1, 1]` et les durées hors bornes sont refusées.
- Décodage greedy, sans traduction implicite, au plus 224 tokens générés ;
  transcription partielle refusée si aucun token de fin n'est obtenu.
- Caches purgés avant/après chaque requête, y compris erreur. L'échéance est
  coopérative : un forward Candle commencé n'est pas préemptible.
- Indice lexical optionnel par le préfixe Whisper `<|startofprev|>`, limité à
  512 octets/64 tokens ordinaires. Aucun remplacement du texte reconnu après
  décodage ; la détection de langue est calculée séparément.
- Silence presque nul éliminé avant forward ; filtre neuronal combinant
  `no_speech_probability > 0.6` et vraisemblance moyenne inférieure à `-1`.
  Une probabilité non calculée reste `None`, notamment pour la langue imposée
  ou le silence éliminé avant inférence.

La banque Slaney 80 × 201 est générée en Rust. La comparaison des 16 080 valeurs
à `melfilters.bytes` de l'exemple Candle 0.11.0 donne un écart absolu maximal
d'environ `1.34e-9` ; des valeurs de référence figurent dans les tests unitaires.
Le calcul mel Candle et son contexte paddé jusqu'à 30 secondes n'ont pas été
raccourcis pour obtenir les gains de vitesse.

Aucun microphone physique, boucle acoustique haut-parleur/microphone, accent
supplémentaire, bruit domestique réel, Windows, macOS, mobile, Web ou GPU n'a été
validé par ces essais ASR. Le temps d'inférence ne mesure pas la latence complète
de capture, de segmentation VAD, de transport et d'interface.

## Modèle et empreintes

Source : [openai/whisper-tiny](https://huggingface.co/openai/whisper-tiny/tree/169d4a4341b33bc18d8881c4b69c2e104e1cc0af),
révision `169d4a4341b33bc18d8881c4b69c2e104e1cc0af`, version multilingue, pas
`tiny.en`. Identifiant du catalogue : `whisper-tiny-multilingual`.

| Fichier local dans `models/` | Octets | SHA-256 |
| --- | ---: | --- |
| `whisper-tiny.safetensors` | 151061672 | `7ebd0e69e78190ffe1438491fa05cc1f5c1aa3a4c4db3bc1723adbb551ea2395` |
| `whisper-tiny-config.json` | 1983 | `ffdccec4f3211f4c63310f2b7098f309fe70f3952cedc5e4d11e43f5b2379b98` |
| `whisper-tiny-tokenizer.json` | 2480466 | `27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566` |

Le catalogue conserve URL épinglées, tailles et SHA-256. Le moteur vérifie les
formes tiny/base multilingues et les identifiants des tokens spéciaux/langues ;
il ne télécharge rien implicitement. Les poids restent hors Git.

## Comparaison synthétique avec langue fixée

Fixtures créées par eSpeak NG Debian `1.51+dfsg-10+deb12u2`, voix `fr` ou `en-us`,
vitesse 150 mots/minute. Ce sont des voix **synthétiques**, pas des
enregistrements humains ni une mesure de fiabilité sur microphone.

| Texte synthétisé | Langue | Sortie sans indice | Sortie avec `Hakille` |
| --- | --- | --- | --- |
| Hakille, ouvre le calendrier. | fr | qui ouvre le cas long de l'aie. | Hakille, ou de le caron aille |
| Hakille | fr | . | Hakille |
| Ouvre le calendrier. | fr | vous de cah, non rien. | C'est le casement, eh |
| Bonjour, quelle heure est-il ? | fr | Bonjour, c'est le métil. | Bonjour, c'est un petit |
| Hakille, what time is it? | en | hack it, what time is it? | Hakille, what time is it? |
| Hello, what time is it? | en | Hello, what time is it? | Hello, what time is it? |

Les variantes françaises « Akille » produisent les mêmes fichiers audio que
« Hakille », H muet dans cette synthèse. Le rapport comporte huit cas, mais
seulement **six audios distincts**, dont trois positifs et trois négatifs.
Avec langue fixée et indice, le nom apparaît dans les cinq cas positifs, soit
trois audios distincts ; sans indice, aucun ne contient le nom littéral.
Les trois audios négatifs restent sans « Hakille », avec ou sans indice.

Cette mesure porte sur le nom, pas sur une commande correctement comprise.
La commande française reste mal reconnue. L'indice peut biaiser le modèle ;
six essais négatifs sur trois audios ne donnent pas un taux général de faux
réveils.

En autodétection, « Hakille, ouvre le calendrier. » donne `ja`, probabilité
`0.1751041`, puis « キー、動作方向へ » sans indice et « 奥浜の彼女へ » avec indice.
L'indice ne corrige pas cette erreur de langue. Les résultats positifs du
tableau exigent une langue fixée (`fr` ou `en`).

## Enregistrements humains : échecs de qualité conservés

### Français

Source : [PolyAI/minds14](https://huggingface.co/datasets/PolyAI/minds14), révision
`40ce77cb32a384e4d50a568e1ec39ac804019d33`, configuration `fr-FR`, split `train`,
ligne 0, fichier source `fr-FR~ADDRESS/response_4.wav`.

- Référence du dataset : **« je souhaite changer mon adresse »**.
- Sortie réelle, sans et avec indice : **« Je suis changé mon adresse. »**.
- Langue automatique `fr`, probabilité `0.92551094` ; no-speech `0.040299617`.
- Le fragment strict `changer mon adresse` **échoue**.
- Aucun « Hakille » ajouté dans le passage avec indice.

### Anglais

Source : [fixture publique sherpa en.wav](https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/test_wavs/en.wav),
déjà utilisée dans la [validation du 3 novembre](VALIDATION_INFERENCE_LOCALE_2026-11-03.md).
La [référence publiée par sherpa](https://github.com/k2-fsa/sherpa/blob/master/docs/source/onnx/sense-voice/code/2024-07-17-itn.txt)
est une sortie upstream indépendante, pas une annotation humaine exhaustive.

- Référence upstream : « The tribal chieftain called for the boy and presented
  him with 50 pieces of code. »
- Sortie Whisper avec indice : **« The Tribal Chiefs didn't call for the boy
  and presented him with 50 pieces of gold. »**
- Langue automatique `en`, probabilité `0.9616776` ; no-speech `0.020675302`.
- Le fragment strict `tribal chieftain` **échoue**.
- La négation **« didn't »** est absente de la référence et peut changer le
  sens. Cet écart ne doit pas être présenté comme de la casse ou de la
  ponctuation. Une écoute/annotation dédiée reste nécessaire pour une
  évaluation exhaustive.
- Aucun « Hakille » ajouté dans cette sortie.

Une passe distincte de smoke partiel vérifie uniquement les fragments `mon
adresse` et `presented him with`, les langues et l'absence de faux nom. Elle
passe sans corriger ni annuler ces deux échecs stricts. Aucun score WER complet
ni aucune preuve de transcription exacte n'est fourni ici.

## Silence et signaux non verbaux

`nonverbal_noise_with_lexical_prompt` emploie réellement l'indice `Hakille`.
Le silence nul d'une seconde est arrêté avant forward. Les deux signaux de
trois secondes suivants sont décodés en auto puis avec `fr` imposé :

| Signal synthétique | No-speech mesuré | Résultat dans les deux modes |
| --- | ---: | --- |
| Bruit blanc déterministe, amplitude maximale 0.02 | 0.7103946 | Texte vide, silence supprimé |
| Sinusoïde 440 Hz, amplitude 0.1 | 0.8167387 | Texte vide, silence supprimé |

Aucun « Hakille » halluciné dans les quatre forwards. Le test passe en 5.04 s,
avec des forwards de 974 à 1483 ms. Cela ne couvre pas la musique, une télévision,
des conversations voisines ou tous les bruits domestiques.

## Performance

Les dépendances Candle étaient déjà en `opt-level=3`, mais les matrices FP32
passent aussi par `gemm`, `gemm-common` et `gemm-f32`. Le manifeste racine ajoute
leurs overrides de profil de développement.

Comparaison sur **le même audio** `fr_hakille_command`, le même mode automatique
et les mêmes prompts. Les textes et probabilités avant/après sont identiques,
y compris la mauvaise langue `ja` :

| Appel | Avant optimisation GEMM | Après optimisation GEMM |
| --- | ---: | ---: |
| Sans indice | 47 392 ms | 1 424 ms |
| Avec indice `Hakille` | 74 013 ms | 1 582 ms |

Ce sont des observations ponctuelles, pas des quantiles ni une garantie de
latence. La mesure initiale avec indice était concurrencée par une compilation.
Les seize passes avec langue fixée prennent 41.38 s au total ; les forwards vont
de 905 à 3499 ms. Une dernière passe humaine avec indice mesure respectivement
1351 et 2217 ms, avec les erreurs textuelles documentées. Cette optimisation
accélère le calcul ; elle n'améliore pas la précision du modèle.

## Provenance et empreintes des fixtures

Fixtures hors Git ; chemins de cette session :

- Synthèse et manifeste : `/workspace/.hakille-tools/espeak/fixtures/manifest.json`.
- Générateur : `/workspace/.hakille-tools/espeak/generate-fixtures.py`.
- Métadonnées humaines FR : `/workspace/.hakille-tools/model-fixtures/whisper-human-fr-source.json`.

Le générateur reproductible livré est désormais
[`scripts/prepare_voice_fixtures.py`](../scripts/prepare_voice_fixtures.py).
Il utilise uniquement la bibliothèque standard Python pour appeler eSpeak NG
et ffmpeg déjà installés, sans téléchargement. Exemple :

```bash
python3 scripts/prepare_voice_fixtures.py \
  --output /tmp/hakille-voice-fixtures \
  --espeak-ng /chemin/vers/espeak-ng \
  --ffmpeg /chemin/vers/ffmpeg
```

Avec les mêmes versions d'outils, ses seize fichiers WAV/PCM ont été comparés
octet par octet aux originaux et sont identiques. Il refuse l'écrasement sans
`--force` et produit un manifeste compatible avec le test synthétique.

Les scripts Python servent uniquement à préparer/contrôler les fixtures, pas
au produit. PCM IEEE754 float32 little-endian brut, mono 16 kHz ; conversion
`ffmpeg -ar 16000 -ac 1 -f f32le`. Empreintes des octets soumis au moteur :

| Fixture PCM | SHA-256 |
| --- | --- |
| `fr_hakille_command.f32le`, `fr_akille_command_variant.f32le` | `9fa77977716140daa87b55991727d0c2edd62421c43de1d247ae5313fceee2e3` |
| `fr_hakille_only.f32le`, `fr_akille_only_variant.f32le` | `ce7f6b72f4defa18a1435988da82930f869b68e003012ce28e4169dc8264412e` |
| `fr_command_negative.f32le` | `7c5d4765dd7a3179ad6f0ce1f09a506bdf57f824ea047e0975e169e0a968bb1e` |
| `fr_greeting_negative.f32le` | `1f3ddf5156ac346b1f271d92bbec17e50f18a5fe4ad8f71bf0d490f901032415` |
| `en_hakille_command.f32le` | `ac25149c7adc3d9dd003e522da28edd080ef137fd9ebc7e4654c808538b4a662` |
| `en_greeting_negative.f32le` | `54c2dff089f11978e1cec74369cc700cfe04751285457f0bd8ee49732166f8c2` |
| `whisper-human-fr.f32` | `7c76593563744079079d21d3d9c9ce4dcfae7a1dbbde3970d67abf6a05d39468` |
| `sensevoice-en.f32` | `6227bb965937fa663d1780d45cb3a064879df6b18fc73f6ba44ee92a02b65b59` |

WAV FR source : SHA-256
`84defdc828ef59cec10364354fbc284bc2cc683fdd4a5edd5863b7bb2c6123a8`.
WAV EN source : SHA-256
`eb1eb008904465b74c304aad8342e8c7d3c6e61ffe9f66adcaca9cf0f76a93f4`.
L'URL anglaise contient `main` ; son empreinte identifie le contenu testé, pas
cette branche mutable.

## Reproduction

Depuis la racine du dépôt, avec Rust et les bibliothèques natives configurés :

```bash
# Helpers propres à cette session cloud, si disponibles.
. /workspace/.hakille-tools/env.sh
. /workspace/.hakille-tools/onnx/env.sh
cargo test -p hakille-inference --locked whisper_engine_tests -- --nocapture
```

Suite ordinaire : **5 tests passent, 3 ignorés explicitement**. Contrôles PCM,
langues, formes de configuration, taille des artefacts, filtres mel, logits
invalides et échéance expirée. Les trois tests ignorés nécessitent les modèles
locaux ; ils sont distincts des `12slow` de conformance, non lancés ici.

### `real_multilingual_fixtures_and_silence`

Pour reproduire l'**échec anglais connu**, après le fragment français partiel :

```bash
HAKILLE_MODELS_DIR="$PWD/models" \
HAKILLE_WHISPER_TEST_FR_PCM=/workspace/.hakille-tools/model-fixtures/whisper-human-fr.f32 \
HAKILLE_WHISPER_TEST_EN_PCM=/workspace/.hakille-tools/model-fixtures/sensevoice-en.f32 \
HAKILLE_WHISPER_TEST_FR_EXPECTED='mon adresse' \
HAKILLE_WHISPER_TEST_EN_EXPECTED='tribal chieftain' \
HAKILLE_WHISPER_TEST_PROMPT=Hakille \
HAKILLE_WHISPER_FORBID_WAKE_NAME=1 \
cargo test -p hakille-inference --locked real_multilingual_fixtures_and_silence -- --ignored --nocapture
```

Le fragment FR strict `changer mon adresse` reproduit également un échec.
`HAKILLE_WHISPER_FORBID_WAKE_NAME=1` vérifie l'absence de nom ajouté lorsque les
autres assertions passent. Le smoke partiel distinct utilise `presented him
with` comme fragment EN, sans mesurer l'exactitude globale. Sans
`HAKILLE_WHISPER_TEST_PROMPT`, il n'y a pas d'indice lexical. Ce test vérifie
aussi silence, prompts invalides, échéance expirée puis réutilisation du moteur.

### `synthetic_wake_comparison_report`

```bash
HAKILLE_WHISPER_WAKE_MANIFEST=/workspace/.hakille-tools/espeak/fixtures/manifest.json \
HAKILLE_WHISPER_WAKE_LANGUAGE=fixture \
cargo test -p hakille-inference --locked synthetic_wake_comparison_report -- --ignored --nocapture
```

`fixture` choisit `fr`/`en` d'après `voice`. Sans cette variable : autodétection ;
une valeur comme `fr` fixe la langue de tous les cas. Le test compare sans
indice puis avec `Hakille`, imprime les sorties réelles et échoue si un négatif
contient le nom littéral. Son succès ne signifie pas que les positifs ou les
commandes sont tous corrects.

Le manifeste JSON contient `synthetic: true` et `fixtures`, avec `name`, `voice`,
`pcm` et `contains_intended_wake_name` par cas. Adapter les chemins PCM après
déplacement des fixtures.

### `nonverbal_noise_with_lexical_prompt`

```bash
cargo test -p hakille-inference --locked nonverbal_noise_with_lexical_prompt -- --ignored --nocapture
```

Signaux déterministes générés en Rust, modèles locaux requis. Le test affiche
les transcriptions réelles, sans remplacer lui-même une hallucination par du
silence, puis vérifie l'absence du mot littéral « Hakille ».

## Journaux de session

- Suite ordinaire : `/tmp/hakille-whisper-tests-final.log`.
- Français humain strict : `/tmp/hakille-whisper-real.log` — **échec**.
- Humains avec indice, anglais strict : `/tmp/hakille-whisper-human-hint.log` — **échec**.
- Smoke humain partiel : `/tmp/hakille-whisper-human-partial-smoke.log` — passe,
  erreurs textuelles conservées.
- Synthèse/langue fixée : `/tmp/hakille-whisper-wake-forced.log`.
- Bruit/tonalité : `/tmp/hakille-whisper-noise.log`.
- Baseline : `/tmp/hakille-whisper-wake-baseline.log`, processus arrêté après
  les deux mesures utiles, pas une suite complète.
- Comparaison identique optimisée : `/tmp/hakille-whisper-wake-baseline-optimized.log`.

Ces journaux et chemins de préparation sont des artefacts de session, pas des
fichiers livrés dans le dépôt. Les résultats VAD/activation et gates workspace
sont consignés séparément par l'intégration ; ils ne sont pas déduits de ces
seuls essais ASR.

## Intégration applicative et régressions

La suite `cargo test --workspace --locked` a terminé avec **1207 tests
réussis, aucun échec et 22 tests explicitement ignorés**. Elle couvre notamment
70 tests de perception, 31 tests audio (un test matériel ignoré), 47 tests du
démon et 102 tests de la bibliothèque desktop. Les 12 tests Qwen lents restent
exclus de cette suite conformément à `AGENTS.md`.

Les vérifications incluent l'armement sur invocation seule, la commande dans
le même énoncé, le rejet des mentions ordinaires, l'expiration basée sur le
début de capture malgré un ASR lent, les erreurs et pertes audio, le refus
avant ASR d'un segment tronqué, les commandes annulées encore en file et leur
consommation unique. Aucune carte d'approbation n'est acceptée par la voix.

Le test supplémentaire `vad_whisper_activation_synthetic_fixtures` dans
`crates/hakille-voice/tests/activation_real.rs` utilise les vrais modèles
Silero et Whisper, avec la même configuration de segmentation que le worker.
Il nécessite les modèles et les fixtures générées ; il ne remplace pas une
capture CPAL sur microphone physique.

```bash
HAKILLE_VOICE_FIXTURES=/chemin/voice-fixtures \
HAKILLE_MODELS_DIR=/chemin/models \
cargo test --workspace --locked --test activation_real -- --ignored --nocapture --test-threads=1
```

Le test intégré a été **exécuté et réussi** depuis le binaire de test construit
par la suite workspace (`activation_real-01acbca3903db879 --ignored --nocapture
--test-threads=1`). Cette exécution évitait une reconstruction d'un autre graphe
de dépendances Cargo ; elle utilise le code final du workspace. Résultats :

| Fixture synthétique | Segment Silero | Transcription réelle | Décision |
| --- | ---: | --- | --- |
| `fr_hakille_only` | 1,22 s | `Hakille` | `Armed` |
| `en_hakille_command` | 2,56 s | `Hakille, what time is it?` | `Command("what time is it?")` |
| `en_greeting_negative` | 2,53 s | `Hello, what time is it?` | `Ignored(NoWakeWord)` |

La transcription de ces segments a pris respectivement 990, 1395 et 1311 ms.
Le test complet termine en 5,39 s, sans capture physique ni action exécutée.
Journal : `/tmp/hakille-voice-integration-real.log`.

Le binaire CLI a également été lancé réellement :

- `voice devices` inventorie seulement une entrée ALSA virtuelle produisant
  des zéros, sans microphone physique ; aucune capture ouverte par l'inventaire.
- `--serve --listen` et une langue `zz` échouent avant la création d'identité.
- Les trois artefacts Whisper passent la vérification de taille et SHA-256.
- `voice test` français en deux fichiers passe de `armed` à `command` ; la
  deuxième transcription synthétique reste erronée (`C'est le casement, eh`).
  Ce résultat prouve la transition, pas la bonne compréhension du calendrier.
- `voice test` anglais produit `what time is it?` avec invocation et ignore
  la salutation sans invocation. `actions_executed=false` dans tous ces essais.

Journaux : `/tmp/hakille-voice-cli-smokes.json` et
`/tmp/hakille-voice-file-smokes.json`.

Les gates finales passent : formatage, tokens générés, `cargo check` et Clippy
sur tous les targets, tests workspace, conformance (**66 réussis, 7 lents
ignorés**), `cargo deny check bans sources advisories`, et construction de tous
les binaires du workspace. Clippy conserve des avertissements préexistants
sur les substrats Windows/Linux ; aucun avertissement dans les modules vocaux
modifiés. Les erreurs qualitatives de transcription décrites plus haut restent
présentes malgré la réussite des tests logiciels.
