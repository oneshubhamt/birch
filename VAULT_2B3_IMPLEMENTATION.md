# Rustnote Vault 2b.3 — Publication-token consolidation

## Status

Implementation complete against the `2b.2` PrivacyCoordinator + unified plaintext-authority baseline.

This pass is intentionally limited to the 2b.3 contract. It does **not** begin the 2b.4 granular protected-note persistence cutover.

## Audit result

The pre-edit audit found that 2a.18 correctly revoked decrypted UI state, but authorization remained distributed across feature-local request counters, `UiPrivacyEpoch` generation comparisons, Vault permits, and ad-hoc checks around native/WebView operations. Several less obvious publication surfaces also carried protected-derived values: paginated note-list state, selection reconciliation, editor synchronization, attachment/link registries, autosave completion, image/attachment workflows, source/hierarchy mutation results, and note PDF/native print lifetime.

A publication token that merely wrapped the old UI generation was insufficient. Storage transition admission can begin after a UI check, and unrelated note transitions may run concurrently. The final design therefore anchors privacy authorization in `PrivacyCoordinator` itself and keeps UI-local generations only for lifecycle/remount/request identity.

## Objective closure

### O1 — One explicit PrivacyPublicationToken authority — CLOSED

`src/ui/privacy.rs` defines the caller-facing `PrivacyPublicationToken`. It combines:

- an opaque coordinator-backed publication authorization;
- the UI fail-closed revocation epoch;
- optional library generation;
- optional note/editor generation.

Feature UI asks only `is_publishable()` or, for an asynchronous handoff, `begin_publication()`.

### O2 — Invalidate at authoritative transition admission — CLOSED

`PrivacyCoordinator` now owns a publication generation and active-publication lease count. Note, multi-note and whole-library transition admission:

1. reserves the transition scope;
2. advances the publication generation immediately;
3. prevents fresh publication authorization/leases;
4. drains already-started publication leases;
5. only then continues to derivative/Vault/storage transition work.

Generation advancement was removed from transition-guard release. This means a Lock/Protect/Delete/Restore invalidates stale UI authorization even while it is still waiting for existing protected readers/producers.

### O3 — Keep coordinator internals out of feature UI — CLOSED

The boundary is:

`PrivacyCoordinator -> PrivacyPublicationStore -> PrivacyPublicationService -> PrivacyPublicationToken`.

Feature UI does not receive transition primitives or coordinator generation values. The application layer exposes only opaque authorization/lease wrappers.

### O4 — Migrate protected async publication surfaces — CLOSED

Token-gated publication now covers the existing protected-derived async surfaces audited in this pass, including:

- backlinks;
- note picker/search results;
- hierarchy reads and mutation results;
- properties;
- sources/citations reads, candidate search and mutation results;
- revision previews;
- note-list page loading and selection reconciliation;
- note-card privacy mutations and canonical post-transition reread;
- autosave completion publication;
- editor synchronization, checkpointing and note switching;
- initial live-editor installation;
- attachment and note-link WebView registries;
- note-link target refresh;
- editor projection callbacks;
- attachment insertion/recovery/media/image workflows;
- revision validation/replacement/remount publication;
- archive restore result publication;
- shutdown/backup/navigation synchronization paths carrying selected-note state.

Feature-local request counters remain where they still have a separate non-privacy role (search debounce, request supersession, editor remount identity, autosave version ordering).

### O5 — Eliminate superseded manual privacy-generation choreography — CLOSED

Outside `src/ui/privacy.rs`, feature code no longer calls `privacy_epoch.current()` or `privacy_epoch.is_current(...)` to authorize publication. The old repeated `{capture generation -> await -> compare generation}` pattern is replaced by the token.

`UiPrivacyEpoch::observe()` remains only where a reactive dependency is useful for cache invalidation; it is not an authorization decision.

### O6 — Protected revision preview/restore lifetime — CLOSED

Revision preview uses both authorities required by its lifetime:

- `RevisionSnapshotResource` retains exact Vault/session authorization through protected snapshot derivation;
- `PrivacyPublicationToken` authorizes the resulting async UI publication.

Revision restore carries its existing `RevisionRestoreAuthorization`, while all browser validation/replacement/install work and final Rust UI publication are token-gated. WebView replacement/install uses publication leases where the handoff itself crosses an await.

### O7 — Native dialogs and OS handoff retain authorization — CLOSED

Attachment Open/Reveal/Save-As continues to retain the derivative permit and, for protected attachments, the exact Vault read permit through the complete sensitive operation. Save-As revalidates after the native chooser; all commands revalidate immediately before native handoff.

Annotated-PDF export acquires its command-lifetime resource before protected attachment metadata is read.

Note PDF/native print now owns `NoteExportResource` for the exact selected note. The resource contains:

- a plaintext derivative permit, blocking Protect/Unprotect/Delete from crossing the interaction;
- an exact Vault read permit when the note is protected, making Lock wait through the interaction.

The print lifetime ends on the browser/native `afterprint` signal rather than a fixed timeout. The note identity is rechecked both after acquiring authorization and after editor synchronization, preventing note A authorization from covering note B content after a selection change.

### O8 — Close the pre-admission UI window and stale event origins — CLOSED

`UiPrivacyState` has a reference-counted `active_revocations` count. User-visible privacy actions begin this fail-closed local revocation before dispatching storage work and resume publication only after the transition scope ends. Overlapping actions cannot reopen one another's revocation window.

Whole-library restore and background trash hard-deletion use the same local revocation mechanism.

