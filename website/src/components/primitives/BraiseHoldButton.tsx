import { useCallback, useEffect, useRef, useState } from 'react';
import {
  HOLD_REQUIRED_MS,
  HoldState,
  beginHold,
  createInitialHoldState,
  releaseHoldEarly,
  tickHold,
  triggerAccessibleStep,
} from '@/engine/hold';
import { braiseAllowed } from '@/engine/colorGuard';
import { hakilleAudio } from '@/engine/audioPlayer';
import { usePreferences } from '@/context/PreferencesContext';

interface BraiseHoldButtonProps {
  readonly idleLabel: string;
  readonly holdingLabel: string;
  readonly armedLabel: string;
  readonly confirmedLabel: string;
  readonly cancelledHint: string;
  readonly accessibleTwoStep: boolean;
  readonly disabled?: boolean;
  readonly onConfirmed: () => void;
}

/**
 * The ONLY button in `hakille.ai` permitted to render `--hakille-braise` (`#FF7A2F` / `#C2410C`),
 * guarded at runtime by `braiseAllowed('validation')` and `braiseAllowed('hold-progress')`.
 */
export function BraiseHoldButton({
  idleLabel,
  holdingLabel,
  armedLabel,
  confirmedLabel,
  cancelledHint,
  accessibleTwoStep,
  disabled = false,
  onConfirmed,
}: BraiseHoldButtonProps) {
  const { soundEnabled } = usePreferences();
  const [hold, setHold] = useState<HoldState>(() => createInitialHoldState());
  const rafRef = useRef<number | null>(null);
  const confirmedFiredRef = useRef(false);

  if (!braiseAllowed('validation') || !braiseAllowed('hold-progress')) {
    throw new Error('Sacred Braise invariant violation');
  }

  const stopLoop = useCallback(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  useEffect(() => stopLoop, [stopLoop]);

  const completeConfirmation = useCallback(() => {
    if (confirmedFiredRef.current) return;
    confirmedFiredRef.current = true;
    hakilleAudio.playStateSignal('fait', soundEnabled, 0.72);
    onConfirmed();
    window.setTimeout(() => {
      confirmedFiredRef.current = false;
      setHold(createInitialHoldState());
    }, 1400);
  }, [onConfirmed, soundEnabled]);

  const startPhysicalHold = useCallback(() => {
    if (disabled || accessibleTwoStep || hold.phase === 'confirmed') return;
    stopLoop();
    const startMs = performance.now();
    let current = beginHold(startMs);
    setHold(current);
    hakilleAudio.playStateSignal('attention', soundEnabled, 0.3);

    const step = () => {
      const next = tickHold(current, performance.now());
      current = next;
      setHold(next);
      if (next.phase === 'confirmed') {
        stopLoop();
        completeConfirmation();
        return;
      }
      rafRef.current = requestAnimationFrame(step);
    };
    rafRef.current = requestAnimationFrame(step);
  }, [accessibleTwoStep, completeConfirmation, disabled, hold.phase, soundEnabled, stopLoop]);

  const cancelPhysicalHold = useCallback(() => {
    if (accessibleTwoStep || hold.phase !== 'holding') return;
    stopLoop();
    setHold((prev) => releaseHoldEarly(prev));
  }, [accessibleTwoStep, hold.phase, stopLoop]);

  const handleAccessibleActivate = useCallback(() => {
    if (disabled || !accessibleTwoStep) return;
    const next = triggerAccessibleStep(hold);
    setHold(next);
    if (next.phase === 'armed-accessible') {
      hakilleAudio.playStateSignal('attention', soundEnabled, 0.35);
    } else if (next.phase === 'confirmed') {
      completeConfirmation();
    }
  }, [accessibleTwoStep, completeConfirmation, disabled, hold, soundEnabled]);

  const circumference = 2 * Math.PI * 11;
  const dashOffset = circumference * (1 - hold.progress);

  let label = idleLabel;
  if (hold.phase === 'holding') {
    label = `${holdingLabel} (${Math.min(HOLD_REQUIRED_MS, Math.round(hold.elapsedMs))} / ${HOLD_REQUIRED_MS} ms)`;
  } else if (hold.phase === 'armed-accessible') {
    label = armedLabel;
  } else if (hold.phase === 'confirmed') {
    label = confirmedLabel;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <button
        type="button"
        className="braise-hold-btn"
        data-braise-context="validation"
        disabled={disabled}
        aria-live="polite"
        onPointerDown={!accessibleTwoStep ? startPhysicalHold : undefined}
        onPointerUp={!accessibleTwoStep ? cancelPhysicalHold : undefined}
        onPointerLeave={!accessibleTwoStep ? cancelPhysicalHold : undefined}
        onPointerCancel={!accessibleTwoStep ? cancelPhysicalHold : undefined}
        onKeyDown={(e) => {
          if (disabled) return;
          if (!accessibleTwoStep && (e.key === 'Enter' || e.key === ' ') && !e.repeat) {
            e.preventDefault();
            startPhysicalHold();
          }
        }}
        onKeyUp={(e) => {
          if (!accessibleTwoStep && (e.key === 'Enter' || e.key === ' ')) {
            e.preventDefault();
            cancelPhysicalHold();
          }
        }}
        onClick={accessibleTwoStep ? handleAccessibleActivate : undefined}
      >
        <svg width="28" height="28" viewBox="0 0 28 28" aria-hidden="true">
          <circle
            cx="14"
            cy="14"
            r="11"
            fill="none"
            stroke="currentColor"
            strokeOpacity="0.28"
            strokeWidth="2.5"
          />
          <circle
            cx="14"
            cy="14"
            r="11"
            fill="none"
            data-braise-context="hold-progress"
            style={{ stroke: 'currentColor' }}
            strokeWidth="2.5"
            strokeDasharray={circumference}
            strokeDashoffset={dashOffset}
            strokeLinecap="round"
            transform="rotate(-90 14 14)"
          />
        </svg>
        <span className="tabular">{label}</span>
      </button>
      {hold.phase === 'cancelled' && (
        <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
          {cancelledHint}
        </p>
      )}
    </div>
  );
}
