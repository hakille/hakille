# ADR 004: Dialogue Vocal Full-Duplex via Kyutai Moshi en Candle Rust

## Statut
Accepté

## Contexte
Dans les chaînes conversationnelles en cascade classiques (VAD → ASR → LLM → TTS), l'accumulation des délais inhérents à chaque étape repousse couramment la latence de premier son au-delà de 600 à 900 ms.
De plus, la gestion des interruptions naturelles de l'utilisateur (*barge-in*) requiert des logiques de compensation complexes et dégrade la spontanéité d'un dialogue parlé naturel.

## Décision
Adopter **Kyutai Moshi** et le codec neuronal **Mimi** pour le dialogue parlé en temps réel dans le Tier Souverain :
- Moshi opère en **full-duplex** : il ingère et produit deux flux audio parallèles simultanément sans distinction rigide de tours de parole.
- Le codec Mimi assure une compression 12.5 Hz avec une latence de trame intrinsèque causale de 80 ms.
- Intégration en **Rust pur** via le framework tensoriel **Candle** de Hugging Face en réutilisant directement le backend Rust officiel maintenu par Kyutai Labs.
- Le dialogue Système 1 (Moshi) est asynchronement nourri par les faits vérifiés du graphe et du Système 2 via l'architecture MoshiRAG.

## Conséquences
- **Positives** : Latence théorique de 160 ms (200 ms mesuré en conditions réelles), gestion native des interruptions et interjections, zéro dépendance Python.
- **Négatives / Défis** : Nécessite environ 8 Go de VRAM en quantification 8 bits, limitant l'exécution résidente continue au Tier Souverain et aux machines équipées de GPU dédié ou de mémoire unifiée.
