import { lazy, Suspense, useState } from 'react';
import { LEGAL_CONFIG } from '@/config/legal';
import { CONSTITUTION_RULES } from '@/data/constitutionData';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { HARDWARE_TIERS } from '@/data/substratesData';
import { CULTURAL_SCRIPT_SPECIMENS } from '@/i18n/culturalScripts';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { AppLink } from '@/context/RouterContext';
import { DirectionalArrow, SimulatorSkeleton } from '@/components/common';

const KoraPresenceInstrument = lazy(() =>
  import('@/components/simulators/KoraPresenceInstrument').then((m) => ({
    default: m.KoraPresenceInstrument,
  }))
);
const KlattVoiceStudio = lazy(() =>
  import('@/components/simulators/KlattVoiceStudio').then((m) => ({
    default: m.KlattVoiceStudio,
  }))
);
const ActionCardAndAuditLab = lazy(() =>
  import('@/components/simulators/ActionCardAndAuditLab').then((m) => ({
    default: m.ActionCardAndAuditLab,
  }))
);
const MemoryGraphExplorer = lazy(() =>
  import('@/components/simulators/MemoryGraphExplorer').then((m) => ({
    default: m.MemoryGraphExplorer,
  }))
);
const ArchitectureAndFabricExplorer = lazy(() =>
  import('@/components/simulators/ArchitectureAndFabricExplorer').then((m) => ({
    default: m.ArchitectureAndFabricExplorer,
  }))
);
const AmbientAndAutonomyLab = lazy(() =>
  import('@/components/simulators/AmbientAndAutonomyLab').then((m) => ({
    default: m.AmbientAndAutonomyLab,
  }))
);
const DesktopHudToastSandbox = lazy(() =>
  import('@/components/simulators/DesktopHudToastSandbox').then((m) => ({
    default: m.DesktopHudToastSandbox,
  }))
);
const MobileCompanionSimulator = lazy(() =>
  import('@/components/simulators/MobileCompanionSimulator').then((m) => ({
    default: m.MobileCompanionSimulator,
  }))
);

const QUICKSTART_COMMANDS = [
  'git clone https://github.com/hakille/hakille.git && cd hakille',
  'cargo run -p xtask -- tokens --check',
  'cargo test --workspace',
  'cargo run -p hakille-ui-desktop --bin hakille-desktop',
] as const;

