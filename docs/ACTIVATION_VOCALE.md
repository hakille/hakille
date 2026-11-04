# Activation vocale de Hakille

État de l'analyse : 4 novembre 2026. Le principe est strict : **une activité
sonore n'est pas la reconnaissance d'un mot**. L'activation ne constitue ni
une vérification d'identité, ni une approbation d'action.

## Pourquoi l'ancien chemin était trompeur

La méthode historique `WakeWordDetector::process` calculait une énergie RMS,
puis comparait une confiance dérivée au seuil. Un tableau constant de valeur
0,3 pouvait ainsi produire `detected=true` et le libellé « Dis Hakille », sans
qu'aucun mot n'ait été prononcé ou reconnu.

`process_gated` ajoutait Silero VAD, une énergie dans la bande 300–3400 Hz et
une durée minimale. Ces critères peuvent caractériser de la parole, mais ne
distinguent pas « Hakille » de toute autre phrase. Diminuer le seuil ou la
durée faisait déclencher les anciens tests sur des voyelles synthétiques.
Ces fixtures ne constituaient aucune preuve de reconnaissance du mot-clé.

Enfin, la présence d'un fichier `openwakeword_*.onnx` provoquait son ouverture
avec `OrtScalarScorer`, alimenté par des ratios de bande. Un frontend
openWakeWord requiert un traitement audio et des embeddings compatibles avec
le classifieur entraîné pour le mot cible. Les ratios artisanaux n'ont pas
ce contrat. Même un forward ONNX techniquement exécuté ne démontrait donc
pas la reconnaissance de Hakille ; les échecs pouvaient conserver une
décision énergétique positive.

## Comportement corrigé du code historique

Dans [wake_word.rs](../crates/hakille-perception/src/audio/wake_word.rs) :

- `process` reste disponible pour la compatibilité source, avec un attribut
  de dépréciation explicite. Il retourne toujours `detected=false` et
  `confidence=0`. `measure_activity` expose les seules mesures acoustiques.
- `process_gated` conserve de vraies mesures Silero et de bande, alignées sur
  les mêmes fenêtres de 512 échantillons, y compris avec des chunks
  fragmentés. Il ne retourne jamais une détection de mot.
- `recognition_available()` retourne faux ; `unavailable_reason()` explique
  le frontend et le checkpoint compatibles manquants. Le champ `keyword`
  représente le mot demandé, pas un mot reconnu lorsque `detected` est faux.
- Sans checkpoint, `maybe_upgrade_to_ort()` retourne `Ok(false)`. Un fichier
  présent est explicitement refusé : aucun frontend compatible n'est raccordé.
  Il n'y a plus de bascule vers `OrtReal`, ni de forward `OrtScalarScorer`.
- `provision` peut encore télécharger un fichier, mais ne raccorde aucun
  moteur de reconnaissance. Les signatures publiques historiques restent
  disponibles ; leurs anciens seuils ne réactivent pas l'heuristique.

Dans [pipeline.rs](../crates/hakille-perception/src/audio/pipeline.rs), la
segmentation `Idle → Listening → Transcribing` reste fondée sur le vrai VAD.
Le pipeline n'appelle plus le faux détecteur de mot-clé et ne produit aucun
événement `Wake` à partir de l'énergie. Les champs historiques `wake_word` et
`wake_threshold` sont conservés et validés pour compatibilité ; ils ne
filtrent pas la parole. Une configuration de mot acoustique est signalée
comme indisponible et `wake_backend()` retourne `None`.

`reset_audio()` abandonne les échantillons en attente, le tampon circulaire,
l'énoncé courant et le carry VAD. Il réinitialise l'état récurrent sans
recharger la session ONNX. Cette opération sert à empêcher qu'un ancien
audio soit traité après une pause/reprise du microphone. Les compteurs
cumulés restent consultables.

## Activation après transcription

