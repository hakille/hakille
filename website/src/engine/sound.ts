/**
 * Hakille Acoustic Engine — Deterministic Karplus-Strong Plucked String Synthesis
 *
 * Faithful port of `crates/hakille-design/src/sound.rs`:
 * - Sample rate: 48,000 Hz mono PCM Float32
 * - Delay-line damping: 0.996
 * - Pluck gain: 0.8 (never clips)
 * - Anti-click envelope: 2 ms linear attack, 15 ms linear release (first & last sample = 0)
 * - Deterministic 64-bit xorshift64 PRNG matching Rust u64 arithmetic
 * - Includes the 3 canonical system state signals (`eveil`, `fait`, `attention`)
 *   sourced from `tokens/motion.toml`, plus a 21-string Manding Kora scale.
 */

import { MOTION_TOKENS } from '@/tokens/hakilleTokens.generated';

export const SAMPLE_RATE_HZ = 48_000;
export const KS_DAMPING = 0.996;
export const PLUCK_GAIN = 0.8;
export const ATTACK_MS = 2;
export const RELEASE_MS = 15;

const U64_MASK = 0xffffffffffffffffn;
const U64_MAX_F64 = Number(U64_MASK);

export type StateSoundKind = 'eveil' | 'fait' | 'attention';

export const ALL_STATE_SOUNDS: readonly StateSoundKind[] = ['eveil', 'fait', 'attention'] as const;

/**
 * 21-string West African Kora tuning (Silaba / F-major Manding harp-lute scale,
 * ascending from bass F2 to treble E6 across the 21 strings of `brand.rs::WIRES_FULL`).
 */
export const KORA_21_MIDI_NOTES: readonly number[] = [
  41, 45, 48, 50, 52, 53, 55,
  57, 60, 62, 64, 65, 67, 69,
  72, 74, 76, 77, 79, 81, 84,
] as const;

export function getStateSoundNotes(kind: StateSoundKind): ReadonlyArray<readonly [number, number]> {
  return MOTION_TOKENS.sound[kind];
}

export function getStateSoundDurationMs(kind: StateSoundKind): number {
  return getStateSoundNotes(kind).reduce((acc, [, ms]) => acc + Math.max(0, ms), 0);
}

export function midiToHz(midi: number): number {
  const clamped = Math.max(0, Math.min(127, Math.round(midi)));
  return 440.0 * Math.pow(2, (clamped - 69) / 12);
}

export function sanitizeVolume(volume: number): number {
  if (Number.isNaN(volume)) {
    return 0.0;
  }
  if (volume === Number.NEGATIVE_INFINITY || volume < 0) {
    return 0.0;
  }
  if (volume === Number.POSITIVE_INFINITY || volume > 1) {
    return 1.0;
  }
  return volume;
}

export function samplesForMs(ms: number): number {
  const safeMs = Math.max(0, Math.round(ms));
  return Math.floor((SAMPLE_RATE_HZ * safeMs + 500) / 1000);
}

function applyAttackRelease(buf: Float32Array): void {
  if (buf.length === 0) return;
  const attack = Math.min(buf.length, Math.max(1, Math.floor((SAMPLE_RATE_HZ * ATTACK_MS) / 1000)));
  const release = Math.min(buf.length, Math.max(1, Math.floor((SAMPLE_RATE_HZ * RELEASE_MS) / 1000)));

  for (let i = 0; i < attack; i++) {
    const val = buf[i] ?? 0;
    const scaled = Math.fround(val * Math.fround(i / attack));
    buf[i] = scaled === 0 ? 0 : scaled;
  }
  for (let i = 0; i < release; i++) {
    const idx = buf.length - 1 - i;
    const val = buf[idx] ?? 0;
    const scaled = Math.fround(val * Math.fround(i / release));
    buf[idx] = scaled === 0 ? 0 : scaled;
  }
}

/**
 * Deterministic Karplus-Strong plucked string generator.
 */
export function pluck(freqHz: number, durationMs: number, seed: bigint = 0x9e37n): Float32Array {
  const total = samplesForMs(durationMs);
  const safeFreq = Number.isFinite(freqHz) && freqHz > 0 ? freqHz : 1.0;
  const n = Math.max(2, Math.round(SAMPLE_RATE_HZ / safeFreq));

  let s = seed > 1n ? seed & U64_MASK : 1n;
  const nextNoise = (): number => {
    s = (s ^ ((s << 13n) & U64_MASK)) & U64_MASK;
    s = (s ^ (s >> 7n)) & U64_MASK;
    s = (s ^ ((s << 17n) & U64_MASK)) & U64_MASK;
    const unit = Number(s) / U64_MAX_F64;
    return Math.fround(unit * 2.0 - 1.0);
  };

  const line = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    line[i] = nextNoise();
  }

  const out = new Float32Array(total);
  let idx = 0;
  for (let t = 0; t < total; t++) {
    const cur = line[idx] ?? 0;
    const nxt = line[(idx + 1) % n] ?? 0;
    line[idx] = Math.fround(KS_DAMPING * 0.5 * (cur + nxt));
    out[t] = Math.fround(cur * PLUCK_GAIN);
    idx = (idx + 1) % n;
  }

  applyAttackRelease(out);
  return out;
}

export function playNotes(notes: ReadonlyArray<readonly [number, number]>): Float32Array {
  const segments = notes.map(([midi, ms], idx) =>
    pluck(midiToHz(midi), Math.max(0, ms), 0x9e37n + BigInt(idx))
  );
  const totalLength = segments.reduce((sum, seg) => sum + seg.length, 0);
  const merged = new Float32Array(totalLength);
  let offset = 0;
  for (const seg of segments) {
    merged.set(seg, offset);
    offset += seg.length;
  }
  return merged;
}

export function renderStateSound(kind: StateSoundKind, volume: number): Float32Array {
  const gain = sanitizeVolume(volume);
  const pcm = playNotes(getStateSoundNotes(kind));
  if (gain === 0) {
    pcm.fill(0);
    return pcm;
  }
  if (gain !== 1) {
    for (let i = 0; i < pcm.length; i++) {
      pcm[i] = Math.fround((pcm[i] ?? 0) * gain);
    }
  }
  return pcm;
}

export function pluckKoraWire(wireIndex: number, volume = 0.55, durationMs = 340): Float32Array {
  const clampedIdx = Math.max(0, Math.min(KORA_21_MIDI_NOTES.length - 1, Math.floor(wireIndex)));
  const midi = KORA_21_MIDI_NOTES[clampedIdx] ?? 69;
  const gain = sanitizeVolume(volume);
  const pcm = pluck(midiToHz(midi), durationMs, 0x9e37n + BigInt(clampedIdx * 17));
  if (gain === 0) {
    pcm.fill(0);
    return pcm;
  }
  for (let i = 0; i < pcm.length; i++) {
    pcm[i] = Math.fround((pcm[i] ?? 0) * gain);
  }
  return pcm;
}
