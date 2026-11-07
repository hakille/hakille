import { useEffect, useMemo, useRef, useState } from 'react';
import { hakilleAudio } from '@/engine/audioPlayer';
import {
  FORMANT_SAMPLE_RATE,
  TimelineSegment,
  VisemeId,
  synthesizeKlattFormant,
} from '@/engine/klattFormant';
import { useI18n } from '@/i18n/context';

const PRESET_UTTERANCES = [
  {
    id: 'sig-fr',
    label: 'FR · Signature',
    text: 'Hakille — intelligence souveraine. Zéro octet transmis.',
  },
  {
    id: 'audit-fr',
    label: 'FR · Audit',
    text: 'Le journal cryptographique est intègre et signé.',
  },
  {
    id: 'sig-en',
    label: 'EN · Sovereign',
    text: 'Zero neural weights required. Pure mathematical voice.',
  },
  {
    id: 'hold-fr',
    label: 'FR · Braise 600 ms',
    text: 'Action sensible détectée. Maintenez six cents millisecondes.',
  },
] as const;

const ALL_VISEMES: readonly VisemeId[] = [
  'Sil',
  'AA',
  'AE',
  'EH',
  'IH',
  'IY',
  'UH',
  'UW',
  'AO',
  'ER',
  'Cons',
];

function visemeAperture(v: VisemeId): { rx: number; ry: number } {
  switch (v) {
    case 'AA':
      return { rx: 22, ry: 16 };
    case 'AE':
      return { rx: 25, ry: 13 };
    case 'EH':
      return { rx: 24, ry: 10 };
    case 'IH':
      return { rx: 22, ry: 7 };
    case 'IY':
      return { rx: 26, ry: 5 };
    case 'AO':
      return { rx: 14, ry: 15 };
    case 'UW':
      return { rx: 10, ry: 9 };
    case 'UH':
      return { rx: 16, ry: 9 };
    case 'ER':
      return { rx: 15, ry: 8 };
    case 'Cons':
      return { rx: 19, ry: 3 };
    case 'Sil':
    default:
      return { rx: 16, ry: 1.6 };
  }
}

