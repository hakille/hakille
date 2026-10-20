# Contributing to Hakille

Hakille is a pre-alpha sovereign cognitive operating system written in Rust.
Contributions are welcome, but the project deliberately prefers explicit
failures and documented limits over simulated success.

Before contributing, read:

- [README.md](README.md) — status, architecture, and quick start.
- [SECURITY.md](SECURITY.md) — security guarantees and private reporting.
- [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) — build, test, and
  substrate details.

## Reporting security issues

Do **not** open a public issue for a vulnerability. Use GitHub's private
security advisory flow described in [SECURITY.md](SECURITY.md).

## Prerequisites

- Rust **1.95** (the repository's `rust-toolchain.toml` pins the toolchain).
- `cargo`, `rustfmt`, and `clippy`.
- On Linux: `libasound2-dev` and `pkg-config`.
- Model artifacts are **not** committed (about 2 GB). Tests and features that
  need them print an explicit `SKIP` or return a clear error when absent. See
  [docs/MODELS.md](docs/MODELS.md).

## Build and test

```bash
cargo build --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps
cargo test --workspace --locked
cargo run -p xtask -- tokens --check
cargo run -p xtask -- conformance
cargo deny check bans sources advisories
```

The full `cargo test` run may be slow when local model artifacts are present.
CI runs the same gates on Windows and Ubuntu, plus macOS and UI-quality jobs.

## Honesty contract

Every contribution must preserve the project's honesty contract:

- No silent stubs, fabricated measurements, or fake success.
- Missing models, devices, permissions, or platform bindings must produce an
  explicit error or a clearly labelled `SKIP`.
- Capability checks and sandbox boundaries must not be weakened.
- Never commit `models/`, secrets, tokens, private keys, or personal data.
- Document limitations and failing paths instead of hiding them.

## Pull requests

- Keep changes focused; open an issue first for large architectural changes.
- Add or update tests for behavior changes.
- Update the relevant documentation when a contract or limitation changes.
- Use clear commit messages; conventional prefixes such as `fix:`, `feat:`,
  `docs:`, `test:`, and `chore:` are preferred.
- Make sure all required CI checks pass. The `main` branch is protected and
  requires the Windows, Ubuntu, macOS, deny, OS-compat, and UI-quality jobs.

## License

By contributing, you agree that your contributions are licensed under the
Apache License 2.0, the same license as the project.
