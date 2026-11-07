/**
 * Pure TypeScript Port of `crates/hakille-voice/src/formant.rs` (2,511 LoC in Rust).
 *
 * Real Zero-Weight Mathematical Speech Synthesis (Simplified Klatt Formant Model):
 *  1. Rule-based FR/EN grapheme-to-phoneme tokenizer (`ch`, `gn`, `eau`, `ou`, `oi`,
 *     `an`/`en`, `in`, `on`, `eu`, `th`, `sh`, `ee`, `oo`, vowels & consonants).
 *  2. Differentiated Rosenberg glottal pulse source (`80–300 Hz` F0) + aspiration
 *     breathiness + deterministic LFSR fricative noise.
 *  3. 5 cascaded 2nd-order formant resonators (`F1..F5`, bandwidths `[70, 90, 130, 180, 250] Hz`)
 *     with ~6 ms boundary coarticulation interpolation.
 *  4. Prosodic declination + punctuation pauses + 11 synchronized mouth visemes.
 *
 * Output: 24,000 Hz mono `Float32Array` normalized inside `[-1, 1]` via soft `Math.tanh`.
 * Zero neural weights (0 MB), zero external network requests.
 */

export const FORMANT_SAMPLE_RATE = 24_000;
export const FORMANT_MIN_F0 = 80;
export const FORMANT_MAX_F0 = 300;

const FORMANT_BW: readonly [number, number, number, number, number] = [70, 90, 130, 180, 250];

export type VisemeId =
  | 'Sil'
  | 'AA'
  | 'AE'
  | 'EH'
  | 'IH'
  | 'IY'
  | 'UH'
  | 'UW'
  | 'AO'
  | 'ER'
  | 'Cons';

export interface PhonemeSpec {
  readonly sym: string;
  readonly formants: readonly [number, number, number, number, number];
  readonly bwScale: number;
  readonly durMs: number;
  readonly voiced: boolean;
  readonly noiseGain: number;
  readonly viseme: VisemeId;
}

export interface TimelineSegment {
  readonly sym: string;
  readonly viseme: VisemeId;
  readonly formants: readonly [number, number, number, number, number];
  readonly startMs: number;
  readonly endMs: number;
  readonly voiced: boolean;
}

export interface KlattSynthesisResult {
  readonly samples: Float32Array;
  readonly sampleRate: number;
  readonly durationMs: number;
  readonly timeline: readonly TimelineSegment[];
}

export interface KlattVoiceConfig {
  readonly f0BaseHz: number;
  readonly speedRate: number;
  readonly breathiness: number;
}