Le nouveau contrôleur
[activation.rs](../crates/hakille-perception/src/audio/activation.rs) reçoit
du **texte produit par un ASR**, sa langue et un temps monotone. Il ne reçoit
aucune énergie audio et ne peut donc convertir une hausse de volume en mot.

Le service fournit désormais à Whisper l'indice lexical explicite `Hakille`
via son contexte de décodage natif. Cet indice aide le modèle à transcrire
le nom propre ; aucun remplacement de mot ni texte de commande fabriqué
n'est appliqué après ASR. Il peut toutefois biaiser la reconnaissance : la
validation négative et la confirmation des actions restent nécessaires.

Le flux raccordé par le worker d'entrée vocale est :

```text
Microphone opt-in → segmentation Silero → ASR local
                → WakeActivation → commande proposée au circuit normal
```

Les alias par défaut sont `Hakille`, `Hakil` et `Hakill`. La correspondance
porte sur des mots explicites, avec normalisation documentée de la casse et
des accents latins. Il n'y a pas de rapprochement flou universel. Par défaut,
l'alias doit commencer l'énoncé ou suivre un vocatif autorisé, comme « Dis »,
« Hey » ou « Bonjour ». Une mention en milieu de conversation n'est pas une
invocation. Les autres écritures nécessitent des alias explicitement définis.

- « Hakille, ouvre le calendrier » fournit une commande sans son préfixe.
- « Dis Hakille » arme l'attente de la prochaine commande pendant 10 secondes.
- Une transcription sans invocation reste ignorée hors période armée.
- L'état `Busy` bloque une deuxième commande ; la fin ouvre un cooldown de
  1 seconde par défaut. `on_transcript_captured` conserve un horodatage de
  capture distinct du moment de traitement et refuse les résultats ASR
  acquis avant une annulation, une période occupée ou la fin du cooldown.

Le texte, sa ponctuation interne et sa langue sont conservés. La limite par
défaut est de 8192 octets. L'ASR peut se tromper ou halluciner une invocation :
ce contrôleur réduit les faux déclenchements énergétiques, mais n'est pas une
preuve que l'utilisateur a effectivement prononcé le mot. Les contrôles de
capabilities et les confirmations des cartes restent nécessaires.

## Démarrage et intégration : constat initial

Avant ce lot, l'inspection de `hakille-daemon/src/main.rs` montrait que :

- `--listen` appelait `listen_once` pour **un seul tour vocal** au démarrage,
  puis poursuivait le REPL texte ; ce n'était pas une écoute permanente.
- `_wake_word` était construit mais inutilisé par cette écoute.
- `--serve` retournait dans `run_serve` avant le bloc `--listen` : combiner ces
  options ne raccordait pas l'écoute au serveur IPC.
- Le desktop ne capturait aucun microphone et ne proposait pas de contrôle
  utilisateur pour le démarrer ou l'arrêter.

Ces constats décrivent le point de départ. Le nouveau worker
[`hakille_voice::input`](../crates/hakille-voice/src/input.rs) raccorde la
capture, le VAD, Whisper et l'activation à la session CLI ou au desktop.
L'écoute reste désactivée par défaut et n'est ouverte qu'après une activation
explicite. Une absence de périphérique, une permission refusée ou un modèle
manquant donne une erreur, sans transcription inventée.

## Utilisation

Depuis la racine du dépôt, provisionner les modèles nécessaires :

```bash
cargo run --locked -p hakille-daemon -- models download --model silero-vad-v5
cargo run --locked -p hakille-daemon -- models download --model whisper-tiny-multilingual
cargo run --locked -p hakille-daemon -- models verify --model silero-vad-v5
cargo run --locked -p hakille-daemon -- models verify --model whisper-tiny-multilingual
```

Whisper utilise les poids, la configuration et le tokenizer du catalogue,
vérifiés par SHA-256. Pour traiter ensuite une commande avec le noyau, les
modèles nécessaires à ce traitement doivent également être présents, notamment
Qwen ; voir [le catalogue des modèles](MODELS.md).

