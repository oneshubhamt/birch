# Vault 2b.12 — AEGIS-256 profile + backup privacy disclosure closure

## Status

Implementation closure over the 2b.11 atomic privacy-authority baseline.

This pass does not redesign Vault persistence or re-open E1–E3. It closes the two residual items from the independent E0–E3 audit after the product decision to standardize on AEGIS-256 and to treat comparative crypto benchmarking as non-gating.

## Audit findings

1. Rustnote already used the strongest AEGIS-256 authentication-tag form (256 bits), random 256-bit bounded-object/key-envelope nonces, random 256-bit NoteKeys, authenticated format/context metadata, and a 192-bit random protected-blob prefix plus 64-bit per-record counter.
2. The `aegis` crate remains exactly pinned and uses its default backend. Rustnote does not opt into the slower `pure-rust` backend.
3. Argon2id v1 already matched RFC 9106's second recommended profile: 64 MiB, three passes, four lanes, 128-bit salt, 256-bit output.
4. The parser's historical hostile-envelope ceiling (256 MiB / 8 passes / 8 lanes) was broader than the only production profile Rustnote intentionally creates.
5. Protect correctly maintained durable privacy-reset obligations for disconnected backup repositories, but its immediate success message could still read as if every historical Rustnote-managed backup artifact had already been cleaned.

## Objectives

- Freeze the Rustnote v1 crypto decision as AEGIS-256 rather than requiring comparative benchmark evidence.
- Keep the strongest AEGIS-256 format choices already in production.
- Make key/nonce/tag/salt/blob-nonce geometry explicit through compile-time invariants.
- Bound hostile Argon2 envelope requests to Rustnote's actual v1 resource profile.
- Preserve versioned/authenticated KDF parameters so future policy changes remain explicit.
- Report pending backup privacy-reset obligations immediately after a successful Protect without turning a post-commit informational lookup failure into a false encryption failure.
- Preserve the existing persistent warning in Backup settings.
- Add static and native regression coverage for the new invariants.

## Implemented

### Fixed AEGIS-256 v1 profile

`crates/rustnote-vault` remains the only production low-level cryptographic boundary and uses:

- 256-bit AEGIS-256 keys;
- 256-bit random nonces for bounded objects and key envelopes;
- the full 256-bit AEGIS-256 authentication tag;
- OS cryptographic randomness through `getrandom`;
- random 256-bit NoteKeys;
- versioned and authenticated object/key/blob headers;
- domain + object identity + purpose binding in associated data;
- protected blobs with a random 192-bit per-blob nonce prefix plus 64-bit record counter;
- a reserved all-ones counter for the authenticated blob header, keeping header and data-record nonce spaces disjoint;
- 1 MiB default protected-blob records, with existing 4 KiB–8 MiB validation bounds;
- authenticated chunk index and plaintext length so reorder/truncation/substitution fails closed.

The crate does not enable the `aegis` `pure-rust` feature. The existing default backend remains the canonical production backend.

### Argon2id v1 profile and hostile-input bound

Rustnote v1 remains:

- Argon2id v=19;
- 64 MiB memory;
- three passes;
- four lanes;
- 128-bit random salt;
- 256-bit derived wrapping key.

The maximum resource request accepted from an untrusted v1 key envelope is now exactly the v1 profile rather than 256 MiB / 8 / 8. `Argon2idParams` no longer exposes constructible parameter fields or the obsolete candidate alias, so production callers can select the canonical v1 profile but cannot casually manufacture a weaker wrapping profile. A future stronger KDF policy must deliberately change the profile/version instead of silently increasing the parser's denial-of-service budget.

### Protect-result privacy disclosure

`VaultProtectionService::protect_note` now returns a small `ProtectNoteOutcome` containing the post-commit count of backup repositories still carrying a durable privacy-reset obligation.

The query is intentionally read-only and does not open, create, mount, validate, or mutate a backup repository. Failure to refresh this informational count does not retroactively turn a successful canonical Protect commit into a failed encryption operation.

UI behavior:

- zero pending resets: existing concise success message;
- one/many pending resets: explicitly says encryption is complete locally while unavailable backup locations may retain earlier plaintext copies and will be cleaned before Rustnote uses those repositories again;
- status-refresh failure: conservatively tells the user to check Settings > Backups before assuming historical backup copies are clean.

The persistent Backup-settings privacy-reset warning remains unchanged.

## Deliberate non-changes

- No cipher fallback or dual encrypted format was introduced.
- No benchmark results are required for the fixed product decision.
- No whole-database encryption, encrypted search, notebook encryption, or sync redesign was added.
- No native Rust gate is claimed in environments without `cargo`/`rustc`.

## Release gate

Static 2b.12 validation must pass together with all inherited 2b.1–2b.11 and E0–E3 closure guards. Native Rust formatting/check/clippy/tests remain the final machine-local release gate.