const PHONEME_TABLE: Readonly<Record<string, PhonemeSpec>> = {
  _: { sym: '_', formants: [500, 1500, 2500, 3500, 4500], bwScale: 1.5, durMs: 95, voiced: false, noiseGain: 0, viseme: 'Sil' },
  a: { sym: 'a', formants: [730, 1090, 2440, 3500, 4500], bwScale: 1.0, durMs: 92, voiced: true, noiseGain: 0, viseme: 'AA' },
  ae: { sym: 'ae', formants: [700, 1700, 2500, 3500, 4500], bwScale: 1.0, durMs: 92, voiced: true, noiseGain: 0, viseme: 'AE' },
  e: { sym: 'e', formants: [450, 1930, 2560, 3500, 4500], bwScale: 1.0, durMs: 88, voiced: true, noiseGain: 0, viseme: 'EH' },
  E: { sym: 'E', formants: [610, 1720, 2410, 3500, 4500], bwScale: 1.0, durMs: 92, voiced: true, noiseGain: 0, viseme: 'EH' },
  i: { sym: 'i', formants: [270, 2290, 3010, 3500, 4500], bwScale: 1.0, durMs: 88, voiced: true, noiseGain: 0, viseme: 'IY' },
  I: { sym: 'I', formants: [400, 1900, 2550, 3500, 4500], bwScale: 1.0, durMs: 82, voiced: true, noiseGain: 0, viseme: 'IH' },
  o: { sym: 'o', formants: [500, 890, 2410, 3500, 4500], bwScale: 1.0, durMs: 90, voiced: true, noiseGain: 0, viseme: 'AO' },
  O: { sym: 'O', formants: [560, 900, 2560, 3400, 4400], bwScale: 1.0, durMs: 92, voiced: true, noiseGain: 0, viseme: 'AO' },
  u: { sym: 'u', formants: [300, 870, 2240, 3500, 4500], bwScale: 1.0, durMs: 90, voiced: true, noiseGain: 0, viseme: 'UW' },
  y: { sym: 'y', formants: [300, 1750, 2150, 3500, 4500], bwScale: 1.0, durMs: 88, voiced: true, noiseGain: 0, viseme: 'UW' },
  eu: { sym: 'eu', formants: [450, 1500, 2500, 3500, 4500], bwScale: 1.0, durMs: 88, voiced: true, noiseGain: 0, viseme: 'ER' },
  '@': { sym: '@', formants: [500, 1500, 2500, 3500, 4500], bwScale: 1.0, durMs: 56, voiced: true, noiseGain: 0, viseme: 'UH' },
  an: { sym: 'an', formants: [700, 1150, 2500, 3500, 4500], bwScale: 1.05, durMs: 108, voiced: true, noiseGain: 0, viseme: 'AA' },
  in: { sym: 'in', formants: [550, 1650, 2450, 3500, 4500], bwScale: 1.05, durMs: 105, voiced: true, noiseGain: 0, viseme: 'AE' },
  on: { sym: 'on', formants: [480, 980, 2600, 3500, 4500], bwScale: 1.05, durMs: 108, voiced: true, noiseGain: 0, viseme: 'AO' },
  p: { sym: 'p', formants: [500, 1200, 2500, 3500, 4500], bwScale: 1.3, durMs: 65, voiced: false, noiseGain: 0.45, viseme: 'Cons' },
  t: { sym: 't', formants: [400, 1800, 2900, 3500, 4500], bwScale: 1.3, durMs: 65, voiced: false, noiseGain: 0.55, viseme: 'Cons' },
  k: { sym: 'k', formants: [400, 1500, 2400, 3500, 4500], bwScale: 1.3, durMs: 68, voiced: false, noiseGain: 0.5, viseme: 'Cons' },
  b: { sym: 'b', formants: [500, 1200, 2500, 3500, 4500], bwScale: 1.25, durMs: 58, voiced: true, noiseGain: 0.2, viseme: 'Cons' },
  d: { sym: 'd', formants: [400, 1800, 2900, 3500, 4500], bwScale: 1.25, durMs: 58, voiced: true, noiseGain: 0.2, viseme: 'Cons' },
  g: { sym: 'g', formants: [400, 1500, 2400, 3500, 4500], bwScale: 1.25, durMs: 60, voiced: true, noiseGain: 0.2, viseme: 'Cons' },
  f: { sym: 'f', formants: [400, 1300, 2500, 3500, 4500], bwScale: 1.6, durMs: 95, voiced: false, noiseGain: 0.7, viseme: 'Cons' },
  s: { sym: 's', formants: [350, 2000, 3200, 4500, 5500], bwScale: 1.6, durMs: 105, voiced: false, noiseGain: 0.85, viseme: 'Cons' },
  S: { sym: 'S', formants: [350, 1700, 2500, 3500, 4500], bwScale: 1.6, durMs: 105, voiced: false, noiseGain: 0.8, viseme: 'Cons' },
  v: { sym: 'v', formants: [400, 1300, 2500, 3500, 4500], bwScale: 1.5, durMs: 85, voiced: true, noiseGain: 0.35, viseme: 'Cons' },
  z: { sym: 'z', formants: [350, 2000, 3200, 4500, 5500], bwScale: 1.5, durMs: 95, voiced: true, noiseGain: 0.4, viseme: 'Cons' },
  Z: { sym: 'Z', formants: [350, 1700, 2500, 3500, 4500], bwScale: 1.5, durMs: 95, voiced: true, noiseGain: 0.4, viseme: 'Cons' },
  m: { sym: 'm', formants: [280, 900, 2200, 3300, 4300], bwScale: 1.0, durMs: 78, voiced: true, noiseGain: 0, viseme: 'Cons' },
  n: { sym: 'n', formants: [280, 1700, 2500, 3300, 4300], bwScale: 1.0, durMs: 75, voiced: true, noiseGain: 0, viseme: 'Cons' },
  gn: { sym: 'gn', formants: [300, 1900, 2600, 3400, 4400], bwScale: 1.0, durMs: 78, voiced: true, noiseGain: 0.05, viseme: 'Cons' },
  l: { sym: 'l', formants: [350, 1450, 2600, 3500, 4500], bwScale: 1.0, durMs: 72, voiced: true, noiseGain: 0, viseme: 'Cons' },
  R: { sym: 'R', formants: [460, 1300, 2150, 3400, 4400], bwScale: 1.2, durMs: 74, voiced: true, noiseGain: 0.22, viseme: 'ER' },
  w: { sym: 'w', formants: [310, 780, 2250, 3400, 4400], bwScale: 1.0, durMs: 68, voiced: true, noiseGain: 0, viseme: 'UW' },
  j: { sym: 'j', formants: [280, 2200, 2950, 3500, 4500], bwScale: 1.0, durMs: 65, voiced: true, noiseGain: 0, viseme: 'IY' },
  h: { sym: 'h', formants: [500, 1500, 2500, 3500, 4500], bwScale: 1.4, durMs: 75, voiced: false, noiseGain: 0.55, viseme: 'UH' },
};