L'inventaire suivant n'ouvre aucune capture :

```bash
cargo run --locked -p hakille-daemon -- voice devices
```

Une session vocale CLI continue, sans REPL texte concurrent, se lance ainsi :

```bash
cargo run --locked -p hakille-daemon -- --listen --models-dir models --voice-language fr
```

Dire « Hakille, … » ou « Dis Hakille », puis la commande. `Ctrl+C` arrête la
session. `--voice-language auto` autorise la détection de langue, mais les
phrases courtes peuvent être mal identifiées ; préciser `fr` est utile pour
une session en français. `--voice-device "NOM EXACT"` sélectionne une entrée
annoncée par `voice devices`. `--wake-alias "MOT"` ajoute un alias explicite
et peut être répété. `--listen` est incompatible avec `--serve` et
`--eval-prompt` ; les options vocales CLI exigent `--listen`.

Pour le desktop, démarrer le démon IPC puis, dans un autre terminal, le HUD :

```bash
cargo run --locked -p hakille-daemon -- --serve
cargo run --locked -p hakille-ui-desktop --bin hakille-desktop -- --voice --models-dir models --voice-language fr
```

`--voice` (alias `--voiceactive`) est un opt-in et rend le HUD visible. Les
options `--voice-device` et `--wake-alias` sont également disponibles.
**F8 agit uniquement lorsque la fenêtre a le focus**, ce n'est pas un
raccourci global. Le bouton microphone et son action AccessKit permettent
également de démarrer ou d'arrêter l'écoute. Sans `--voice`, elle reste
initialement désactivée.

Masquer le HUD ne désactive pas une écoute déjà choisie : le microphone peut
rester actif. Une activation seule ou une erreur rouvre le HUD. Le texte
vocal suit la soumission habituelle, sans contourner les cartes d'approbation.
La capture est suspendue pendant une génération, une carte en attente ou une
soumission IPC ; un brouillon clavier existant est protégé. Les modes de test
`--smoke` et `--authorized` refusent la capture vocale.

La capture impose une limite de **12 secondes par segment**, incluant le
contexte audio conservé par le pipeline. Si cette borne force la fin, le
segment est signalé comme tronqué et **refusé avant transcription**, afin de
ne pas transformer le début d'une phrase en commande complète. Le worker
arrête la capture avant ASR et réinitialise les tampons lors des transitions ;
les résultats d'une session annulée sont rejetés.

### Diagnostic sans microphone

Le diagnostic utilise le même moteur Whisper et le même contrôleur lexical,
mais n'exécute ni ne soumet aucune commande au noyau :

```bash
cargo run --locked -p hakille-daemon -- voice test --audio commande.wav --models-dir models --language fr
# Invocation seule puis commande dans un second fichier :
cargo run --locked -p hakille-daemon -- voice test --audio invocation.wav --next-audio commande.wav --models-dir models --language fr
```

Ici les options sont `--language` et `--alias`, alors que la capture utilise
`--voice-language` et `--wake-alias`. La sortie JSON distingue transcription,
langue, décision `ignored` / `armed` / `command`, et indique explicitement
`microphone_tested=false`, `actions_executed=false` et `lexical_hint="Hakille"`.
Ce diagnostic prend des fichiers de 0,1 à 30 secondes ; cette borne du moteur ne remplace pas la limite
plus courte de la capture continue. Un fichier WAV ne valide ni le micro,
ni ses permissions, ni le comportement du VAD dans une pièce réelle.

## Langues et limites

Le nouveau backend Whisper expose 99 codes de langue multilingues. Cela ne
garantit ni une bonne transcription de toutes ces langues, ni la
reconnaissance du nom propre « Hakille » dans tous les accents, bruits et
écritures. L'activation reste dépendante du texte effectivement produit par
l'ASR et des alias configurés. Les essais audio réels doivent identifier le
checkpoint, la langue, le signal et les résultats constatés. Un test textuel
du contrôleur ne vaut pas un test de reconnaissance vocale.

