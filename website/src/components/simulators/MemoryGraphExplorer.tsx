import { useMemo, useState } from 'react';
import { SpoMemoryNode, evaluateSpoMemoryAtTime } from '@/engine/cognitiveSimulators';
import { useI18n } from '@/i18n/context';

const SAMPLE_SPO_NODES: readonly SpoMemoryNode[] = [
  {
    id: 'm1',
    subject: 'user:self',
    predicate: 'native_heritage',
    object: 'Pulaar & French bilingual',
    partition: 'Personal',
    category: 'identity-core',
    initialWeight: 0.99,
    halfLifeMinutes: 60 * 24 * 365 * 5,
  },
  {
    id: 'm2',
    subject: 'user:self',
    predicate: 'music_preference',
    object: 'Toumani Diabaté · 21-string Kora',
    partition: 'Personal',
    category: 'preference',
    initialWeight: 0.92,
    halfLifeMinutes: 60 * 24 * 240,
  },
  {
    id: 'm3',
    subject: 'cognition:voice_prosody',
    predicate: 'detected_affect',
    object: 'fatigue_mild (valence 0.42)',
    partition: 'Personal',
    category: 'cognition-affective',
    initialWeight: 0.86,
    halfLifeMinutes: 5, // Mandatory 5-min EU AI Act half-life & physical purge
  },
  {
    id: 'm4',
    subject: 'cognition:posture_17pt',
    predicate: 'neck_inclination_deg',
    object: '26° forward lean',
    partition: 'Personal',
    category: 'cognition-affective',
    initialWeight: 0.81,
    halfLifeMinutes: 5,
  },
  {
    id: 'm5',
    subject: 'project:hakille_v0_1',
    predicate: 'architecture_rule',
    object: 'Zero Python · Rust 2024 MSRV 1.95',
    partition: 'Professional',
    category: 'identity-core',
    initialWeight: 0.98,
    halfLifeMinutes: 60 * 24 * 365 * 3,
  },
  {
    id: 'm6',
    subject: 'meeting:board_review',
    predicate: 'temporary_room_code',
    object: 'Salle 4B · 14h00',
    partition: 'Professional',
    category: 'ephemeral-context',
    initialWeight: 0.88,
    halfLifeMinutes: 60 * 24 * 14,
  },
  {
    id: 'm7',
    subject: 'home:thermostat',
    predicate: 'evening_target_celsius',
    object: '19.5 °C',
    partition: 'Shared',
    category: 'preference',
    initialWeight: 0.94,
    halfLifeMinutes: 60 * 24 * 365,
  },
];

const TIME_STOPS_MINUTES = [0, 5, 60 * 24 * 30, 60 * 24 * 730] as const;

