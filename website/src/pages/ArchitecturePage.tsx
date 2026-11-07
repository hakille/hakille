import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { HARDWARE_TIERS } from '@/data/substratesData';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { ArchitectureAndFabricExplorer } from '@/components/simulators/ArchitectureAndFabricExplorer';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

const ALL_14_ADRS = [
  {
    id: 'ADR-001',
    titleFr: '100 % Rust Édition 2024 & Zéro Runtime Python',
    titleEn: 'Pure Rust 2024 & Zero Python Runtime',
    detailFr: 'Élimine le GIL, la dérive pip/conda et les failles mémoire C++ sur les 42 crates.',
    detailEn: 'Eliminates GIL, pip/conda drift, and C++ glue vulnerabilities across all 42 crates.',
  },
  {
    id: 'ADR-002',
    titleFr: 'Capabilities Biscuit v5 au lieu de Permissions Ambiantes',
    titleEn: 'Biscuit v5 Capabilities Over Ambient Permissions',
    detailFr: 'Chaque sous-agent reçoit un jeton Datalog signé qui ne peut que restreindre ses droits.',
    detailEn: 'Every sub-agent receives a Datalog token that can only attenuate rights monotonically.',
  },
  {
    id: 'ADR-003',
    titleFr: 'Graphe SPO sur redb ACID + Index Vectoriel HNSW',
    titleEn: 'SPO Graph on redb ACID + HNSW Vector Index',
    detailFr: 'Votre identité réside dans des triplets inspectables et rectifiables, jamais figée dans les poids.',
    detailEn: 'Identity lives in explicit auditable triples, never baked into opaque neural weights.',
  },
  {
    id: 'ADR-004',
    titleFr: 'Pipeline Audio Full-Duplex Candle & Codec Mimi 24 kHz',
    titleEn: 'Full-Duplex Audio Pipeline on Candle & Mimi 24 kHz',
    detailFr: 'Streaming audio 24 kHz sans serveur Python avec interruption Barge-In < 80 ms.',
    detailEn: '24 kHz streaming audio without Python servers and < 80 ms Barge-In interruption.',
  },
  {
    id: 'ADR-005',
    titleFr: 'Abstraction de Substrat OS (Linux, macOS, Windows, Redox, Mobile, Web)',
    titleEn: 'Per-OS Substrate Abstraction Layer',
    detailFr: 'Sépare proprement le noyau cognitif des primitives système de chaque hôte.',
    detailEn: 'Cleanly isolates cognitive invariants from host-specific kernel primitives.',
  },
  {
    id: 'ADR-006',
    titleFr: 'Synchronisation CRDT Chiffrée de Bout en Bout',
    titleEn: 'End-to-End Encrypted CRDT Multi-Device Sync',
    detailFr: 'Réplication locale entre vos équipements sans jamais exposer le clair à un relais tiers.',
    detailEn: 'Device-to-device state convergence with zero plaintext exposure to relay servers.',
  },
  {
    id: 'ADR-007 / 008',
    titleFr: 'Gel de Slint & Spikes Iced — Leçon d’Ingénierie Documentée',
    titleEn: 'Slint Freeze & Iced Spikes — Documented UI Tradeoffs',
    detailFr: 'Conservation transparente des prototypes évalués avant le choix d’un moteur natif sur mesure.',
    detailEn: 'Open preservation of evaluated UI toolkits prior to building the custom native renderer.',
  },
  {
    id: 'ADR-009',
    titleFr: 'Shell Natif Pur winit + wgpu + AccessKit (< 25 Mo RAM)',
    titleEn: 'Native winit + wgpu + AccessKit GPU Shell (< 25 MB RAM)',
    detailFr: '0 % Electron/WebView, 0.0 fps garanti au repos et arbre d’accessibilité OS natif complet.',
    detailEn: '0% Electron/WebView overhead, true 0.0 fps idle suspension, and full OS Accessibility tree.',
  },
  {
    id: 'ADR-010',
    titleFr: 'Compatibilité Explicite par OS & Matrice des 3 Paliers',
    titleEn: 'Explicit Per-OS Compatibility & 3-Tier Honesty Matrix',
    detailFr: 'Ne promet jamais sur mobile ou navigateur une isolation noyau réservée à Linux/Redox.',
    detailEn: 'Never claims kernel-level guarantees on guest OS platforms that forbid them.',
  },
  {
    id: 'ADR-011',
    titleFr: 'Compagnon Mobile Natif (SwiftUI / Compose) & Checkpoint v2',
    titleEn: 'Native Mobile Companion & Durable Checkpoint v2',
    detailFr: 'Pont UniFFI 0.32.2 marquant « unknown » toute mutation interrompue pour interdire le double envoi.',
    detailEn: 'UniFFI 0.32.2 bridge marking interrupted mutations "unknown" to prevent duplicate side effects.',
  },
  {
    id: 'ADR-012',
    titleFr: 'Stockage Zero-Knowledge, Indexation Aveugle & Crémation Cryptographique',
    titleEn: 'Zero-Knowledge Storage, Blind Indexing & Cryptographic Cremation',
    detailFr: 'Chiffrement XChaCha20-Poly1305 lié au contexte (AAD), sous-clés HKDF ZeroizeOnDrop, index aveugle HMAC et partage Shamir.',
    detailEn: 'Context-bound XChaCha20-Poly1305 AEAD, ZeroizeOnDrop HKDF subkeys, HMAC blind indexing, and Shamir social recovery.',
  },
  {
    id: 'ADR-013',
    titleFr: 'Multi-Tenancy à 5 Niveaux, Moteur Hybride ReBAC + Biscuit & Délégation',
    titleEn: '5-Tier Multi-Tenancy, Hybrid ReBAC + Biscuit & Sovereign Agent Delegation',
    detailFr: ' Sphères Enterprise, Org, Group, Team et Family (invariant NoWallet pour mineurs, sessions invités <= 24 h).',
    detailEn: 'Enterprise, Org, Group, Team, and Family isolation spheres (minor NoWallet invariant, <= 24h bounded guest sessions).',
  },
  {
    id: 'ADR-014',
    titleFr: 'Biométrie Faciale Révocable BioHash, PAD ISO/IEC 30107-3 & Fusion Continue',
    titleEn: 'Revocable BioHash Face Auth, ISO/IEC 30107-3 Liveness PAD & Multimodal Fusion',
    detailFr: 'Anti-usurpation MiniFASNetV2 + défi actif, gabarits BioHash révocables (0 image brute sur disque) et fusion voix+visage.',
    detailEn: 'MiniFASNetV2 passive PAD + active challenge-response, revocable BioHash projections (0 raw face on disk), and voice+face fusion.',
  },
] as const;