Ce lot est compilé et testé sur Linux, sans microphone physique disponible.
Il n'établit pas le bon fonctionnement d'un périphérique Windows, macOS ou
Linux réel, ni une compilation complète validée sur les deux autres OS.
Les permissions, pilotes, changements de périphérique, bruits ambiants,
lecteurs d'écran et la latence de bout en bout nécessitent des essais sur ces
machines. Les tests textuels avec alias arabe ou japonais vérifient le
contrôleur Unicode configurable, pas la reconnaissance acoustique de ces
langues. Les scripts sans séparation des mots ont également des limites de
segmentation ; une invocation seule suivie d'un second énoncé est distincte
d'une invocation accolée à une commande.

Les preuves acoustiques détaillées figurent dans le [rapport de validation](VALIDATION_VOCALE_2026-11-04.md). Un essai sur
une voix humaine française a produit « Je suis changé mon adresse » pour la
référence « je souhaite changer mon adresse » : c'est une transcription
partielle, pas une réussite exacte. Un court énoncé français a aussi été
classé comme japonais en mode automatique. Sur un échantillon synthétique
avec `fr` imposé, le nom a été mal reconnu sans indice lexical ; l'indice
« Hakille » a permis de retrouver le nom, mais la commande restait erronée.
Dans la comparaison synthétique de 16 passages, les cinq cas positifs ont
retrouvé le nom avec indice contre zéro sans indice ; trois cas négatifs
testés avec et sans indice n'ont pas produit le nom. **Il n'y avait que six
audios distincts** : des variantes orthographiques étaient prononcées de
façon identique. Ces nombres ne constituent donc pas une estimation fiable
de précision ou du taux de faux déclenchements.

L'énoncé anglais synthétique « Hakille what time is it » et l'invocation
française seule ont été correctement reconnus avec indice ; une commande
française restait erronée. Les voix humaines FR/EN avec indice n’ont pas ajouté le nom, mais leur transcription reste imparfaite ; l’essai anglais a notamment ajouté une négation absente de la référence. Le silence, le bruit blanc et une tonalité de 440 Hz ont donné des transcriptions vides. Ces résultats interdisent d'annoncer
une compréhension universelle ou une précision garantie sur les 99 codes.

Une première mesure CPU après optimisation GEMM est passée d'environ
47,392 secondes à 1,424 seconde sur le même audio, avec la même sortie et le même prétraitement. C'est une mesure
ponctuelle d'inférence, pas une garantie de latence microphone de bout en
bout, et cette accélération ne corrige pas les erreurs de transcription.

## Vérifications exécutées

La suite `cargo test --workspace --locked` passe : **1207 tests réussis,
0 échec, 22 tests explicitement ignorés**. Cela inclut 70 tests de perception,
31 tests audio, les tests du worker, du CLI et de l’interface. Les tests
ignorés nécessitent des fixtures, du matériel ou des inférences longues ;
les essais réels lancés séparément sont distingués dans le
[rapport de validation](VALIDATION_VOCALE_2026-11-04.md).

Les régressions couvrent l’absence de faux mot-clé énergétique, le refus d’un
checkpoint incompatible, les interruptions et erreurs microphone, les files
bornées, les commandes annulées, les fenêtres d’activation avec ASR lent et
les segments tronqués. Le test Silero de fin forcée vérifie la conservation
du signal de troncature après `take_utterance`, ainsi que sa remise à zéro.
Une compilation Linux et ces tests ne remplacent pas un essai de microphone
sur Windows, macOS ou Linux.

Le test intégré réel Silero → Whisper → activation a également réussi sur
l’invocation française seule, une commande anglaise et un négatif anglais.
Les fichiers sont synthétiques ; le [rapport](VALIDATION_VOCALE_2026-11-04.md)
distingue ces preuves des essais matériels encore nécessaires.
