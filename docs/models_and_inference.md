# Sélection des Modèles, Runtimes d'Inférence & Provisioning Autonome

> **Document historique (phase « inspection-only » révolue).** L'état courant est décrit dans
> [MODELS.md](MODELS.md) : forwards `ort`/Candle réels (VAD, ASR, CAM++, MiniLM, Qwen 2.5),
> RVQ Mimi sur vrais codebooks et TTS formantique. Les tableaux ci-dessous restent utiles pour
> les artefacts, checksums et le provisioning, mais les mentions « inspection-only / SIMULÉ »
> ne reflètent plus le runtime actuel.

Ce document consigne la matrice des modèles fondations ouverts retenus pour Hakille, leurs runtimes d'exécution natifs en Rust, les stratégies d'allocation mémoire, ainsi que le système de téléchargement et d'approvisionnement autonome à déploiement zéro-touch (*Zero-Touch Model Provisioning*).

---

## 1. Matrice Globale des Modèles Téléchargés et Inspectés (7/7 inspectés (présence+SHA, pas d'exécution))

| Rôle Fonctionnel | Modèle Retenu | Format & Taille | Empreinte RAM / VRAM | Runtime Rust (inspection-only) | Tier Requis | Checksum SHA-256 Validé |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Détection d'Activité Vocale (VAD)** | **Silero VAD v5** | ONNX (2.3 Mo) | RAM: 15 Mo / VRAM: 0 Mo | ONNX Runtime (inspection-only) | Guest / Native | `1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3` |
| **Signature Biométrique Locuteur** | **CAM++ 16k Advanced** | ONNX (28.2 Mo) | RAM: 30 Mo / VRAM: 0 Mo | ONNX Runtime (inspection-only) | Guest / Native | `aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2` |
| **Embeddings Graphe de Connaissance** | **All-MiniLM-L6-v2** | ONNX (90.4 Mo) | RAM: 90 Mo / VRAM: 0 Mo | ONNX Runtime (inspection-only) | Native | `6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452` |
| **Écoute Continue & ASR Instantané** | **SenseVoice-Small INT8** | ONNX (239 Mo + tokens) | RAM: 230 Mo / VRAM: 0 Mo | ONNX Runtime (inspection-only) | Native | `c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51` |
| **Raisonnement Rapide System 1** | **Qwen2.5 0.5B Instruct** | GGUF Q4_K_M (491 Mo) | RAM: 550 Mo / VRAM: 500 Mo | GGUF Native (inspection-only) | Native | `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` |
| **Contrôle Graphique (Computer-Use)** | **UI-TARS 2B SFT** | GGUF Q4_K_M (1.11 Go) | RAM: 1.8 Go / VRAM: 1.6 Go | GGUF Native (inspection-only) | Native | `f316ecef2e2e8fa9a29e81ba2c9efa8ea6f408241e175331332960c7160e9c53` |
| **Codec Neural Audio S2S** | **Kyutai Mimi** | Safetensors (384.6 Mo) | RAM: 450 Mo / VRAM: 400 Mo | Candle (inspection-only, SIMULÉ) | Sovereign | `bac7e85083dcded655d24eaadde7e6eea34c0da1b35fa2d284e641bd2b942a5e` |

---

## 2. Système d'Approvisionnement Autonome (Zero-Touch Provisioning)

Pour garantir qu'un utilisateur n'ait **jamais** besoin de configurer Python, Hugging Face CLI ou d'exécuter des scripts manuels :

