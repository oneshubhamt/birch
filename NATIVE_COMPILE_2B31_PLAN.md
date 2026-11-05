# Rustnote 2b.31 — Native Full-QA Closure Plan

## Audit findings

1. The serial 277-test library run exposed one scheduler-sensitive privacy-coordinator test failure. Re-audit found the production coordinator ordering correct, but the test used a one-second spin deadline to observe admission and a two-second completion deadline on a heavily loaded native suite.
2. `validate-architecture-a0.sh` redundantly reran the entire workspace test suite with Cargo's default parallel test execution. That exposed four `ENOENT` failures during storage startup which were absent from the preceding serial suite.
3. Deeper audit of those `ENOENT` failures found a real production robustness race in `normalize_directory_for_comparison`: it checked a historical path with `symlink_metadata` and then canonicalized it in a second syscall. A path could disappear between those operations. The correct design is a direct canonicalize-and-peel loop that treats `NotFound` as a missing suffix and retries from the parent.
4. The full 101-test browser suite exposed five stale assertions:
   - one applied atomic-boundary semantics to a list edge;
   - two constructed invalid top-level atomic fixtures missing the required opposite edge boundary;
   - two matched obsolete source spelling instead of current equivalent behavior.

## Objectives

- Preserve production PrivacyCoordinator, editor, database-schema, encryption, and AEGIS semantics.
- Fix the real storage path-disappearance race without weakening backup ownership/path validation.
- Make privacy-coordinator concurrency tests scheduler-robust without weakening their negative/waiting assertions.
- Make the A0 architecture guard non-redundant: architecture checks remain architecture checks; full Rust behavior remains the explicit serial Cargo gate.
- Repair browser tests so they assert canonical atomic-boundary and UI contracts.
- Preserve the macOS 12.0 / native AEGIS 0.9.15 deployment contract from 2b.30.
- Run inherited Vault/E0-E3 validators and targeted JavaScript/static QA.
