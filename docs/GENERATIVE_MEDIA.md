# Moteur Génératif Multimodal Souverain (`hakille-media` — ADR-024)

Hakille intègre un moteur génératif multimodal **100 % Rust pur (zéro Python)** couvrant l'**image**, l'**audio** (parole, musique, ambiances, Foley, mixage), la **vidéo** et la **composition multimodale en DAG**, gouverné par la Constitution Hakille, Biscuit v5 et la provenance cryptographique Ed25519.

---

## 1. Inventaire honnête des moteurs

Chaque moteur déclare sa nature réelle (`EngineKind`) et sa disponibilité instantanée (`EngineAvailability`). **Un moteur neural sans poids locaux ne bascule jamais silencieusement vers le DSP lorsqu'il est explicitement demandé** : il échoue avec `MediaError::ModelMissing` en listant les fichiers attendus.

| ID moteur | Modalité | Nature (`EngineKind`) | Statut sans poids téléchargés | Sortie de données |
| :--- | :--- | :--- | :--- | :--- |
| `sd-turbo-candle` | Image | `NEURAL LOCAL (Candle)` | `MODÈLE ABSENT` | Local |
| `sd-1.5-candle` | Image | `NEURAL LOCAL (Candle)` | `MODÈLE ABSENT` | Local |
| `flux-schnell-candle` | Image | `NEURAL LOCAL (Candle)` | `MODÈLE ABSENT` | Local |
| `onnx-diffusion` | Image | `NEURAL LOCAL (ONNX)` | `MODÈLE ABSENT` | Local |
| `procedural-dsp` | Image | `PROCEDURAL DSP (Rust pur)` | `PRÊT` | Local |
| `parler-tts-candle` | Audio | `NEURAL LOCAL (Candle)` | `MODÈLE ABSENT` | Local |
| `onnx-tts` | Audio | `NEURAL LOCAL (ONNX)` | `MODÈLE ABSENT` | Local |
| `formant-dsp` | Audio | `FORMANT DSP (Klatt 24 kHz)` | `PRÊT` | Local |
| `procedural-audio-dsp` | Audio | `PROCEDURAL DSP (Rust pur)` | `PRÊT` | Local |
| `onnx-video` | Vidéo | `NEURAL LOCAL (ONNX)` | `MODÈLE ABSENT` | Local |
| `candle-keyframe-video` | Vidéo | `NEURAL LOCAL (Candle)` | `MODÈLE ABSENT` | Local |
| `procedural-video-dsp` | Vidéo | `PROCEDURAL DSP (Rust pur)` | `PRÊT` | Local |
| `loopback-media` | Image/Audio/Vidéo | `NEURAL LOOPBACK (127.0.0.1)` | `NON CONFIGURÉ` (`HAKILLE_MEDIA_LOOPBACK_URL`) | Loopback `127.0.0.1` |
| `cloud-media` | Image/Audio | `NEURAL CLOUD (HTTPS)` | `NON CONFIGURÉ` (`HAKILLE_MEDIA_CLOUD_*`) | Cloud HTTPS opt-in |

En mode `--engine auto`, le routeur choisit le meilleur moteur **local réellement prêt** et inscrit systématiquement son `engine_id` et son badge exact dans la réponse, le journal d'audit et le manifeste Ed25519.

---

## 2. Utilisation CLI, REPL et Studio Média (`F5`)

### CLI autonome (`hakille-daemon media ...`)

```bash
# Inventaire honnête des moteurs et détection du muxer ffmpeg
hakille-daemon media engines [--json]

# Génération d'image (PNG + chunk iTXt + sidecar .provenance.json)
hakille-daemon media image "nébuleuse souveraine" --size 512x512 --steps 12 --seed 42

# Génération audio (parole Klatt + visèmes, musique, ambiance ou foley)
hakille-daemon media audio "Bonjour Hakille" --mode speech
hakille-daemon media audio "synthwave calme" --mode music --duration 6 --bpm 100
hakille-daemon media audio "pluie nocturne" --mode soundscape --duration 5

# Génération vidéo (APNG / Y4M / PNG-seq pur Rust, ou MP4 / WebM si ffmpeg est présent)
hakille-daemon media video "aurore boréale" --size 320x180 --fps 12 --frames 24 --motion orbit --format mp4

# Pipeline multimodal DAG (affiche + narration + ambiance + mix sidechain + vidéo + mux)
hakille-daemon media compose "cité solaire souveraine" --size 256x144 --fps 10 --duration 3.0

# Galerie locale et vérification cryptographique Ed25519
hakille-daemon media list --limit 20
hakille-daemon media verify <chemin_vers_artefact>
```

### REPL interactif (`hakille>`)

- `:gen-image <prompt> [--size WxH] [--engine ID] [--seed N]`
- `:gen-audio <prompt> [--mode speech|music|soundscape|foley] [--duration S]`
- `:gen-video <prompt> [--size WxH] [--fps N] [--frames N] [--format apng|y4m|mp4|webm] [--motion M]`
- `:gen-story <prompt> [--size WxH] [--fps N] [--duration S]`
- `:media-engines` — affiche l'inventaire honnête des moteurs
- `:media-list` — liste la galerie locale signée
- `:media-verify <chemin>` — vérifie la signature Ed25519, le SHA-256 et le chunk PNG `iTXt`
- `:media-cancel <job_id>` — demande l'annulation coopérative d'un job actif

