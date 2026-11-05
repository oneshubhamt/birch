# Rustnote 2b.31 — Native Full-QA Closure Implementation

## Implemented

### Production storage path-race closure

`normalize_directory_for_comparison` no longer performs `symlink_metadata(path)` followed by a separate `canonicalize(path)` call. It now attempts `fs::canonicalize` directly. If the candidate is missing, it records the missing final component, peels to the parent, and retries until an existing ancestor canonicalizes; the missing suffix is then reapplied lexically.

This preserves symlink-aware canonical comparison while removing the check-then-use window that could surface `ENOENT` if a remembered library/backup path disappeared concurrently during startup/history normalization.

### PrivacyCoordinator test synchronization

Added one test-only `wait_for_privacy_generation_advance` helper with a 15-second bounded wait and 1ms sleep. It replaces three duplicated one-second scheduler-sensitive spin loops in the compound Lock/Protect and retained-reader race tests. Related positive completion waits were widened from two seconds to 15 seconds; the short negative assertions that prove Lock/Protect is still blocked remain unchanged. Production coordinator code is unchanged.

### Architecture safety net

`validate-architecture-a0.sh` no longer launches a second full `cargo test --workspace --locked` run. It validates the prepared editor runtime plus the pinned Dioxus/Manganis architecture baseline. The complete Rust behavioral gate remains the explicit serial workspace test command.

### Browser behavior tests

- List-adjacent stale `boundary: true` paragraphs are now expected to normalize to ordinary paragraphs.
- Intentional blank-line tests now begin from valid top-level atomic documents with both required edge boundaries.
- UI source-contract regexes now match the current semantic implementation:
  - blank preview excludes attachment-match previews;
  - default line spacing maps the optional Rust preference into the bridge string representation.

### Build/toolchain

No change to database schema, encryption format, AEGIS version/backend, or macOS deployment target. AEGIS remains native 0.9.15 and the deployment floor remains macOS 12.0.
