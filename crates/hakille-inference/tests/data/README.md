# Référence indépendante MiniLM

`minilm_reference.json` contient deux séquences de tokens et les 384 composantes
de leurs embeddings. Référence produite le 3 novembre 2026 avec Python
ONNX Runtime 1.24.1 (CPU), tokenizers 0.22.2 et NumPy, indépendamment du forward
Rust testé. Ces outils servent uniquement à établir la référence ; ils ne sont
pas requis pour compiler, exécuter ou tester Hakille.

Procédure : charger le `tokenizer.json` officiel, activer troncature et padding
à 256 tokens, encoder avec tokens spéciaux, fournir les trois entrées int64
`input_ids`, `attention_mask`, `token_type_ids` au modèle ONNX, puis calculer en
float64 la moyenne de `last_hidden_state` sur les tokens où le masque vaut 1,
et normaliser par la norme L2. Les poids et le tokenizer sont identifiés par
SHA-256 dans le fichier. Les deux phrases couvrent anglais et accents français,
ponctuation Unicode, caractères japonais et emoji.

Le test Rust exige les mêmes SHA-256, tokens identiques et un écart absolu
maximum de 3e-5 sur les vecteurs (tolérance entre versions CPU d'ONNX Runtime).
Un changement de checkpoint nécessite une nouvelle référence indépendante,
pas une simple modification des empreintes pour rendre le test vert.

La normalisation officielle corrige celle du précédent tokenizer manuel.
Les embeddings persistés provenant de cet ancien tokenizer doivent être
recalculés avant de mélanger anciens et nouveaux vecteurs. Aucun identifiant
de version du modèle n'est encore stocké avec l'index de mémoire.