function lookupPhoneme(sym: string): PhonemeSpec {
  return PHONEME_TABLE[sym] ?? PHONEME_TABLE['@']!;
}

/**
 * Rule-based FR/EN grapheme-to-phoneme converter mirroring `formant.rs`.
 */
export function phonemizeText(rawText: string): readonly PhonemeSpec[] {
  const cleaned = rawText.trim().toLowerCase();
  if (!cleaned) return [];

  const tokens: PhonemeSpec[] = [];
  let i = 0;

  while (i < cleaned.length) {
    const rest = cleaned.slice(i);
    const ch = cleaned[i]!;

    if (/\s/.test(ch)) {
      if (tokens.length > 0 && tokens[tokens.length - 1]?.sym !== '_') {
        tokens.push({ ...lookupPhoneme('_'), durMs: 42 });
      }
      i += 1;
      continue;
    }

    if (/[.,!?;:—–-]/.test(ch)) {
      tokens.push({ ...lookupPhoneme('_'), durMs: ch === ',' ? 95 : 155 });
      i += 1;
      continue;
    }

    // Trigraphs & Digraphs (FR/EN)
    if (rest.startsWith('eau')) {
      tokens.push(lookupPhoneme('o'));
      i += 3;
      continue;
    }
    if (rest.startsWith('oin')) {
      tokens.push(lookupPhoneme('w'), lookupPhoneme('in'));
      i += 3;
      continue;
    }
    if (rest.startsWith('ch') || rest.startsWith('sh')) {
      tokens.push(lookupPhoneme('S'));
      i += 2;
      continue;
    }
    if (rest.startsWith('gn')) {
      tokens.push(lookupPhoneme('gn'));
      i += 2;
      continue;
    }
    if (rest.startsWith('ph')) {
      tokens.push(lookupPhoneme('f'));
      i += 2;
      continue;
    }
    if (rest.startsWith('th')) {
      tokens.push(lookupPhoneme('f'));
      i += 2;
      continue;
    }
    if (rest.startsWith('qu')) {
      tokens.push(lookupPhoneme('k'));
      i += 2;
      continue;
    }
    if (rest.startsWith('ou') || rest.startsWith('oo')) {
      tokens.push(lookupPhoneme('u'));
      i += 2;
      continue;
    }
    if (rest.startsWith('oi')) {
      tokens.push(lookupPhoneme('w'), lookupPhoneme('a'));
      i += 2;
      continue;
    }
    if (rest.startsWith('ai') || rest.startsWith('ei') || rest.startsWith('è') || rest.startsWith('ê')) {
      tokens.push(lookupPhoneme('E'));
      i += rest.startsWith('ai') || rest.startsWith('ei') ? 2 : 1;
      continue;
    }
    if (rest.startsWith('au')) {
      tokens.push(lookupPhoneme('o'));
      i += 2;
      continue;
    }
    if (rest.startsWith('eu') || rest.startsWith('œu')) {
      tokens.push(lookupPhoneme('eu'));
      i += 2;
      continue;
    }
    if (rest.startsWith('ee') || rest.startsWith('ea')) {
      tokens.push(lookupPhoneme('i'));
      i += 2;
      continue;
    }
    if (/^(an|en|am|em)(?![aeiouyéèêëàâîïôùûü])/.test(rest)) {
      tokens.push(lookupPhoneme('an'));
      i += 2;
      continue;
    }
    if (/^(in|im|ain|ein)(?![aeiouyéèêëàâîïôùûü])/.test(rest)) {
      tokens.push(lookupPhoneme('in'));
      i += rest.startsWith('ain') || rest.startsWith('ein') ? 3 : 2;
      continue;
    }
    if (/^(on|om)(?![aeiouyéèêëàâîïôùûü])/.test(rest)) {
      tokens.push(lookupPhoneme('on'));
      i += 2;
      continue;
    }
    if (rest.startsWith('ll') && i > 0 && cleaned[i - 1] === 'i') {
      // e.g. "hakille" -> [a k i l] / [j]
      tokens.push(lookupPhoneme('l'));
      i += 2;
      continue;
    }

    // Single letters & accented chars
    switch (ch) {
      case 'a':
      case 'à':
      case 'â':
        tokens.push(lookupPhoneme('a'));
        break;
      case 'é':
        tokens.push(lookupPhoneme('e'));
        break;
      case 'e':
        // Word-final mute 'e' in French becomes very brief schwa or silent
        if (i === cleaned.length - 1 || /\s|[.,!?;:]/.test(cleaned[i + 1] ?? '')) {
          // silent caduc e
        } else {
          tokens.push(lookupPhoneme('@'));
        }
        break;
      case 'i':
      case 'î':
      case 'ï':
        tokens.push(lookupPhoneme('i'));
        break;
      case 'y':
        tokens.push(lookupPhoneme('j'));
        break;
      case 'o':
      case 'ô':
        tokens.push(lookupPhoneme('o'));
        break;
      case 'u':
      case 'ù':
      case 'û':
        tokens.push(lookupPhoneme('y'));
        break;
      case 'p':
        tokens.push(lookupPhoneme('p'));
        break;
      case 't':
        tokens.push(lookupPhoneme('t'));
        break;
      case 'k':
      case 'q':
        tokens.push(lookupPhoneme('k'));
        break;
      case 'c':
        tokens.push(
          /[eéèêëiy]/.test(cleaned[i + 1] ?? '') ? lookupPhoneme('s') : lookupPhoneme('k')
        );
        break;
      case 'ç':
      case 's':
        tokens.push(
          ch === 's' &&
            /[aeiouyéèêàâîôùû]/.test(cleaned[i - 1] ?? '') &&
            /[aeiouyéèêàâîôùû]/.test(cleaned[i + 1] ?? '')
            ? lookupPhoneme('z')
            : lookupPhoneme('s')
        );
        break;
      case 'b':
        tokens.push(lookupPhoneme('b'));
        break;
      case 'd':
        tokens.push(lookupPhoneme('d'));
        break;
      case 'g':
        tokens.push(
          /[eéèêëiy]/.test(cleaned[i + 1] ?? '') ? lookupPhoneme('Z') : lookupPhoneme('g')
        );
        break;
      case 'j':
        tokens.push(lookupPhoneme('Z'));
        break;
      case 'f':
        tokens.push(lookupPhoneme('f'));
        break;
      case 'v':
        tokens.push(lookupPhoneme('v'));
        break;
      case 'z':
        tokens.push(lookupPhoneme('z'));
        break;
      case 'x':
        tokens.push(lookupPhoneme('k'), lookupPhoneme('s'));
        break;
      case 'm':
        tokens.push(lookupPhoneme('m'));
        break;
      case 'n':
        tokens.push(lookupPhoneme('n'));
        break;
      case 'l':
        tokens.push(lookupPhoneme('l'));
        break;
      case 'r':
        tokens.push(lookupPhoneme('R'));
        break;
      case 'w':
        tokens.push(lookupPhoneme('w'));
        break;
      case 'h':
        // Aspirated h in "hakille" / English words
        if (i === 0 || /\s/.test(cleaned[i - 1] ?? '')) {
          tokens.push(lookupPhoneme('h'));
        }
        break;
      default:
        if (/[0-9]/.test(ch)) {
          tokens.push(lookupPhoneme('a'));
        }
        break;
    }

    // Collapse double identical consonants (`nn`, `mm`, `tt`, `ss`, `rr`)
    if (cleaned[i + 1] === ch && /[bcdfghklmnprstvz]/.test(ch)) {
      i += 2;
    } else {
      i += 1;
    }
  }

  return tokens;
}

