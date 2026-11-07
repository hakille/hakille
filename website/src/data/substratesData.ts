/**
 * Hakille — 3 Hardware Tiers & 6 OS Substrates (`manifests/*.toml` & `docs/ANALYSE_MULTIPLATEFORME_2026-11-03.md`)
 */

export interface TierDefinition {
  readonly id: 'sovereign' | 'native' | 'guest';
  readonly badge: string;
  readonly manifestPath: string;
  readonly nameFr: string;
  readonly nameEn: string;
  readonly targetHardwareFr: string;
  readonly targetHardwareEn: string;
  readonly isolationLevelFr: string;
  readonly isolationLevelEn: string;
  readonly ramFootprint: string;
  readonly capabilitiesFr: readonly string[];
  readonly capabilitiesEn: readonly string[];
  readonly honestLimitFr: string;
  readonly honestLimitEn: string;
}

export const HARDWARE_TIERS: readonly TierDefinition[] = [
  {
    id: 'sovereign',
    badge: 'PALIER 1 · SOUVERAIN',
    manifestPath: 'manifests/sovereign.toml',
    nameFr: 'Station Dédiée & Micro-Noyau Redox OS / Linux Durci',
    nameEn: 'Dedicated Station & Redox OS Microkernel / Hardened Linux',
    targetHardwareFr: 'Station personnelle dédiée (16–64 Go RAM, NPU/GPU local, Redox OS ou Linux minimal)',
    targetHardwareEn: 'Dedicated personal station (16–64 GB RAM, local GPU/NPU, Redox OS or minimal Linux)',
    isolationLevelFr: 'Isolation maximale par namespaces de capabilities / schémas Redox en espace utilisateur Rust pur',
    isolationLevelEn: 'Maximum isolation via capability namespaces / pure-Rust Redox user-space schemes',
    ramFootprint: 'Shell < 25 Mo · Stack IA complète 2–6 Go',
    capabilitiesFr: [
      'Démon maître détenant la racine du graphe redb et la clé maîtresse Ed25519',
      'Pipeline vocal complet local (Silero + Whisper/SenseVoice + Mimi + Klatt)',
      'Hôte Wasmtime 48 zéro-import pour tous les agents locaux',
      'Passerelle chiffrée pour les compagnons mobiles du propriétaire',
    ],
    capabilitiesEn: [
      'Primary daemon holding the authoritative redb SPO graph and root Ed25519 seed',
      'Full local voice pipeline (Silero + Whisper/SenseVoice + Mimi + Klatt)',
      'Zero-import Wasmtime 48 sandbox host for all local agents',
      'Authenticated local/mesh gateway serving the owner’s mobile companions',
    ],
    honestLimitFr: 'L’adaptateur hakille-substrate-redox compile et passe ses tests de contrat, mais l’image bootable Redox complète reste un jalon Phase 2–3.',
    honestLimitEn: 'hakille-substrate-redox compiles and passes contract tests, while a full bootable Redox desktop appliance remains a Phase 2–3 milestone.',
  },
  {
    id: 'native',
    badge: 'PALIER 2 · NATIF',
    manifestPath: 'manifests/native.toml',
    nameFr: 'Poste Quotidien Windows, Linux & macOS (Zéro Electron)',
    nameEn: 'Everyday Desktop on Windows, Linux & macOS (Zero Electron)',
    targetHardwareFr: 'Ordinateur portable ou fixe standard (8–32 Go RAM, CPU moderne AVX2 / Apple Silicon)',
    targetHardwareEn: 'Standard laptop or desktop (8–32 GB RAM, modern AVX2 CPU / Apple Silicon)',
    isolationLevelFr: 'Seccomp-BPF + Cgroups v2 (Linux), Job Objects (Windows), Seatbelt SBPL (macOS)',
    isolationLevelEn: 'Seccomp-BPF + Cgroups v2 (Linux), Job Objects (Windows), Seatbelt SBPL (macOS)',
    ramFootprint: 'HUD GPU ~22 Mo · 0 fps au repos · Système 1 ~550 Mo',
    capabilitiesFr: [
      'Shell natif winit + wgpu + AccessKit invocable par Alt+Space sans navigateur embarqué',
      'Inférence CPU locale optimisée (Whisper Tiny ×46 GEMM, Qwen 2.5 GGUF)',
      'Arbre lecteur d’écran natif AccessKit (NVDA, Narrateur, Orca, VoiceOver)',
      'Synchronisation CRDT v2 et Handoff v2 signé entre appareils',
    ],
    capabilitiesEn: [
      'Native winit + wgpu + AccessKit GPU shell invoked via Alt+Space with zero WebView',
      'CPU-optimized local inference (×46 GEMM Whisper Tiny, Qwen 2.5 GGUF)',
      'Native AccessKit accessibility tree (NVDA, Narrator, Orca, VoiceOver)',
      'Signed CRDT v2 and Handoff v2 peer sync across personal machines',
    ],
    honestLimitFr: 'Chiffrement complet au repos du fichier redb et attestation matérielle TPM/Secure Enclave prévus pour v0.2 (le trousseau OS protège déjà les graines HAKKEY02).',
    honestLimitEn: 'Full-file redb encryption at rest and hardware TPM/Secure Enclave attestation target v0.2 (HAKKEY02 seeds already use OS keychains).',
  },
  {
    id: 'guest',
    badge: 'PALIER 3 · INVITÉ',
    manifestPath: 'manifests/guest.toml',
    nameFr: 'Compagnon Mobile iOS (SwiftUI) & Android (Compose) + Web WASM',
    nameEn: 'Mobile Companion on iOS (SwiftUI) & Android (Compose) + Web WASM',
    targetHardwareFr: 'iPhone / Android lié par UniFFI 0.32.2 (4 661 LoC natives) ou navigateur WASM restreint',
    targetHardwareEn: 'iPhone / Android linked via UniFFI 0.32.2 (4,661 native LoC) or bounded WASM browser',
    isolationLevelFr: 'Sandbox iOS/Android natif + Trousseau Keychain/Keystore + Protocole Checkpoint v2',
    isolationLevelEn: 'Native iOS/Android OS sandbox + Keychain/Keystore + Durable Checkpoint v2 protocol',
    ramFootprint: 'Pont Rust FFI < 18 Mo · Zéro démon énergivore permanent',
    capabilitiesFr: [
      'Navigation sobre à 4 onglets : `Voix · Mémoire · Activité · Hub`',
      'Dictée vocale forcée 100 % on-device (requiresOnDeviceRecognition = true sur iOS, EXTRA_PREFER_OFFLINE sur Android)',
      'Approbation tactile 600 ms à retour haptique de 10 ms',
      'Checkpoint durable v2 : en cas de coupure réseau pendant une mutation, statut unknown honnête sans jamais rejouer l’action à l’aveugle',
    ],
    capabilitiesEn: [
      'Calm 4-tab mobile shell: `Voice · Memory · Activity · Hub`',
      'Strictly on-device speech recognition (requiresOnDeviceRecognition = true on iOS, EXTRA_PREFER_OFFLINE on Android)',
      '600 ms tactile Braise confirmation with 10 ms haptic tick',
      'Durable Checkpoint v2: if connectivity drops mid-mutation, marks unknown honestly and never replays blindly',
    ],
    honestLimitFr: 'Sur iOS et Android, Hakille respecte honnêtement les limites OS d’arrière-plan : il agit comme compagnon souverain et non comme remplacement du noyau mobile.',
    honestLimitEn: 'On iOS and Android, Hakille respects host OS background limits honestly — acting as a sovereign companion rather than claiming full OS replacement.',
  },
] as const;
