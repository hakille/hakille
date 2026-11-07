import { useMemo, useState } from 'react';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { KlattVoiceStudio } from '@/components/simulators/KlattVoiceStudio';
import { MobileCompanionSimulator } from '@/components/simulators/MobileCompanionSimulator';

interface OsTarget {
  readonly id: 'linux' | 'macos' | 'windows' | 'android' | 'ios' | 'redox';
  readonly label: string;
  readonly tierFr: string;
  readonly tierEn: string;
  readonly substrateCrate: string;
  readonly summaryFr: string;
  readonly summaryEn: string;
  readonly commands: readonly string[];
}

const OS_TARGETS: readonly OsTarget[] = [
  {
    id: 'linux',
    label: 'Linux (x86_64 / ARM64)',
    tierFr: 'Palier 1–2 · Souverain & Natif',
    tierEn: 'Tier 1–2 · Sovereign & Native',
    substrateCrate: 'hakille-substrate-linux',
    summaryFr:
      'Isolation Landlock ABI v3 + seccomp-bpf, trousseau Secret Service, shell GPU natif Wayland/X11 (winit + wgpu + AccessKit AT-SPI) et démon IPC complet.',
    summaryEn:
      'Landlock ABI v3 + seccomp-bpf sandboxing, Secret Service keychain, native Wayland/X11 GPU shell (winit + wgpu + AccessKit AT-SPI), and full local daemon.',
    commands: [
      'git clone https://github.com/hakille/hakille.git && cd hakille',
      'cargo run -p xtask -- tokens --check',
      'cargo test --workspace --exclude hakille-conformance-slow',
      'cargo run --release -p hakille-daemon &',
      'cargo run --release -p hakille-ui-desktop --bin hakille-desktop',
    ],
  },
  {
    id: 'macos',
    label: 'macOS (Apple Silicon / Intel)',
    tierFr: 'Palier 2 · Hôte Natif Metal',
    tierEn: 'Tier 2 · Native Metal Host',
    substrateCrate: 'hakille-substrate-macos',
    summaryFr:
      'Rendu GPU Metal via wgpu (0.0 fps au repos), Keychain Services macOS, AccessKit NSAccessibility VoiceOver et accélération SIMD ARM NEON.',
    summaryEn:
      'Metal GPU rendering via wgpu (0.0 fps idle), macOS Keychain Services, AccessKit NSAccessibility VoiceOver, and ARM NEON SIMD acceleration.',
    commands: [
      'git clone https://github.com/hakille/hakille.git && cd hakille',
      'cargo build --release -p hakille-daemon -p hakille-ui-desktop',
      './target/release/hakille-desktop',
    ],
  },
  {
    id: 'windows',
    label: 'Windows 11 (DX12 / UIA)',
    tierFr: 'Palier 2 · Hôte Natif DirectX 12',
    tierEn: 'Tier 2 · Native DirectX 12 Host',
    substrateCrate: 'hakille-substrate-windows',
    summaryFr:
      'Backend DirectX 12 / Vulkan via wgpu, protection DPAPI + Credential Manager, AccessKit UI Automation (NVDA/Narrator) et Job Objects.',
    summaryEn:
      'DirectX 12 / Vulkan backend via wgpu, DPAPI + Credential Manager keystore, AccessKit UI Automation (NVDA/Narrator), and Job Objects.',
    commands: [
      'git clone https://github.com/hakille/hakille.git',
      'cd hakille',
      'cargo run --release -p hakille-ui-desktop --bin hakille-desktop',
    ],
  },
  {
    id: 'android',
    label: 'Android 14+ (Jetpack Compose)',
    tierFr: 'Palier 3 · Compagnon Invité Honnête',
    tierEn: 'Tier 3 · Honest Guest Companion',
    substrateCrate: 'hakille-mobile (UniFFI 0.32.2)',
    summaryFr:
      'Interface native Jetpack Compose, Inter Variable embarquée, dictée hors ligne, Durable Checkpoint v2 et connexion TLS épinglée vers votre Hub.',
    summaryEn:
      'Native Jetpack Compose UI, bundled Inter Variable, offline dictation, Durable Checkpoint v2, and TLS-pinned Hub transport.',
    commands: [
      'cargo build --release -p hakille-mobile',
      'cd mobile/android && ./gradlew assembleDebug',
    ],
  },
  {
    id: 'ios',
    label: 'iOS 17+ (SwiftUI)',
    tierFr: 'Palier 3 · Compagnon Invité Honnête',
    tierEn: 'Tier 3 · Honest Guest Companion',
    substrateCrate: 'hakille-mobile (UniFFI 0.32.2)',
    summaryFr:
      'Interface native SwiftUI, Keychain iOS, consentement explicite Dictée vs Conversation, Checkpoint v2 anti-rejeu et zéro écoute micro en arrière-plan.',
    summaryEn:
      'Native SwiftUI interface, iOS Keychain, explicit Dictation vs Conversation consent, anti-replay Checkpoint v2, and zero background mic capture.',
    commands: [
      'cargo build --release -p hakille-mobile',
      'open mobile/ios/Hakille.xcodeproj',
    ],
  },
  {
    id: 'redox',
    label: 'Redox OS (Micro-noyau Rust)',
    tierFr: 'Palier 1 · Souveraineté Intégrale (Feuille de route Phase 3)',
    tierEn: 'Tier 1 · Full Appliance Sovereignty (Phase 3 Roadmap)',
    substrateCrate: 'hakille-substrate-redox',
    summaryFr:
      'Substrat Rust sans héritage POSIX C : capabilities par descripteur de schéma. Contrat Rust prêt ; image appliance bootable planifiée en Phase 3.',
    summaryEn:
      'Pure-Rust microkernel substrate with zero legacy C attack surface: scheme-level capabilities. Trait contract compiled today; bootable image targets Phase 3.',
    commands: [
      'cargo check -p hakille-substrate-redox',
      'cargo test -p hakille-substrate',
    ],
  },
] as const;

