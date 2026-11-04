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

## Current guarantees (hardening reviewed 2026-11-03)

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
  Q4_K_M via Candle, the complete neural Mimi codec, and UI-TARS through a
  separately configured local llama.cpp server and verified vision projector.
  Formant TTS and CV grounding remain separate non-neural components.
  Missing artifacts produce explicit errors or `SKIP`, never simulated output.

## Explicitly NOT guaranteed yet

- No TPM/VBS hardware attestation (local dev signature only).
- UI-TARS accepts only an explicitly configured loopback server. Model alias
  validation cannot attest that another local process loaded the intended weights;
  the server operator remains trusted. Proposals never execute GUI actions.
- Moshi 7.6B is not provisioned; ASR rejects unavailable beam search; TTS is non-neural.
- The reference WASM runner has a custom `ResourceLimiter`, fuel and epoch
  interruption; it exposes no host imports. Bounds do not establish native OS
  isolation or a mediated filesystem/network API for business agents. The six
  substrate `spawn_sandboxed` entry points reject execution explicitly until a
  runner is connected; a valid capability alone does not launch a process.
- The daemon loads capability-root and journal keys through the OS keystore.
  A **plaintext-file fallback** is allowed only at a first development boot when
  the root keychain is unavailable and no signed history exists. Existing raw
  identities stay on that backend in development; production refuses them.
  Corrupt, partial or missing identities with existing history stop startup;
  the daemon does not silently regenerate keys or migrate backends. Unix raw
  seeds are created with mode 0600 and permissive existing files are refused.
  The configured Linux `linux-native` keyring is session-volatile. Registry
  rotation/revocation helpers exist, but end-to-end migration, recovery and
  cross-session persistence still require validation; no authenticated TiKV
  replication deployment is established.
- `http_request` defaults to loopback-only, checks redirect destinations against
  the configured host allowlist, rejects HTTPS-to-HTTP downgrade and bounds the
  response while streaming. It has no TLS pinning and host allowlisting alone
  does not establish an IP-level policy against DNS rebinding.
- The optional encrypted-file keystore helper uses authenticated v2 storage
  (Argon2id and ChaCha20Poly1305). Legacy unauthenticated v1 files are refused
  without replacement; migration requires an explicit recovery procedure from
  a trusted backup. This helper is **not** the daemon's plaintext fallback and
  remains disallowed when `HAKILLE_PROD=1/true`. File and lock operations require
  a trusted parent directory; Windows ACLs are inherited, not hardened here.
- The daemon’s outbox snapshots use atomic replacement and report persistence failures, but
  the JSON outbox and redb audit journal do not share one transaction. Crash
  recovery across those resources must be addressed before a business executor
  consumes queued actions. `:flush` refuses transmission while none is wired.
- No DPIA for voice biometrics; liveness is heuristic RMS/ZCR, never
  sufficient for payments or strong auth.

See [platform analysis](docs/ANALYSE_MULTIPLATEFORME_2026-11-03.md) and
[implementation review](docs/ANALYSE_APPROFONDIE_2026-11-03.md) for scope and remaining gaps.
