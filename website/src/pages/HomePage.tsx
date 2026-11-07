import { useState } from 'react';
import { LEGAL_CONFIG } from '@/config/legal';
import { CONSTITUTION_RULES } from '@/data/constitutionData';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { HARDWARE_TIERS } from '@/data/substratesData';
import { CULTURAL_SCRIPT_SPECIMENS } from '@/i18n/culturalScripts';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { AppLink } from '@/context/RouterContext';
import { KoraPresenceInstrument } from '@/components/simulators/KoraPresenceInstrument';
import { KlattVoiceStudio } from '@/components/simulators/KlattVoiceStudio';
import { ActionCardAndAuditLab } from '@/components/simulators/ActionCardAndAuditLab';
import { MemoryGraphExplorer } from '@/components/simulators/MemoryGraphExplorer';
import { ArchitectureAndFabricExplorer } from '@/components/simulators/ArchitectureAndFabricExplorer';
import { AmbientAndAutonomyLab } from '@/components/simulators/AmbientAndAutonomyLab';
import { MobileCompanionSimulator } from '@/components/simulators/MobileCompanionSimulator';

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
                className="code-command-box"
                onClick={onOpenSpotlight}
                style={{ width: '100%', maxWidth: '580px', textAlign: 'start' }}
              >
                <span className="mono" style={{ color: 'var(--hakille-texte-2)' }}>
                  › {t.home.heroSpotlightPrompt}
                </span>
                <kbd className="kbd-chip">{t.header.spotlightShortcut}</kbd>
              </button>
            </div>

            <KoraPresenceInstrument />
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
            <KlattVoiceStudio />
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

          <ActionCardAndAuditLab />
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

          <MemoryGraphExplorer />
        </div>
      </section>

      {/* ACT V — 6-Layer Architecture, 41 Crates & 300s Biscuit Lease */}
      <section className="act-section">
        <div className="container">
          <div className="section-header">
            <span className="eyebrow-mono">{t.home.act5Eyebrow}</span>
            <h2 className="section-h2">{t.home.act5Title}</h2>
            <p className="section-lead">{t.home.act5Body}</p>
          </div>

          <ArchitectureAndFabricExplorer />

          <div style={{ marginTop: '20px' }}>
            <AppLink to="/architecture" className="ghost-btn">
              {t.common.readFullDoc} → /architecture
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

          <AmbientAndAutonomyLab />
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
            <MobileCompanionSimulator />
          </div>

          <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', marginTop: '24px' }}>
            <AppLink to="/installer" className="primary-lueur-btn">
              {pickBi(
                locale,
                'Dimensionner ma RAM & Configurer mon Substrat OS → /installer',
                'Size My RAM & Configure My OS Substrate → /installer'
              )}
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
              {t.routes.constitution.navLabel} (10 rules) →
            </AppLink>
            <AppLink to="/accessibilite" className="ghost-btn">
              {t.routes.accessibility.navLabel} (Adlam · N&apos;Ko · RTL) →
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
                <div key={cmd} className="code-command-box">
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