export function HomePage({ onOpenSpotlight }: { onOpenSpotlight: () => void }) {
  const { locale, t } = useI18n();
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);

  const copyCommand = (cmd: string, idx: number) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(cmd);
      setCopiedIdx(idx);
      window.setTimeout(() => setCopiedIdx(null), 1600);
    }
  };

  return (
    <div>
      {/* ACT I — Sovereign Awakening Hero */}
      <section className="hero-section">
        <div className="container">
          <div className="hero-grid">
            <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
              <div className="eyebrow-mono">{t.home.heroMeta}</div>
              <h1 className="display-h1">{t.home.heroTitle}</h1>
              <p className="hero-lead">{t.home.heroSubtitle}</p>

              <div className="hero-cta-row">
                <a href="#act-audit-lab" className="primary-lueur-btn">
                  {t.home.heroPrimaryCta}
                </a>
                <AppLink to="/etat-reel" className="secondary-outline-btn">
                  {t.home.heroSecondaryCta}
                </AppLink>
              </div>

              <button
                type="button"
                className="code-command-box ltr-isolate"
                onClick={onOpenSpotlight}
                style={{ width: '100%', maxWidth: '580px', textAlign: 'start' }}
              >
                <span className="mono" style={{ color: 'var(--hakille-texte-2)' }}>
                  › {t.home.heroSpotlightPrompt}
                </span>
                <kbd className="kbd-chip">{t.header.spotlightShortcut}</kbd>
              </button>
            </div>

            <Suspense
              fallback={
                <SimulatorSkeleton
                  title={pickBi(locale, 'Instrument Kora 21 Cordes (48 kHz)', '21-String Kora Instrument (48 kHz)')}
                  description={pickBi(
                    locale,
                    'Synthèse physique Karplus-Strong temps réel, corde pincée et animation orbitale.',
                    'Real-time Karplus-Strong physical modeling, plucked string audio synthesis, and orbital animation.'
                  )}
                  minHeight={420}
                />
              }
            >
              <KoraPresenceInstrument />
            </Suspense>
          </div>

          <div className="metrics-strip">
            <div className="metric-cell">
              <strong className="mono tabular" style={{ fontSize: '1.35rem', color: 'var(--hakille-lueur)' }}>
                {WORKSPACE_METRICS.totalPackages}
              </strong>
              <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                {t.home.metricsPackages}
              </span>
            </div>
            <div className="metric-cell">
              <strong className="mono tabular" style={{ fontSize: '1.35rem', color: 'var(--hakille-lueur)' }}>
                {WORKSPACE_METRICS.totalRustLoc.toLocaleString('fr-FR')}
              </strong>
              <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                {t.home.metricsLoc}
              </span>
            </div>
            <div className="metric-cell">
              <strong className="mono tabular" style={{ fontSize: '1.35rem', color: 'var(--hakille-lueur)' }}>
                {WORKSPACE_METRICS.totalAutomatedTests.toLocaleString('fr-FR')}
              </strong>
              <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                {t.home.metricsTests}
              </span>
            </div>
            <div className="metric-cell">
              <strong className="mono tabular" style={{ fontSize: '1.35rem', color: 'var(--hakille-lueur)' }}>
                0 %
              </strong>
              <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                {t.home.metricsPython}
              </span>
            </div>
          </div>
        </div>
      </section>

      {/* ACT II — The Attentional Counter-Thesis */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act2Eyebrow}</span>
            <h2 className="section-h2">{t.home.act2Title}</h2>
            <p className="section-lead">{t.home.act2Body}</p>
          </div>

          <div className="comparison-grid" style={{ marginBottom: 0 }}>
            <div className="arch-card">
              <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', color: 'var(--hakille-texte-2)' }}>
                {t.home.act2IndustryTitle}
              </h3>
              <ul className="clean-list">
                {t.home.act2IndustryPoints.map((pt, i) => (
                  <li key={i}>
                    <span className="mono" style={{ color: 'var(--hakille-texte-3)' }}>—</span>
                    <span>{pt}</span>
                  </li>
                ))}
              </ul>
            </div>

            <div className="arch-card" data-emphasis="lueur">
              <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', color: 'var(--hakille-lueur)' }}>
                {t.home.act2HakilleTitle}
              </h3>
              <ul className="clean-list">
                {t.home.act2HakillePoints.map((pt, i) => (
                  <li key={i}>
                    <span className="mono" style={{ color: 'var(--hakille-lueur)' }}>◆</span>
                    <span style={{ color: 'var(--hakille-texte-1)' }}>{pt}</span>
                  </li>
                ))}
              </ul>
            </div>
          </div>

          <div style={{ marginTop: '32px' }}>
            <Suspense
              fallback={
                <SimulatorSkeleton
                  title={pickBi(locale, 'Studio Vocal Klatt 5 Formants (24 kHz)', 'Klatt 5-Formant Speech Studio (24 kHz)')}
                  description={pickBi(
                    locale,
                    'Synthèse vocale temps réel sans dépendance externe, 11 visèmes synchronisés et coupure Barge-In.',
                    'Zero-weight real-time formant speech synthesis, 11 synchronized mouth visemes, and Barge-In cutoff.'
                  )}
                  minHeight={480}
                />
              }
            >
              <KlattVoiceStudio />
            </Suspense>
          </div>
        </div>
      </section>

      {/* ACT III — 600ms Physical Hold & Live Cryptographic Audit Laboratory */}
      <section id="act-audit-lab" className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act3Eyebrow}</span>
            <h2 className="section-h2">{t.home.act3Title}</h2>
            <p className="section-lead">{t.home.act3Body}</p>
          </div>

          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(locale, 'Action Card & Registre d’Audit Chaîné', 'Action Card & Cryptographic Audit Chain')}
                description={pickBi(
                  locale,
                  'Maintien physique Braise 600 ms, signature Ed25519 locale et détection d’altération au bit près.',
                  '600 ms physical Braise hold, local Ed25519 signatures, and bit-level cryptographic tamper detection.'
                )}
                minHeight={520}
              />
            }
          >
            <ActionCardAndAuditLab />
          </Suspense>
        </div>
      </section>

      {/* ACT IV — SPO Memory Graph & 5-Minute EU AI Act Affective Purge */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act4Eyebrow}</span>
            <h2 className="section-h2">{t.home.act4Title}</h2>
            <p className="section-lead">{t.home.act4Body}</p>
          </div>

          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(locale, 'Graphe Mémoire SPO & Purge EU AI Act', 'SPO Memory Graph & EU AI Act Purge')}
                description={pickBi(
                  locale,
                  'Triplestore ACID redb, décroissance exponentielle W(t) et purge sans pierre tombale à 5 minutes.',
                  'ACID triplestore model, exponential memory decay W(t), and EU AI Act 5-minute tombstone-free purge.'
                )}
                minHeight={440}
              />
            }
          >
            <MemoryGraphExplorer />
          </Suspense>
        </div>
      </section>

      {/* ACT V — 6-Layer Architecture, 42 Crates & 300s Biscuit Lease */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act5Eyebrow}</span>
            <h2 className="section-h2">{t.home.act5Title}</h2>
            <p className="section-lead">{t.home.act5Body}</p>
          </div>

          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(locale, 'Explorateur d’Architecture & 42 Crates', 'Architecture Explorer & 42 Crates')}
                description={pickBi(
                  locale,
                  'Cartographie interactive des 6 couches architecturales et bail Biscuit 300s avec compteur de dépenses.',
                  'Interactive mapping of 6 architectural layers and 300s Biscuit lease with live network byte meter.'
                )}
                minHeight={560}
              />
            }
          >
            <ArchitectureAndFabricExplorer />
          </Suspense>

          <div style={{ marginTop: '20px' }}>
            <AppLink to="/architecture" className="ghost-btn">
              {t.common.readFullDoc} <DirectionalArrow /> /architecture
            </AppLink>
          </div>
        </div>
      </section>

      {/* ACT VI — Multimodal Silence Budget & Autonomy Cranks */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act6Eyebrow}</span>
            <h2 className="section-h2">{t.home.act6Title}</h2>
            <p className="section-lead">{t.home.act6Body}</p>
          </div>

          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(locale, 'Laboratoire d’Autonomie & Budget de Silence', 'Autonomy Lab & Daily Silence Budget')}
                description={pickBi(
                  locale,
                  'Sélecteur de canal multimodal, budget de 3 suggestions/jour et échelle de manivelle d’autonomie 0..4.',
                  'Multimodal channel selector, 3 proactive suggestions/day budget, and 0..4 autonomy crank ladder.'
                )}
                minHeight={460}
              />
            }
          >
            <AmbientAndAutonomyLab />
          </Suspense>

          <div style={{ marginTop: '32px' }}>
            <Suspense
              fallback={
                <SimulatorSkeleton
                  title={pickBi(locale, 'Bac à Sable HUD Desktop & Toasts Acrylique', 'Desktop HUD Toast Sandbox & Acrylic UI')}
                  description={pickBi(
                    locale,
                    'Toasts GPU winit+wgpu avec flou acrylique 30px, barre d’évaporation et bouton Braise 600 ms.',
                    'Native GPU toasts with 30px acrylic blur, fluid auto-evaporation bar, and 600 ms Braise confirmation.'
                  )}
                  minHeight={400}
                />
              }
            >
              <DesktopHudToastSandbox />
            </Suspense>
          </div>
        </div>
      </section>

      {/* ACT VII — Three Honest Hardware Tiers */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act7Eyebrow}</span>
            <h2 className="section-h2">{t.home.act7Title}</h2>
            <p className="section-lead">{t.home.act7Body}</p>
          </div>

          <div className="tier-grid-3">
            {HARDWARE_TIERS.map((tier) => (
              <article key={tier.id} className="arch-card">
                <div style={{ display: 'flex', justifyContent: 'space-between', gap: '8px', flexWrap: 'wrap' }}>
                  <span className="status-tag">{tier.badge}</span>
                  <code style={{ color: 'var(--hakille-texte-3)', fontSize: '0.74rem' }}>{tier.manifestPath}</code>
                </div>
                <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                  {pickBi(locale, tier.nameFr, tier.nameEn)}
                </h3>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                  {tier.ramFootprint}
                </p>
                <ul className="clean-list" style={{ fontSize: 'var(--hakille-type-caption-size)', flex: 1 }}>
                  {(locale === 'fr' ? tier.capabilitiesFr : tier.capabilitiesEn).map((cap, idx) => (
                    <li key={idx}>
                      <span className="mono" style={{ color: 'var(--hakille-lueur)' }}>·</span>
                      <span>{cap}</span>
                    </li>
                  ))}
                </ul>
                <div
                  style={{
                    paddingTop: '12px',
                    borderTop: '1px solid var(--hakille-ligne)',
                    fontSize: '0.76rem',
                    color: 'var(--hakille-texte-3)',
                  }}
                >
                  {pickBi(locale, tier.honestLimitFr, tier.honestLimitEn)}
                </div>
              </article>
            ))}
          </div>

          <div style={{ marginTop: '32px' }}>
            <Suspense
              fallback={
                <SimulatorSkeleton
                  title={pickBi(locale, 'Simulateur Compagnon Mobile & Checkpoint v2', 'Mobile Companion Simulator & Checkpoint v2')}
                  description={pickBi(
                    locale,
                    'Écran tactile smartphone iOS/Android, tabs Voix/Mémoire/Activité/Hub et transport crash-safe.',
                    'iOS/Android smartphone touch frame, Voice/Memory/Activity/Hub tabs, and durable Checkpoint v2 recovery.'
                  )}
                  minHeight={540}
                />
              }
            >
              <MobileCompanionSimulator />
            </Suspense>
          </div>

          <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', marginTop: '24px' }}>
            <AppLink to="/installer" className="primary-lueur-btn">
              {pickBi(
                locale,
                'Dimensionner ma RAM & Configurer mon Substrat OS',
                'Size My RAM & Configure My OS Substrate'
              )}{' '}
              <DirectionalArrow /> /installer
            </AppLink>
          </div>
        </div>
      </section>

      {/* ACT VIII — Inviolable Constitution & Cultural Scripts (Adlam, N'Ko, Arabic, CJK) */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act8Eyebrow}</span>
            <h2 className="section-h2">{t.home.act8Title}</h2>
            <p className="section-lead">{t.home.act8Body}</p>
          </div>

          <div className="crates-grid" style={{ marginBottom: '32px' }}>
            {CONSTITUTION_RULES.slice(0, 6).map((rule) => (
              <article key={rule.id} className="crate-card">
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                    {rule.code}
                  </span>
                  <span className="status-tag">{rule.severity}</span>
                </div>
                <strong style={{ fontSize: '0.95rem' }}>
                  {pickBi(locale, rule.titleFr, rule.titleEn)}
                </strong>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, rule.invariantFr, rule.invariantEn)}
                </p>
              </article>
            ))}
          </div>

          <div className="script-specimen-grid">
            {CULTURAL_SCRIPT_SPECIMENS.slice(0, 3).map((spec) => (
              <div key={spec.id} className="script-card" dir={spec.dir}>
                <div className="eyebrow-mono" dir="ltr">
                  {spec.languageName} · {spec.scriptName}
                </div>
                <div className="script-sample-large">{spec.wordmarkSample}</div>
                <div style={{ fontSize: '0.95rem', color: 'var(--hakille-texte-1)' }}>
                  {spec.phraseSample}
                </div>
                <p dir="ltr" style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
                  {pickBi(locale, spec.meaningFr, spec.meaningEn)}
                </p>
              </div>
            ))}
          </div>

          <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', marginTop: '24px' }}>
            <AppLink to="/constitution" className="ghost-btn">
              {t.routes.constitution.navLabel} (10 rules) <DirectionalArrow />
            </AppLink>
            <AppLink to="/accessibilite" className="ghost-btn">
              {t.routes.accessibility.navLabel} (Adlam · N&apos;Ko · RTL) <DirectionalArrow />
            </AppLink>
          </div>
        </div>
      </section>

      {/* ACT IX — Pre-Alpha Honesty Contract & Developer Quickstart */}
      <section className="act-section" style={{ borderBlockEnd: 'none' }}>
        <div className="container">
          <div className="comparison-grid" style={{ marginBottom: 0 }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
              <span className="eyebrow-mono">{t.home.act9Eyebrow}</span>
              <h2 className="section-h2">{t.home.act9Title}</h2>
              <p className="section-lead">{t.home.act9Body}</p>
              <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', paddingTop: '8px' }}>
                <a
                  href={LEGAL_CONFIG.domains.repository}
                  className="primary-lueur-btn"
                  rel="noopener noreferrer"
                >
                  GitHub · hakille/hakille ↗
                </a>
                <AppLink to="/confidentialite" className="secondary-outline-btn">
                  {t.routes.privacy.navLabel}
                </AppLink>
              </div>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
              {QUICKSTART_COMMANDS.map((cmd, idx) => (
                <div key={cmd} className="code-command-box ltr-isolate">
                  <code style={{ color: 'var(--hakille-lueur)' }}>$ {cmd}</code>
                  <button
                    type="button"
                    className="ghost-btn"
                    style={{ minHeight: '30px', paddingInline: '10px' }}
                    onClick={() => copyCommand(cmd, idx)}
                  >
                    {copiedIdx === idx ? t.home.act9Copied : t.home.act9CopyCmd}
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
