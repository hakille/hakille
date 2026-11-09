# ADR-004: Defense-in-Depth with Biscuit v5 Datalog Attenuation & 4-Tier Risk Classification

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Chief Security Officer, Kernel Architect
- **Target Modules**: `crates/hakille-core/src/capability.rs`, `crates/hakille-capability/src/security/`

## Context & Problem Statement

Extensible AI systems must never operate with ambient authority. When tools possess excessive privileges or when an LLM is manipulated by indirect prompt injection, sensitive user data can be exfiltrated or local systems damaged.
Existing security mechanisms in Hakille included:
- `biscuit-auth 5` capability tokens verified against a pinned root key (`PinnedRootPublicKey`).
- `RevocationStore` with persistent nonces in Redb.
- An action confirmation simulation mechanism (`DryRunSimulator`) with a 600ms hold-to-confirm UI ActionCard.
However, these mechanisms were mapped to ad-hoc string checks. Hakille requires an objective, formal classification and monotonic attenuation model for all capabilities.

## Decision Drivers

- **Zero Ambient Authority**: Capabilities must carry mathematical proof of authorization.
- **Monotonic Attenuation**: Sub-agents and child processes can only reduce permissions, never escalate.
- **Proportionate Friction**: Read-only operations execute with zero friction; critical actions require physical human confirmation.
- **Credential Protection**: Tools must never see raw secrets or API keys.

## Considered Alternatives

1. **Static Role-Based Access Control (RBAC)**:
   - *Cons*: Cannot express dynamic contextual caveats (e.g. "allow write only to `/workspace/docs` before 5:00 PM").
   - *Verdict*: Rejected.
2. **System Prompt Instructions Alone ("Please don't delete files")**:
   - *Cons*: Trivial to bypass with prompt injection and jailbreaks.
   - *Verdict*: Completely rejected.
3. **Defense-in-Depth with Biscuit v5 Datalog Attenuation + 4-Tier Risk Classification + Zero-Knowledge Secret Egress**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

Hakille formalizes security enforcement across four layers:
1. **The 4-Tier Risk Classification**:
   - **Tier 0 (ReadOnly)**: Math, datetime, cached queries. Auto-granted silently.
   - **Tier 1 (StateMutating)**: Local writes within active workspace (`/vfs/workspace`). Auto-granted in trusted projects with ambient HUD indicator.
   - **Tier 2 (ExternalNetwork)**: Outbound HTTP to public endpoints. Domain allowlist check and anti-SSRF verification.
   - **Tier 3 (CriticalExecution)**: Shell commands, file deletion, financial actions, camera/mic. Mandatory physical human consent (**600ms hold-to-confirm ActionCard ring**).
2. **Biscuit Datalog Attenuation**:
   - Tokens attenuated at each delegation step via Datalog caveats (`check if path.starts_with(...)`, `check if time <= ...`).
   - Nonces registered in `RevocationStore` for instant revocation.
3. **Zero-Knowledge Secret Egress**:
   - Tools declare `secret_ref`. The host egress proxy verifies Biscuit rights and injects `Authorization: Bearer <token>` directly into socket buffers. The guest runtime never observes raw secret bytes.
4. **Anti-SSRF Protection**:
   - Sockets strictly reject loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16`), and private RFC 1918 addresses.

## Consequences

- **Positive**: Formal mathematical verification of every tool call; absolute user sovereignty over critical operations; zero raw secret exposure.
- **Negative**: Adds Biscuit authorizer evaluation latency (~50–100 µs per invocation).