1. **Client Réseau Natif Pur Rust** :
   - Basé sur `reqwest` avec backend cryptographique `rustls` (zéro dépendance OpenSSL ou C système).
   - Suivi automatique des redirections CDN sécurisées (Cloudflare, Azure Blob, AWS S3).
   - Support des flux HTTP avec **reprise automatique de téléchargement sur coupure réseau (*HTTP Range headers*)** et boucle de résilience auto-cicatrisante (jusqu'à 10 tentatives avec backoff exponentiel).

2. **Garanties Cryptographiques & Promotion Atomique** :
   - Flux d'écriture dirigé vers un fichier temporaire `.part`.
   - Calcul continu de l'empreinte SHA-256 en parallèle du flux d'octets.
   - Vérification stricte du condensat SHA-256 avant finalisation.
   - En cas d'intégrité validée, renommage atomique (`rename`) du fichier `.part` vers son chemin final, protégeant l'OS de toute corruption ou coupure d'alimentation intempestive.

3. **Orchestrateur Local de Modèles (`ModelProvisioner`)** :
   - Découverte automatique des fichiers présents dans `./models` ou `$HAKILLE_MODELS_DIR`.
   - Inspection du catalogue et calcul des métriques de disque et mémoire.
   - Téléchargement autonome à la demande, par niveau de déploiement (`Guest`, `Native`, `Sovereign`) ou pour l'ensemble du catalogue.

---

## 3. Commandes CLI et REPL

### A. Ligne de Commande Système (`hakille-daemon`)

```bash
# Lister l'état du catalogue et les modèles présents localement
hakille-daemon models list

# Télécharger un modèle spécifique de manière autonome
hakille-daemon models download --model silero-vad-v5

# Télécharger l'intégralité des modèles d'un tier (ex: guest ou native)
hakille-daemon models download --tier native

# Vérifier l'intégrité cryptographique SHA-256 de tous les modèles installés
hakille-daemon models verify

# Lancer le démon avec téléchargement automatique des modèles essentiels manquants
hakille-daemon --auto-download
```

### B. Shell Interactif du Démon Hakille (`hakille>`)

```text
hakille> :models
Foundation Model Catalog (Directory: ./models):
  • silero-vad-v5              Guest      [INSTALLED - 2.2 MB]   Silero Voice Activity Detector v5
  • campplus-speaker-id        Guest      [INSTALLED - 27.0 MB]  CAM++ Neural Speaker Verifier
  • all-minilm-l6-v2           Native     [INSTALLED - 86.2 MB]  All-MiniLM-L6-v2 Vector Embeddings
  • sensevoice-small-int8      Native     [INSTALLED - 228.5 MB] SenseVoice-Small Multi-Task ASR
  • qwen2.5-0.5b-instruct-q4km Native     [INSTALLED - 468.6 MB] Qwen2.5 0.5B Instruct GGUF (System 1)
  • ui-tars-2b-sft-q4km        Native     [INSTALLED - 1065.6 MB] UI-TARS 2B SFT GGUF (GUI Agent)
  • moshi-mimi-s2s             Sovereign  [INSTALLED - 366.8 MB] Kyutai Mimi Streaming Audio Codec (Candle Native)

hakille> :verify-models
Verifying models integrity...
  • silero-vad-v5              [OK - Valid]
  • campplus-speaker-id        [OK - Valid]
  • all-minilm-l6-v2           [OK - Valid]
  • sensevoice-small-int8      [OK - Valid]
  • qwen2.5-0.5b-instruct-q4km [OK - Valid]
  • ui-tars-2b-sft-q4km        [OK - Valid]
  • moshi-mimi-s2s             [OK - Valid]
```

### C. Outil Workspace Xtask (`cargo xtask`)

```bash
# Préparer et pré-télécharger les modèles pour la distribution ou le packaging air-gapped
cargo xtask download-models guest
cargo xtask list-models
cargo xtask verify-models
```

---

## 4. Gestionnaire de Mémoire et Pagination VRAM LRU

Sur les postes de travail grand public (ex: 8 à 16 Go de VRAM) :
- Les micro-modèles d'écoute passive (**SenseVoice-Small**, **CAM++**, **Silero VAD**) mobilisent ensemble moins de **300 Mo de mémoire** et restent résidents en permanence sur le CPU (`LoadingStrategy::AlwaysResident`) sans réveiller le GPU dédié.
- Les modèles plus lourds (**UI-TARS**, **Qwen**, **VibeVoice**) utilisent la politique `LoadingStrategy::OnDemand`.
- Dès qu'une tâche est déclenchée, le `ModelRegistry` pagine le modèle en mémoire et applique un déchargement LRU (*Least Recently Used*) en respectant un budget mémoire strict sans dépassement.
