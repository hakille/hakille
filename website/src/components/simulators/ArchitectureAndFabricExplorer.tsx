import { useEffect, useMemo, useState } from 'react';
import {
  ARCHITECTURE_LAYERS,
  ArchitectureLayerId,
  CRATES_CATALOG,
} from '@/data/cratesCatalog';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';

export function ArchitectureAndFabricExplorer() {
  const { locale, t } = useI18n();
  const [tab, setTab] = useState<'layers' | 'crates' | 'system2'>('layers');
  const [selectedLayer, setSelectedLayer] = useState<ArchitectureLayerId | 'all'>('all');
  const [search, setSearch] = useState('');

  // System 2 Biscuit 300s Lease Simulator
  const [leaseRemainingSec, setLeaseRemainingSec] = useState<number>(0);
  const [outboundBytes, setOutboundBytes] = useState<number>(0);
  const [costCents, setCostCents] = useState<number>(0);

  useEffect(() => {
    if (leaseRemainingSec <= 0) return;
    const id = window.setInterval(() => {
      setLeaseRemainingSec((s) => Math.max(0, s - 1));
    }, 1000);
    return () => window.clearInterval(id);
  }, [leaseRemainingSec]);

  const filteredCrates = useMemo(() => {
    const q = search.trim().toLowerCase();
    return CRATES_CATALOG.filter((c) => {
      if (selectedLayer !== 'all' && c.layer !== selectedLayer) return false;
      if (!q) return true;
      return (
        c.name.toLowerCase().includes(q) ||
        c.keyTech.toLowerCase().includes(q) ||
        c.summaryFr.toLowerCase().includes(q) ||
        c.summaryEn.toLowerCase().includes(q)
      );
    });
  }, [search, selectedLayer]);

  return (
    <div className="sim-frame">
      <div className="sim-topbar">
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
          {(
            [
              ['layers', t.fabricLab.layersTab],
              ['crates', t.fabricLab.cratesTab],
              ['system2', t.fabricLab.system2Tab],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              className="state-pill"
              aria-pressed={tab === id}
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </div>

        <span className="mono" style={{ color: 'var(--hakille-texte-3)', fontSize: '0.75rem' }}>
          Cargo.toml · resolver = &quot;3&quot; · edition = &quot;2024&quot;
        </span>
      </div>

      <div className="sim-body">
        {tab === 'layers' && (
          <div className="layer-stack">
            {[...ARCHITECTURE_LAYERS].reverse().map((layer) => {
              const cratesInLayer = CRATES_CATALOG.filter((c) => c.layer === layer.id);
              const layerLoc = cratesInLayer.reduce((s, c) => s + c.loc, 0);
              const layerTests = cratesInLayer.reduce((s, c) => s + c.tests, 0);

              return (
                <button
                  key={layer.id}
                  type="button"
                  className="layer-band"
                  onClick={() => {
                    setSelectedLayer(layer.id);
                    setTab('crates');
                  }}
                >
                  <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                    {layer.code}
                  </span>
                  <div>
                    <div style={{ fontWeight: 600, color: 'var(--hakille-texte-1)' }}>
                      {pickBi(locale, layer.nameFr, layer.nameEn)}
                    </div>
                    <div style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)', marginTop: '2px' }}>
                      {pickBi(locale, layer.thesisFr, layer.thesisEn)}
                    </div>
                  </div>
                  <div className="mono" style={{ fontSize: '0.75rem', color: 'var(--hakille-texte-3)', textAlign: 'end' }}>
                    <div>{cratesInLayer.length} crates</div>
                    <div>{layerLoc.toLocaleString('en-US')} LoC · {layerTests} tests</div>
                  </div>
                </button>
              );
            })}
          </div>
        )}

        {tab === 'crates' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '12px', justifyContent: 'space-between' }}>
              <input
                type="search"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t.fabricLab.searchCratesPlaceholder}
                style={{
                  flex: '1 1 280px',
                  minHeight: '38px',
                  paddingInline: '12px',
                  borderRadius: '8px',
                  border: '1px solid var(--hakille-ligne)',
                  background: 'var(--hakille-surface-0)',
                }}
              />
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                <button
                  type="button"
                  className="state-pill"
                  aria-pressed={selectedLayer === 'all'}
                  onClick={() => setSelectedLayer('all')}
                >
                  {t.fabricLab.allLayersFilter}
                </button>
                {ARCHITECTURE_LAYERS.map((l) => (
                  <button
                    key={l.id}
                    type="button"
                    className="state-pill"
                    aria-pressed={selectedLayer === l.id}
                    onClick={() => setSelectedLayer(l.id)}
                  >
                    L{l.id}
                  </button>
                ))}
              </div>
            </div>

            <div className="crates-grid">
              {filteredCrates.map((crate) => (
                <article key={crate.name} className="crate-card">
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '8px' }}>
                    <code style={{ fontWeight: 600, color: 'var(--hakille-lueur)' }}>{crate.name}</code>
                    <span className="status-tag">L{crate.layer} · {crate.readiness}</span>
                  </div>
                  <div className="mono" style={{ fontSize: '0.73rem', color: 'var(--hakille-texte-3)' }}>
                    {crate.keyTech}
                  </div>
                  <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)', flex: 1 }}>
                    {pickBi(locale, crate.summaryFr, crate.summaryEn)}
                  </p>
                  <div
                    className="mono"
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      paddingTop: '8px',
                      borderTop: '1px solid var(--hakille-ligne)',
                      fontSize: '0.73rem',
                      color: 'var(--hakille-texte-3)',
                    }}
                  >
                    <span>{crate.loc.toLocaleString('en-US')} LoC</span>
                    <span>{crate.tests} #[test]</span>
                    <span>{crate.rsFiles} .rs</span>
                  </div>
                </article>
              ))}
            </div>
          </div>
        )}

        {tab === 'system2' && (
          <div className="comparison-grid" style={{ marginBottom: 0 }}>
            <div className="arch-card">
              <div className="eyebrow-mono">manifests/native.toml</div>
              <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                {t.fabricLab.localSystem1Title}
              </h3>
              <pre
                style={{
                  padding: '12px',
                  borderRadius: '8px',
                  background: 'var(--hakille-surface-0)',
                  border: '1px solid var(--hakille-ligne)',
                  overflowX: 'auto',
                  fontSize: '0.76rem',
                  color: 'var(--hakille-texte-2)',
                }}
              >{`[fabric.system_1]
model = "qwen2.5-0.5b-instruct-q4km"
runtime = "candle-gguf"
network = "forbidden"
cost_per_token_eur = 0.0`}</pre>
              <div className="pastille-row">
                <span className="pastille">0,0000 €</span>
                <span className="pastille">100% Offline</span>
                <span className="pastille">RAM ~550 MB</span>
              </div>
            </div>

            <div className="arch-card" data-emphasis="lueur">
              <div className="eyebrow-mono">RULE-09 · Biscuit Datalog Lease</div>
              <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                {t.fabricLab.remoteSystem2Title}
              </h3>

              <div
                className="mono tabular"
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(3, 1fr)',
                  gap: '12px',
                  padding: '12px',
                  borderRadius: '8px',
                  background: 'var(--hakille-surface-0)',
                  border: '1px solid var(--hakille-ligne)',
                }}
              >
                <div>
                  <div style={{ fontSize: '0.7rem', color: 'var(--hakille-texte-3)' }}>TTL</div>
                  <div style={{ fontSize: '1.1rem', color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                    {leaseRemainingSec}s
                  </div>
                </div>
                <div>
                  <div style={{ fontSize: '0.7rem', color: 'var(--hakille-texte-3)' }}>EGRESS</div>
                  <div style={{ fontSize: '1.1rem', fontWeight: 600 }}>
                    {(outboundBytes / 1024).toFixed(1)} KB
                  </div>
                </div>
                <div>
                  <div style={{ fontSize: '0.7rem', color: 'var(--hakille-texte-3)' }}>TAXI-METER</div>
                  <div style={{ fontSize: '1.1rem', fontWeight: 600 }}>
                    €{(costCents / 100).toFixed(3)}
                  </div>
                </div>
              </div>

              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() => setLeaseRemainingSec(300)}
                >
                  {t.fabricLab.issueBiscuitBtn}
                </button>
                <button
                  type="button"
                  className="ghost-btn"
                  disabled={leaseRemainingSec <= 0}
                  onClick={() => {
                    if (leaseRemainingSec <= 0) return;
                    setOutboundBytes((b) => b + 1840);
                    setCostCents((c) => Number((c + 0.4).toFixed(2)));
                  }}
                >
                  {t.fabricLab.simulateStreamBtn}
                </button>
                <button
                  type="button"
                  className="ghost-btn"
                  disabled={leaseRemainingSec <= 0}
                  onClick={() => setLeaseRemainingSec(0)}
                >
                  {t.fabricLab.revokeTokenBtn}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
