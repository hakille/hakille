import { useMemo, useState } from 'react';
import { LEGAL_CONFIG } from '@/config/legal';
import { CONSTITUTION_ARTICLES, CONSTITUTION_RULES } from '@/data/constitutionData';
import {
  ConstitutionalDomain,
  DOMAIN_THRESHOLDS,
  evaluateConstitutionalConfidence,
} from '@/engine/cognitiveSimulators';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

export function ConstitutionPage() {
  const { locale, t } = useI18n();
  const [domain, setDomain] = useState<ConstitutionalDomain>('medical');
  const [modelConf, setModelConf] = useState(0.88);
  const [groundingConf, setGroundingConf] = useState(0.84);

  const evalResult = useMemo(
    () => evaluateConstitutionalConfidence(modelConf, groundingConf, domain),
    [domain, groundingConf, modelConf]
  );

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">
            CONSTITUTION/V1.0.MD · VERSION {LEGAL_CONFIG.software.constitutionVersion} · QUORUM &gt;= 5
          </span>
          <h1 className="display-h1">{t.routes.constitution.title}</h1>
          <p className="hero-lead">{t.routes.constitution.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        {/* Interactive Confidence Threshold Calculator (Article III) */}
        <section className="sim-shell">
          <div className="sim-header">
            <strong>
              {pickBi(
                locale,
                'Article III · Calculateur ConfidenceEngine (C = 0,6 × M + 0,4 × G)',
                'Article III · ConfidenceEngine Calculator (C = 0.6 × M + 0.4 × G)'
              )}
            </strong>
            <span className="status-tag">{evalResult.decision}</span>
          </div>
          <div className="sim-body comparison-grid" style={{ marginBottom: 0 }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
              <div>
                <label
                  htmlFor="const-domain-select"
                  style={{ display: 'block', fontSize: 'var(--hakille-type-caption-size)', marginBottom: '6px' }}
                >
                  {pickBi(
                    locale,
                    'Domaine cible & Seuil constitutionnel requis',
                    'Target Domain & Constitutional Threshold'
                  )}
                </label>
                <select
                  id="const-domain-select"
                  className="lang-select"
                  style={{ width: '100%' }}
                  value={domain}
                  onChange={(e) => setDomain(e.target.value as ConstitutionalDomain)}
                >
                  {(Object.keys(DOMAIN_THRESHOLDS) as ConstitutionalDomain[]).map((d) => (
                    <option key={d} value={d}>
                      {d.toUpperCase()} —{' '}
                      {pickBi(locale, 'Seuil requis :', 'Required threshold:')}{' '}
                      {DOMAIN_THRESHOLDS[d].toFixed(2)}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <label htmlFor="model-conf-range" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
                    {pickBi(locale, 'Confiance du modèle (M)', 'Model Confidence (M)')}
                  </label>
                  <span className="mono tabular">{modelConf.toFixed(2)}</span>
                </div>
                <input
                  id="model-conf-range"
                  type="range"
                  min={0.2}
                  max={1.0}
                  step={0.01}
                  value={modelConf}
                  onChange={(e) => setModelConf(Number(e.target.value))}
                  style={{ width: '100%' }}
                />
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <label htmlFor="ground-conf-range" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
                    {pickBi(
                      locale,
                      'Ancrage factuel graphe SPO / sources (G)',
                      'SPO Memory / Source Grounding (G)'
                    )}
                  </label>
                  <span className="mono tabular">{groundingConf.toFixed(2)}</span>
                </div>
                <input
                  id="ground-conf-range"
                  type="range"
                  min={0.2}
                  max={1.0}
                  step={0.01}
                  value={groundingConf}
                  onChange={(e) => setGroundingConf(Number(e.target.value))}
                  style={{ width: '100%' }}
                />
              </div>
            </div>

            <div className="arch-card" style={{ justifyContent: 'center' }}>
              <div className="mono tabular" style={{ fontSize: '0.84rem', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                <div>
                  composite_C :{' '}
                  <strong style={{ color: 'var(--hakille-lueur)', fontSize: '1.25rem' }}>
                    {evalResult.compositeScore.toFixed(3)}
                  </strong>
                </div>
                <div>
                  min_threshold : {evalResult.threshold.toFixed(2)} ({domain})
                </div>
                <div>
                  disagreement : {String(evalResult.internalDisagreement)} (|M - G| &gt;= 0.35)
                </div>
                <div>
                  outcome : <strong>{evalResult.decision}</strong>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* 7 Articles */}
        <section>
          <h2 className="section-h2" style={{ marginBottom: '20px' }}>
            {pickBi(locale, 'Les 7 Articles Fondamentaux', 'The 7 Foundational Articles')}
          </h2>
          <div className="crate-grid">
            {CONSTITUTION_ARTICLES.map((art) => (
              <article key={art.number} className="crate-item">
                <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                  {art.number}
                </span>
                <strong style={{ color: 'var(--hakille-texte-1)' }}>
                  {pickBi(locale, art.titleFr, art.titleEn)}
                </strong>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, art.summaryFr, art.summaryEn)}
                </p>
              </article>
            ))}
          </div>
        </section>

        {/* 10 Machine-Verifiable Rules */}
        <section>
          <h2 className="section-h2" style={{ marginBottom: '20px' }}>
            {pickBi(
              locale,
              'Les 10 Règles Machine-Vérifiables (hakille-constitution)',
              'The 10 Machine-Verifiable Rules (hakille-constitution)'
            )}
          </h2>
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>{pickBi(locale, 'Règle', 'Rule')}</th>
                  <th>Article</th>
                  <th>{pickBi(locale, 'Sévérité', 'Severity')}</th>
                  <th>{pickBi(locale, 'Titre & Invariant', 'Title & Invariant')}</th>
                  <th>{pickBi(locale, 'Crate d’Application', 'Enforcement Crate')}</th>
                </tr>
              </thead>
              <tbody>
                {CONSTITUTION_RULES.map((r) => (
                  <tr key={r.id}>
                    <td className="mono">
                      <strong style={{ color: 'var(--hakille-lueur)' }}>{r.code}</strong>
                    </td>
                    <td className="mono">{r.article}</td>
                    <td>
                      <span className="status-tag">{r.severity}</span>
                    </td>
                    <td>
                      <strong style={{ color: 'var(--hakille-texte-1)', display: 'block' }}>
                        {pickBi(locale, r.titleFr, r.titleEn)}
                      </strong>
                      <span>{pickBi(locale, r.invariantFr, r.invariantEn)}</span>
                    </td>
                    <td className="mono">{r.enforcementCrate}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* Human Crisis Helplines (RULE-05) */}
        <section className="summary-callout">
          <div className="eyebrow-mono">
            {pickBi(
              locale,
              'RULE-05 · PROTOCOLE CALME D’URGENCE HUMAINE',
              'RULE-05 · CALM HUMAN DISTRESS PROTOCOL'
            )}
          </div>
          <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
            {pickBi(
              locale,
              'Hakille ne remplace jamais le soin humain',
              'Hakille Never Substitutes for Human Care'
            )}
          </h2>
          <div className="crate-grid">
            {LEGAL_CONFIG.crisisHotlines.map((h) => (
              <div key={h.region} className="crate-item">
                <span className="eyebrow-mono">{h.region}</span>
                <strong className="mono tabular" style={{ fontSize: '1.25rem', color: 'var(--hakille-lueur)' }}>
                  {h.number}
                </strong>
                <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {h.label}
                </span>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