interface SizableModule {
  readonly id: string;
  readonly name: string;
  readonly ramMb: number;
  readonly required?: boolean;
  readonly descFr: string;
  readonly descEn: string;
}

const SIZABLE_MODULES: readonly SizableModule[] = [
  {
    id: 'core-daemon',
    name: 'hakille-daemon + redb + Biscuit + Wasmtime 48',
    ramMb: 64,
    required: true,
    descFr: 'Noyau obligatoire : graphe mémoire SPO ACID, chaîne SHA-256 + Ed25519 et bac à sable WASM.',
    descEn: 'Mandatory core: ACID SPO memory graph, SHA-256 + Ed25519 audit chain, and WASM sandbox.',
  },
  {
    id: 'klatt-yin',
    name: 'Klatt 5-Formant TTS + YIN F0 + Karplus-Strong',
    ramMb: 4,
    required: true,
    descFr: 'Voix mathématique pure Rust 24 kHz + 11 visèmes + cordes Kora 48 kHz (0 Mo de poids).',
    descEn: 'Pure-Rust 24 kHz formant speech + 11 visemes + 48 kHz Kora strings (0 MB neural weights).',
  },
  {
    id: 'silero-cam',
    name: 'Silero VAD v5 + CAM++ Speaker 192-d (ONNX)',
    ramMb: 42,
    descFr: 'Détection d’activité vocale (< 2 ms/trame) et vérification locale du propriétaire légitime.',
    descEn: 'Voice activity detection (< 2 ms/frame) and offline legitimate owner speaker verification.',
  },
  {
    id: 'minilm',
    name: 'all-MiniLM-L6-v2 384-d + Index HNSW',
    ramMb: 96,
    descFr: 'Recherche sémantique locale hybride couplée aux triplets SPO exacts.',
    descEn: 'Offline 384-d semantic vector search paired with exact SPO graph triples.',
  },
  {
    id: 'whisper',
    name: 'Whisper Tiny 99 langues (Candle FP32 GEMM ×46)',
    ramMb: 195,
    descFr: 'Transcription vocale hors ligne accélérée ×46 sur CPU (~1,4 s pour 3 s d’audio).',
    descEn: '99-language offline ASR accelerated ×46 on CPU (~1.4 s per 3 s utterance).',
  },
  {
    id: 'qwen',
    name: 'Qwen 2.5 0.5B Instruct Q4_K_M (GGUF)',
    ramMb: 440,
    descFr: 'Raisonnement et synthèse locale Système 1 entièrement hors connexion.',
    descEn: 'Offline System 1 local reasoning and synthesis via quantized GGUF.',
  },
  {
    id: 'mimi',
    name: 'Kyutai Mimi 24 kHz Neural Audio Codec (8×2048 RVQ)',
    ramMb: 410,
    descFr: 'Codec neuronal temps réel 12,5 Hz préparant le pipeline vocal full-duplex.',
    descEn: 'Real-time 12.5 Hz neural audio codec powering the full-duplex voice pipeline.',
  },
] as const;

