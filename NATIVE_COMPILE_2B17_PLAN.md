# Rustnote 2b.17 — Native test-compile closure plan

## Audit finding
The 2b.16 production library/bin compile reached test compilation. The remaining failures were test-integration drift only:

1. plaintext-staging restart tests passed borrowed `PathBuf`s to the owning `Storage::at_with_backup` API;
2. protection tests reached across a sibling module to a private note-save helper;
3. one protection test used the superseded coordinator attachment-path name;
4. binary unit tests referenced `crate::storage` and a library-only `cfg(test)` fixture across a crate boundary.

## Objectives
- Keep production storage/privacy APIs narrow.
- Repair tests against the current owning/storage/privacy APIs.
- Add only a feature-gated internal test-support bridge needed by binary unit tests under `--all-features`.
- Preserve all 2b/E0–E3 invariants and prior native compile closures.
- Add a validator that prevents the stale test patterns from returning.
