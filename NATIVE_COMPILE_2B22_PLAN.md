# Rustnote 2b.22 — Media Lock Test + Validator Closure Plan

## Audit findings

1. `protected_media_file_keeps_lock_waiting_until_file_handle_drops` deadlocks in the test harness: after Lock admission, its main thread calls `AttachmentTransportStore::attachment_resource` as a revocation probe. That read correctly waits for the admitted privacy transition, while Lock correctly waits for the still-open media lease owned by the main thread.
2. `validate-vault-2b21-native-test-state-lifetime-closure.sh` searches for a formatting-sensitive single-line Lock call. `cargo fmt` may split the method chain and makes the validator reject semantically identical source.
3. The same-path library identity recovery/cross-library fail-closed tests already pass on the user's Mac; the deleted Documents library/backup directories are not causal for this media-test deadlock.

## Objectives

- Preserve production Lock/revocation and media-handle lifetime semantics unchanged.
- Replace the blocking media-test revocation probe with a nonblocking test-only inspection of Vault read availability.
- Keep the test proving both prompt revocation publication and Lock waiting until the OS file lease drops.
- Make the inherited 2b.21 validator insensitive to `cargo fmt` line wrapping.
- Run inherited security/Vault static guards and dependency-free behavioral QA, then package an exact source archive.