export function InstallPage() {
  const { locale, t } = useI18n();
  const [osId, setOsId] = useState<OsTarget['id']>('linux');
  const [ramGb, setRamGb] = useState<number>(8);
  const [enabledIds, setEnabledIds] = useState<readonly string[]>([
    'core-daemon',
    'klatt-yin',
    'silero-cam',
    'minilm',
    'whisper',
    'qwen',
  ]);
  const [copiedCmd, setCopiedCmd] = useState<string | null>(null);

  const selectedOs = OS_TARGETS.find((o) => o.id === osId) ?? OS_TARGETS[0]!;

  const toggleModule = (mod: SizableModule) => {
    if (mod.required) return;
    setEnabledIds((prev) =>
      prev.includes(mod.id) ? prev.filter((x) => x !== mod.id) : [...prev, mod.id]
    );
  };

  const totalUsedMb = useMemo(
    () =>
      SIZABLE_MODULES.filter((m) => enabledIds.includes(m.id)).reduce(
        (acc, m) => acc + m.ramMb,
        0
      ),
    [enabledIds]
  );

  const budgetMb = ramGb * 1024;
  const recommendedCeilingMb = Math.round(budgetMb * 0.45); // Never starve the user's OS
  const usagePct = Math.min(100, Math.round((totalUsedMb / budgetMb) * 100));
  const fitsComfortably = totalUsedMb <= recommendedCeilingMb;

  const copyLine = (cmd: string) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(cmd);
      setCopiedCmd(cmd);
      window.setTimeout(() => setCopiedCmd(null), 1500);
    }
  };

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">
            RUST 1.85+ (EDITION 2024) · APACHE-2.0 · 0 % PYTHON · 0 % ELECTRON
          </span>
          <h1 className="display-h1">{t.routes.install.title}</h1>
          <p className="hero-lead">{t.routes.install.description}</p>
        </div>
      </section>

      <div className="container page-content">
        {/* 1. Interactive Hardware RAM Budget & Model Configurator */}
        <div className="sim-shell">
          <div className="sim-header">
            <div>
              <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                {pickBi(
                  locale,
                  '1. Dimensionneur d’Empreinte RAM Locale & Choix des Moteurs',
                  '1. Local RAM Footprint Sizer & Engine Configurator'
                )}
              </h2>
              <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                {pickBi(
                  locale,
                  'Hakille respecte votre machine : sélectionnez votre mémoire vive totale et activez les moteurs locaux pour vérifier votre marge système.',
                  'Hakille respects your hardware: choose your host RAM and toggle offline DSP/neural engines to inspect exact memory headroom.'
                )}
              </p>
            </div>
            <div className="pill-tabs">
              {[2, 4, 8, 16].map((gb) => (
                <button
                  key={gb}
                  type="button"
                  className="pill-tab"
                  aria-selected={ramGb === gb}
                  onClick={() => setRamGb(gb)}
                >
                  {gb === 16 ? '16 Go+' : `${gb} Go RAM`}
                </button>
              ))}
            </div>
          </div>

          <div className="sim-body" style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
            {/* Live Memory Gauge */}
            <div
              style={{
                padding: '16px 20px',
                borderRadius: '12px',
                border: '1px solid var(--hakille-ligne)',
                background: 'var(--hakille-surface-0)',
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', flexWrap: 'wrap', gap: '8px' }}>
                <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, 'Empreinte Hakille sélectionnée :', 'Selected Hakille footprint:')}{' '}
                  <strong className="mono tabular" style={{ color: 'var(--hakille-lueur)', fontSize: '1.05rem' }}>
                    {totalUsedMb.toLocaleString('en-US')} MB
                  </strong>{' '}
                  / {budgetMb.toLocaleString('en-US')} MB ({usagePct}%)
                </span>
                <span className="status-tag">
                  {fitsComfortably
                    ? pickBi(locale, '✓ Fluide hors ligne (marge OS > 55 %)', '✓ Smooth offline headroom (> 55% free for OS)')
                    : pickBi(
                        locale,
                        '⚠ Marge serrée sur 2 Go : privilégiez le mode Noyau + Klatt léger',
                        '⚠ Tight on 2 GB RAM: disable Mimi/Qwen for ultra-light daemon mode'
                      )}
                </span>
              </div>

              <div
                style={{
                  width: '100%',
                  height: '10px',
                  borderRadius: '999px',
                  background: 'var(--hakille-surface-2)',
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    width: `${Math.max(3, usagePct)}%`,
                    height: '100%',
                    background: 'var(--hakille-lueur)',
                    transition: 'width 160ms ease',
                  }}
                />
              </div>
            </div>

            <div className="crate-grid">
              {SIZABLE_MODULES.map((mod) => {
                const active = enabledIds.includes(mod.id);
                return (
                  <button
                    key={mod.id}
                    type="button"
                    onClick={() => toggleModule(mod)}
                    className="crate-item"
                    style={{
                      textAlign: 'start',
                      cursor: mod.required ? 'default' : 'pointer',
                      borderColor: active ? 'var(--hakille-texte-2)' : 'var(--hakille-ligne)',
                      opacity: active ? 1 : 0.62,
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', gap: '8px', width: '100%' }}>
                      <strong className="mono" style={{ fontSize: '0.82rem', color: 'var(--hakille-texte-1)' }}>
                        {active ? '◉ ' : '○ '}
                        {mod.name}
                      </strong>
                      <span className="status-tag">{mod.ramMb} MB</span>
                    </div>
                    <p style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-2)' }}>
                      {pickBi(locale, mod.descFr, mod.descEn)}
                    </p>
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {/* 2. Per-OS Substrate Selector & Exact Build Commands */}
        <div className="sim-shell">
          <div className="sim-header">
            <div>
              <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                {pickBi(
                  locale,
                  '2. Compiler Depuis les Sources par Substrat Système (ADR-010)',
                  '2. Build From Source Per Operating System Substrate (ADR-010)'
                )}
              </h2>
            </div>
            <span className="status-tag">{selectedOs.substrateCrate}</span>
          </div>

          <div className="sim-body" style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
            <div className="pill-tabs">
              {OS_TARGETS.map((o) => (
                <button
                  key={o.id}
                  type="button"
                  className="pill-tab"
                  aria-selected={osId === o.id}
                  onClick={() => setOsId(o.id)}
                >
                  {o.label}
                </button>
              ))}
            </div>

            <div className="arch-card">
              <div style={{ display: 'flex', justifyContent: 'space-between', flexWrap: 'wrap', gap: '8px' }}>
                <strong style={{ fontSize: '1.05rem' }}>{selectedOs.label}</strong>
                <span className="status-tag">
                  {pickBi(locale, selectedOs.tierFr, selectedOs.tierEn)}
                </span>
              </div>
              <p style={{ color: 'var(--hakille-texte-2)' }}>
                {pickBi(locale, selectedOs.summaryFr, selectedOs.summaryEn)}
              </p>

              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {selectedOs.commands.map((cmd) => (
                  <div key={cmd} className="code-command-box">
                    <code className="mono" style={{ color: 'var(--hakille-texte-1)' }}>
                      {cmd}
                    </code>
                    <button
                      type="button"
                      className="ghost-btn"
                      onClick={() => copyLine(cmd)}
                    >
                      {copiedCmd === cmd ? t.home.act9Copied : t.home.act9CopyCmd}
                    </button>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* 3. Live Voice & Mobile Simulators */}
        <KlattVoiceStudio />
        <MobileCompanionSimulator />
      </div>
    </div>
  );
}
