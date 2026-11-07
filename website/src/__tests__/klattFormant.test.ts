import { describe, expect, it } from 'vitest';
import {
  FORMANT_SAMPLE_RATE,
  phonemizeText,
  synthesizeKlattFormant,
} from '@/engine/klattFormant';

describe('Klatt 5-Formant Zero-Weight Speech Engine (formant.rs port)', () => {
  it('phonemizes French and English phrases with synchronized visemes', () => {
    const tokens = phonemizeText('Hakille — intelligence souveraine');
    expect(tokens.length).toBeGreaterThan(8);
    expect(tokens.some((t) => t.viseme === 'AA')).toBe(true);
    expect(tokens.some((t) => t.viseme === 'IY')).toBe(true);
  });

  it('synthesizes finite 24,000 Hz PCM normalized within [-1, 1] with non-zero RMS', () => {
    const res = synthesizeKlattFormant('Le journal est intègre.', {
      f0BaseHz: 120,
      speedRate: 1.0,
    });
    expect(res.sampleRate).toBe(FORMANT_SAMPLE_RATE);
    expect(res.samples.length).toBeGreaterThan(4_000);
    expect(res.timeline.length).toBeGreaterThan(5);

    let sumSq = 0;
    for (let i = 0; i < res.samples.length; i++) {
      const s = res.samples[i]!;
      expect(Number.isFinite(s)).toBe(true);
      expect(Math.abs(s)).toBeLessThanOrEqual(1);
      sumSq += s * s;
    }
    const rms = Math.sqrt(sumSq / res.samples.length);
    expect(rms).toBeGreaterThan(0.01);
  });

  it('scales utterance duration inversely with speedRate within [0.7, 1.6]', () => {
    const slow = synthesizeKlattFormant('Hakille souverain', { speedRate: 0.8 });
    const fast = synthesizeKlattFormant('Hakille souverain', { speedRate: 1.5 });
    expect(slow.samples.length).toBeGreaterThan(fast.samples.length);
  });
});
