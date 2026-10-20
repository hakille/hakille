# Security Policy — Hakille (pre-alpha prototype)

> Status: **prototype / pre-alpha**. Do not deploy with production secrets,
> biometric auth, autonomous payments, or unreviewed network egress.

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x (workspace) | Best-effort fixes only, no SLA |

## Reporting a vulnerability

Open a private security advisory on GitHub (preferred). Do not open a public
issue for a vulnerability. Include:

- crate + version, OS, reproduction steps
- capability token scope involved (if any)
- whether `with_trusted_root` was pinned

We aim to acknowledge within 72h and publish a fix + CVE note.

## Current guarantees (post 2026-10-11 hardening)

- Windows substrate enforces Biscuit scope/action at point of effect
  (`read/write/list/delete`, network, audio, camera, screen, GUI input).
- `VirtualFS` path traversal (`..`, drive letters, UNC, NUL) is rejected and
  confined to the sandbox root.
- Pinned-root verification (`with_trusted_root`) rejects attacker
  self-signed tokens. Unpinned mode remains for tests only.
- Audit journal is persistent (`LOCALAPPDATA/Hakille/audit.redb` or
  `./hakille_data/Hakille/audit.redb`), Ed25519-chained.
- With artifacts provisioned, inference is **real and golden-tested**: Silero
  VAD, SenseVoice CTC ASR, CAM++ and MiniLM embeddings via `ort`, Qwen 2.5
  Q4_K_M via Candle, Mimi RVQ codebooks, formant TTS, and CV grounding.
  Missing artifacts produce explicit errors or `SKIP`, never simulated output.

## Explicitly NOT guaranteed yet

- No TPM/VBS hardware attestation (local dev signature only).
- UI-TARS VLM forward is not implemented (CV grounding only); Moshi 7.6B is not
  provisioned; ASR has no beam search; TTS is non-neural.
- No WASM `StoreLimits`, network/file sandboxing per agent (fuel only).
- No durable key management (ephemeral Biscuit root + journal keys per boot),
  no rotation/revocation store, no TiKV replication auth.
- `http_request` defaults to loopback-only and enforces the configured host
  allowlist, but has no TLS pinning.
- No DPIA for voice biometrics; liveness is heuristic RMS/ZCR, never
  sufficient for payments or strong auth.

See `docs/AUDIT_PROJET_2026-10-08.md` for the full gap analysis.
