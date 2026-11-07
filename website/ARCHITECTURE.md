# Architecture of `@hakille/website`

## 1. Design Goals

1. **Zero Drift with the Rust Kernel**: Colors, motion durations, cubic-bezier curves, Karplus-Strong MIDI sequences, and typography scales are never duplicated by hand. `scripts/sync-from-repo.mjs` parses `../tokens/{color,motion,type}.toml` at build time and emits `src/styles/tokens.generated.css` and `src/tokens/hakilleTokens.generated.ts`.
2. **Prove Rather Than Promise**: Every core claim of Hakille is backed by a deterministic client-side engine in `src/engine/`:
   - `sound.ts`: Bit-accurate 48 kHz Karplus-Strong synthesis (`xorshift64`, `KS_DAMPING = 0.996`, 2 ms / 15 ms anti-click ramps) matching `crates/hakille-design/src/sound.rs`.
   - `klattFormant.ts`: Real-time 24 kHz 5-Formant Klatt speech synthesis (`F1..F5` cascaded resonators, differentiated Rosenberg glottal pulse, FR/EN grapheme-to-phoneme rules, 11 mouth visemes, `< 80 ms` Barge-In ramp) ported from `crates/hakille-voice/src/formant.rs`.
   - `hold.ts`: Monotonic `600 ms` hold state machine + 2-step accessible confirmation matching `crates/hakille-design/src/hold.rs`.
   - `auditChain.ts`: Real WebCrypto `SHA-256` hash chaining + `Ed25519` digital signatures with live tamper simulation and 3-level `WhySheet`.
   - `cognitiveSimulators.ts`: Ambient dB `ChannelSelector`, `3/day` `SilenceBudget`, `Crank 0..4` autonomy ladder, Constitutional `ConfidenceEngine` (`0.6M + 0.4G`), and `SPO` memory decay with mandatory **5-minute tombstone-free physical purge** of `cognition:*` affective signals (EU AI Act).
3. **Sovereign Web Delivery**:
   - Pure Vanilla CSS (`@layer tokens, base, layout, simulators, pages` + `@media print` official dossier stylesheet) with logical RTL layout properties across 8 UI languages (`fr`, `en`, `es`, `de`, `pt`, `ar`, `sw`, `ja`).
   - Static Site Generation (`scripts/prerender.mjs`) producing standalone HTML across all 11 canonical routes so crawlers, terminal browsers, and NoScript users receive full content.
   - `scripts/audit-sovereignty.mjs` fails the build if any route lacks a strict CSP, contains `!= 1` `<h1>` tag, or references an external CDN URL.
