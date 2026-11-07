/**
 * Hakille — Color Contrast & Sacred Braise Guard
 *
 * Faithful port of `crates/hakille-design/src/color.rs`.
 * Enforces:
 *   1. WCAG 2.1 relative luminance and contrast ratio verification.
 *   2. The Sacred Rule of Braise (`#FF7A2F` dark / `#C2410C` light):
 *      permitted EXCLUSIVELY in `validation`, `hold-progress`, and `decision-ring`.
 */

import { COLOR_TOKENS } from '@/tokens/hakilleTokens.generated';

export type BraiseContext = (typeof COLOR_TOKENS.braise.allowed_in)[number];

const ALLOWED_SET = new Set<string>(COLOR_TOKENS.braise.allowed_in);

export function braiseAllowed(context: string): context is BraiseContext {
  return ALLOWED_SET.has(context);
}

export function parseHexRgb(hex: string): readonly [number, number, number] {
  const cleaned = hex.trim().replace(/^#/, '');
  if (!/^[0-9a-fA-F]{6}$/.test(cleaned)) {
    throw new Error(`Invalid 6-digit hex color: ${hex}`);
  }
  const r = Number.parseInt(cleaned.slice(0, 2), 16);
  const g = Number.parseInt(cleaned.slice(2, 4), 16);
  const b = Number.parseInt(cleaned.slice(4, 6), 16);
  return [r, g, b] as const;
}

function channelLinear(c8: number): number {
  const s = c8 / 255;
  return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}

export function relativeLuminance(hex: string): number {
  const [r, g, b] = parseHexRgb(hex);
  return 0.2126 * channelLinear(r) + 0.7152 * channelLinear(g) + 0.0722 * channelLinear(b);
}

export function contrastRatio(hexA: string, hexB: string): number {
  const lumA = relativeLuminance(hexA);
  const lumB = relativeLuminance(hexB);
  const lighter = Math.max(lumA, lumB);
  const darker = Math.min(lumA, lumB);
  return (lighter + 0.05) / (darker + 0.05);
}
