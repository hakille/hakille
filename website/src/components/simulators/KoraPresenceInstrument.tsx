import { useCallback, useEffect, useRef, useState } from 'react';
import { KORA_21_MIDI_NOTES, midiToHz } from '@/engine/sound';
import { hakilleAudio } from '@/engine/audioPlayer';
import { braiseAllowed } from '@/engine/colorGuard';
import { usePreferences } from '@/context/PreferencesContext';
import { useI18n } from '@/i18n/context';

export type PresenceMode =
  | 'idle'
  | 'listening'
  | 'thinking'
  | 'speaking'
  | 'decision'
  | 'emergency';

const WIRES_FULL = 21; // crates/hakille-design/src/brand.rs::WIRES_FULL
const WIRES_SMALL = 7; // crates/hakille-design/src/brand.rs::WIRES_SMALL (< 24 px)

export function KoraPresenceInstrument() {
  const { locale, t } = useI18n();
  const isFr = locale === 'fr';
  const { motion, soundEnabled } = usePreferences();
  const [mode, setMode] = useState<PresenceMode>('speaking');
  const [compactScale, setCompactScale] = useState<boolean>(false);
  const [phase, setPhase] = useState(0);
  const [frameCount, setFrameCount] = useState<number>(0);
  const [pluckedWire, setPluckedWire] = useState<{ index: number; amp: number } | null>(null);
  const [lastPluckedInfo, setLastPluckedInfo] = useState<string | null>(null);
  const [bargeInNotice, setBargeInNotice] = useState(false);
  const rafRef = useRef<number | null>(null);
  const arpeggioTimersRef = useRef<number[]>([]);

  if (!braiseAllowed('decision-ring')) {
    throw new Error('Sacred Braise decision-ring guard failed');
  }

  const wireCount = compactScale ? WIRES_SMALL : WIRES_FULL;

  // Zero-FPS invariant: in `idle`, `emergency`, or `reduced` motion, NO animation loop runs!
  useEffect(() => {
    const shouldAnimate =
      motion === 'normal' &&
      (mode === 'speaking' || mode === 'listening' || mode === 'thinking' || pluckedWire !== null);

    if (!shouldAnimate) {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      return;
    }

    let last = performance.now();
    const loop = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      setPhase((p) => (p + dt * 4.2) % (Math.PI * 20));
      setFrameCount((c) => (c + 1) % 100000);
      setPluckedWire((pw) => {
        if (!pw) return null;
        const decayed = pw.amp * Math.pow(0.06, dt);
        return decayed < 0.02 ? null : { index: pw.index, amp: decayed };
      });
      rafRef.current = requestAnimationFrame(loop);
    };

    rafRef.current = requestAnimationFrame(loop);
    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [mode, motion, pluckedWire]);

  useEffect(() => {
    return () => {
      arpeggioTimersRef.current.forEach((id) => window.clearTimeout(id));
    };
  }, []);

  const handlePluck = useCallback(
    (index: number, forceAudible = false) => {
      const mappedKoraIdx = compactScale ? Math.round((index / (WIRES_SMALL - 1)) * 20) : index;
      const midi = KORA_21_MIDI_NOTES[mappedKoraIdx] ?? 69;
      const hz = midiToHz(midi);
      setPluckedWire({ index, amp: 1 });
      setLastPluckedInfo(`#${index + 1} · MIDI ${midi} · ${hz.toFixed(1)} Hz`);
      hakilleAudio.playKoraWire(mappedKoraIdx, forceAudible || soundEnabled, 0.45);
    },
    [compactScale, soundEnabled]
  );

  const playKoraArpeggio = () => {
    arpeggioTimersRef.current.forEach((id) => window.clearTimeout(id));
    arpeggioTimersRef.current = [];
    setMode('speaking');
    for (let i = 0; i < wireCount; i++) {
      const timer = window.setTimeout(() => {
        handlePluck(i, true);
      }, i * 52);
      arpeggioTimersRef.current.push(timer);
    }
  };

  const handleBargeIn = () => {
    arpeggioTimersRef.current.forEach((id) => window.clearTimeout(id));
    arpeggioTimersRef.current = [];
    hakilleAudio.stopActiveVoice();
    setMode('listening');
    setPluckedWire(null);
    setBargeInNotice(true);
    window.setTimeout(() => setBargeInNotice(false), 3200);
  };

  const selectMode = (next: PresenceMode) => {
    setMode(next);
    setBargeInNotice(false);
    if (next === 'listening') hakilleAudio.playStateSignal('eveil', soundEnabled, 0.45);
    if (next === 'decision') hakilleAudio.playStateSignal('attention', soundEnabled, 0.5);
  };

  const isZeroFps = mode === 'idle' || mode === 'emergency' || motion === 'reduced';

  return (
    <div className="kora-instrument" aria-label={t.kora.title}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-start',
          gap: '12px',
          flexWrap: 'wrap',
        }}
      >
        <div>
          <div style={{ fontWeight: 600, color: 'var(--hakille-texte-1)' }}>{t.kora.title}</div>
          <div style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {t.kora.subtitle}
          </div>
        </div>
        <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap', alignItems: 'center' }}>
          <span className="status-tag">
            {isZeroFps ? t.kora.fpsZeroBadge : `${t.kora.fpsActiveBadge} · f#${frameCount}`}
          </span>
          <button
            type="button"
            className="status-tag"
            onClick={() => setCompactScale((v) => !v)}
            style={{ cursor: 'pointer' }}
            title="crates/hakille-design/src/brand.rs::wire_count(px)"
          >
            {compactScale ? '7 fils (<24px)' : '21 fils (≥24px)'}
          </button>
        </div>
      </div>

      {/* Waveform stage (direction: ltr invariant per typo.rs) */}
      <div className="kora-stage" data-waveform-invariant="true">
        <svg className="kora-svg" viewBox="0 0 520 256" role="img" aria-label={t.kora.title}>
          {/* Subtle architectural calibration crosshairs (not sensor decor, pure CAD geometry) */}
          <g stroke="var(--hakille-ligne)" strokeWidth="1">
            <path d="M 18 18 L 26 18 M 22 14 L 22 22" />
            <path d="M 494 18 L 502 18 M 498 14 L 498 22" />
            <path d="M 18 238 L 26 238 M 22 234 L 22 242" />
            <path d="M 494 238 L 502 238 M 498 234 L 498 242" />
          </g>

          {/* Subtle architectural arch curve connecting the Kora bridges */}
          <path
            d="M 48 188 Q 260 28 472 188"
            fill="none"
            stroke="var(--hakille-ligne)"
            strokeWidth="1"
            strokeDasharray="3 4"
          />

          {/* Kora vertical acoustic strings (21 full or 7 small per brand.rs) */}
          {Array.from({ length: wireCount }).map((_, i) => {
            const norm = i / Math.max(1, wireCount - 1);
            const x = 56 + norm * 408;
            const archTopY = 166 - Math.sin(norm * Math.PI) * 112;
            const baseBottomY = 194;

            let waveDeltaX = 0;
            if (motion === 'normal') {
              if (mode === 'speaking') {
                waveDeltaX =
                  Math.sin(phase * 2.1 + i * 0.55) * (7.5 * Math.sin(norm * Math.PI) + 1.5);
              } else if (mode === 'listening') {
                waveDeltaX = Math.sin(phase * 1.1 + i * 0.35) * 2.2;
              } else if (mode === 'thinking') {
                waveDeltaX = Math.cos(phase * 1.6 - i * 0.45) * 3.1;
              }
              if (pluckedWire && pluckedWire.index === i) {
                waveDeltaX += Math.sin(phase * 9) * 12 * pluckedWire.amp;
              }
            }

            const isPlucked = pluckedWire?.index === i;
            const strokeOpacity =
              mode === 'idle'
                ? 0.32
                : mode === 'emergency'
                ? 0.22
                : isPlucked
                ? 1
                : 0.58 + Math.sin(norm * Math.PI) * 0.32;

            const mappedIdx = compactScale ? Math.round(norm * 20) : i;
            const midiNote = KORA_21_MIDI_NOTES[mappedIdx] ?? 69;

            return (
              <g
                key={i}
                className="kora-wire-hit"
                onPointerEnter={() => handlePluck(i)}
                onClick={() => handlePluck(i, true)}
              >
                <title>{`Kora String #${i + 1} · MIDI ${midiNote}`}</title>
                <line
                  x1={x}
                  y1={archTopY}
                  x2={x}
                  y2={baseBottomY}
                  stroke="transparent"
                  strokeWidth="16"
                />
                <path
                  d={`M ${x} ${archTopY} Q ${x + waveDeltaX} ${(archTopY + baseBottomY) / 2} ${x} ${baseBottomY}`}
                  fill="none"
                  stroke="var(--hakille-lueur)"
                  strokeOpacity={strokeOpacity}
                  strokeWidth={isPlucked ? '2.2' : '1.25'}
                />
                <circle
                  cx={x}
                  cy={archTopY}
                  r={isPlucked ? 3.2 : 1.7}
                  fill="var(--hakille-lueur)"
                  fillOpacity={strokeOpacity}
                />
              </g>
            );
          })}

          {/* Central Presence Lueur Focal Point beneath the 21-wire arch */}
          <circle
            cx="260"
            cy="218"
            r={mode === 'listening' && motion === 'normal' ? 9 + Math.sin(phase * 2) * 1.8 : 8}
            fill="var(--hakille-lueur)"
            fillOpacity={mode === 'idle' ? 0.55 : 0.96}
          />

          {/* Decision Ring — ONLY rendered when mode === 'decision' (`data-braise-context="decision-ring"`) */}
          {mode === 'decision' && (
            <circle
              cx="260"
              cy="218"
              r="18"
              fill="none"
              data-braise-context="decision-ring"
              strokeWidth="2.2"
            />
          )}
        </svg>

        {lastPluckedInfo && (
          <div
            className="mono tabular"
            style={{
              position: 'absolute',
              insetBlockStart: '10px',
              insetInlineEnd: '12px',
              fontSize: '0.7rem',
              color: 'var(--hakille-texte-3)',
              background: 'var(--hakille-surface-1)',
              padding: '2px 8px',
              borderRadius: '4px',
              border: '1px solid var(--hakille-ligne)',
            }}
          >
            {lastPluckedInfo}
          </div>
        )}

        {mode === 'emergency' && (
          <div
            style={{
              position: 'absolute',
              insetInline: '16px',
              insetBlockEnd: '12px',
              padding: '8px 12px',
              background: 'var(--hakille-surface-1)',
              border: '1px solid var(--hakille-texte-2)',
              borderRadius: '8px',
              fontSize: 'var(--hakille-type-caption-size)',
              display: 'flex',
              justifyContent: 'space-between',
              flexWrap: 'wrap',
              gap: '8px',
            }}
          >
            <span>RULE-05 · Assistance humaine immédiate :</span>
            <strong className="mono">FR 3114 · UE 112 · US/CA 988</strong>
          </div>
        )}
      </div>

      <div className="kora-state-pills" role="group" aria-label="Presence state">
        {(
          [
            ['idle', t.kora.stateIdle],
            ['listening', t.kora.stateListening],
            ['thinking', t.kora.stateThinking],
            ['speaking', t.kora.stateSpeaking],
            ['decision', t.kora.stateDecision],
            ['emergency', t.kora.stateEmergency],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className="state-pill"
            aria-pressed={mode === id}
            onClick={() => selectMode(id)}
          >
            {label}
          </button>
        ))}
      </div>

      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: '10px',
          flexWrap: 'wrap',
        }}
      >
        <span style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
          {bargeInNotice ? t.kora.bargeInStatus : t.kora.pluckHint}
        </span>
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          <button type="button" className="ghost-btn" onClick={playKoraArpeggio}>
            {isFr ? '♫ Arpège Kora (48 kHz)' : '♫ Play Kora Sweep (48 kHz)'}
          </button>
          {mode === 'speaking' && (
            <button type="button" className="ghost-btn" onClick={handleBargeIn}>
              {t.kora.bargeInBtn}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
