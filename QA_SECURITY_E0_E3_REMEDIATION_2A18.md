# QA — Security E0–E3 Remediation 2a.18

Build: **0.12.5 / 2a.18**

## Scope

This QA record covers pass 2a.18 only: UI/process-memory privacy revocation, protected attachment command lifetime, permit-bearing protected revision preview, and protected revision restore. Filesystem staging ownership remains governed by 2a.16 and persistent backup/quarantine residue by 2a.17. Independent final E0–E3 certification remains deferred to 2a.19.

## Source-level objectives verified

- One `UiPrivacyEpoch` is created at the application root and propagated to the affected UI surfaces.
- Encrypt, Unlock, Lock and Remove Encryption revoke that epoch before the note-card Vault action is dispatched.
- Backlinks, note picker/search candidates, hierarchy, properties, sources/citations, note-link target registries and revision previews reject stale async publication and suppress stale cached render/action state.
- Protected attachment Open/Reveal/Save-As uses one command resource whose plaintext-derivative permit spans the entire command and whose Vault permit spans the entire protected command, including Save-As dialog lifetime.
- Protected attachment command errors no longer include decrypted filenames in errors that can outlive authorization.
- Protected revision preview obtains `RevisionSnapshotResource`, retains its Vault permit through editor projection + Markdown/HTML derivation and through the final privacy-generation publish/discard check, then releases authorization without retaining it in the preview cache.
- The UI/application preview path contains no direct call to the unscoped revision-snapshot API.
- Protected revision restore reseals the authenticated payload rather than restoring into ordinary SQLite revision/tag/FTS/content persistence.
- Protected runtime normalization persists through the encrypted payload and updates only the structural note timestamp in SQLite.

## Behavioral regressions present in source

The Rust test suite now contains targeted regressions that assert:

1. Lock cannot complete while a protected attachment command resource remains alive.
2. Encrypt cannot cross an ordinary attachment command resource lifetime.
3. Lock cannot complete while a protected revision-preview resource remains alive.
4. Protected revision restore survives Lock → Unlock while ordinary note content, tags, FTS and `note_revisions` remain plaintext-empty.
5. The restored protected marker is absent from SQLite/WAL byte scans.

These Rust regressions were inspected and are required by the 2a.18 static guard. They could not be executed in this environment because no Rust toolchain is installed.

## Executed validation

The following checks were executed successfully in the supplied environment:

- `RUSTNOTE_E0_STATIC_ONLY=1` E0 validator — PASS
- `RUSTNOTE_E1_STATIC_ONLY=1` E1 validator — PASS
- `RUSTNOTE_E2_STATIC_ONLY=1` E2 validator — PASS
- `RUSTNOTE_E3_STATIC_ONLY=1` E3 validator — PASS
- 2a.16 canonical plaintext-ownership regression guard — PASS
- 2a.17 persistent-residue/backup regression guard — PASS
- 2a.18 UI privacy-lifetime/protected-revision guard — PASS
- cumulative E0–E3 targeted remediation guard — PASS
- shell syntax check for all `scripts/*.sh` — PASS
- diff whitespace check against the packaged 2a.17 baseline — PASS
- source-tree generated-artifact/VCS hygiene check — PASS
- direct grep audit proving the UI/application revision-preview path does not call `revision_snapshot_for_note`/unscoped `revision_snapshot` — PASS

## Native gates not executable here

This environment does not contain `cargo`, `rustc` or `rustfmt`. Therefore the following are **not claimed as executed** here:

```text
cargo fmt --all -- --check
cargo metadata --locked --format-version 1
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

They remain mandatory native release gates on the development Mac.

## Re-audit result

The changed 2a.18 surfaces were re-read after implementation. The previously identified stale Backlinks/note-picker publication defects are covered by the global epoch; protected attachment metadata no longer outlives command authorization; protected revision preview no longer has an authorization-free derivation interval; and protected revision restore no longer depends on the scrubbed ordinary revision table.

No new filesystem ownership or backup-policy architecture was introduced in this pass.

## Conclusion

Within its stated scope, 2a.18 satisfies the planned UI/in-memory privacy lifetime and E2 protected-revision functional objectives at source/static-validation level. Native Rust compilation/testing remains outstanding solely because the toolchain is unavailable in this environment. Final E0–E3 certification is intentionally reserved for the independent 2a.19 adversarial closure audit.
