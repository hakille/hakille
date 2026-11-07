/**
 * Opt-in WebAudio PCM Player for Hakille's Karplus-Strong Signals & 24 kHz Klatt Formant Voice.
 *
 * Strictly silent unless triggered by user interaction (respecting Hakille's
 * Silence Doctrine and browser autoplay ethics). Supports < 80 ms Barge-In cutoff.
 */

import { SAMPLE_RATE_HZ, StateSoundKind, pluckKoraWire, renderStateSound } from './sound';

class HakilleAudioController {
  private ctx: AudioContext | null = null;
  private lastWirePluckAt = 0;
  private activeVoiceSource: AudioBufferSourceNode | null = null;
  private activeVoiceGain: GainNode | null = null;

  private ensureContext(): AudioContext | null {
    if (typeof window === 'undefined') return null;
    const AudioCtx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioCtx) return null;
    if (!this.ctx) {
      this.ctx = new AudioCtx({ sampleRate: SAMPLE_RATE_HZ });
    }
    if (this.ctx.state === 'suspended') {
      void this.ctx.resume();
    }
    return this.ctx;
  }

  public playPcm(samples: Float32Array, sampleRate = SAMPLE_RATE_HZ): void {
    const ctx = this.ensureContext();
    if (!ctx || samples.length === 0) return;

    const buffer = ctx.createBuffer(1, samples.length, sampleRate);
    buffer.getChannelData(0).set(samples);

    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);
    source.start();
  }

  /**
   * Plays synthesized 24 kHz Klatt Formant speech on explicit user trigger,
   * returning an instant (< 20 ms ramp, well inside Hakille's < 80 ms Barge-In SLA) cutoff handle.
   */
  public playKlattUtterance(
    samples: Float32Array,
    sampleRate: number,
    onEnded?: () => void
  ): () => void {
    this.stopActiveVoice();
    const ctx = this.ensureContext();
    if (!ctx || samples.length === 0) {
      onEnded?.();
      return () => undefined;
    }

    const buffer = ctx.createBuffer(1, samples.length, sampleRate);
    buffer.getChannelData(0).set(samples);

    const gain = ctx.createGain();
    gain.gain.setValueAtTime(0.88, ctx.currentTime);
    gain.connect(ctx.destination);

    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(gain);

    this.activeVoiceSource = source;
    this.activeVoiceGain = gain;

    source.onended = () => {
      if (this.activeVoiceSource === source) {
        this.activeVoiceSource = null;
        this.activeVoiceGain = null;
      }
      onEnded?.();
    };

    source.start();
    return () => this.stopActiveVoice();
  }

  /**
   * Instant < 80 ms Barge-In cutoff (click-free 18 ms linear de-click).
   */
  public stopActiveVoice(): void {
    if (!this.ctx || !this.activeVoiceSource || !this.activeVoiceGain) return;
    try {
      const now = this.ctx.currentTime;
      this.activeVoiceGain.gain.cancelScheduledValues(now);
      this.activeVoiceGain.gain.setValueAtTime(this.activeVoiceGain.gain.value, now);
      this.activeVoiceGain.gain.linearRampToValueAtTime(0, now + 0.018);
      const src = this.activeVoiceSource;
      this.activeVoiceSource = null;
      this.activeVoiceGain = null;
      window.setTimeout(() => {
        try {
          src.stop();
          src.disconnect();
        } catch {
          // Already stopped
        }
      }, 22);
    } catch {
      this.activeVoiceSource = null;
      this.activeVoiceGain = null;
    }
  }

  public playStateSignal(kind: StateSoundKind, enabled: boolean, volume = 0.6): void {
    if (!enabled) return;
    this.playPcm(renderStateSound(kind, volume));
  }

  public playKoraWire(wireIndex: number, enabled: boolean, volume = 0.42): void {
    if (!enabled) return;
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
    if (now - this.lastWirePluckAt < 35) return;
    this.lastWirePluckAt = now;
    this.playPcm(pluckKoraWire(wireIndex, volume, 360));
  }
}

export const hakilleAudio = new HakilleAudioController();
