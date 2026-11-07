import { FONT_PROVENANCE, LOGO_PROVENANCE } from '@/tokens/hakilleTokens.generated';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

const ALLOWED_SPDX_LICENSES = [
  'Apache-2.0',
  'MIT',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'ISC',
  'Unicode-3.0',
  'Unicode-DFS-2016',
  'Zlib',
  'BSL-1.0',
  'CC0-1.0',
  'SIL OFL-1.1 (Fonts)',
] as const;

export function LicensesPage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">LICENSE-APACHE · DENY.TOML · DEFENSIVE PATENT PLEDGE · SHA-256 PROVENANCE</span>
          <h1 className="display-h1">{t.routes.licenses.title}</h1>
          <p className="hero-lead">{t.routes.licenses.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <div className="comparison-grid" style={{ marginBottom: 0 }}>
          <div className="arch-card" data-emphasis="lueur">
            <span className="eyebrow-mono">SPDX: Apache-2.0</span>
            <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
              Apache License, Version 2.0 + Defensive Patent Grant
            </h2>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
              {pickBi(
                locale,
                'Les 42 crates Rust, les applications compagnons iOS/Android et l’intégralité de ce site web (website/) sont publiés sous licence Apache-2.0 : usage personnel, académique et commercial libre, modification et redistribution autorisées, avec concession explicite de brevets (§3) et résiliation défensive automatique contre tout attaquant en contrefaçon de brevet.',
                'All 42 Rust crates, native iOS/Android companion code, and this entire website (website/) are licensed under Apache-2.0 — granting perpetual copyright and patent licenses (§3) with automatic defensive termination against offensive patent litigation.'
              )}
            </p>
          </div>

          <div className="arch-card">
            <span className="eyebrow-mono">deny.toml · cargo-deny CI Gate</span>
            <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
              {pickBi(locale, 'Liste Blanche Stricte Sans Copyleft Contaminant', 'Strict Copyleft-Free Dependency Whitelist')}
            </h2>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
              {pickBi(
                locale,
                'Chaque pull request est auditée automatiquement par cargo-deny (deny.toml) pour refuser toute dépendance GPL/AGPL ou crate présentant un avis RustSec non résolu.',
                'Every build is audited via cargo-deny (deny.toml) to reject viral copyleft licenses and unpatched RustSec advisories.'
              )}
            </p>
            <div className="pastille-row">
              {ALLOWED_SPDX_LICENSES.map((lic) => (
                <span key={lic} className="pastille">{lic}</span>
              ))}
            </div>
          </div>
        </div>

        {/* Cryptographic Font & Emblem Provenance */}
        <section>
          <h2 className="section-h2" style={{ marginBottom: '16px' }}>
            {pickBi(locale, 'Provenance Cryptographique SHA-256 des Polices & Emblèmes', 'SHA-256 Cryptographic Provenance of Fonts & Brand Assets')}
          </h2>

          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Asset</th>
                  <th>License</th>
                  <th>Version / Size</th>
                  <th>Verified SHA-256 Checksum</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td>
                    <strong style={{ color: 'var(--hakille-texte-1)' }}>{FONT_PROVENANCE.family}</strong>
                    <div className="mono">{FONT_PROVENANCE.localPath}</div>
                  </td>
                  <td><a href="/fonts/OFL.txt">{FONT_PROVENANCE.license}</a></td>
                  <td className="mono">{FONT_PROVENANCE.version} · {(FONT_PROVENANCE.sizeBytes / 1024).toFixed(0)} KB</td>
                  <td className="mono hash-isolate ltr-isolate">{FONT_PROVENANCE.interVariableSha256}</td>
                </tr>
                <tr>
                  <td>
                    <strong style={{ color: 'var(--hakille-texte-1)' }}>SIL OFL-1.1 Full License Text</strong>
                    <div className="mono">/fonts/OFL.txt</div>
                  </td>
                  <td>SIL OFL 1.1</td>
                  <td className="mono">4.3 KB</td>
                  <td className="mono hash-isolate ltr-isolate">{FONT_PROVENANCE.oflLicenseSha256}</td>
                </tr>
                {Object.entries(LOGO_PROVENANCE).map(([filename, hash]) => (
                  <tr key={filename}>
                    <td>
                      <strong style={{ color: 'var(--hakille-texte-1)' }}>{filename}</strong>
                      <div className="mono">/brand/{filename}</div>
                    </td>
                    <td>Hakille Brand Assets (Trademark §6 reserved)</td>
                    <td className="mono">PNG</td>
                    <td className="mono hash-isolate ltr-isolate">{hash}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="arch-card">
          <span className="eyebrow-mono">CODE_OF_CONDUCT.MD · CONTRIBUTING.MD</span>
          <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
            Contributor Covenant v2.1 &amp; DCO
          </h2>
          <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
            {pickBi(
              locale,
              'La communauté Hakille applique le Contributor Covenant v2.1. Toute contribution soumise au dépôt est couverte par la clause « Inbound = Outbound » sous licence Apache-2.0.',
              'The Hakille community upholds Contributor Covenant v2.1. All contributions submitted to the monorepo follow standard Apache-2.0 inbound=outbound licensing.'
            )}
          </p>
        </section>
      </div>
    </div>
  );
}
