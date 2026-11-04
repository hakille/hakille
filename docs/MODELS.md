# Modèles locaux et moteurs réellement disponibles

Le catalogue contient **sept modèles**, définis dans
[`catalog.rs`](../crates/hakille-inference/src/catalog.rs). Les commandes et
exemples reproductibles sont dans [Inférence locale](INFERENCE_LOCALE.md).
Les poids sont téléchargés séparément et restent hors Git. L'installation
ou l'inspection d'un fichier ne prouve jamais qu'un forward a réussi sur la
machine courante.

| Identifiant exact pour `--model` | Niveau | Fichiers locaux requis | Moteur et résultat |
| --- | --- | --- | --- |
| `silero-vad-v5` | guest | `silero_vad.onnx` | ONNX Runtime CPU ; score d'activité vocale par fenêtre de 32 ms |
| `campplus-speaker-id` | guest | `campplus_zh_en_16k_advanced.onnx` | ONNX Runtime CPU ; vecteur de locuteur à 192 composantes |
| `all-minilm-l6-v2` | native | `all-minilm-l6-v2.onnx`, `minilm_tokenizer.json` | ONNX Runtime CPU ; vecteur de texte à 384 composantes |
| `sensevoice-small-int8` | native | `sensevoice_small.int8.onnx`, `tokens.txt` | ONNX Runtime CPU ; transcription CTC, langue et marqueurs acoustiques |
| `qwen2.5-0.5b-instruct-q4km` | native | `qwen2.5-0.5b-instruct-q4_k_m.gguf`, `tokenizer.json` | Candle CPU ; génération de texte Qwen 2.5 0,5B Q4_K_M |
| `ui-tars-2b-sft-q4km` | native | `ui-tars-2b-sft-q4_k_m.gguf`, `mmproj-UI-TARS-2B-SFT-f16.gguf` | Serveur natif llama.cpp local ; proposition d'action depuis une image et une instruction |
| `moshi-mimi-s2s` | sovereign | `mimi_candle.safetensors` | Candle CPU ; encodage et décodage audio neuronal Mimi |

Les niveaux sont cumulatifs : `guest` sélectionne deux modèles, `native` les
six premiers et `sovereign` les sept. Ce classement décrit une sélection de
modèles, pas une autorisation ou une garantie de ressources. Les onze fichiers
représentent environ **3,69 Go sur disque** ; cette taille n'est pas la RAM
nécessaire à leur exécution. UI-TARS représente à lui seul environ 2,45 Go de
poids, projecteur inclus. Le serveur llama.cpp et les bibliothèques natives
s'ajoutent à ces fichiers.

## Installation, intégrité et exécution

`models list` fournit un inventaire rapide de présence et de taille.
`models download` télécharge les artefacts du catalogue et contrôle leur
SHA-256 avant de les installer ; `models verify` recalcule l'intégrité des
poids **et** des fichiers auxiliaires. Les empreintes et URL attendues sont
dans le catalogue, sans duplication ici.

`models test` et `models infer` contrôlent cette intégrité puis exécutent le
moteur. Un modèle absent, corrompu ou inexécutable produit une erreur. Ces
commandes ne démarrent pas le HUD et n'exécutent aucune action GUI proposée.
Les tests de fonctionnement par défaut emploient parfois du silence ou une
sinusoïde : ils vérifient le chargement, les tenseurs et le forward, sans
mesurer la qualité de transcription, de voix ou de reconnaissance.

## Contrats et limites des moteurs

**Silero VAD.** Le flux mono 16 kHz est traité par fenêtres de 512 échantillons,
avec 64 échantillons de contexte et l'état récurrent du modèle. La fin de flux
traite le reliquat avec padding ; un nouveau flux doit réinitialiser son état.
Les scores doivent être finis et compris entre 0 et 1. Un score de parole ne
constitue ni une transcription ni une identification de personne.

**CAM++.** Le frontend Kaldi applique notamment fenêtre Povey, préaccentuation
et normalisation des caractéristiques. Le graphe à axe temporel dynamique
reçoit tout le segment ; la sortie de 192 composantes est normalisée L2.
L'API accepte du PCM mono 16 kHz de 25 ms à 120 s. Les comparaisons de locuteurs
n'établissent pas une identité légale, une résistance au rejeu ou une
autorisation de capability. Aucun taux général d'erreur biométrique n'est
mesuré par les tests de fonctionnement.