class Resonator2ndOrder {
  private y1 = 0;
  private y2 = 0;

  public step(x: number, freqHz: number, bwHz: number, sr: number): number {
    const r = Math.exp((-Math.PI * bwHz) / sr);
    const c = -(r * r);
    const b = 2 * r * Math.cos((2 * Math.PI * freqHz) / sr);
    const a = 1 - b - c;
    const y = a * x + b * this.y1 + c * this.y2;
    this.y2 = this.y1;
    this.y1 = y;
    return y;
  }
}

/**
 * Differentiated Rosenberg glottal pulse (`0 <= phase < 1`).
 */
function rosenbergPulse(phase: number): number {
  const openQuota = 0.42;
  const closeQuota = 0.16;
  if (phase < openQuota) {
    const u = phase / openQuota;
    return 0.75 * Math.PI * Math.sin(Math.PI * u);
  }
  if (phase < openQuota + closeQuota) {
    const u = (phase - openQuota) / closeQuota;
    return -Math.cos((Math.PI * u) / 2);
  }
  return 0;
}

export function synthesizeKlattFormant(
  text: string,
  config: Partial<KlattVoiceConfig> = {}
): KlattSynthesisResult {
  const f0Base = Math.min(FORMANT_MAX_F0, Math.max(FORMANT_MIN_F0, config.f0BaseHz ?? 122));
  const rate = Math.min(1.6, Math.max(0.7, config.speedRate ?? 1.0));
  const breathiness = Math.min(0.45, Math.max(0.0, config.breathiness ?? 0.14));

  const phonemes = phonemizeText(text);
  if (phonemes.length === 0) {
    return {
      samples: new Float32Array(0),
      sampleRate: FORMANT_SAMPLE_RATE,
      durationMs: 0,
      timeline: [],
    };
  }

  const isQuestion = text.trim().endsWith('?');
  const timeline: TimelineSegment[] = [];
  let cursorMs = 0;
  for (const p of phonemes) {
    const dur = Math.max(24, p.durMs / rate);
    timeline.push({
      sym: p.sym,
      viseme: p.viseme,
      formants: p.formants,
      startMs: cursorMs,
      endMs: cursorMs + dur,
      voiced: p.voiced,
    });
    cursorMs += dur;
  }

  const totalSamples = Math.ceil((cursorMs / 1000) * FORMANT_SAMPLE_RATE);
  const out = new Float32Array(totalSamples);
  const resonators = [
    new Resonator2ndOrder(),
    new Resonator2ndOrder(),
    new Resonator2ndOrder(),
    new Resonator2ndOrder(),
    new Resonator2ndOrder(),
  ] as const;

  let glottalPhase = 0;
  let lfsr = 0x9e3779b9;
  const nextNoise = (): number => {
    lfsr ^= lfsr << 13;
    lfsr ^= lfsr >>> 17;
    lfsr ^= lfsr << 5;
    return ((lfsr >>> 0) / 0xffffffff) * 2 - 1;
  };

  let segIdx = 0;
  let curFormants = [...timeline[0]!.formants];
  const interpAlpha = 1 - Math.exp(-1 / (0.006 * FORMANT_SAMPLE_RATE)); // ~6 ms coarticulation

  for (let n = 0; n < totalSamples; n++) {
    const timeMs = (n / FORMANT_SAMPLE_RATE) * 1000;
    while (segIdx < timeline.length - 1 && timeMs >= timeline[segIdx]!.endMs) {
      segIdx += 1;
    }
    const seg = timeline[segIdx]!;
    const spec = lookupPhoneme(seg.sym);

    for (let k = 0; k < 5; k++) {
      curFormants[k] = curFormants[k]! + interpAlpha * (seg.formants[k]! - curFormants[k]!);
    }

    // Prosodic F0 contour: gentle natural declination, rising tail on '?'
    const progress = n / Math.max(1, totalSamples - 1);
    const prosodyFactor = isQuestion && progress > 0.72
      ? 0.94 + (progress - 0.72) * 0.85
      : 1.04 - 0.12 * progress;
    const f0 = f0Base * prosodyFactor;

    glottalPhase += f0 / FORMANT_SAMPLE_RATE;
    if (glottalPhase >= 1) glottalPhase -= 1;

    const segProgress =
      (timeMs - seg.startMs) / Math.max(1, seg.endMs - seg.startMs);
    const segEnv =
      Math.min(1, segProgress / 0.12) * Math.min(1, (1 - segProgress) / 0.14);

    const noise = nextNoise();
    const voicedExcitation = seg.voiced
      ? rosenbergPulse(glottalPhase) + breathiness * noise
      : 0;
    const fricativeExcitation = spec.noiseGain * 0.36 * noise;
    let signal = (voicedExcitation + fricativeExcitation) * segEnv;

    for (let k = 0; k < 5; k++) {
      signal = resonators[k]!.step(
        signal,
        curFormants[k]!,
        FORMANT_BW[k]! * spec.bwScale,
        FORMANT_SAMPLE_RATE
      );
    }

    // Soft tanh saturation (`[-1, 1]`) per `crates/hakille-voice/src/tts.rs`
    out[n] = Math.fround(Math.tanh(signal * 1.45) * 0.82);
  }

  return {
    samples: out,
    sampleRate: FORMANT_SAMPLE_RATE,
    durationMs: cursorMs,
    timeline,
  };
}
