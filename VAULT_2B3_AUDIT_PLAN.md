# Rustnote Vault 2b.3 — Audit & Objective Plan

Baseline: Rustnote v0.12.5 build 2b.2
Governing contract: Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model

## Mandatory audit scope

- Existing `UiPrivacyEpoch` and every caller-side privacy generation check.
- Async note-list publication and selected-note reconciliation.
- Backlinks, note picker/search candidates, hierarchy, note properties, sources/citations and note-link target publication.
- Revision-summary and revision-preview async publication.
- Attachment Open / Reveal / Save-As and all protected attachment metadata/filename lifetimes.
- Library replacement/restore and Protect/Lock/Unlock/Unprotect invalidation ordering.
- Vault read-permit publication checks and coordinator privacy generation semantics.
- Dialogs/caches opened from protected content.

## Finite objectives

1. Introduce one authoritative `PrivacyPublicationToken` whose final `is_publishable`/validation decision combines the relevant privacy generation and protected-note session authorization.
2. Invalidate publication tokens at privacy-transition admission, before stale ordinary or protected results can publish.
3. Expose narrow application/storage token capture APIs; do not expose VaultRuntime or coordinator internals to UI features.
4. Migrate every existing async UI surface capable of carrying protected plaintext to tokens; retain feature-local request counters only for non-privacy supersession.
5. Replace manual cached-generation render/action guards with token-backed publication state where the old privacy epoch is superseded.
6. Ensure protected revision preview keeps authorization through derivation and token validation, without storing a permit in the UI cache afterward.
7. Ensure Open / Reveal / Save-As retain their protected authorization through the full native/dialog lifetime and verify authorization before external handoff.
8. Remove superseded `UiPrivacyEpoch` choreography and duplicated privacy-generation checks from production callers.
9. Add adversarial tests/static guards racing Lock/Protect/library restore against every migrated async publication class.
10. Re-run inherited E0–E3, 2a.16–2a.19, 2b.1 and 2b.2 validators plus dependency-free behavior QA; document native Rust gates separately if unavailable.

## Non-goals

- No 2b.4 granular protected-note persistence cutover.
- No search architecture change.
- No cryptographic format change.
- No broad UI generation-counter refactor unrelated to privacy.

## Explicit closure scope addition — RTF removal

Before final packaging, remove Rich Text Format (`.rtf`) import/export completely rather than retaining a dormant compatibility path. The closure objective is finite:

- delete the RTF codec and its codec tests;
- remove `PortabilityFormat::Rtf` and its capability declaration;
- remove RTF import discovery/counting and file-dialog filters;
- remove RTF export choices and extension filters;
- remove RTF-specific UI/help/contract assertions;
- prove no production/test portability surface still recognizes or emits `.rtf`.

This is a deletion/consolidation request only. It does not change TXT, Markdown, HTML, Rustnote bundle, or PDF behavior.