**MiniLM.** Le tokenizer JSON officiel est exécuté intégralement avec
`tokenizers` : normalisation BERT, accents, ponctuation, caractères chinois,
WordPiece et tokens spéciaux. La séquence est tronquée puis complétée à
**256 tokens**, tokens spéciaux compris. La sortie est une moyenne des états
cachés pondérée par le masque d'attention, puis normalisée L2. Les formes,
types, valeurs non finies et vecteurs nuls sont contrôlés. Le texte Unicode
est pris en charge par le tokenizer ; cela ne garantit pas une qualité
sémantique égale dans toutes les langues.

Les [références indépendantes MiniLM](../crates/hakille-inference/tests/data/README.md)
comparent les tokens et les 384 composantes à ONNX Runtime Python, utilisé
uniquement pour produire les fixtures. Le runtime de production reste Rust
et ONNX Runtime natif. Les anciens embeddings issus du tokenizer manuel
doivent être recalculés avant mélange avec les nouveaux : l'index de mémoire
ne stocke pas encore de version de modèle. Les insertions et recherches
refusent les valeurs NaN/Inf ; ce contrôle ne migre pas un ancien index.

**SenseVoice.** L'export sherpa INT8 utilise le frontend Kaldi, LFR(7,6) et le
CMVN global lu dans le modèle. Le décodage CTC est greedy ; une demande de
beam search non implémenté échoue. Les langues annoncées par le checkpoint
sont mandarin, anglais, japonais, coréen et cantonais ; le français n'est pas
garanti. L'API accepte du PCM mono 16 kHz de 25 ms à 120 s. Langue, émotions
et événements proviennent des tokens décodés. La confiance est une mesure
issue des sorties du réseau, pas une probabilité calibrée de transcription
correcte. Sans filtrage VAD, l'ASR peut produire du texte sur du silence.

**Qwen.** Le moteur charge le GGUF et le tokenizer, applique le format de
conversation puis effectue le forward CPU et le décodage avec cache KV.
La capacité de contexte est lue dans le GGUF ; elle est de 32 768 tokens pour
le fichier du catalogue. Prompt et budget de sortie doivent y tenir ensemble.
Une échéance et une demande d'annulation sont contrôlées coopérativement
entre blocs de préremplissage de 256 tokens et forwards de décodage. Un
forward Candle déjà en cours n'est pas préemptible : aucune échéance temps
réel stricte n'est garantie. La sortie et sa latence dépendent du prompt,
du budget de tokens et de la machine. Ce petit modèle peut se tromper.

**UI-TARS.** Le forward multimodal nécessite les **deux** GGUF et un serveur
`llama-server` compatible Qwen2-VL, installé et démarré explicitement. Le
client accepte uniquement un endpoint HTTP sur IP littérale de boucle locale,
sans proxy ni redirection, et l'alias `ui-tars-2b-sft`. Cet alias n'atteste pas
cryptographiquement les poids chargés dans le serveur. La réponse JSON est
validée ; les coordonnées sont normalisées de 0 à 1000. Elle reste une
proposition, sans clic ni frappe automatique. Le détecteur classique de
régions de l'interface est un chemin distinct. Voir les
[commandes, limites et preuves UI-TARS](../crates/hakille-perception/UI_TARS_LOCAL.md).

**Mimi.** Malgré l'identifiant historique `moshi-mimi-s2s`, ce fichier contient
le **codec Mimi**, sans modèle conversationnel Moshi. Le moteur exécute
l'encodeur SEANet, les transformers causaux, la quantification et le décodeur
SEANet du checkpoint complet. Il traite du mono 24 kHz, par trames de
1 920 échantillons (80 ms), avec huit codebooks de 2 048 entrées. Chaque flux
doit conserver son propre état ; `fresh_stream`/reset sépare les historiques.
Les tokens de l'ancien codec de projection et ceux du réseau sont distingués
et ne sont pas interchangeables. Le contexte positionnel Candle impose un
reset avant environ 327 secondes de flux continu. Le forward neuronal ne
prouve ni une reconstruction perceptuelle parfaite ni le temps réel.

## Portée de la validation

Les forwards réels décrits dans les rapports de validation ont été exécutés
sur **Linux CPU**. La présence de code portable ne démontre pas une
installation native testée sous Windows, macOS, Android, iOS, Web ou Redox.
Les bibliothèques et le serveur natifs doivent être disponibles pour la
plateforme visée. La CLI locale du démon est destinée à Linux, Windows et
macOS ; les validations des autres cibles restent séparées.

Les anciens documents citaient VibeVoice, Nemotron, Moshi 7,6B, Qwen3 et
UI-TARS 7B comme objectifs. Ils ne font pas partie de ces sept entrées.
Le TTS formantique et les chemins historiques simulés ne doivent pas être
confondus avec des modèles vocaux neuronaux installés. Aucun téléchargement,
inspection de header ou registre de mémoire ne remplace un test d'inférence.
