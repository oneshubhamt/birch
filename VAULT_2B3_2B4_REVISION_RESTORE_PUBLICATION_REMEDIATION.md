# Vault 2b.3–2b.4 Revision Restore Publication Remediation

## Status

Source-level closure remediation completed after the independent 2b.1–2b.4 audit found a deterministic post-Restore publication failure.

## Audit finding

Before this remediation, the revision-restore UI captured one `PrivacyPublicationToken` before persistence and reused it after protected Restore committed. `NotePrivacyTransition::Restore` correctly advances the coordinator privacy generation at admission, so the pre-Restore token is necessarily stale after the mutation. The workflow therefore could commit the restored granular Vault state and then reject its own UI publication with `EditorChanged`.

A deeper audit of the proposed refresh also identified two related hazards that had to be closed rather than patched around:

1. A fresh token must refresh privacy authorization only. It must not silently adopt a newer library, note, or UI generation if the user changed context while Restore was running.
2. Protected browser-runtime normalization after Restore must not reacquire Vault access while the restore result intentionally retains the original protected read authorization. Reacquisition could deadlock with a concurrently admitted Lock waiting for that retained authorization to drain.

## Objectives

- Invalidate all pre-Restore publication work at Restore admission.
- Never reuse the pre-Restore publication token after persistence starts.
- Capture a fresh coordinator publication authorization only after the Restore transition releases.
- Preserve the operation-start UI, library, and note generations across that refresh.
- Publish the restored Dioxus state only under the fresh post-Restore token and a publication lease.
- Keep protected post-Restore normalization under the exact retained Restore authorization without acquiring another Vault reader.
- Keep `VaultNoteAccess` non-clonable; clone only the opaque restore authorization wrapper.
- Centralize UI generation composition behind semantic publication-capture helpers.
- Add regressions/guards for the defect and Lock interaction.

## Implementation

### Semantic publication scope

`src/ui/privacy.rs` now owns a `NotePrivacyPublicationScope` that snapshots:

- UI privacy generation;
- library generation;
- note/editor generation;
- the publication service used to refresh coordinator authorization.

A post-mutation `capture()` refreshes only the coordinator authorization while retaining the original generation expectations. If the UI privacy epoch, library, or note generation changed, recapture fails closed.

Direct feature-level calls to `with_library_generation` and `with_note_generation` were removed; the raw composition helpers are private to the UI privacy authority.

### Two-token Restore flow

`restore_revision_flow` now has an explicit split:

1. pre-Restore token validates all work before the protected persistence mutation;
2. protected Restore commits and advances privacy generation;
3. the pre-Restore token is discarded;
4. a fresh token is captured from the original note-publication scope;
5. bridge replacement/installation, normalization projection, and final Dioxus publication use only that fresh token.

The returned `RestoredRevision` carries the fresh token, and the caller holds a publication lease across the final signal updates.

### Retained protected normalization authorization

Protected Restore returns an opaque `RevisionRestoreAuthorization` containing an `Arc<VaultNoteAccess>`. `VaultNoteAccess` itself remains non-clonable and has only a redacted `Debug` implementation.

If browser replacement produces a runtime-normalized editor state, the protected normalization path reuses that retained access. It does not call `acquire_note_access` again. The storage operation lock is the mutation linearization point and the retained permit is checked after the lock is acquired:

- if Lock/Unprotect won admission first, the permit is revoked and normalization fails closed without waiting behind the transition;
- if normalization won the operation boundary first, the later privacy transition waits for the already-authorized read lifetime to finish.

This removes the reader-reacquisition cycle while preserving the contract requirement that Lock cannot complete during a live protected post-Restore operation.

## Result

The original deterministic state mismatch is removed:

```text
protected Restore
→ pre-Restore work invalidated
→ granular state commits
→ Restore transition releases
→ fresh privacy authorization captured for original UI/library/note scope
→ restored state published under lease
```

Stale pre-Restore work remains permanently invalid and a changed UI/library/note context cannot be adopted by the refresh.