export function MemoryGraphExplorer() {
  const { t } = useI18n();
  const [partition, setPartition] = useState<'All' | 'Personal' | 'Professional' | 'Shared'>('All');
  const [stopIndex, setStopIndex] = useState(0);

  const elapsedMinutes = TIME_STOPS_MINUTES[stopIndex] ?? 0;

  const evaluated = useMemo(() => {
    const all = evaluateSpoMemoryAtTime(SAMPLE_SPO_NODES, elapsedMinutes);
    return partition === 'All' ? all : all.filter((n) => n.partition === partition);
  }, [elapsedMinutes, partition]);

  const handleDownloadOpenExport = () => {
    const payload = {
      schema: 'hakille-graph-export',
      version: 1,
      exported_at: new Date().toISOString(),
      constitution_version: '1.0.0',
      simulated_elapsed_minutes: elapsedMinutes,
      triples: evaluated
        .filter((n) => !n.physicallyPurgedNoTombstone)
        .map((n) => ({
          id: n.id,
          subject: n.subject,
          predicate: n.predicate,
          object: n.object,
          partition: n.partition,
          weight: n.currentWeight,
          epistemic_status: n.epistemicTag,
        })),
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'hakille-graph-export-v1.json';
    a.click();
    URL.revokeObjectURL(url);
  };

  const stopLabels = [
    t.memoryLab.timeNow,
    t.memoryLab.time5m,
    t.memoryLab.time30d,
    t.memoryLab.time2y,
  ];

  return (
    <div className="sim-frame">
      <div className="sim-topbar">
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
          {(
            [
              ['All', t.common.filterAll],
              ['Personal', t.memoryLab.partitionPersonal],
              ['Professional', t.memoryLab.partitionPro],
              ['Shared', t.memoryLab.partitionShared],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              className="state-pill"
              aria-pressed={partition === id}
              onClick={() => setPartition(id)}
            >
              {label}
            </button>
          ))}
        </div>

        <button type="button" className="ghost-btn" onClick={handleDownloadOpenExport}>
          {t.memoryLab.exportJsonBtn}
        </button>
      </div>

      <div className="sim-body spo-layout">
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', gap: '12px', flexWrap: 'wrap' }}>
              <label htmlFor="spo-time-slider" style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
                {t.memoryLab.timeSliderLabel}
              </label>
              <span className="mono" style={{ color: 'var(--hakille-lueur)' }}>
                {stopLabels[stopIndex]}
              </span>
            </div>
            <input
              id="spo-time-slider"
              type="range"
              min={0}
              max={3}
              step={1}
              value={stopIndex}
              onChange={(e) => setStopIndex(Number(e.target.value))}
              className="range-input"
            />
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.72rem', color: 'var(--hakille-texte-3)' }}>
              {stopLabels.map((lbl, idx) => (
                <span key={idx}>{lbl}</span>
              ))}
            </div>
          </div>

          {/* Visual SPO Topology SVG */}
          <div className="spo-graph-canvas">
            <svg viewBox="0 0 540 270" width="100%" height="270" role="img" aria-label="SPO Memory Graph">
              <circle cx="110" cy="135" r="28" fill="var(--hakille-surface-2)" stroke="var(--hakille-texte-1)" strokeWidth="1.5" />
              <text x="110" y="139" textAnchor="middle" fill="var(--hakille-texte-1)" fontSize="11" fontFamily="var(--hakille-font-mono)">
                redb::SPO
              </text>

              {evaluated.map((node, i) => {
                const y = 26 + i * (230 / Math.max(1, evaluated.length - 1 || 1));
                const x = 395;
                const opacity = node.physicallyPurgedNoTombstone ? 0.14 : Math.max(0.28, node.currentWeight);

                return (
                  <g key={node.id} opacity={opacity}>
                    <line
                      x1="138"
                      y1="135"
                      x2={x - 95}
                      y2={y}
                      stroke="var(--hakille-lueur)"
                      strokeWidth={node.physicallyPurgedNoTombstone ? '1' : '1.4'}
                      strokeDasharray={node.category === 'cognition-affective' ? '3 3' : undefined}
                    />
                    <rect
                      x={x - 95}
                      y={y - 13}
                      width="225"
                      height="26"
                      rx="6"
                      fill="var(--hakille-surface-1)"
                      stroke="var(--hakille-ligne)"
                    />
                    <text
                      x={x - 85}
                      y={y + 4}
                      fill="var(--hakille-texte-1)"
                      fontSize="10.5"
                      fontFamily="var(--hakille-font-mono)"
                    >
                      {node.physicallyPurgedNoTombstone
                        ? `[PURGED 0B] ${node.subject}`
                        : `${node.predicate} (${node.currentWeight.toFixed(2)})`}
                    </text>
                  </g>
                );
              })}
            </svg>
          </div>

          <div
            style={{
              padding: '12px 16px',
              borderRadius: '8px',
              border: '1px solid var(--hakille-ligne)',
              background: 'var(--hakille-surface-0)',
              fontSize: 'var(--hakille-type-caption-size)',
              color: 'var(--hakille-texte-2)',
            }}
          >
            {t.memoryLab.aiActNotice}
          </div>
        </div>

        {/* Fact list + Digital Twilight */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {evaluated.map((node) => (
            <div
              key={node.id}
              className="spo-fact-row"
              data-purged={node.physicallyPurgedNoTombstone ? 'true' : 'false'}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: '8px' }}>
                <span className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                  {node.partition} · {node.category}
                </span>
                <span className="status-tag">
                  [{node.epistemicTag}] W={node.currentWeight.toFixed(2)}
                </span>
              </div>
              <code style={{ color: 'var(--hakille-texte-1)', fontSize: '0.8rem' }}>
                ({node.subject} → {node.predicate} → &ldquo;{node.object}&rdquo;)
              </code>
            </div>
          ))}

          <div className="arch-card" style={{ marginTop: '8px', padding: '16px' }}>
            <div style={{ fontWeight: 600, fontSize: 'var(--hakille-type-caption-size)' }}>
              {t.memoryLab.digitalTwilightTitle}
            </div>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
              {t.memoryLab.digitalTwilightBody}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
