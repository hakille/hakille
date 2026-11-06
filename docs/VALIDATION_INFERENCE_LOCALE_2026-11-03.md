# Inférence locale : implémentation et validation du 3 novembre 2026

Périmètre : les **sept entrées du catalogue du dépôt**, avec leurs onze
artefacts, exécutées sous Linux x86_64 CPU. Les modèles supplémentaires cités
comme objectifs historiques (Moshi 7,6B, Qwen3, VibeVoice, Nemotron…) ne sont
pas inclus. Les commandes reproductibles sont dans
[INFERENCE_LOCALE.md](INFERENCE_LOCALE.md).

## Résultat fonctionnel

La commande intégrée suivante a réussi pour les sept modèles, après contrôle
de taille et SHA-256 de chaque artefact sélectionné :

```bash
hakille-daemon models test --all --models-dir ./models \
  --image /tmp/hakille-ui-tars-screen.png \
  --text 'Click the Save button' \
  --endpoint http://127.0.0.1:18081 --json
```

L'image de test contient un bouton « Save », sans données utilisateur. Le
serveur local est lancé avec l'alias `ui-tars-2b-sft`, le GGUF du catalogue et
son projecteur. La commande ne réalise aucune action clavier/souris.
Le résultat est conservé dans `/tmp/hakille-models-all-test.json` : sept lignes
`ok: true`. Ce test de fonctionnement ne constitue pas un benchmark de qualité.

| Modèle | Implémentation et preuve complémentaire |
| --- | --- |
| Silero VAD | Contexte de 64 échantillons, état récurrent et reliquat final traités. Sur la voix de référence : 157 fenêtres sur 224 dépassent 0,5 ; maximum 0,9999976. |
| SenseVoice | Frontend Kaldi corrigé, LFR et CMVN du modèle, décodage CTC, langue/émotion/événements. La transcription du fichier anglais correspond à la sortie publiée par sherpa ; confiance calculée 0,9481407, langue `en`, `NEUTRAL`, `Speech`. |
| CAM++ | Frontend Kaldi et segment entier. Sortie de 192 composantes, norme 0,99999994 et répétition déterministe ; changer l'audio après deux secondes change effectivement le vecteur. Ce résultat n'évalue pas la précision biométrique sur des identités annotées. |
| MiniLM | Tokenizer officiel, fenêtre 256, pooling masqué et normalisation. Comparaison des tokens et des 384 composantes avec deux références Python ONNX/tokenizers indépendantes, tolérance 3e-5. |
| Qwen 2.5 | Génération Candle Q4_K_M réelle : `2+2=4`, cinq tokens générés. Annulation avant et pendant une requête, puis réutilisation du même moteur ; streaming égal au texte final et callbacks concurrents sans perte. |
| Mimi | Encodeur et décodeur complets SEANet/transformers/RVQ, huit codebooks, trames mono 24 kHz de 80 ms. Trois trames réelles encodées/décodées ; reset strictement déterministe et états utilisateur/système indépendants. |
| UI-TARS 2B | Forward Qwen2-VL via llama.cpp natif, avec projecteur vérifié. Le client Rust et son schéma complet proposent `click(750,750)` dans une image 560×280, soit le pixel (420,210), à l'intérieur du bouton attendu. |

Le premier test UI-TARS direct a pris environ 119 secondes CPU sous charge de
compilation. Le test CLI intègre aussi le coût du contrôle des fichiers : ces
temps ne sont ni des garanties de service ni des performances GPU. Les moteurs
audio et texte ne doivent pas être annoncés temps réel sur cette seule preuve.

## Fiabilité du chargement et de l'utilisation

- Le téléchargement utilise la confiance TLS native de l'hôte et conserve les
  vérifications TLS et SHA-256. Le test CLI a téléchargé Silero depuis un cache
  vide, refusé une corruption de même taille, réparé le cache puis revérifié
  son SHA. Les reprises HTTP et écritures atomiques ont leurs régressions locales.
- Les auxiliaires sont vérifiés comme les poids principaux ; UI-TARS inclut
  désormais son projecteur. Le statut rapide de présence ne remplace jamais
  la vérification d'intégrité avant une exécution CLI.
- `models infer` accepte du texte, du WAV ou une image selon le modèle. Il
  refuse les options incompatibles. Les sorties WAV sont publiées atomiquement
  sans écraser un fichier existant. Les modèles peuvent être testés séparément.
- Le dossier explicite est respecté par la CLI et les moteurs audio du démon,
  ainsi que par le singleton Qwen configuré. L'absence d'un modèle visuel ne
  bloque plus la configuration audio du démon.
- Le registre de réservations refuse les dépassements et conserve son état en
  cas d'échec. Il ne constitue pas un allocateur qui décharge les tenseurs des
  moteurs. Le budget du registre n'est pas une garantie de RAM du processus.
- Les lignes UI distinguent `Available` (« Présent — non testé ») d'un forward
  réel. La seule présence d'un fichier ne produit plus un badge `Real`.

## Vérification automatisée

La suite `cargo test --workspace --locked` a terminé avec **1 153 tests réussis,
aucun échec et 17 tests ignorés par défaut**. Parmi ces derniers, les trois
fixtures audio et les deux tests Mimi neuronaux ont été exécutés explicitement
et ont réussi. Les douze scénarios longs Qwen n'ont pas été lancés en debug,
conformément à `AGENTS.md` ; ils restent à exécuter dans leur gate dédié.