export function ArchitecturePage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">
            {WORKSPACE_METRICS.totalPackages} PACKAGES ·{' '}
            {WORKSPACE_METRICS.totalRustLoc.toLocaleString('en-US')} LOC · MSRV {WORKSPACE_METRICS.msrv}
          </span>
          <h1 className="display-h1">{t.routes.architecture.title}</h1>
          <p className="hero-lead">{t.routes.architecture.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <ArchitectureAndFabricExplorer />

        <section>
          <div className="section-header">
            <span className="eyebrow-mono">ADR-001 .. ADR-014 (/adr/*.md)</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Les 14 Décisions Architecturales Fondatrices (ADR)',
                'The 14 Foundational Architectural Decision Records (ADRs)'
              )}
            </h2>
          </div>
          <div className="crate-grid">
            {ALL_14_ADRS.map((adr) => (
              <article key={adr.id} className="crate-item">
                <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                  {adr.id}
                </span>
                <strong style={{ color: 'var(--hakille-texte-1)' }}>
                  {pickBi(locale, adr.titleFr, adr.titleEn)}
                </strong>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, adr.detailFr, adr.detailEn)}
                </p>
              </article>
            ))}
          </div>
        </section>

        <section>
          <div className="section-header">
            <span className="eyebrow-mono">MANIFESTS/*.TOML</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Matrice des 3 Paliers Matériels & 6 Substrats OS',
                '3 Hardware Tiers & 6 OS Substrates'
              )}
            </h2>
          </div>
          <div className="tier-grid-3">
            {HARDWARE_TIERS.map((tier) => (
              <article key={tier.id} className="arch-card">
                <span className="status-tag">{tier.badge}</span>
                <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                  {pickBi(locale, tier.nameFr, tier.nameEn)}
                </h3>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  <strong>{pickBi(locale, 'Matériel :', 'Hardware:')}</strong>{' '}
                  {pickBi(locale, tier.targetHardwareFr, tier.targetHardwareEn)}
                </p>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  <strong>{pickBi(locale, 'Isolation :', 'Isolation:')}</strong>{' '}
                  {pickBi(locale, tier.isolationLevelFr, tier.isolationLevelEn)}
                </p>
                <p className="mono" style={{ fontSize: '0.76rem', color: 'var(--hakille-lueur)' }}>
                  {tier.ramFootprint}
                </p>
              </article>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
