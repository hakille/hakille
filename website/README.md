# `@hakille/website` — Official Showcase, Interactive Laboratories & Legal Trust Center (`hakille.ai`)

This directory contains the official open-source website, interactive product laboratories, and regulatory compliance center for **Hakille** (*"intelligence, discernment, wisdom"* in Pulaar/Fulani).

True to Hakille's engineering doctrine (*"The system that forgets nothing and always proves what it did"*), **`website/` practices every principle the operating system preaches**:

- **Single Source of Truth with Rust**: `scripts/sync-from-repo.mjs` reads [`../tokens/color.toml`](../tokens/color.toml), [`../tokens/motion.toml`](../tokens/motion.toml), and [`../tokens/type.toml`](../tokens/type.toml) directly and verifies the `SHA-256` checksum of [`../mobile/ios/Hakille/Fonts/InterVariable.ttf`](../mobile/ios/Hakille/Fonts/InterVariable.ttf) (`SIL OFL 1.1`).
- **Zero Exfiltration**: `0` tracking cookies, `0` third-party analytics scripts, `0` external CDN or Google Fonts requests, enforced by strict Content-Security-Policy (`default-src 'self'`) and automated build gates.
- **Sacred Rule of Braise**: `#FF7A2F` (dark) / `#C2410C` (light) is structurally restricted to `validation`, `hold-progress`, and `decision-ring` — verified by unit tests and static codebase scanning.
- **Multilingual & RTL by Construction**: 8 complete UI languages (`fr`, `en`, `es`, `de`, `pt`, `ar` with logical `dir="rtl"` mirroring, `sw` Kiswahili, and `ja`) plus West African **Adlam (`𞤖𞤢𞤳𞤭𞤤𞥆𞤫`)** and **N'Ko (`ߤߊߞߌߟߌ`)** typographic specimens.
- **Static Multi-Route Pre-Rendering (SSG)**: All 11 routes pre-render into clean, semantic, zero-JS-readable HTML with `sitemap.xml` (`hreflang`), official print/PDF `@media print` dossier sheets, and RFC 9116 `/.well-known/security.txt`.

---

## Quickstart

```bash
cd website
npm install

# Run automated unit & sovereignty invariant tests (Vitest)
npm test

# Start local development server (syncs ../tokens/*.toml automatically)
npm run dev

# Build full static pre-rendered bundle + run sovereignty verification gate
npm run build

# Preview production static bundle
npm run preview
```

---

## Directory Layout

```text
website/
├── scripts/
│   ├── sync-from-repo.mjs      # Syncs ../tokens/*.toml + SHA-256 verifies fonts/logos
│   ├── prerender.mjs           # SSG pre-renderer for all 11 routes + 8-language sitemap.xml
│   └── audit-sovereignty.mjs   # CI gate: 1x <h1> per route, strict CSP, 0 external URLs
├── public/
│   ├── .well-known/security.txt # RFC 9116 Coordinated Vulnerability Disclosure
│   ├── brand/                  # Synced official static emblems from ../logo/
│   └── fonts/                  # Synced SIL OFL 1.1 InterVariable.ttf + OFL.txt + PROVENANCE.txt
└── src/
    ├── config/                 # Route registry (11 routes) + centralized LCEN/GDPR legal constants
    ├── tokens/                 # Auto-generated TS constants from ../tokens/*.toml
    ├── engine/                 # Pure TS ports: 48kHz Karplus-Strong, 24kHz Klatt 5-Formant TTS, 600ms Hold, WebCrypto SHA-256/Ed25519 Audit, SPO Decay
    ├── data/                   # Ground-truth catalogs: 41 Cargo crates, 15 DSP/AI models, 10 Constitutional Rules, 3 Tiers
    ├── i18n/                   # Strict `TranslationSchema` + 8 locales (FR, EN, ES, DE, PT, AR RTL, SW, JA) + Adlam/N'Ko
    ├── components/
    │   ├── layout/             # Header, Footer, Accessibility Drawer, SEO Meta sync
    │   ├── primitives/         # BraiseHoldButton (sole Braise primitive) + PrintDossierButton
    │   └── simulators/         # 9 interactive cryptographic, acoustic, formant, mobile & cognitive laboratories
    ├── pages/                  # 11 complete product, installer/RAM sizer, architecture, security, constitution & legal pages
    ├── styles/                 # Layered Vanilla CSS (`@layer tokens, base, layout, simulators, pages` + `@media print`)
    └── __tests__/              # Automated verification of Karplus-Strong, Klatt Formant TTS, WCAG contrast, Braise guard, audit chain & 8-locale parity
```

## License

Licensed under the **Apache License, Version 2.0** ([`../LICENSE-APACHE`](../LICENSE-APACHE)), identical to the rest of the `hakille/hakille` workspace.
