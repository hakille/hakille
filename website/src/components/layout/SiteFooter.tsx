import { ROUTES_LIST } from '@/config/routes';
import { LEGAL_CONFIG } from '@/config/legal';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { useI18n } from '@/i18n/context';
import { usePreferences } from '@/context/PreferencesContext';
import { AppLink } from '@/context/RouterContext';

export function SiteFooter() {
  const { t } = useI18n();
  const { theme } = usePreferences();

  const productRoutes = ROUTES_LIST.filter((r) => r.group === 'product');
  const legalRoutes = ROUTES_LIST.filter((r) => r.group === 'legal');
  const logoSrc = theme === 'dark' ? '/brand/hakille-logo-dark.png' : '/brand/hakille-logo.png';

  return (
    <footer className="site-footer">
      <div className="container">
        <div className="footer-grid">
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <img src={logoSrc} alt="" width={28} height={28} className="brand-emblem" />
              <span className="hakille-wordmark" style={{ fontSize: '1.15rem' }}>
                {LEGAL_CONFIG.brand.wordmark}
              </span>
            </div>
            <p style={{ color: 'var(--hakille-texte-1)', fontWeight: 500 }}>
              {t.footer.manifestoLine}
            </p>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
              {t.footer.etymologyNote}
            </p>
            <div style={{ paddingTop: '4px' }}>
              <span className="status-tag">{t.footer.zeroExfiltrationBadge}</span>
            </div>
          </div>

          <div>
            <h2 className="footer-col-title">{t.footer.productHeading}</h2>
            <ul className="footer-list">
              {productRoutes.map((route) => (
                <li key={route.path}>
                  <AppLink to={route.path}>{t.routes[route.id].navLabel}</AppLink>
                </li>
              ))}
              <li>
                <a href={LEGAL_CONFIG.domains.repository} rel="noopener noreferrer">
                  GitHub · hakille/hakille ↗
                </a>
              </li>
            </ul>
          </div>

          <div>
            <h2 className="footer-col-title">{t.footer.trustHeading}</h2>
            <ul className="footer-list">
              {legalRoutes.map((route) => (
                <li key={route.path}>
                  <AppLink to={route.path}>{t.routes[route.id].navLabel}</AppLink>
                </li>
              ))}
              <li>
                <a href="/.well-known/security.txt">RFC 9116 · security.txt</a>
              </li>
            </ul>
          </div>

          <div>
            <h2 className="footer-col-title">{t.footer.engineeringHeading}</h2>
            <ul className="footer-list mono">
              <li>{WORKSPACE_METRICS.totalPackages} Cargo crates · Edition {WORKSPACE_METRICS.edition}</li>
              <li>{WORKSPACE_METRICS.totalRustLoc.toLocaleString('en-US')} Rust LoC · MSRV {WORKSPACE_METRICS.msrv}</li>
              <li>{WORKSPACE_METRICS.totalAutomatedTests.toLocaleString('en-US')} #[test] · {WORKSPACE_METRICS.adrCount} ADRs</li>
              <li>0% Python · 0% Electron/WebView</li>
              <li>{LEGAL_CONFIG.contacts.securityEmail}</li>
            </ul>
          </div>
        </div>

        <div className="footer-bottom">
          <div style={{ maxWidth: '72ch' }}>{t.footer.preAlphaDisclaimer}</div>
          <div>{t.footer.copyrightLine}</div>
        </div>
      </div>
    </footer>
  );
}
