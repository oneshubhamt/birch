# QA — Rustnote 2b.43.3 OS secure-key-store + BackupKeyAuthority

## Scope

This QA record covers the 2b.43.3 implementation checkpoint on Rustnote `v0.12.6+2b.43`.

The pass changes the security/platform/key-authority layer only. It does not change managed-backup repository persistence.

## Objective checklist

| Objective | Result |
|---|---|
| One narrow platform secure-store boundary | PASS |
| OS root key hidden from storage | PASS |
| macOS data-protection/non-sync/device-only profile | PASS at source/API level; native rerun pending after compile remediation |
| Add-only/non-overwriting macOS root creation | PASS |
| Android non-exportable-root bridge | PASS as Rust/mobile-shell contract; native Android shell implementation remains platform integration work |
| Unsupported platforms fail closed | PASS |
| Existing repository open cannot create a root | PASS |
| Independent random repository keysets | PASS |
| Exact 2b.43.2 keyset/header encoding | PASS |
| Key/root/keyset secret buffers zeroizing/redacted | PASS |
| No filesystem/environment root-key fallback | PASS |
| No backup storage cutover | PASS |
| Old E0-E3/Vault crypto-isolation guards remain strong | PASS after exact audited-boundary modernization |
| Temporary dead-code staging kept narrow | PASS |

## Static/source validation

The dedicated guard checks 27 independent source contracts:

```text
bash scripts/validate-backup-2b43-3-secure-key-store.sh
```

Result:

```text
Backup 2b.43.3 secure-key-store source validation passed (27/27 contracts).
```

The historical 2b.43.1 guard was modernized only so it preserves evidence that *that pass* was contract-only without incorrectly forbidding later 2b.43 implementation.

The inherited E1/E2/E3 and 2b.42 independent crypto-isolation guards were modernized to an exact allowlist:

```text
src/security/mod.rs
src/security/backup_key_authority.rs
src/platform/secure_key_store.rs
```

Low-level primitive imports remain forbidden in application code. The platform adapter invokes only the canonical `rustnote-vault` envelope service.

## Regression QA executed in the assistant environment

```text
2b.43.1 key-authority contract            12/12 PASS
2b.43.2 byte-format contract              18/18 PASS
2b.43.3 secure-key-store contract         27/27 PASS
E0 static closure                         PASS
E1 static closure                         PASS
E2 static closure                         PASS
E3 static closure                         PASS
2b.42.10 independent closure              16/16 PASS
2b.42.11 final pre-S1 static closure      PASS
shell syntax                              92/92 PASS
JS/MJS syntax                             54/54 PASS
browser/source behavior suite             111/111 PASS
```

The behavior runner reports editor-build dependencies absent, so these are the dependency-free editor/PDF/source behavior tests. The dependency-backed editor suite remains a native prepared-runtime gate.

## Direct source sweeps

PASS:

- no `BackupKeyAuthority` use in `src/storage`;
- old `repository.json` writer remains the active backup repository;
- no `repository.header` persistence yet;
- no environment-based backup root key;
- no ordinary-file backup root key;
- exactly two temporary staged `#[expect(dead_code)]` module boundaries and no `allow(dead_code)`;
- no new `TODO`, `FIXME`, or `HACK` markers;
- direct `rustnote_vault` references in `src` confined to exactly the three audited files;
- no low-level AEGIS/Argon2/ChaCha/AES-GCM primitive imports in app code;
- Cargo/lockfile contain the pinned dependency closure needed by `--locked` builds.

## Dependency/API cross-check

The source implementation was checked against the pinned APIs used by the pass:

- `security-framework 3.7.0` exposes `PasswordOptions::use_protected_keychain`, `set_access_synchronized`, `set_access_control`, `SecAccessControl::create_with_protection`, and `AccessibleAfterFirstUnlockThisDeviceOnly`;
- `security-framework-sys 2.17.0` exposes the SecItem constants/functions used for add-only creation;
- `getrandom 0.3.4` exposes `getrandom::fill` on macOS/Android and the supported desktop targets.

This reduces API-drift risk but is not a substitute for native compilation.

## Native Rust gate — mandatory before final pass closure

The first macOS native gate completed the optimized release preparation/build, then exposed the localized warnings/test compile error recorded below. Those defects are repaired in this candidate. The assistant environment has no `cargo`, `rustc`, or `rustfmt`, so the corrected candidate still requires a second native compile/Clippy/test run before final pass closure.

On macOS, run from the exact 2b.43.3 checkpoint:

```bash
cargo fmt --all
cargo fmt --all -- --check

./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh

cargo check --workspace --all-targets --all-features --locked

cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

cargo test --workspace --all-features --locked -- --test-threads=1

node scripts/run-browser-behavior-tests.mjs --require-editor-deps

bash scripts/validate-backup-2b43-1-key-contract.sh
bash scripts/validate-backup-2b43-2-byte-format.sh
bash scripts/validate-backup-2b43-3-secure-key-store.sh
./scripts/validate-vault-2b42-final-pre-s1-closure.sh
```

