import { useEffect } from 'react';
import { LOCALE_METADATA, LocaleCode, SUPPORTED_LOCALES } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { usePreferences } from '@/context/PreferencesContext';

interface AccessibilityDrawerProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

export function AccessibilityDrawer({ open, onClose }: AccessibilityDrawerProps) {
  const { locale, setLocale, t } = useI18n();
  const {
    theme,
    setTheme,
    contrast,
    setContrast,
    textScale,
    setTextScale,
    motion,
    setMotion,
    soundEnabled,
    toggleSound,
    resetAllLocalData,
  } = usePreferences();

  useEffect(() => {
    if (!open || typeof window === 'undefined') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="drawer-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={t.a11yDrawer.title}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="drawer-panel">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: '12px' }}>
          <div>
            <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
              {t.a11yDrawer.title}
            </h2>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)', marginTop: '4px' }}>
              {t.a11yDrawer.subtitle}
            </p>
          </div>
          <button type="button" className="ghost-btn" onClick={onClose}>
            {t.a11yDrawer.closeBtn}
          </button>
        </div>

        <div className="drawer-group">
          <span style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.header.languageLabel}
          </span>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
            {SUPPORTED_LOCALES.map((code) => (
              <button
                key={code}
                type="button"
                className="ghost-btn"
                aria-pressed={locale === code}
                onClick={() => setLocale(code as LocaleCode)}
              >
                {LOCALE_METADATA[code].nativeName}
              </button>
            ))}
          </div>
        </div>

        <div className="drawer-group">
          <span style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.a11yDrawer.themeLabel}
          </span>
          <div className="segmented-row">
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={theme === 'dark'}
              onClick={() => setTheme('dark')}
            >
              {t.header.themeDark} (#0B0C0F)
            </button>
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={theme === 'light'}
              onClick={() => setTheme('light')}
            >
              {t.header.themeLight} (#F6F4F0)
            </button>
          </div>
        </div>

        <div className="drawer-group">
          <span style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.a11yDrawer.contrastLabel}
          </span>
          <div className="segmented-row">
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={contrast === 'normal'}
              onClick={() => setContrast('normal')}
            >
              {t.a11yDrawer.contrastNormal}
            </button>
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={contrast === 'high'}
              onClick={() => setContrast('high')}
            >
              {t.a11yDrawer.contrastHigh}
            </button>
          </div>
        </div>

        <div className="drawer-group">
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <label htmlFor="a11y-scale-range" style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
              {t.a11yDrawer.textScaleLabel}
            </label>
            <span className="mono">{Math.round(textScale * 100)}%</span>
          </div>
          <input
            id="a11y-scale-range"
            type="range"
            min={0.8}
            max={2.0}
            step={0.05}
            value={textScale}
            onChange={(e) => setTextScale(Number(e.target.value))}
            className="range-input"
          />
        </div>

        <div className="drawer-group">
          <span style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.a11yDrawer.motionLabel}
          </span>
          <div className="segmented-row">
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={motion === 'normal'}
              onClick={() => setMotion('normal')}
            >
              {t.a11yDrawer.motionNormal}
            </button>
            <button
              type="button"
              className="ghost-btn"
              aria-pressed={motion === 'reduced'}
              onClick={() => setMotion('reduced')}
            >
              {t.a11yDrawer.motionReduced}
            </button>
          </div>
        </div>

        <div className="drawer-group">
          <span style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.a11yDrawer.soundLabel}
          </span>
          <button
            type="button"
            className="ghost-btn"
            aria-pressed={soundEnabled}
            onClick={toggleSound}
          >
            {soundEnabled ? t.header.audioOn : t.header.audioOff}
          </button>
          <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {t.a11yDrawer.soundHint}
          </p>
        </div>

        <button
          type="button"
          className="secondary-outline-btn"
          onClick={() => {
            resetAllLocalData();
            setLocale('fr');
          }}
        >
          {t.a11yDrawer.resetBtn}
        </button>
      </div>
    </div>
  );
}
