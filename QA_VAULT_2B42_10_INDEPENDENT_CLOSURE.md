# QA — Rustnote Vault 2b.42.10 Independent Closure Audit

## Environment limitation

The assistant-side environment does not provide `cargo` or `rustc`. Native Rust formatting, compilation, strict Clippy and Rust test execution therefore remain **PENDING on macOS**.

This limitation is an execution/QA gate, not a known security defect. Source/architecture security matrices are PASS after repair and re-audit.

## Assistant-side QA completed

### Independent source validation

```text
Vault 2b.42.10 independent closure source validation passed (16/16 contracts).
```

The validator checks the `NotFound`-only path-observation rule, repaired ownership call sites, process owner arbitration, SQLite authority, transition authority, publication, deny-by-default persistence, E3 residue order, backup namespace ownership, export gates, crypto isolation, I7 locality and the adversarial regression corpus.

### Dependency-free browser/static behavior suite

```text
tests 111
pass 111
fail 0
```

This includes `vault-independent-closure-2b42-10-contract.test.mjs` and the inherited editor/privacy/navigation/library/Vault/no-VACUUM/cleanup/PDF contracts that do not require the missing pinned editor dependency tree.

The full dependency-backed ProseMirror editor behavior subset remains part of the Mac gate because `editor-build/node_modules` is absent in this sandbox.

### Shell syntax

```text
bash -n PASS: 87 shell scripts
```

### Inherited Vault regression validators

All passed:

```text
validate-vault-2b6-independent-closure.sh
validate-vault-2b7-closure-repair.sh
validate-vault-2b8-plaintext-authority-closure.sh
validate-vault-2b9-backup-ownership-closure.sh
validate-vault-2b10-cross-cutting-invariant-closure.sh
validate-vault-2b11-atomic-privacy-authority-closure.sh
validate-vault-2b12-aegis-privacy-disclosure-closure.sh
validate-vault-2b42-1-scoped-publication-core.sh
validate-vault-2b42-2-navigation-source-target.sh
validate-vault-2b42-3-note-local-ui.sh
validate-vault-2b42-4-multi-note-ui.sh
validate-vault-2b42-5-vault-action-controller.sh
validate-vault-2b42-6-library-wide-operations.sh
validate-vault-2b42-7-profiling.sh
validate-vault-2b42-7-native-compile-fix.sh
validate-vault-2b42-8-no-vacuum-forensic.sh
validate-vault-2b42-9-cleanup.sh
validate-vault-2b42-10-independent-closure.sh
```

### Cumulative E0–E3 regression closure

Static mode passed after repairing two stale validator assumptions:

```text
Security E0 static contract passed.
Security E1 static contract passed.
Security E2 static contract passed.
Security E3 static contract passed.
E0-E3 2a.16 canonical plaintext ownership guard passed.
E0-E3 2a.17 persistent residue and backup closure guard passed.
E0-E3 2a.18 scoped UI lifetime guard passed.
E0-E3 2a.19 cumulative independent adversarial closure guard passed.
```

## Repeated de-novo cleanup sweep

A production-function reference sweep found exactly one non-test Rust function identifier with
no caller: `Storage::add_derived_image_attachment_file`. It was removed; the explicitly
authorized `add_derived_image_attachment_file_authorized` path remains canonical. A static
closure assertion and dependency-free behavior contract guard the cleanup.

## Targeted adversarial Rust tests added

These compile/run only in the native Rust gate:

```text
true_not_found_is_conclusive_plaintext_absence
metadata_failure_is_not_treated_as_plaintext_absence
ambiguous_materialized_attachment_presence_fails_closed_without_replacement
ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry
ambiguous_attachment_operation_stage_presence_fails_closed_until_retry
retired_plaintext_restore_rejects_ambiguous_absence_and_retries_after_repair
strict_plaintext_cleanup_does_not_accept_ambiguous_absence
ambiguous_owner_marker_metadata_never_authorizes_cleanup
```

The Unix-specific cases use symlink-loop `ELOOP` fixtures to distinguish ambiguous observation failure from true `NotFound`.

## Required macOS native closure gate

Run, in order, from the extracted candidate root:

```bash
cargo fmt --all
cargo fmt --all -- --check
./scripts/prepare-rustnote-mac.sh
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
node scripts/run-browser-behavior-tests.mjs --require-editor-deps
./scripts/validate-vault-2b42-10-independent-closure.sh
RUSTNOTE_E0_STATIC_ONLY=1 RUSTNOTE_E1_STATIC_ONLY=1 RUSTNOTE_E2_STATIC_ONLY=1 RUSTNOTE_E3_STATIC_ONLY=1 bash scripts/validate-security-e0-e3-closure.sh
```

If any native compile, Clippy, Rust test or dependency-backed browser failure is substantive, repair it inside 2b.42.10 and re-run the affected audit before moving to 2b.42.11.