Because the macOS implementation is behind `cfg(target_os = "macos")`, the macOS `cargo check`/Clippy/test gate is especially important: it compiles the actual Security.framework integration rather than the unsupported-target stub.

## Keychain runtime note

2b.43.3 intentionally has no production repository caller yet, so normal application execution does not create or read the device backup root in this pass.

The first live production Keychain exercise occurs when 2b.43.4 connects fresh encrypted-repository creation to `BackupKeyAuthority`.

The data-protection Keychain uses the host executable's signing/access-group identity. Failure of an unqualified host must remain a fail-closed `SecureStoreUnavailable` condition; the design does not fall back to the legacy file-based keychain or plaintext storage.

## Closure assessment

**Source/architecture/static QA: PASS.**

**Native Rust/macOS compile gate: PENDING.**

Therefore this package is the 2b.43.3 native-QA closure candidate. If the native gate finds a compile/Clippy/test defect, it belongs in 2b.43.3 and must be repaired before 2b.43.4 begins.

## Native Mac compile remediation (2026-09-15)

The first macOS native gate successfully completed the optimized release preparation/build, then exposed four strict compile/Clippy defects in the new 2b.43.3 modules. These were all localized QA defects rather than architecture changes:

- moved `BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES` behind `#[cfg(target_os = "android")]`;
- moved `BackupDeviceRootKey` / `BackupKeyEnvelopeService` imports behind `#[cfg(test)]`;
- replaced pattern matching on Security.framework's lower-case C constants with equality checks, avoiding `non_upper_case_globals` without an allow attribute;
- removed the cfg-branch `return` rejected by `clippy::needless_return`;
- rewrote the secure-store failure test to extract `Err` explicitly instead of requiring `InitializedBackupRepositoryKeys: Debug`, avoiding an unnecessary trait implementation on a secret-owning aggregate.

The 2b.43.3 source validator now has dedicated regression contracts for these native findings. No broad warning suppression was added, no secret-bearing `Debug` implementation was introduced, and no storage behavior changed. A second Mac native gate is still required because this environment does not contain Rust/Cargo.


## Native Mac compile remediation — second round (2026-09-15)

The second macOS run progressed into strict Clippy and the full Rust test target. It exposed no new architecture defect, but found staged/test QA issues that are now repaired:

- the two staged modules now use `#[expect(dead_code)]` for both library and test targets instead of `cfg_attr(not(test), allow(dead_code))`;
- the old plaintext-owner regression uses `assert!(!...)` instead of a bool `assert_eq!`;
- the multi-note privacy linearization test no longer calls the admission-taking publication-generation accessor while it intentionally owns storage and another thread owns admission. That was a deterministic test-only lock inversion. It now proves admission retention via non-blocking `try_lock()` and explicit transition-attempt/completion channels.

After these repairs the assistant-side gates are:

```text
2b.43.1 key-authority contract            12/12 PASS
2b.43.2 byte-format contract              18/18 PASS
2b.43.3 secure-key-store contract         28/28 PASS
2b.42.9 cleanup                            14/14 PASS
2b.42.10 independent closure              16/16 PASS
E0-E3 + 2a.16-2a.19 static closure        PASS
shell syntax                              92/92 PASS
browser/source behavior suite             111/111 PASS
```

A further native Mac run is still required because this environment cannot execute Cargo/Clippy/Rust tests.


## Native Mac closure and rustfmt-stable source-contract remediation (2026-09-15)

The subsequent macOS native run completed the full Rust gate successfully through:

```text
cargo fmt --all / --check                 PASS
cargo check --workspace --all-targets     PASS
cargo clippy ... -- -D warnings           PASS
targeted privacy-coordinator regression   PASS
cargo test --workspace --all-features     PASS
```

The final dependency-backed browser/source run then reported six failures. Audit of the failure inputs showed the governed Rust semantics were still present; all six were false negatives caused by source-contract assertions that depended on pre-rustfmt whitespace/layout (wrapped method chains, multiline function arguments, and long match/boolean expressions). Production Rust code was not changed.

The five affected JS contract files now use one shared `tests/js/source-contract-helpers.mjs` utility. It compacts Rust whitespace for both the source and expected fragments and checks either semantic containment or ordered token sequences. The assertions therefore still prove the same architectural ordering/identity properties while remaining stable across `cargo fmt`.

Assistant-executable verification after this remediation:

```text
affected Vault/UI source-contract tests    50/50 PASS
dependency-free browser/source suite      111/111 PASS
2b.43.1 key-authority contract             12/12 PASS
2b.43.2 byte-format contract               18/18 PASS
2b.43.3 secure-key-store contract          29/29 PASS
```

A final dependency-backed Mac browser run is required only to confirm the six formatter-induced false negatives are gone in the prepared environment. Since the preceding native Rust gate and the other 170 dependency-backed browser tests already passed on that same formatted tree, no production-code defect is presently open.
