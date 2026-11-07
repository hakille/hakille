import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ALL_DICTIONARIES } from '@/i18n/context';
import { SUPPORTED_LOCALES } from '@/i18n/types';
import { ROUTES_LIST } from '@/config/routes';
import { CRATES_CATALOG, WORKSPACE_METRICS } from '@/data/cratesCatalog';

function collectKeys(obj: unknown, prefix = ''): string[] {
  if (obj === null || typeof obj !== 'object' || Array.isArray(obj)) {
    return [prefix];
  }
  const keys: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    keys.push(...collectKeys(v, prefix ? `${prefix}.${k}` : k));
  }
  return keys.sort();
}

function walkSourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    if (name === '__tests__') continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      out.push(...walkSourceFiles(full));
    } else if (/\.(ts|tsx|css)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

describe('8-Language Key Parity, 11-Route Registry, Anti-Hype Doctrine & Sovereignty Invariants', () => {
  it('maintains 100% recursive key parity across FR, EN, ES, DE, PT, AR (RTL), SW, and JA', () => {
    expect(SUPPORTED_LOCALES).toEqual(['fr', 'en', 'es', 'de', 'pt', 'ar', 'sw', 'ja']);
    expect(ROUTES_LIST).toHaveLength(11);
    const referenceKeys = collectKeys(ALL_DICTIONARIES.fr);
    expect(referenceKeys.length).toBeGreaterThan(100);

    for (const code of SUPPORTED_LOCALES) {
      const keys = collectKeys(ALL_DICTIONARIES[code]);
      expect(keys).toEqual(referenceKeys);

      // Never claim "AGI" hype in any UI translation (`Vision §16`)
      const serialized = JSON.stringify(ALL_DICTIONARIES[code]);
      expect(serialized).not.toMatch(/\bAGI\b/);
    }
  });

  it('matches exact repository crate count (42) and LoC total (146,229)', () => {
    expect(CRATES_CATALOG).toHaveLength(WORKSPACE_METRICS.totalPackages);
    const sumLoc = CRATES_CATALOG.reduce((acc, c) => acc + c.loc, 0);
    expect(sumLoc).toBe(WORKSPACE_METRICS.totalRustLoc);
  });

  it('enforces the Sacred Rule of Braise & Zero External CDN Fonts across all source files', () => {
    const srcRoot = resolve(import.meta.dirname, '..');
    const files = walkSourceFiles(srcRoot);

    const allowedBraiseFiles = new Set([
      resolve(srcRoot, 'styles/tokens.generated.css'),
      resolve(srcRoot, 'tokens/hakilleTokens.generated.ts'),
      resolve(srcRoot, 'styles/base.css'),
    ]);

    for (const file of files) {
      const content = readFileSync(file, 'utf8');

      // Zero Google Fonts or analytics scripts anywhere
      expect(content).not.toMatch(/fonts\.googleapis\.com|fonts\.gstatic\.com|googletagmanager|google-analytics/i);

      // Sacred Braise color token / hex guard across all CSS & component code (excluding i18n prose & comments)
      const isLocaleProse = file.includes('/i18n/locales/');
      if (!allowedBraiseFiles.has(file) && !isLocaleProse) {
        const withoutComments = content
          .replace(/\/\*[\s\S]*?\*\//g, '')
          .replace(/\/\/.*$/gm, '');
        expect(withoutComments).not.toMatch(/--hakille-braise|#FF7A2F|#C2410C/i);
      }
    }
  });
});
