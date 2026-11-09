# ADR 021: Cryptographically Bound Audit Trail in redb with SHA-256 Merkle Chaining & Token Binding

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Principal Cryptographer, Audit Architect
- **Target Crate**: `crates/hakille-audit`

## Context & Problem Statement

In an autonomous agent system executing local and external tools, post-incident forensics, accountability, and regulatory compliance demand an immutable, tamper-evident audit log.
Traditional plain-text logs or syslog are vulnerable to:
1. Log tampering or deletion by a compromised tool or root adversary.
2. Token substitution attacks where an attacker claims an action was authorized by Token A when it was executed using Token B.
3. Repudiation: inability to mathematically prove that the host authorized the execution.
Hakille already contains a basic audit journal in `crates/hakille-audit` backed by `redb 2.0`.
We must formalize the cryptographic binding between capability invocations, Biscuit tokens, Merkle hash chains, and Ed25519 signatures.

## Decision Drivers

- **Tamper Evidence**: Every entry must include `prev_hash` (SHA-256) of the prior entry, creating an immutable hash chain.
- **Anti-Substitution Token Binding**: Every entry must store `token_hash = sha256_token_hash(&biscuit_bytes)`.
- **Host Non-Repudiation**: Entries must be signed using the sovereign daemon's Ed25519 key.
- **ACID Local Storage**: Pure-Rust embedded storage with transactions (`redb 2.0`).
- **Non-Destructive Reversals**: Reverting an action writes a signed `ReversalFlag`, never modifying historical entries.

## Considered Alternatives

1. **External Cloud Logging (Datadog / CloudWatch)**:
   - *Cons*: Violates user sovereignty; leaks private operations to third-party servers.
   - *Verdict*: Rejected.
2. **SQLite Relational Audit Table**:
   - *Cons*: C dependency; lacks built-in cryptographic Merkle chaining.
   - *Verdict*: Rejected.
3. **Dedicated redb 2.0 Hash-Chained Ledger with Biscuit Token Binding**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

The cryptographic audit ledger is structured as follows:
1. **Entry Schema (`AuditEntry`)**:
   - `id`: EntryId (UUIDv7 sequential).
   - `timestamp`: Timestamp (UTC).
   - `actor`: ActorId (Daemon / Agent / User).
   - `action`: ActionRecord (type, params, reversible, reversed).
   - `capability_used`: Option<CapabilityScope>.
   - `prev_hash`: Vec<u8> (SHA-256 of entry $N-1$).
   - `token_hash`: [u8; 32] (SHA-256 of serialized Biscuit token bytes).
   - `signature`: Vec<u8> (Ed25519 host signature over entry payload).
2. **Hash Chain Verification**:
   - $H_n = \text{SHA-256}(H_{n-1} \,\|\, \text{Serialize}(\text{Entry}_n))$.
   - Any alteration of historical entry $k$ ($k < n$) immediately breaks chain verification at entry $k+1$.
3. **Token Anti-Substitution Binding**:
   - Mathematical proof that the specific Biscuit token was presented at invocation time.
4. **Non-Destructive Reversals**:
   - Reversal operations append a new `ReversalFlag` record, preserving immutable historical provenance.

## Consequences

- **Positive**: Complete forensic integrity; mathematical proof linking Biscuit token to execution; non-destructive reversible saga; pure Rust ACID storage.
- **Negative**: Adds storage overhead per execution (~500 bytes per entry).
