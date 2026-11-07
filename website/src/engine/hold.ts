/**
 * Hakille — Monotonic 600 ms Hold-to-Confirm State Machine
 *
 * Faithful port of `crates/hakille-design/src/hold.rs`:
 * Irreversible mutations require either:
 *   1. Continuous physical hold for `600 ms` (`tokens/motion.toml::t600`), OR
 *   2. Explicit 2-step accessible keyboard/switch confirmation (`arm` -> `confirm`)
 *      so motor-impaired users never depend on timed pointer pressure (WCAG 2.1 AA/AAA).
 */

import { MOTION_TOKENS } from '@/tokens/hakilleTokens.generated';

export const HOLD_REQUIRED_MS = MOTION_TOKENS.duration_ms.t600;

export type HoldPhase = 'idle' | 'holding' | 'armed-accessible' | 'confirmed' | 'cancelled';

export interface HoldState {
  readonly phase: HoldPhase;
  readonly startedAtMs: number | null;
  readonly elapsedMs: number;
  readonly progress: number; // 0.0 .. 1.0
}

export function createInitialHoldState(): HoldState {
  return {
    phase: 'idle',
    startedAtMs: null,
    elapsedMs: 0,
    progress: 0,
  };
}

export function beginHold(nowMs: number): HoldState {
  return {
    phase: 'holding',
    startedAtMs: nowMs,
    elapsedMs: 0,
    progress: 0,
  };
}

export function tickHold(state: HoldState, nowMs: number): HoldState {
  if (state.phase !== 'holding' || state.startedAtMs === null) {
    return state;
  }
  const elapsed = Math.max(state.elapsedMs, Math.max(0, nowMs - state.startedAtMs));
  const progress = Math.min(1, elapsed / HOLD_REQUIRED_MS);
  if (elapsed >= HOLD_REQUIRED_MS) {
    return {
      phase: 'confirmed',
      startedAtMs: state.startedAtMs,
      elapsedMs: HOLD_REQUIRED_MS,
      progress: 1,
    };
  }
  return {
    phase: 'holding',
    startedAtMs: state.startedAtMs,
    elapsedMs: elapsed,
    progress,
  };
}

export function releaseHoldEarly(state: HoldState): HoldState {
  if (state.phase === 'confirmed') {
    return state;
  }
  if (state.phase === 'holding') {
    return {
      phase: 'cancelled',
      startedAtMs: null,
      elapsedMs: state.elapsedMs,
      progress: 0,
    };
  }
  return createInitialHoldState();
}

export function triggerAccessibleStep(state: HoldState): HoldState {
  if (state.phase === 'armed-accessible') {
    return {
      phase: 'confirmed',
      startedAtMs: null,
      elapsedMs: HOLD_REQUIRED_MS,
      progress: 1,
    };
  }
  return {
    phase: 'armed-accessible',
    startedAtMs: null,
    elapsedMs: 0,
    progress: 0.5,
  };
}
