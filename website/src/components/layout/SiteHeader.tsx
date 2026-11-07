import { useState } from 'react';
import { ROUTES_LIST } from '@/config/routes';
import { LEGAL_CONFIG } from '@/config/legal';
import { LOCALE_METADATA, LocaleCode, SUPPORTED_LOCALES } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { usePreferences } from '@/context/PreferencesContext';
import { AppLink, useRouter } from '@/context/RouterContext';

interface SiteHeaderProps {
  readonly onOpenSpotlight: () => void;
  readonly onOpenA11y: () => void;
}

export function SiteHeader({ onOpenSpotlight, onOpenA11y }: SiteHeaderProps) {
  const { locale, setLocale, t } = useI18n();
  const { theme, toggleTheme, soundEnabled, toggleSound } = usePreferences();
  const { currentPath } = useRouter();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  const productRoutes = ROUTES_LIST.filter((r) => r.group === 'product');
  const logoSrc = theme === 'dark' ? '/brand/hakille-logo-dark.png' : '/brand/hakille-logo.png';

  return (
    <header className="site-header no-print">
      <a href="#main-content" className="skip-link">
        {t.meta.skipToContent}
      </a>
      <div className="container header-inner">
        <AppLink
          to="/"
          className="brand-link"
          onClickAfter={() => setMobileMenuOpen(false)}
        >
          <img
            src={logoSrc}
            alt="hakille emblem"
            width={32}
            height={32}
            className="brand-emblem"
          />
          <span className="brand-stack">
            <span className="hakille-wordmark brand-word">{LEGAL_CONFIG.brand.wordmark}</span>
            <span className="brand-sub">
              {t.meta.brandSignature.replace(/^Hakille\s*—\s*/i, '')}
            </span>
          </span>
        </AppLink>

        <nav className="primary-nav" aria-label="Primary">
          {productRoutes.map((route) => (
            <AppLink
              key={route.path}
              to={route.path}
              className="nav-link"
              ariaCurrent={currentPath === route.path ? 'page' : undefined}
            >
              {t.routes[route.id].navLabel}
            </AppLink>
          ))}
        </nav>

        <div className="header-actions">
          <button
            type="button"
            className="ghost-btn"
            onClick={onOpenSpotlight}
            title={t.header.spotlightShortcut}
          >
            <span>{t.header.spotlightTrigger}</span>
            <kbd className="kbd-chip hide-tablet">Ctrl+K</kbd>
          </button>

          <button
            type="button"
            className="ghost-btn hide-tablet"
            aria-pressed={soundEnabled}
            onClick={toggleSound}
          >
            <span aria-hidden="true">{soundEnabled ? '◉' : '○'}</span>
            <span>48 kHz</span>
          </button>

          {/* Explicit text label per DESIGN_MOBILE_2026-11-04.md: never replace an important action label with a bare icon */}
          <button
            type="button"
            className="ghost-btn hide-tablet"
            onClick={toggleTheme}
          >
            <span aria-hidden="true">{theme === 'dark' ? '◐' : '◑'}</span>
            <span>{theme === 'dark' ? t.header.themeDark : t.header.themeLight}</span>
          </button>

          <select
            className="lang-select"
            aria-label={t.header.languageLabel}
            value={locale}
            onChange={(e) => setLocale(e.target.value as LocaleCode)}
          >
            {SUPPORTED_LOCALES.map((code) => (
              <option key={code} value={code}>
                {LOCALE_METADATA[code].nativeName}
              </option>
            ))}
          </select>

          <button
            type="button"
            className="ghost-btn hide-tablet"
            onClick={onOpenA11y}
          >
            <span>{t.header.accessibilityBtn}</span>
          </button>

          <button
            type="button"
            className="ghost-btn mobile-menu-toggle"
            aria-expanded={mobileMenuOpen}
            aria-label={mobileMenuOpen ? t.header.menuClose : t.header.menuOpen}
            onClick={() => setMobileMenuOpen((v) => !v)}
          >
            <span>{mobileMenuOpen ? '✕ Menu' : '☰ Menu'}</span>
          </button>
        </div>
      </div>

      {mobileMenuOpen && (
        <div className="mobile-nav-panel">
          <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div className="mobile-nav-grid">
              {ROUTES_LIST.map((route) => (
                <AppLink
                  key={route.path}
                  to={route.path}
                  className="nav-link"
                  ariaCurrent={currentPath === route.path ? 'page' : undefined}
                  onClickAfter={() => setMobileMenuOpen(false)}
                >
                  {t.routes[route.id].navLabel}
                </AppLink>
              ))}
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
              <button
                type="button"
                className="ghost-btn"
                aria-pressed={soundEnabled}
                onClick={toggleSound}
              >
                {soundEnabled ? t.header.audioOn : t.header.audioOff}
              </button>
              <button type="button" className="ghost-btn" onClick={toggleTheme}>
                {theme === 'dark' ? t.header.themeDark : t.header.themeLight}
              </button>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => {
                  setMobileMenuOpen(false);
                  onOpenA11y();
                }}
              >
                {t.header.accessibilityBtn}
              </button>
            </div>
          </div>
        </div>
      )}
    </header>
  );
}