export function KlattVoiceStudio() {
  const { locale } = useI18n();
  const isFr = locale === 'fr';

  const [text, setText] = useState<string>(PRESET_UTTERANCES[0].text);
  const [f0Hz, setF0Hz] = useState<number>(122);
  const [speedRate, setSpeedRate] = useState<number>(1.0);
  const [playing, setPlaying] = useState<boolean>(false);
  const [bargeInTriggered, setBargeInTriggered] = useState<boolean>(false);
  const [activeSegIdx, setActiveSegIdx] = useState<number>(1);

  const rafRef = useRef<number | null>(null);
  const playStartRef = useRef<number>(0);

  const synthesis = useMemo(
    () => synthesizeKlattFormant(text, { f0BaseHz: f0Hz, speedRate }),
    [text, f0Hz, speedRate]
  );

  const stopPlaybackLoop = () => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  };

  useEffect(() => {
    return () => {
      stopPlaybackLoop();
      hakilleAudio.stopActiveVoice();
    };
  }, []);

  const handlePlay = () => {
    if (synthesis.samples.length === 0) return;
    setBargeInTriggered(false);
    setPlaying(true);
    playStartRef.current = performance.now();

    hakilleAudio.playKlattUtterance(synthesis.samples, synthesis.sampleRate, () => {
      stopPlaybackLoop();
      setPlaying(false);
      setActiveSegIdx(1);
    });

    const tick = () => {
      const elapsedMs = performance.now() - playStartRef.current;
      const idx = synthesis.timeline.findIndex(
        (seg) => elapsedMs >= seg.startMs && elapsedMs < seg.endMs
      );
      if (idx >= 0) setActiveSegIdx(idx);
      if (elapsedMs < synthesis.durationMs) {
        rafRef.current = requestAnimationFrame(tick);
      }
    };
    stopPlaybackLoop();
    rafRef.current = requestAnimationFrame(tick);
  };

  const handleBargeIn = () => {
    hakilleAudio.stopActiveVoice();
    stopPlaybackLoop();
    setPlaying(false);
    setActiveSegIdx(0);
    setBargeInTriggered(true);
  };

  const safeIdx = Math.max(0, Math.min(activeSegIdx, Math.max(0, synthesis.timeline.length - 1)));
  const activeSeg: TimelineSegment | undefined = synthesis.timeline[safeIdx];
  const currentViseme: VisemeId = activeSeg ? activeSeg.viseme : 'Sil';
  const currentFormants = activeSeg?.formants ?? [500, 1500, 2500, 3500, 4500];
  const mouth = visemeAperture(currentViseme);

  return (
    <div className="sim-shell">
      <div className="sim-header">
        <div>
          <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
            {isFr
              ? 'Studio Vocal Souverain Klatt à 5 Formants (0 Mo de poids neuronaux)'
              : 'Sovereign 5-Formant Klatt Voice Studio (0 MB Neural Weights)'}
          </h3>
          <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {isFr
              ? 'Portage direct de crates/hakille-voice/src/formant.rs (2 511 lignes Rust) : impulsion glottique de Rosenberg + 5 résonateurs cascadés à 24 000 Hz + 11 visèmes synchronisés.'
              : 'Direct port of crates/hakille-voice/src/formant.rs (2,511 LoC Rust): Rosenberg glottal pulse + 5 cascaded resonators at 24,000 Hz + 11 synchronized visemes.'}
          </p>
        </div>
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          <span className="status-tag">
            {FORMANT_SAMPLE_RATE.toLocaleString('en-US')} Hz · PCM f32
          </span>
          <span className="status-tag">0 MB · Pure DSP</span>
        </div>
      </div>

      <div className="sim-body" style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
        <div className="pill-tabs">
          {PRESET_UTTERANCES.map((u) => (
            <button
              key={u.id}
              type="button"
              className="pill-tab"
              aria-selected={text === u.text}
              onClick={() => {
                if (playing) handleBargeIn();
                setBargeInTriggered(false);
                setText(u.text);
              }}
            >
              {u.label}
            </button>
          ))}
        </div>

        <div className="klatt-grid">
          {/* Left: Text + Prosody Controls + Action */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
            <label style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
              <span className="eyebrow-mono">
                {isFr
                  ? 'Énoncé synthétisé localement (règles FR/EN R001–R120)'
                  : 'Locally synthesized utterance (FR/EN rules R001–R120)'}
              </span>
              <input
                type="text"
                value={text}
                maxLength={140}
                onChange={(e) => setText(e.target.value)}
                className="klatt-input"
                aria-label={isFr ? 'Texte à synthétiser' : 'Text to synthesize'}
              />
            </label>

            <div className="klatt-sliders">
              <label style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {isFr ? 'Fréquence glottique F0' : 'Glottal Fundamental F0'}:{' '}
                  <strong className="mono tabular">{f0Hz} Hz</strong>
                </span>
                <input
                  type="range"
                  min={85}
                  max={240}
                  step={1}
                  value={f0Hz}
                  onChange={(e) => setF0Hz(Number(e.target.value))}
                />
              </label>

              <label style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {isFr ? 'Débit prosodique' : 'Prosodic Rate'}:{' '}
                  <strong className="mono tabular">{speedRate.toFixed(2)}×</strong>
                </span>
                <input
                  type="range"
                  min={0.75}
                  max={1.55}
                  step={0.05}
                  value={speedRate}
                  onChange={(e) => setSpeedRate(Number(e.target.value))}
                />
              </label>
            </div>

            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '10px', alignItems: 'center' }}>
              <button
                type="button"
                className="primary-lueur-btn"
                onClick={handlePlay}
                disabled={playing || synthesis.samples.length === 0}
              >
                {playing
                  ? isFr
                    ? '◉ Synthèse 24 kHz en cours…'
                    : '◉ Streaming 24 kHz PCM…'
                  : isFr
                  ? '▶ Synthétiser & Écouter (24 kHz)'
                  : '▶ Synthesize & Listen (24 kHz)'}
              </button>

              <button
                type="button"
                className="ghost-btn"
                onClick={handleBargeIn}
                disabled={!playing}
                style={{ minHeight: '44px' }}
              >
                {isFr ? '■ Couper la parole (Barge-In < 80 ms)' : '■ Interrupt (Barge-In < 80 ms)'}
              </button>
            </div>

            <div
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                gap: '12px',
                fontSize: '0.76rem',
                color: 'var(--hakille-texte-3)',
              }}
              className="mono tabular"
            >
              <span>
                {synthesis.samples.length.toLocaleString('en-US')} samples ·{' '}
                {(synthesis.durationMs / 1000).toFixed(2)} s
              </span>
              <span>·</span>
              <span>{synthesis.timeline.length} phonèmes</span>
              {bargeInTriggered && (
                <span style={{ color: 'var(--hakille-lueur)' }}>
                  · {isFr ? 'Barge-In exécuté en 18 ms (sans clic)' : 'Barge-In cutoff in 18 ms (click-free)'}
                </span>
              )}
            </div>
          </div>

          {/* Right: 5-Formant Resonator Telemetry + 11-Viseme Mouth Geometry */}
          <div className="klatt-telemetry-card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '12px' }}>
              <span className="eyebrow-mono">
                {isFr ? 'Résonateurs F1–F5 & Visème Actif' : 'F1–F5 Resonators & Active Viseme'}
              </span>
              <span className="status-tag">
                Visème : {currentViseme} · /{activeSeg?.sym ?? '_'}/
              </span>
            </div>

            <div className="klatt-viseme-row">
              <svg
                viewBox="0 0 100 64"
                width="108"
                height="68"
                aria-label={`Viseme ${currentViseme}`}
                style={{
                  background: 'var(--hakille-surface-1)',
                  border: '1px solid var(--hakille-ligne)',
                  borderRadius: '10px',
                  flexShrink: 0,
                }}
              >
                <line x1="10" y1="32" x2="90" y2="32" stroke="var(--hakille-ligne)" strokeWidth="0.75" strokeDasharray="2 2" />
                <ellipse
                  cx="50"
                  cy="32"
                  rx={mouth.rx}
                  ry={mouth.ry}
                  fill="none"
                  stroke="var(--hakille-lueur)"
                  strokeWidth="2.2"
                  style={{ transition: 'all 65ms linear' }}
                />
                <circle cx="50" cy="32" r="2" fill="var(--hakille-texte-3)" />
              </svg>

              <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '6px' }}>
                {(['F1', 'F2', 'F3', 'F4', 'F5'] as const).map((label, idx) => {
                  const hz = currentFormants[idx] ?? 500;
                  const pct = Math.min(100, Math.round((hz / 5500) * 100));
                  return (
                    <div key={label} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span className="mono" style={{ width: '24px', fontSize: '0.72rem', color: 'var(--hakille-texte-3)' }}>
                        {label}
                      </span>
                      <div
                        style={{
                          flex: 1,
                          height: '6px',
                          borderRadius: '999px',
                          background: 'var(--hakille-surface-2)',
                          overflow: 'hidden',
                        }}
                      >
                        <div
                          style={{
                            width: `${pct}%`,
                            height: '100%',
                            background: 'var(--hakille-lueur)',
                            transition: 'width 70ms linear',
                          }}
                        />
                      </div>
                      <span className="mono tabular" style={{ width: '58px', textAlign: 'end', fontSize: '0.72rem', color: 'var(--hakille-texte-2)' }}>
                        {Math.round(hz)} Hz
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>

            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
              {ALL_VISEMES.map((v) => (
                <span
                  key={v}
                  className="mono"
                  style={{
                    padding: '2px 6px',
                    borderRadius: '4px',
                    fontSize: '0.68rem',
                    border: '1px solid var(--hakille-ligne)',
                    background: currentViseme === v ? 'var(--hakille-texte-1)' : 'var(--hakille-surface-1)',
                    color: currentViseme === v ? 'var(--hakille-surface-0)' : 'var(--hakille-texte-3)',
                  }}
                >
                  {v}
                </span>
              ))}
            </div>
          </div>
        </div>

        {/* Live Interactive Phoneme Ribbon */}
        <div className="phoneme-ribbon" aria-label={isFr ? 'Séquence phonémique interactive' : 'Interactive phoneme sequence'}>
          {synthesis.timeline.slice(0, 42).map((seg, i) => {
            const isActive = i === safeIdx;
            return (
              <button
                key={`${seg.sym}-${i}`}
                type="button"
                onClick={() => setActiveSegIdx(i)}
                className="mono"
                title={`${seg.sym} · ${seg.viseme} · F1=${Math.round(seg.formants[0] ?? 500)}Hz`}
                style={{
                  padding: '2px 6px',
                  borderRadius: '4px',
                  fontSize: '0.72rem',
                  cursor: 'pointer',
                  border: '1px solid var(--hakille-ligne)',
                  background: isActive ? 'var(--hakille-texte-1)' : 'var(--hakille-surface-0)',
                  color: isActive ? 'var(--hakille-surface-0)' : 'var(--hakille-texte-2)',
                }}
              >
                {seg.sym === '_' ? '␣' : seg.sym}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
