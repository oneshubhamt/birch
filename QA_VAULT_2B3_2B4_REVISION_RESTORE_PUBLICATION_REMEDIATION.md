# QA — Vault 2b.3–2b.4 Revision Restore Publication Remediation

## Targeted regression coverage

Source-defined Rust regressions now include:

- `restore_invalidates_pre_restore_publication_and_allows_fresh_post_restore_authorization`
  - proves Restore invalidates the old coordinator authorization;
  - proves no fresh authorization can be captured while Restore is active;
  - proves a fresh authorization can be captured after Restore releases;
  - proves the old authorization remains stale.

- `protected_restore_normalization_reuses_retained_authorization_without_lock_cycle`
  - proves post-Restore normalization can use the retained authorization;
  - admits Lock while that authorization is retained;
  - proves Lock revokes it and waits;
  - proves normalization fails closed rather than waiting behind Lock;
  - proves Lock completes after authorization release.

The strengthened 2b.3/2b.4 static/adversarial guards also require:

- semantic note-publication scope;
- stable operation-start UI/library/note generations;
- no feature-level manual generation composition;
- strict pre-token → Restore → fresh-token ordering;
- no pre-Restore token use after the persistence call;
- final Dioxus publication through the returned fresh token lease;
- retained-authorization normalization without Vault reader reacquisition.

## Final available validation

Executed on the final remediation tree:

```text
validate-vault-2b1-foundation.sh                    PASS
validate-vault-2b2-privacy-authority.sh             PASS
validate-vault-2b3-publication-token.sh              PASS
validate-vault-2b4-granular-cutover.sh               PASS
validate-vault-2b5-consolidation.sh                  PASS

validate-security-e0-e3-ownership-2a16.sh            PASS
validate-security-e0-e3-residue-2a17.sh              PASS
validate-security-e0-e3-ui-lifetime-2a18.sh          PASS
validate-security-e0-e3-adversarial-closure-2a19.sh  PASS

RUSTNOTE_B6_STATIC_ONLY=1 validate-architecture-b6.sh PASS
run-browser-behavior-tests.mjs                       36/36 PASS
```

The cumulative architecture run discovered 246 Rust tests.

## De-novo post-fix source checks

- `VaultNoteAccess` remains non-clonable.
- Only the opaque restore authorization wraps `VaultNoteAccess` in `Arc`.
- No `.with_library_generation(...)` or `.with_note_generation(...)` calls remain outside `src/ui/privacy.rs`.
- `pre_restore_publication_token` is absent from the post-persistence portion of `restore_revision_flow`.
- Post-Restore flow captures from `publication_scope` and returns that token for the final UI lease.
- Protected normalization uses `persist_revision_normalization_after_protected_restore` and the retained Restore authorization.

## Native release gate — not executed in this environment

This environment contains no `cargo`, `rustc`, or `rustfmt`. Therefore the following must not be represented as passed here:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

These commands remain the native macOS release gate.
