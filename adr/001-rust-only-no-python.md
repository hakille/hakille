# ADR 001: Éradication absolue de Python en production au profit de Rust pur

## Statut
Accepté

## Contexte
Annora est conçu comme un système d'exploitation cognitif souverain et non une simple couche applicative ou un chatbot.
Dans les architectures d'IA conventionnelles, les runtimes Python (CPython, PyTorch, vLLM, transformers) sont omniprésents. Cependant, Python introduit des limites rédhibitoires pour un OS cognitif :
1. **Non-déterminisme mémoire et ramasse-miettes (GC)** : interruptions imprévisibles de thread et consommation de mémoire vive incontrôlée.
2. **Vulnérabilités de supply chain et surface d'attaque** : des milliers de dépendances transitoires pip, wheel, et wrappers C/C++ opaques.
3. **Impossibilité d'exécution directe en micro-noyau (Redox OS) ou bac à sable WASM strict**.
4. **Pente glissante d'administration** : gestion des environnements virtuels, glibc conflicts, et conflits CUDA.

## Décision
Toute l'infrastructure d'Annora — du micro-noyau aux modèles d'inférence, en passant par le graphe de mémoire et les agents — est développée **exclusivement en Rust**.
- L'inférence neuronale s'effectue via **Candle** (Hugging Face) en Rust natif et les liaisons ONNX Runtime (`ort` et `sherpa-onnx`).
- Les modèles GGUF et BitNet sont chargés via des crates Rust sans dépendance Python.
- Le runtime d'agents s'appuie sur **Wasmtime / WASI** avec isolation par jetons Biscuit cryptographiques.

## Conséquences
- **Positives** : Sûreté mémoire statique garantie par le compilateur Rust, empreinte binaire minimale, temps de démarrage instantané (< 50ms), compatibilité native de bout en bout avec Redox OS, Linux, macOS et Windows.
- **Négatives / Défis** : Nécessite l'implémentation et le portage rigoureux des architectures de tenseurs et tokenizers en Rust pur plutôt que l'utilisation de scripts Python prêts à l'emploi.
