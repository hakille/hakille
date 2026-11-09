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

## Mobile and synchronization contracts (2026-11-04)

- Native mobile dictation is opt-in and uses the OS on-device recognizer:
  Android API 31+ requires an available local engine; iOS 16+ requires
  `supportsOnDeviceRecognition` and sets `requiresOnDeviceRecognition`.
  Unavailable engines or language resources fail explicitly, with no cloud
  fallback configured. Permissions are requested on user action; capture is
  bounded and stops on backgrounding or disconnect. An Android permission dialog
  that pauses the activity invalidates the pending start and requires a new tap;
  iOS distinguishes permission-dialog inactivity and waits for the app to become
  active before starting the authorized capture. Only final text is added
  to the draft in dictation mode, without submitting a command or approving a card. There is
  no always-on mobile wake word or bundled Hakille model. This contract does
  not independently audit each OS/vendor recognizer or establish physical
  microphone accuracy and offline behavior on every supported device.
- A separate, explicitly enabled voice conversation mode submits final text
  to the connected hub after each user-started capture. Rust binds the turn to
  an expiring context and request identity; only its matching final reply may
  be spoken. Decision cards, backgrounding and session changes invalidate
  automatic speech. There is no automatic microphone rearming. Speech output
  uses installed local voices and bounded playback; audio is not sent to the hub.
- Phone capabilities use native user-driven camera/document pickers, foreground
  location and a fixed application destination allowlist. Previews and coordinates
  are ephemeral, are not uploaded, and cannot be requested by model text. Document
  reads are bounded to 1 MiB on Android and 8 MiB on iOS. Cancellation and the
  ten-second read deadline do not forcibly stop an unresponsive provider or kernel
  call; iOS checks cancellation/deadline between read blocks. Late results are
  discarded. Android clears previews on panel exit or activity pause, except that
  an explicitly launched external picker/camera may return after the pause. iOS
  retains previews across tab changes until explicit clearing or backgrounding.
  Opening external apps requires confirmation;
  opening documentation contacts GitHub. External apps and file providers retain
  their own storage/network policies. These features do not grant arbitrary
  cross-application automation or unrestricted filesystem access.
- Before starting HTTP, native hosts await an atomic, synchronized **v2
  checkpoint** containing the draft, request counter and bounded uncertainty
  markers. Secrets and replayable request payloads are excluded; the draft
  itself can contain personal data. Android uses private app storage with
  backups disabled; iOS uses protected files excluded from cloud backup.
  Corrupt checkpoints are refused without replacement. An in-flight mutation
  restores as `unknown`, with no automatic replay. A known storage failure
  before transmission is reported as not sent; after transmission starts,
  cancellation cannot prove that the hub stopped or that no effect occurred.
- Status and journal reads remain available during recovery. Mutations remain
  blocked until uncertainty warnings are explicitly acknowledged. Acknowledging
  a warning only archives it locally: it neither confirms success nor undoes
  the operation. Disconnecting or changing hubs preserves warnings. Their
  bounded archives are not a durable audit history and contain no hub identity;
  users must consult the relevant hub's journal. Last-keystroke durability is
  distinct from the mandatory checkpoint barrier before network transmission.
- Signed handoff protocol **v2** in `hakille-sync` requires an explicitly
  approved `(device identity, public key)` pair from a trusted channel.
  Incoming messages cannot approve their own key. Legacy v1 handoffs are
  rejected; key rotation and revocation are explicit. Signatures bind the
  destination and content; bounded expiry and replay checks reject invalid
  transfers. The gateway bearer token is not this peer identity. Hosts must
  protect keys, persist the replay snapshot before using a received context,
  and restore it before accepting further transfers after restart. Protected
  persistence and network pairing are not automatically integrated into the
  mobile applications; rollback-resistant replay storage is not established.
- CRDT merging retains one deterministic version per identifier and gives
  deletion tombstones precedence. It is not lossless semantic conflict
  resolution; a global vector clock is not per-element causality. Transfers
  currently carry full state. Tombstones are retained rather than deleted by
  age alone, so storage growth remains a limitation. Applying the CRDT to the
  knowledge graph does not yet propagate deletions. These primitives do not
  establish a deployed, convergent multi-device graph service.

See the [native mobile guide](mobile/README.md),
[checkpoint contract](crates/hakille-mobile/README.md),
[synchronization contract](crates/hakille-sync/README.md) and
[second-iteration validation report](docs/VALIDATION_MOBILE_SUITE_2026-11-04.md).
APK inspection, Android emulator execution and iOS XCTest execution are
separate evidence. The [design/voice validation report](docs/VALIDATION_DESIGN_VOIX_MOBILE_2026-11-04.md)
records commit-specific native CI evidence. Physical-device speech, camera,
location and accessibility validation remain required.

## Explicitly NOT guaranteed yet

- The optional mobile gateway adds an authenticated HTTP listener on IPv4
  loopback. It is a separate process with a separate bearer token, which grants
  access to the exposed hub operations. Remote mobile access requires a trusted
  HTTPS reverse proxy or tunnel; the gateway supplies no TLS or automatic device
  pairing. The daemon accepts concurrent IPC clients and serializes cognitive turns. See
  [ADR-011](adr/011-mobile-native-client-and-hub.md).
- Mobile native credential storage does not establish hardware attestation or
  a paired-device identity. The mobile substrate reports absent drivers and
  unknown measurements explicitly. Its shadow queue is an acknowledged local
  synchronization primitive, not a deployed TiKV/APNs/FCM service.

- The continuous memory database (`memory.redb`) stores full conversations,
  facts and correction history locally in plaintext (owner-only file mode on
  Unix). Logical forgetting removes active records and explicitly associated
  episodes; it does not securely erase old database pages, legacy JSON backups,
  exports or the independent audit journal. Exact-text association does not
  detect every paraphrase. See [memory lifecycle](docs/MEMORY_END_TO_END.md).

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

### Recherche web explicite

Les pages sont des données externes, pas des instructions ni des faits certains.
Le harness exige une capability liée à la requête, un journal durable et des
budgets d'outils. Les lectures publiques valident HTTPS, port, adresse et chaque
redirection ; les corps sont bornés et aucun cookie de navigateur n'est transmis.
Le transport direct épingle le DNS. Un proxy explicitement configuré reçoit la
confiance nécessaire à la résolution/connexion finale ; sa politique réseau
doit protéger les adresses internes. TLS reste vérifié, y compris avec un bundle
CA privé explicitement configuré. Voir [le contrat réseau](docs/HARNESS_END_TO_END.md).
