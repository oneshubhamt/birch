# Contributing to Rustnote (Birch)

This is an actively developed application. Keep changes **clean, lean, efficient, elegant, robust and reliable**. Prefer a small authoritative implementation over duplicate fallback paths and patch stacks. Backward compatibility/migration of obsolete development-only formats is not a blanket requirement; do not silently weaken current data formats or privacy guarantees.

## Change procedure

1. **Audit first:** locate the authoritative code, contracts, tests, and affected plaintext/UI/storage lifetimes. Record known behavior and risks.
2. **Plan:** define objectives, invariants, expected files, performance impact, and acceptance criteria.
3. **Implement:** make the minimum coherent changes; refactor dead or duplicated logic rather than adding parallel mechanisms.
4. **Re-audit:** check each objective directly against the changed code, not just against a validator's pass/fail message.
5. **Targeted QA:** exercise changed surfaces and negative/race/failure paths. Run full Rust, browser and contractual suites after substantial cross-cutting changes.
6. **Document:** update the affected architecture/security contract or status document; communicate failures and environment limitations explicitly.

See [Development](docs/DEVELOPMENT.md) and [QA](docs/QA.md) for concrete commands.

## Pull requests

Branch off `main`, open a PR and explain why the change is needed, architectural and security consequences, performance implications, and what was actually tested. Do not merge a failing check or claim a native macOS Keychain test when only static source checks ran. Never commit user notes, databases, production keys, signing identities or other secrets. Use GitHub private vulnerability reporting for security issues; see [SECURITY.md](SECURITY.md).

## Dependency policy

Rust and npm dependencies are intentionally pinned. Keep `Cargo.lock` and `editor-build/package-lock.json` committed. Update a pinned version only in a dedicated pass with contract, runtime and native compatibility QA. Dependabot PRs are proposals, not automatic approvals.