Les tests courts avec artefacts ont effectivement utilisé les poids présents.
Les résultats ciblés inclus dans le workspace ne s'ajoutent pas une deuxième
fois à son total. La première tentative globale avait révélé une ancienne
assertion VAD supposant que tout sinus score davantage que le silence : les
tests corrigés vérifient le seuil de non-parole et les compteurs de forwards,
sans inventer une relation que le modèle ne garantit pas.

Les contrôles complémentaires ont également réussi :

| Contrôle | Résultat |
| --- | --- |
| `cargo fmt --all -- --check` | Réussi. |
| `cargo check --workspace --all-targets --locked` | Réussi. |
| `cargo clippy --workspace --all-targets --locked --no-deps` | Réussi ; avertissements préexistants des substrats Windows/Linux conservés, aucun nouveau lint dans le lot inférence. |
| `cargo build --workspace --locked` | Réussi, exécutables liés. |
| `xtask tokens --check` | Sources générées à jour. |
| `xtask conformance` | 66 tests réussis, sept longs ignorés ; déjà compris dans le total workspace. |
| `cargo deny check bans sources advisories` | Trois contrôles réussis, aucune nouvelle exception ajoutée. |

Après un nettoyage de lint sans changement fonctionnel dans la lecture WAV,
les quatre tests de CLI, Clippy workspace, le formatage et la construction ont
été revérifiés avec succès. Journaux `/tmp/hakille-local-final-*.log` et
`/tmp/hakille-local-cli-final-*.log`. Ce sont des résultats locaux, pas de CI distante.

Avec le binaire final, trois commandes `models infer` ont également réussi :

- SenseVoice lit le WAV anglais et restitue la transcription attendue en 3,14 s.
- Qwen répond `4` à « Combien font 2 + 2 ? Réponds uniquement par le nombre. »
  en 13,81 s.
- Mimi reconstruit un extrait vocal de 0,24 s en WAV float32 à 24 kHz :
  5 760 échantillons finis, énergie non nulle, en 7,16 s.

Le journal est `/tmp/hakille-model-cli-exercises.log`, les fichiers d'entrée et
de sortie sont dans `/tmp/hakille-model-infer-95_uf2gd`. Ces durées incluent le
chargement et la vérification des poids dans ce runner, et ne prouvent pas une
capacité temps réel.

La dépendance SHA-256 est désormais optimisée en profil de développement,
comme les dépendances Candle déjà optimisées. La vérification obligatoire des
onze artefacts réussit en **2,932 s** avec le binaire final et le cache disque
chaud ; aucune empreinte ni lecture n'a été supprimée. Résultat :
`/tmp/hakille-models-verify-optimized.json`.

## Sources des preuves et limites

- Suite globale : `/tmp/hakille-local-final-tests.log`.
- CLI des sept moteurs : `/tmp/hakille-models-all-test.json`.
- Intégrité de tous les artefacts : `/tmp/hakille-models-verify.json`.
- Fixtures audio réelles : `/tmp/audio-fixtures-final.log`.
- MiniLM indépendant : `/tmp/hakille-minilm-final.log`.
- Qwen : `/tmp/hakille-qwen-local-tests.log` et `/tmp/hakille-qwen-stream-tests.log`.
- Mimi : `/tmp/hakille-mimi-neural-smoke.log` et `/tmp/hakille-mimi-duplex-smoke-final.log`.
- UI-TARS : `/tmp/hakille-ui-tars-server.log` et [provenance du runtime](../crates/hakille-perception/UI_TARS_LOCAL.md).

La [fixture anglaise sherpa](https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/test_wavs/en.wav)
est un WAV mono 16 kHz de 7,152 secondes, SHA-256
`eb1eb008904465b74c304aad8342e8c7d3c6e61ffe9f66adcaca9cf0f76a93f4`.
La [transcription de référence publiée](https://github.com/k2-fsa/sherpa/blob/master/docs/source/onnx/sense-voice/code/2024-07-17-itn.txt)
est une sortie upstream indépendante, pas une annotation humaine exhaustive.
Les références numériques de fbank proviennent de `kaldi-native-fbank 1.22.5`.
Python sert uniquement à ces comparaisons de validation ; il n'est pas une
dépendance du produit.

Seul le CPU Linux a été exécuté dans cette session. CUDA, Metal, mobile, Web,
microphones physiques, lecteurs d'écran et distribution native exigent leurs
validations propres. SenseVoice ne fournit pas une garantie de transcription
française ; ses langues documentées sont zh/en/ja/ko/yue. Mimi est un codec,
pas le modèle conversationnel Moshi ; son état doit être réinitialisé avant
la limite de contexte du runtime. L'échéance Qwen est coopérative entre les
calculs, pas une préemption du calcul Candle en cours. L'alias d'un serveur
UI-TARS local ne prouve pas cryptographiquement quels poids ce serveur charge.

Les anciens embeddings doivent être recalculés avec le tokenizer corrigé.
Les tokens Mimi historiques et neuronaux portent des formats distincts ; un
décodage incompatible est refusé. Aucun poids, secret ni jeton n'a été ajouté
au suivi Git ; aucune publication n'a été effectuée.
