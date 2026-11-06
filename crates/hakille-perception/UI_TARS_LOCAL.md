# UI-TARS local : proposition visuelle, sans effet GUI

`vision::ui_tars_local::UiTarsLocalClient` interroge un moteur **Qwen2-VL
multimodal réel** : `llama-server` chargé avec les deux GGUF UI-TARS. Il est
distinct du détecteur de régions classique `UiTarsGroundingEngine`, qui ne
décode pas ce modèle. L'absence du serveur, du projecteur ou d'une réponse
valide produit une erreur ; aucun repli ne masque cet échec.

## Artefacts et moteur

Le checkpoint original est
[`bytedance-research/UI-TARS-2B-SFT`](https://huggingface.co/bytedance-research/UI-TARS-2B-SFT),
architecture `Qwen2VLForConditionalGeneration`. La quantification provient de
[`bartowski/UI-TARS-2B-SFT-GGUF`](https://huggingface.co/bartowski/UI-TARS-2B-SFT-GGUF/tree/5a0ef8290ddab04e691efca7fce30105bda2decb).
Le tokenizer et le template de conversation sont intégrés au GGUF principal.

| Fichier distant | Octets | SHA-256 |
| --- | ---: | --- |
| `UI-TARS-2B-SFT-Q4_K_M.gguf` | 1117319712 | `f316ecef2e2e8fa9a29e81ba2c9efa8ea6f408241e175331332960c7160e9c53` |
| `mmproj-UI-TARS-2B-SFT-f16.gguf` | 1331656192 | `b9c248c4ff9770b3cd82d0ed5b5d377ea02500d8d22368db9b7c2d4476b32ce8` |

Le moteur CPU Linux utilisé pour vérifier ce chemin est la distribution
officielle [llama.cpp b12074](https://github.com/ggml-org/llama.cpp/releases/tag/b12074),
commit `b74298c10`, archive `llama-b12074-bin-ubuntu-x64.tar.gz`, 22932760 octets,
SHA-256 `b80d8061496c511da1e12ea785455796b8946e4874ec3dc99a7dbe715cdf6f6a`.
Le digest a été comparé à celui de l'API GitHub avant extraction. Le runtime
est natif C/C++, sans Python en production. Les poids restent hors Git.

Depuis la racine du dépôt, avec `llama-server` installé :

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

Le minimum de 1024 tokens image suit l'avertissement du moteur concernant la
précision du grounding Qwen-VL ; une image reste coûteuse à encoder sur CPU.
Le serveur doit être explicitement provisionné et de confiance. L'alias
vérifié dans la réponse n'est pas une attestation cryptographique des poids.
Le client ne démarre ni ne télécharge automatiquement le serveur.

## API et limites

```rust,ignore
use hakille_perception::vision::ui_tars_local::UiTarsLocalClient;

let client = UiTarsLocalClient::new("http://127.0.0.1:18081")?;
let proposal = client.ground(&png_or_jpeg_bytes, "Click the Save button").await?;
// proposal.action reste une proposition, jamais une autorisation d'effet.
```

- HTTP limité à une IP littérale de boucle locale et un port explicite ;
  DNS, proxy, redirections, identifiants URL et endpoints distants refusés.
- Image PNG/JPEG réellement décodée : 8 Mio encodés, dimensions maximales
  2048 × 2048, allocation du décodeur limitée à 64 Mio. Ce budget n'est pas
  une limite globale de mémoire du serveur ou de tout le processus client.
- Instruction de 8192 octets au maximum ; réponse HTTP limitée à 64 Kio,
  y compris sans `Content-Length` ; connexion limitée à 3 secondes,
  requête à 600 secondes et génération à 256 tokens. La déconnexion côté
  client ne constitue pas à elle seule une preuve d'arrêt du calcul serveur.
- Une seule action JSON contrainte puis validée localement : `click`,
  `double_click`, `right_click`, `drag`, `type`, `hotkey`, `scroll`, `wait`,
  `finished`. Les coordonnées sont des entiers normalisés **0 à 1000**,
  pas des pixels. Les chaînes sont bornées, les champs inconnus refusés.
- Le modèle peut proposer une action incorrecte. Ni cette API, ni le champ
  `finished` ne prouvent un effet réel. Aucune injection d'entrée, conversion
  en capability, approbation de carte ou exécution automatique n'est raccordée.
- Ce chemin valide un runtime CPU Linux. Il ne démontre pas une installation
  native Windows/macOS/mobile/Web/Redox ou une précision générale du modèle.

Les tests du module couvrent les frontières d'entrée, la réponse, le transport
HTTP local et les erreurs. Les réponses de ces tests sont des fixtures
explicitement locales ; elles ne constituent pas une preuve d'inférence.

Un premier forward multimodal a également été exécuté sur CPU Linux avec les
deux artefacts ci-dessus et llama.cpp b12074 : capture synthétique 560 × 280,
bouton « Save » couvrant les pixels x=350..520 et y=180..250, instruction
`Click the Save button`. Le modèle a généré `{"action":"click","x":750,"y":750}`,
soit le pixel (420, 210), situé dans le bouton. Le serveur a traité 1123 tokens
de prompt et produit 28 tokens en 118,77 secondes, pendant d'autres
compilations. Ce premier test utilisait un schéma limité à `click` ; il prouve
le forward vision + langage, sans mesurer une précision générale ou valider
toutes les variantes d'action.
