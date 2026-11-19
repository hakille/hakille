# ADR 024 — Moteur Génératif Multimodal Souverain (`hakille-media`)

Statut : implémenté. Complète ADR 001 (zéro Python), ADR 002 (Biscuit v5), ADR 018 (classification de risque), ADR 021 (audit Ed25519) et ADR 023 (harness borné).

## Contexte

Hakille disposait de pipelines de perception multimodale (audio, voix, vision) et d'inférence texte locale (Qwen Candle), mais ne disposait d'aucune capacité intégrée de génération d'images, d'audio créatif/vocal étendu, de vidéo ni de composition multimodale coordonnée. Introduire ces capacités sans trahir les axiomes d'Hakille impose quatre invariants non négociables :

1. **Zéro Python en production** : toute l'orchestration, les moteurs neuronaux locaux (Candle / ONNX Runtime `ort`), les encodeurs (PNG, JPEG, WAV, APNG, Y4M) et les synthétiseurs DSP sont écrits en Rust natif (édition 2024).
2. **Contrat d'honnêteté absolu** : aucun moteur neural dont les poids locaux sont absents ne peut simuler un succès ni rétrograder silencieusement vers un synthétiseur DSP. Chaque moteur déclare son `EngineKind` (`NeuralLocalCandle`, `NeuralLocalOnnx`, `NeuralLoopbackServer`, `NeuralCloudApi`, `FormantDsp`, `ProceduralDsp`) et son `EngineAvailability` réel (`Ready`, `ModelMissing`, `Unconfigured`, `Unavailable`).
3. **Gouvernance constitutionnelle & Biscuit v5** : toute génération passe par un garde fail-closed (`MediaConstitutionGuard`), un jeton Biscuit v5 (`CapabilityScope::MediaGeneration`, plus `CapabilityScope::Network` et une `ActionCard` irréversible pour toute sortie cloud opt-in), un journal d'audit signé Ed25519 et une traçabilité mémoire SPO.
4. **Provenance cryptographique vérifiable** : chaque artefact produit est stocké en content-addressed (`<data_dir>/media/blobs/`), signé Ed25519 via un manifeste sidecar `.provenance.json` (et un chunk PNG `iTXt` `hakille.provenance` pour les PNG/APNG) portant la divulgation immuable `synthetic_ai_disclosure: true` et la lignée SHA-256 `parent_artifacts`.

## Décision

Introduire le crate dédié `crates/hakille-media` et le relier de bout en bout aux six couches d'Hakille :

- **Fondations & Gouvernance** :
  - Ajout de `CapabilityScope::MediaGeneration` dans `hakille-core` (poids d'authentification `0.50` dans `hakille-auth`), de `PipelineRole::{ImageGeneration, VideoGeneration, AudioGeneration}` dans `hakille-inference`, et de `CapabilityName::{GenerateImage, GenerateVideo, GenerateAudio}` dans `hakille-kernel`.
  - Garde `MediaConstitutionGuard` appliquant les Règles 1 (refus `Untrusted`), 2 (Constitution v1), 4 (Biscuit v5 épinglé + ressource), 7 (profil mineur : cloud bloqué, résolutions/durées réduites) et 8 (interdiction d'exporter un fichier de référence local vers le cloud).
  - Protection anti-SSRF stricte sur les adaptateurs optionnels : `loopback` exige une IP littérale `127.0.0.1` / `[::1]` sans proxy ni redirection ; `cloud` exige HTTPS vérifié (TLS strict) et bloque toute IP privée/loopback/link-local.
- **Moteurs multimodaux natifs** :
  - **Image** : Stable Diffusion 1.5 & SD-Turbo (Candle), FLUX.1 Schnell (Candle), ONNX Diffusion (`ort`), Procédural DSP Rust pur (fBm + harmoniques spectrales, PNG/JPEG + vignette RGBA8).
  - **Audio** : Parler-TTS + DAC (Candle), ONNX TTS (`ort`), Formant Klatt 24 kHz (`hakille-voice::FormantTts` + sidecar `visemes.json`), Synthétiseur polyphonique DSP (musique / ambiances / Foley) et mixeur multipiste avec sidechain ducking automatique.
  - **Vidéo** : ONNX spatio-temporel 5D (`ort`), Candle SLERP latente par images-clés, Procédural cinématique & Audio-Réactif RMS (mouvements de caméra `pan`, `zoom`, `orbit`, `kenburns`), encodeurs purs Rust (APNG, Y4M, séquence PNG) et muxer explicite `FfmpegMuxer` (MP4 H.264 / WebM VP9 lorsqu'un binaire `ffmpeg` système est détecté).
- **Composition DAG & Orchestration** :
  - `MediaPipelineDag` valide l'acyclicité par tri topologique de Kahn, exécute chaque nœud en checkpointing ses artefacts dans `MediaStore` et propage automatiquement les SHA-256 parents (`StoryboardPreset::from_prompt` : affiche → narration + ambiance → mix sidechain → animation vidéo → mux final).
  - `MediaJobManager` applique des échéances dures coopératives (`GenerationControl`), l'annulation temps réel (`cancel_job`) et un bus de progression broadcast.
- **Surfaces utilisateur & Protocole IPC v1 additif** :
  - Extension additive de `hakille-ui-protocol` (`PROTOCOL_VERSION` conservé à `1`, `schema_snapshot.json` intact) : `MediaEnginesRequest`, `MediaGenerateRequest`, `MediaJobCancel`, `MediaGalleryRequest` ↔ `MediaEnginesReply`, `MediaJobProgress`, `MediaJobCompleted`, `MediaJobFailed`, `MediaGalleryReply`.
  - CLI autonome (`hakille-daemon media engines|image|audio|video|compose|list|verify`), REPL (`:gen-image`, `:gen-audio`, `:gen-video`, `:gen-story`, `:media-engines`, `:media-list`, `:media-verify`, `:media-cancel`) et panneau natif HUD desktop **`F5` Studio Média** avec blitting natif de vignettes RGBA8 (`DrawCmd::ImageRgba`).

## Conséquences

- Hakille peut générer localement et de manière vérifiable des images, voix/musiques/ambiances, vidéos et capsules multimodales complètes sans dépendance Python, tout en restant extensible vers les poids neuronaux Candle/ONNX locaux ou des serveurs GPU loopback (`127.0.0.1`).
- Les poids neuronaux volumineux (SD-Turbo, SD 1.5, FLUX.1 Schnell, Parler-TTS, ONNX) ne sont pas versionnés dans Git ; en leur absence, les moteurs neuronaux rapportent honnêtement `ModelMissing` et la sélection `auto` utilise exclusivement les moteurs locaux réellement prêts en affichant leur badge exact (`PROCEDURAL DSP`, `FORMANT DSP`).
- Le muxing MP4/WebM requiert un binaire `ffmpeg` présent sur le système ; en son absence, les conteneurs purs Rust (APNG, Y4M, séquence PNG) restent opérationnels sans aucune erreur silencieuse. Les détails opérationnels et limites sont documentés dans [GENERATIVE_MEDIA.md](../docs/GENERATIVE_MEDIA.md).
