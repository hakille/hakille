import { describe, expect, it } from 'vitest';
import { COLOR_TOKENS } from '@/tokens/hakilleTokens.generated';
import { braiseAllowed, contrastRatio } from '@/engine/colorGuard';
import {
  HOLD_REQUIRED_MS,
  beginHold,
  createInitialHoldState,
  releaseHoldEarly,
  tickHold,
  triggerAccessibleStep,
} from '@/engine/hold';

describe('600 ms Monotonic Hold & Sacred Braise / WCAG Guards', () => {
  it('enforces 600 ms physical hold and cancels safely on early release', () => {
    expect(HOLD_REQUIRED_MS).toBe(600);

    let state = beginHold(1000);
    expect(state.phase).toBe('holding');

    state = tickHold(state, 1420); // 420 ms < 600 ms
    expect(state.phase).toBe('holding');
    expect(state.progress).toBeCloseTo(0.7, 2);

    const cancelled = releaseHoldEarly(state);
    expect(cancelled.phase).toBe('cancelled');
    expect(cancelled.progress).toBe(0);

    // Full 600 ms hold confirms monotonically
    let full = beginHold(2000);
    full = tickHold(full, 2605);
    expect(full.phase).toBe('confirmed');
    expect(full.progress).toBe(1);
  });

  it('supports accessible 2-step keyboard confirmation without timing pressure', () => {
    const s0 = createInitialHoldState();
    const s1 = triggerAccessibleStep(s0);
    expect(s1.phase).toBe('armed-accessible');
    const s2 = triggerAccessibleStep(s1);
    expect(s2.phase).toBe('confirmed');
  });

  it('strictly restricts Braise to the 3 canonical contexts from tokens/color.toml', () => {
    expect(braiseAllowed('validation')).toBe(true);
    expect(braiseAllowed('hold-progress')).toBe(true);
    expect(braiseAllowed('decision-ring')).toBe(true);

    expect(braiseAllowed('link')).toBe(false);
    expect(braiseAllowed('hero-badge')).toBe(false);
    expect(braiseAllowed('border-accent')).toBe(false);
    expect(braiseAllowed('gradient')).toBe(false);
  });

  it('verifies WCAG 2.1 AAA (>= 15:1) primary and AA/AAA secondary contrast in Dark & Light', () => {
    const darkPrimary = contrastRatio(COLOR_TOKENS.texte['1'].dark, COLOR_TOKENS.surface['0'].dark);
    const lightPrimary = contrastRatio(COLOR_TOKENS.texte['1'].light, COLOR_TOKENS.surface['0'].light);
    expect(darkPrimary).toBeGreaterThan(15.5);
    expect(lightPrimary).toBeGreaterThan(15.0);

    const darkSecondary = contrastRatio(COLOR_TOKENS.texte['2'].dark, COLOR_TOKENS.surface['0'].dark);
    const lightSecondary = contrastRatio(COLOR_TOKENS.texte['2'].light, COLOR_TOKENS.surface['0'].light);
    expect(darkSecondary).toBeGreaterThan(7.0);
    expect(lightSecondary).toBeGreaterThan(7.0);
  });
});
