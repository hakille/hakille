import { describe, expect, it } from 'vitest';
import {
  ALL_STATE_SOUNDS,
  KORA_21_MIDI_NOTES,
  SAMPLE_RATE_HZ,
  getStateSoundDurationMs,
  midiToHz,
  pluckKoraWire,
  renderStateSound,
  samplesForMs,
  sanitizeVolume,
} from '@/engine/sound';

describe('Karplus-Strong 48 kHz Acoustic Synthesis (`sound.ts` <-> `sound.rs`)', () => {
  it('matches canonical durations from tokens/motion.toml (180ms, 220ms, 320ms)', () => {
    expect(getStateSoundDurationMs('eveil')).toBe(180);
    expect(getStateSoundDurationMs('fait')).toBe(220);
    expect(getStateSoundDurationMs('attention')).toBe(320);

    for (const kind of ALL_STATE_SOUNDS) {
      const pcm = renderStateSound(kind, 1.0);
      expect(pcm.length).toBe(samplesForMs(getStateSoundDurationMs(kind)));
      const actualMs = (pcm.length * 1000) / SAMPLE_RATE_HZ;
      expect(Math.abs(actualMs - getStateSoundDurationMs(kind))).toBeLessThan(0.5);
    }
  });

  it('produces bitwise-identical deterministic PCM with zero-click edges', () => {
    for (const kind of ALL_STATE_SOUNDS) {
      const a = renderStateSound(kind, 0.75);
      const b = renderStateSound(kind, 0.75);
      expect(a.length).toBe(b.length);
      expect(Array.from(a)).toEqual(Array.from(b));

      expect(a[0]).toBe(0);
      expect(a[a.length - 1]).toBe(0);

      let energy = 0;
      for (const sample of a) {
        expect(Math.abs(sample)).toBeLessThanOrEqual(1.0);
        energy += sample * sample;
      }
      expect(energy).toBeGreaterThan(0.01);
    }
  });

  it('sanitizes volume bounds safely without panicking or changing buffer length', () => {
    expect(sanitizeVolume(Number.NaN)).toBe(0);
    expect(sanitizeVolume(-4)).toBe(0);
    expect(sanitizeVolume(Number.POSITIVE_INFINITY)).toBe(1);

    const muted = renderStateSound('fait', 0);
    expect(muted.length).toBe(samplesForMs(220));
    expect(muted.every((x) => x === 0)).toBe(true);
  });

  it('synthesizes all 21 West African Kora strings cleanly (`WIRES_FULL = 21`)', () => {
    expect(KORA_21_MIDI_NOTES).toHaveLength(21);
    expect(midiToHz(69)).toBeCloseTo(440.0, 3);
    for (let i = 0; i < 21; i++) {
      const wirePcm = pluckKoraWire(i, 0.5, 120);
      expect(wirePcm.length).toBe(samplesForMs(120));
      expect(wirePcm[0]).toBe(0);
      expect(wirePcm[wirePcm.length - 1]).toBe(0);
    }
  });
});
