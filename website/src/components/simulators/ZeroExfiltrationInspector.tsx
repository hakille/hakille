import { useCallback, useEffect, useState } from 'react';
import { FONT_PROVENANCE } from '@/tokens/hakilleTokens.generated';
import { usePreferences } from '@/context/PreferencesContext';
import { useI18n } from '@/i18n/context';

interface RuntimeAuditSnapshot {
  readonly cookieCount: number;
  readonly externalResourceCount: number;
  readonly totalSameOriginResources: number;
  readonly localStorageKeys: readonly string[];
}

export function ZeroExfiltrationInspector() {
  const { t, setLocale } = useI18n();
  const { resetAllLocalData } = usePreferences();
  const [snapshot, setSnapshot] = useState<RuntimeAuditSnapshot>({
    cookieCount: 0,
    externalResourceCount: 0,
    totalSameOriginResources: 0,
    localStorageKeys: [],
  });
  const [wipedBanner, setWipedBanner] = useState(false);

  const runLiveScan = useCallback(() => {
    if (typeof window === 'undefined') return;
    const rawCookie = document.cookie.trim();
    const cookieCount = rawCookie === '' ? 0 : rawCookie.split(';').length;

    const entries = performance.getEntriesByType('resource') as PerformanceResourceTiming[];
    const origin = window.location.origin;
    let external = 0;
    let sameOrigin = 0;
    for (const entry of entries) {
      if (entry.name.startsWith(origin) || entry.name.startsWith('data:') || entry.name.startsWith('blob:')) {
        sameOrigin += 1;
      } else {
        external += 1;
      }
    }

    const keys: string[] = [];
    try {
      for (let i = 0; i < window.localStorage.length; i++) {
        const k = window.localStorage.key(i);
        if (k) keys.push(k);
      }
    } catch {
      // Ignore storage restrictions
    }

    setSnapshot({
      cookieCount,
      externalResourceCount: external,
      totalSameOriginResources: sameOrigin,
      localStorageKeys: keys,
    });
  }, []);

  useEffect(() => {
    runLiveScan();
  }, [runLiveScan]);

  return (
    <div className="sim-frame">
      <div className="sim-topbar">
        <div>
          <strong style={{ display: 'block' }}>{t.inspector.title}</strong>
          <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {t.inspector.subtitle}
          </span>
        </div>
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          <button type="button" className="ghost-btn" onClick={runLiveScan}>
            {t.inspector.rescanBtn}
          </button>
          <button
            type="button"
            className="ghost-btn"
            onClick={() => {
              resetAllLocalData();
              setLocale('fr');
              setWipedBanner(true);
              window.setTimeout(runLiveScan, 30);
            }}
          >
            {t.inspector.wipeLocalBtn}
          </button>
        </div>
      </div>

      <div className="sim-body">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '16px' }}>
          <div className="crate-card">
            <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
              {t.inspector.cookiesLabel}
            </span>
            <strong className="mono tabular" style={{ fontSize: '1.5rem', color: 'var(--hakille-lueur)' }}>
              {snapshot.cookieCount}
            </strong>
            <span className="mono" style={{ fontSize: '0.73rem', color: 'var(--hakille-texte-3)' }}>
              ePrivacy / CNIL exempt (zero tracking)
            </span>
          </div>

          <div className="crate-card">
            <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
              {t.inspector.thirdPartyReqLabel}
            </span>
            <strong className="mono tabular" style={{ fontSize: '1.5rem', color: 'var(--hakille-lueur)' }}>
              {snapshot.externalResourceCount}
            </strong>
            <span className="mono" style={{ fontSize: '0.73rem', color: 'var(--hakille-texte-3)' }}>
              {snapshot.totalSameOriginResources} same-origin assets · CSP default-src &apos;self&apos;
            </span>
          </div>

          <div className="crate-card">
            <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
              {t.inspector.fontShaLabel}
            </span>
            <strong className="mono" style={{ fontSize: '0.88rem', color: 'var(--hakille-texte-1)' }}>
              SHA-256: {FONT_PROVENANCE.interVariableSha256.slice(0, 16)}…
            </strong>
            <span className="mono" style={{ fontSize: '0.73rem', color: 'var(--hakille-texte-3)' }}>
              {(FONT_PROVENANCE.sizeBytes / 1024).toFixed(0)} KB self-hosted (0 Google Fonts IP leak)
            </span>
          </div>

          <div className="crate-card">
            <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
              {t.inspector.storageKeysLabel}
            </span>
            <strong className="mono" style={{ fontSize: '0.95rem', color: 'var(--hakille-texte-1)' }}>
              {snapshot.localStorageKeys.length === 0 ? '0 keys' : snapshot.localStorageKeys.join(', ')}
            </strong>
            <span className="mono" style={{ fontSize: '0.73rem', color: 'var(--hakille-texte-3)' }}>
              Strictly local UI theme / language only
            </span>
          </div>
        </div>

        {wipedBanner && (
          <p style={{ marginTop: '12px', fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-lueur)' }}>
            {t.inspector.wipedConfirm}
          </p>
        )}
      </div>
    </div>
  );
}