### HUD desktop natif (`F5` — Studio Média)

- Appuyer sur **`F5`** ouvre le panneau **Studio Média Souverain** (720×360) et interroge le démon par IPC (`MediaEnginesRequest` + `MediaGalleryRequest`).
- Saisir `:gen-image ...`, `:gen-audio ...`, `:gen-video ...` ou `:gen-story ...` dans le HUD lance la génération côté démon avec :
  - barre de progression temps réel (`KernelToUi::MediaJobProgress` sur trame `id = 0`) ;
  - blitting natif de la vignette RGBA8 (`DrawCmd::ImageRgba`, mise à l'échelle bilinéaire + masque arrondi) ;
  - badge `Ed25519 ✓ · IA synthétique` et lien vers l'artefact réel sur disque.

---

## 3. Provenance cryptographique Ed25519 & Stockage

Tous les artefacts sont stockés en content-addressed sous `<data_dir>/media/` :

- `blobs/<sha256[0..2]>/<sha256[0..24]>-<uuid>.<ext>` : fichier binaire final ;
- `<fichier>.provenance.json` : manifeste JSON canonique signé par la clé Ed25519 du démon (`content_sha256`, `prompt_sha256`, `engine_id`, `engine_kind`, `data_exit`, `seed`, `parent_artifacts`, `synthetic_ai_disclosure: true`) ;
- **PNG / APNG** : un chunk `iTXt` (`hakille.provenance`) est inséré immédiatement après `IHDR` avant calcul du SHA-256 final ;
- **Parole** : un sidecar `<fichier>.visemes.json` expose la timeline des visèmes pour synchronisation labiale ;
- `index.json` : index atomique avec quota LRU borné (`max_store_bytes`, 2 Gio par défaut).

Toute modification d'un seul octet sur l'artefact ou sur son sidecar provoque `MediaError::ProvenanceInvalid` lors de `verify_artifact_provenance`.

---

## 4. Gouvernance, Constitution & Sécurité

Le garde `MediaConstitutionGuard` vérifie chaque requête avant toute allocation de tenseur ou requête réseau :

- **Règle 1** : rejet fail-closed de toute source `TrustLevel::Untrusted` (protection contre les injections indirectes) ;
- **Règle 2** : filtrage du prompt (et du prompt négatif) via `Constitution::default_v1()` ;
- **Règle 4** : vérification Biscuit v5 épinglée (`CapabilityScope::MediaGeneration` + `CapabilityAction::Execute` + liaison de ressource ; toute sortie cloud exige en plus `CapabilityScope::Network` et une `ActionCard` irréversible `DataExit::Cloud`) ;
- **Règle 7 (profil mineur)** : blocage strict du cloud et plafonds réduits (`max_image_dim = 1024`, `max_video_dim = 720`, `max_video_frames = 240`, `max_audio_duration_secs = 60s`) ;
- **Règle 8** : interdiction d'envoyer un fichier local de référence (`ImageToImage`, `Inpaint`, `ImageToVideo`) vers un fournisseur cloud ;
- **Anti-SSRF** : `loopback-media` refuse tout hôte autre qu'une IP littérale `127.0.0.1` / `[::1]` (sans proxy ni redirections) ; `cloud-media` refuse tout schéma non-HTTPS et toute IP privée, loopback ou link-local (`169.254.169.254`, `10.0.0.0/8`, `192.168.0.0/16`, `172.16.0.0/12`, etc.).

---

## 5. Limites assumées et documentées

1. **Poids neuronaux non versionnés** : les poids Stable Diffusion, FLUX.1 Schnell, Parler-TTS et ONNX ne sont pas inclus dans le dépôt Git (`models/` est gitignoré). Tant qu'ils ne sont pas provisionnés dans `<models_dir>/media/`, seuls les moteurs `procedural-dsp`, `formant-dsp`, `procedural-audio-dsp` et `procedural-video-dsp` sont immédiatement `PRÊT`.
2. **Latence CPU** : sur machine sans GPU (CUDA/Metal), l'inférence neurale diffusion/audio sur CPU reste significativement plus lente que les moteurs DSP ; chaque job impose une échéance dure coopérative (`deadline_ms`) et supporte l'annulation.
3. **Muxer `ffmpeg` optionnel** : le conteneur MP4 (H.264/AAC) et WebM (VP9/Opus) repose sur un binaire `ffmpeg` système détecté au runtime. Sans `ffmpeg`, Hakille génère nativement en Rust pur des vidéos APNG, Y4M et séquences PNG.
4. **Cloud opt-in** : aucune clé API ni appel distant n'est actif par défaut ; toute activation cloud requiert les variables d'environnement explicites, les capabilities Biscuit `MediaGeneration` + `Network`, et la validation explicite d'une carte d'action irréversible.
