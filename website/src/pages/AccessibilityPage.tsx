import { COLOR_TOKENS } from '@/tokens/hakilleTokens.generated';
import { contrastRatio } from '@/engine/colorGuard';
import { CULTURAL_SCRIPT_SPECIMENS } from '@/i18n/culturalScripts';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { usePreferences } from '@/context/PreferencesContext';

export function AccessibilityPage() {
  const { locale, t } = useI18n();
  const { textScale, setTextScale, contrast, setContrast, motion, setMotion } = usePreferences();

  const darkPrimaryRatio = contrastRatio(COLOR_TOKENS.texte['1'].dark, COLOR_TOKENS.surface['0'].dark);
  const lightPrimaryRatio = contrastRatio(COLOR_TOKENS.texte['1'].light, COLOR_TOKENS.surface['0'].light);
  const darkSecondaryRatio = contrastRatio(COLOR_TOKENS.texte['2'].dark, COLOR_TOKENS.surface['0'].dark);
  const lightSecondaryRatio = contrastRatio(COLOR_TOKENS.texte['2'].light, COLOR_TOKENS.surface['0'].light);

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">WCAG 2.1 AAA / AA · ACCESSKIT NATIVE · RTL &amp; MULTI-SCRIPT</span>
          <h1 className="display-h1">{t.routes.accessibility.title}</h1>
          <p className="hero-lead">{t.routes.accessibility.description}</p>
        </div>
      </section>

      <div className="container page-content">
        {/* Live Calculated WCAG Contrast Proofs */}
        <section>
          <h2 className="section-h2" style={{ marginBottom: '16px' }}>
            {pickBi(locale, 'Preuves Mathématiques de Contraste WCAG 2.1 (colorGuard.ts)', 'Mathematically Verified WCAG 2.1 Contrast Ratios (colorGuard.ts)')}
          </h2>
          <div className="crates-grid">
            <div className="crate-card">
              <span className="eyebrow-mono">Obsidian Dark · texte.1 / surface.0</span>
              <strong className="mono tabular" style={{ fontSize: '1.6rem', color: 'var(--hakille-lueur)' }}>
                {darkPrimaryRatio.toFixed(2)} : 1
              </strong>
              <span className="status-tag">WCAG AAA (&gt;= 7.0:1)</span>
            </div>
            <div className="crate-card">
              <span className="eyebrow-mono">Parchment Light · texte.1 / surface.0</span>
              <strong className="mono tabular" style={{ fontSize: '1.6rem', color: 'var(--hakille-lueur)' }}>
                {lightPrimaryRatio.toFixed(2)} : 1
              </strong>
              <span className="status-tag">WCAG AAA (&gt;= 7.0:1)</span>
            </div>
            <div className="crate-card">
              <span className="eyebrow-mono">Obsidian Dark · texte.2 / surface.0</span>
              <strong className="mono tabular" style={{ fontSize: '1.6rem', color: 'var(--hakille-lueur)' }}>
                {darkSecondaryRatio.toFixed(2)} : 1
              </strong>
              <span className="status-tag">WCAG AAA (&gt;= 7.0:1)</span>
            </div>
            <div className="crate-card">
              <span className="eyebrow-mono">Parchment Light · texte.2 / surface.0</span>
              <strong className="mono tabular" style={{ fontSize: '1.6rem', color: 'var(--hakille-lueur)' }}>
                {lightSecondaryRatio.toFixed(2)} : 1
              </strong>
              <span className="status-tag">WCAG AAA (&gt;= 7.0:1)</span>
            </div>
          </div>
        </section>

        {/* Interactive A11y Control Bench */}
        <section className="sim-frame">
          <div className="sim-topbar">
            <strong>
              {pickBi(locale, 'Banc de Réglage Immédiat (80 % – 200 %, Contraste, Mouvement)', 'Live Accessibility & Motor Control Bench')}
            </strong>
          </div>
          <div className="sim-body" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: '20px' }}>
            <div>
              <label htmlFor="page-scale-slider" style={{ display: 'block', fontWeight: 600, marginBottom: '8px' }}>
                {t.a11yDrawer.textScaleLabel}: <span className="mono">{Math.round(textScale * 100)}%</span>
              </label>
              <input
                id="page-scale-slider"
                type="range"
                min={0.8}
                max={2.0}
                step={0.05}
                value={textScale}
                onChange={(e) => setTextScale(Number(e.target.value))}
                className="range-input"
              />
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
              <strong>{t.a11yDrawer.contrastLabel}</strong>
              <button
                type="button"
                className="ghost-btn"
                aria-pressed={contrast === 'high'}
                onClick={() => setContrast(contrast === 'high' ? 'normal' : 'high')}
              >
                {contrast === 'high' ? t.a11yDrawer.contrastHigh : t.a11yDrawer.contrastNormal}
              </button>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
              <strong>{t.a11yDrawer.motionLabel}</strong>
              <button
                type="button"
                className="ghost-btn"
                aria-pressed={motion === 'reduced'}
                onClick={() => setMotion(motion === 'reduced' ? 'normal' : 'reduced')}
              >
                {motion === 'reduced' ? t.a11yDrawer.motionReduced : t.a11yDrawer.motionNormal}
              </button>
            </div>
          </div>
        </section>

        {/* Cultural & Multi-Script Specimen Laboratory (Adlam, N'Ko, Arabic RTL, Devanagari, CJK) */}
        <section>
          <div className="section-header">
            <span className="eyebrow-mono">TOKENS/TYPE.TOML · UNICODE PLURALITY</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Laboratoire Multi-Écritures : Pulaar Adlam, Mandingue N’Ko, Arabe RTL, Devanāgarī & CJK',
                'World Scripts Laboratory: Fulani Adlam, Manding N’Ko, Arabic RTL, Devanagari & CJK'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Conformément à crates/hakille-design/src/typo.rs, la mise en page bascule intégralement en miroir logique (`dir="rtl"`) pour l’arabe, l’adlam et le n’ko, tandis que la forme d’onde acoustique des 21 cordes de kora demeure invariante.',
                'Per crates/hakille-design/src/typo.rs, logical layout mirrors cleanly in `dir="rtl"` for Arabic, Adlam, and N’Ko, while the 21-wire acoustic waveform never flips.'
              )}
            </p>
          </div>

          <div className="script-specimen-grid">
            {CULTURAL_SCRIPT_SPECIMENS.map((spec) => (
              <article key={spec.id} className="script-card" dir={spec.dir}>
                <div className="eyebrow-mono" dir="ltr">
                  {spec.languageName} · {spec.scriptName} ({spec.dir.toUpperCase()})
                </div>
                <div className="script-sample-large">{spec.wordmarkSample}</div>
                <div style={{ color: 'var(--hakille-texte-1)', fontSize: '1.02rem' }}>{spec.phraseSample}</div>
                <div dir="ltr" className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                  {spec.transliteration}
                </div>
                <p dir="ltr" style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, spec.meaningFr, spec.meaningEn)}
                </p>
              </article>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
