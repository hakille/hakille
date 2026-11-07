# Contributing to `@hakille/website`

Thank you for contributing to Hakille's public interface and trust documentation.

## Invariants Checked Before Merge

Every pull request touching `website/` must pass:

1. `npm test` — Verifies:
   - Karplus-Strong synthesis determinism, zero-click boundaries, and durations (`180 ms`, `220 ms`, `320 ms`).
   - WCAG 2.1 AAA/AA contrast ratios across Obsidian Dark (`#0B0C0F`) and Parchment Light (`#F6F4F0`).
   - **Sacred Rule of Braise**: `--hakille-braise` (`#FF7A2F` / `#C2410C`) is never used outside `validation`, `hold-progress`, and `decision-ring`.
   - 100% key parity across all 6 UI languages (`fr`, `en`, `es`, `de`, `ar`, `ja`) and absence of prohibited "AGI" hype.
   - WebCrypto `SHA-256` + `Ed25519` audit chain tamper detection.
2. `npm run typecheck` — Zero TypeScript errors under `strict: true` and `noUncheckedIndexedAccess: true`.
3. `npm run build` — Full client + SSR + 10-route static pre-render + `audit:sovereignty` gate.

## Adding a New Language

1. Add the ISO code to `LocaleCode` and `SUPPORTED_LOCALES` in `src/i18n/types.ts` (specify `dir: 'rtl'` if right-to-left).
2. Create `src/i18n/locales/<code>.ts` typed as `TranslationSchema` — TypeScript will enforce every single required key at compile time.
3. Register the dictionary in `src/i18n/context.tsx` and run `npm test`.

## Updating Formal Legal Registration Details

Edit `src/config/legal.ts` (`LEGAL_CONFIG.publisher` and `LEGAL_CONFIG.hosting`) and re-run `npm run build`. All legal pages (`/mentions-legales`, `/confidentialite`, `/conditions`, `/securite`) consume this single module.
