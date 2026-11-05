# Rustnote E0–E3 Remediation — 2a.18 UI Privacy Lifetime + Protected Revision Restore

Build: **0.12.5 / 2a.18**

## Scope

2a.18 is deliberately separate from the filesystem ownership work in 2a.16 and persistent-residue work in 2a.17. It closes the remaining known process-memory/UI publication lifetime defects and completes protected revision restore without changing the E1 cryptographic format.

## Authoritative UI privacy epoch

`UiPrivacyEpoch` is the cross-note revocation authority for UI plaintext. Encrypt, Unlock, Lock and Remove Encryption advance it before the storage/Vault visibility transition begins. Existing note-list, note-selection, editor and persistence generations remain in place for their narrower responsibilities.

Async surfaces that can cache/publish protected or formerly-public note information capture the privacy generation and reject late publication when it changes. Render/action paths suppress stale cached state as a second barrier. The covered surfaces include Backlinks, note picker/search candidates, hierarchy, properties, sources/citations, note-link target registries and revision previews.

## Protected attachment command lifetime

Attachment Open/Reveal/Save-As no longer fetches protected metadata into an unguarded owned `Attachment` and releases authorization before the async command continues. `attachment_file_command_resource` acquires the note plaintext-derivative permit before reading metadata and, for protected content, retains the Vault read permit and materialized plaintext path. That single resource remains alive through the native operation and the complete Save-As dialog lifetime.

Protected attachment filenames are not embedded into command errors that can be published after the resource is released. Lock waits for the protected command resource; Encrypt waits for ordinary attachment commands through the derivative permit.

## Permit-bearing protected revision preview

Protected revision preview no longer releases its Vault read permit immediately after obtaining `RevisionSnapshot`. `RevisionSnapshotResource` owns the snapshot and an optional Vault permit. Ordinary revisions carry no permit; protected revisions retain one.

The UI obtains this resource inside the blocking read, derives editor projections/Markdown/HTML while the resource is alive, returns the derived preview together with the authorization resource, performs the final request-generation/privacy-epoch check, publishes or discards, and only then releases authorization. The preview cache itself does not retain a Vault permit; a pre-Lock privacy-epoch revocation plus selected-note unmount/render suppression prevents stale cache visibility while Lock waits for active derivation to finish.

## E2 protected revision restore

Ordinary notes continue through the existing SQLite revision restore path. An unlocked protected note instead restores directly inside its authenticated protected payload:

- authenticate and validate the current payload and selected protected revision;
- preserve current notebook/pin/archive/trash/creation routing state;
- reconstruct the historical logical note and attachment metadata in memory;
- verify each historical attachment has a consistent protected blob mapping;
- checkpoint the pre-restore protected state;
- replace protected note, attachment and outgoing-link state;
- checkpoint the restored state;
- reseal and transactionally replace the canonical protected payload;
- rebuild only locked attachment placeholder rows required for routing/accounting.

The path does not repopulate plaintext `note_revisions`, tags, FTS or note content columns. Runtime normalization after restore persists into the encrypted payload and updates only the structural timestamp in the ordinary note stub.

## Targeted regressions

Source includes behavioral regressions for:

- Lock waiting while a protected attachment command resource remains active;
- Encrypt waiting while an ordinary attachment command resource remains active;
- Lock waiting while a protected revision-preview resource remains active;
- protected revision restore surviving Lock → Unlock while ordinary note content, tag membership, FTS and `note_revisions` remain empty;
- restored protected plaintext remaining absent from SQLite/WAL byte scans.

The 2a.18 static guard also requires privacy-generation checks on every named UI cache and forbids the UI/application preview path from calling the unscoped revision snapshot API.

## Architectural non-changes

2a.18 does not change AEGIS-256, Argon2id, encrypted object/blob formats, SQLite/FTS architecture, backup policy, canonical plaintext staging ownership or attachment quarantine policy. It adds no encrypted notebooks, cloud sync, PWA, SQLCipher or RAM-only global search.

## Scope boundary

2a.18 closes the known UI/async lifetime findings and the known E2 protected-revision functional gap. It is not the independent final E0–E3 certification. Pass 2a.19 remains the de-novo adversarial closure audit and must treat all validators added through 2a.18 as untrusted.
