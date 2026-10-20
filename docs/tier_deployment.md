# Déploiement par Palier Matériel (Tiers)

Ce document décrit l'organisation du déploiement d'Hakille selon les contraintes physiques et logicielles des plateformes d'accueil.

---

## 1. La Matrice des Trois Paliers Matériels

Hakille refuse l'illusion d'un comportement strictement identique sur toutes les machines. Les systèmes d'exploitation commerciaux (notamment mobiles) imposent des restrictions de sandbox infranchissables. Hakille assume trois paliers honnêtes :

```
                                PALIER SOUVERAIN
                       Station dédiée Redox / Linux
                        Contrôle total & Hub Local
                                    │
                                    ▼
                               PALIER NATIF
                        Laptop Windows, macOS, Linux
                      Contrôle quasi-total (Services)
                                    │
                                    ▼
                               PALIER INVITÉ
                        Mobile (Android/iOS) & Web
                       Ombre déportée sur le Hub via CRDT
```

---

## 2. Spécifications Détaillées des Paliers

### Tier Souverain (Redox OS / Linux Souverain, Hub Domestique)
- **Environnement** : Alimentation sur secteur continue, micro-noyau Redox ou Linux allégé sans couches d'espionnage tierces.
- **Accélération matérielle** : GPU dédié (16 Go VRAM+) ou mémoire unifiée (32 Go+).
- **Perception** : SenseVoice-Small résident pour l'écoute instantanée, VibeVoice-ASR pour l'archivage de réunions longues, Nemotron-3 pour la diarisation continue.
- **Dialogue** : Kyutai Moshi en Candle Rust (full-duplex sub-200ms) résident en VRAM.
- **Action** : UI-TARS-7B pour le pilotage complet de l'affichage Wayland/Redox, Qwen3-8B pour le raisonnement local.
- **Manifeste** : `manifests/sovereign.toml`.

### Tier Natif (Laptop Linux standard, macOS, Windows)
- **Environnement** : PC portable personnel avec contraintes de batterie et de dissipation thermique.
- **Accélération matérielle** : GPU milieu de gamme (6 à 8 Go de VRAM) ou puces Apple Silicon (M-series).
- **Perception** : SenseVoice-Small en INT8 (veille continue sur cœurs CPU basse consommation, < 1% CPU), CAM++ pour l'authentification mono-utilisateur.
- **Dialogue** : CosyVoice2 streaming ou NeuTTS Air (0.5B GGUF) activé au réveil.
- **Action** : UI-TARS-7B paginé en mémoire à la demande uniquement lors d'une automatisation.
- **Manifeste** : `manifests/native.toml`.

### Tier Invité (Mobile Android / iOS, Navigateur Web WASM)
- **Environnement** : Systèmes fermés tuant agressivement les processus d'arrière-plan.
- **Perception locale** : openWakeWord et modèle Moonshine ou SenseVoice Mobile compilé pour NPU/CoreML.
- **L'Ombre Cloud / Hub** : Pour préserver le critère non négociable de **continuité d'état** malgré le sommeil forcé des applications mobiles :
  1. Une instance légère du noyau cognitif (System 1) tourne en permanence sur le hub domestique ou le cluster TiKV — une "ombre" toujours active.
  2. Quand le téléphone se réveille, il synchronise son contexte via le protocole CRDT avec l'ombre.
  3. L'appareil mobile n'est qu'un point d'accès ; l'intelligence et la continuité vivent dans le cluster personnel de l'utilisateur.
- **Manifeste** : `manifests/guest.toml`.

---

## 3. Configuration et Sélection Automatique

Au démarrage, le démon `hakille-daemon` interroge le `Substrate` de la machine hôte :
```rust
let manifest = substrate.manifest();
println!("Host OS: {} {}", manifest.os_name, manifest.os_version);
println!("Detected Tier: {:?}", manifest.tier);
```
Le Cognitive Fabric charge automatiquement le fichier de manifeste correspondant dans `manifests/` pour adapter la chaîne de modèles aux capacités exactes de la machine.