Autosave keeps its existing note-version check because that proves the event originated after the latest Vault barrier. A publication token is then required as a second gate so a transition beginning after the version check still prevents stale cache publication.

### O9 — Adversarial/static proof — CLOSED for all executable/static checks

Coordinator tests now include:

- note transition admission invalidates existing publication authorization;
- fresh authorization cannot be captured while that transition is active;
- library transition admission invalidates authorization;
- admitted transitions drain already-started publication leases;
- Lock invalidates both publication authorization and Vault read authorization before existing-reader drainage finishes.

Native lifetime regression tests additionally require:

- protected note export keeps Lock waiting until the native lifetime ends;
- ordinary note export keeps Protect waiting until the native lifetime ends.

`scripts/validate-vault-2b3-publication-token.sh` statically enforces the token/coordinator boundary, the named publication classes, WebView leases, native lifetime resources, autosave origin+privacy gating, and absence of predictive/manual parallel privacy publication schemes.

### O10 — Regression, cleanup and scope closure — CLOSED subject to native Rust release gate

Inherited E0-E3, 2a.16-2a.19, 2b.1 and 2b.2 static/security guards pass after updating only superseded 2a.18/2a.19 implementation markers. Architecture B3's attachment-finalization ordering guard was adapted to recognize the publication-leased WebView helper while preserving its original finalizer-before-browser-await requirement.

No 2b.2 file is removed except the explicitly requested deletion of the RTF codec (`src/portability/rtf.rs`). No 2b.4 object-model work is introduced.

## Important defects found and corrected during the pass

The implementation audit deliberately treated interim code as untrusted and found/fixed the following rather than documenting them away:

1. A UI-generation-only token could miss the interval between UI revocation and storage admission. Publication authorization was moved into `PrivacyCoordinator`.
2. A predictive “next privacy generation” token was invalid under safe cross-note concurrency. It was removed; completed privacy actions now capture fresh authorization and canonically reread before publishing.
3. Multi-await WebView publication needed a linearization boundary. A short-lived coordinator publication lease was introduced.
4. Editor synchronization could internally publish a checkpointed note before an outer stale-result gate. Authorization was moved into the synchronization boundary.
5. Autosave completion could publish a pre-transition `Saved(Note, version)` if only a fresh event-time token were used. The existing originating-version barrier is retained and combined with the token.
6. Annotated PDF export read protected filename metadata before acquiring operation-lifetime authorization. Authorization is now acquired first.
7. Note PDF print initially retained only derivative authorization. Protected exports now retain the exact Vault read permit until `afterprint`.
8. Note PDF lifetime initially did not prove the synchronized note was still the note for which the permit was acquired. Exact note identity is now checked before and after synchronization.
9. Source/citation and hierarchy reads were tokenized before their mutation-result publication paths. The mutation results are now gated too.
10. The B3 architecture validator encoded a raw `document::eval` call rather than the semantic browser-await boundary. It now recognizes the authoritative publication-leased helper and preserves the original ordering invariant.

## Final architecture for 2b.3

```text
privacy transition request
        |
        +--> UI local revocation window (immediate fail-closed)
        |
        +--> PrivacyCoordinator admission
                 |
                 +--> invalidate publication generation
                 +--> deny new publication auth / leases
                 +--> drain started publication leases
                 +--> derivative/Vault/storage transition ordering (2b.2)

protected async read
        |
        +--> PrivacyPublicationToken
                 |
                 +--> coordinator authorization
                 +--> UI revocation epoch
                 +--> library generation (where relevant)
                 +--> note/editor generation (where relevant)
                 |
                 +--> is_publishable() for Rust UI publication
                 +--> begin_publication() for async WebView handoff

native protected operation
        |
        +--> operation resource retains derivative permit
        +--> exact Vault permit where protected
        +--> native chooser / print / handoff completes
        +--> resource drops
```

## Native release gate

This execution environment has no `cargo`, `rustc`, or `rustfmt`. Native compilation, formatting, Clippy and Rust test execution therefore remain mandatory on the development Mac before release. They are not represented as passing here.

## Explicit closure addition — RTF import/export removal — CLOSED

At the user's request during final 2b.3 closure, Rustnote's RTF import/export capability was removed completely rather than hidden or deprecated:

- deleted `src/portability/rtf.rs`, including its parser/serializer and RTF-specific tests;
- removed `PortabilityFormat::Rtf` from the canonical portability vocabulary and reduced `PortabilityFormat::ALL` to TXT, Markdown, HTML and Rustnote bundle;
- removed `.rtf` path recognition, import inspection/counting, import-dialog filtering and RTF-specific validation messages;
- removed RTF export filters/options from application/UI format enumeration;
- removed the RTF-specific editor UI contract assertion while retaining the HTML paragraph-layout portability assertion;
- updated historical QA wording that explicitly claimed RTF portability;
- added a 2b.3 validator rule that fails if the RTF codec, enum variant, extension recognition, import/export filter, or UI/test capability reappears.

No compatibility stub, dormant decoder, hidden export branch, or dual portability path remains. TXT, Markdown, HTML, Rustnote bundle and the separate PDF export path are unchanged by this deletion.

## Packaging closure

The final 2b.3 source is packaged from the audited tree and then re-extracted into a clean directory for file-set and byte-for-byte comparison. The extracted package is required to rerun the 2b.3/2b.2 Vault validators, inherited 2a.18/2a.19 privacy guards, editor-document contract, dependency-free browser/editor/PDF behavior suite, and the RTF-absence sweep before delivery.
