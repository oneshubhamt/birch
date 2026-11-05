# Vault 2b.42.4 — Multi-note UI privacy lifetime

## Objective

Make every UI surface that can contain plaintext derived from more than one note depend on the multi-note privacy generation and fail closed when any note visibility transition begins, without disturbing an unrelated selected editor note.

## Implemented

- The running application owns one aggregate privacy-lifetime effect derived from `UiPrivacyEpoch::observe_multi_note()`.
- On aggregate revocation admission it immediately invalidates pending list work and clears process-local note-list summaries, title-sensitive pagination cursors, tag catalog labels and tag-derived filter labels. It does **not** clear `selected_note` or the live editor session.
- When the aggregate revocation window closes, tags and the note list are repopulated only after fresh multi-note publication authority and a publication lease are acquired.
- All Notes/list rendering observes the multi-note epoch directly, hides cached note rows and tag labels while revocation is active, and suppresses keyboard navigation through stale list identities.
- Note-list page workers already publish through `capture_multi_note_publication`; this pass closes the retained-cache lifetime around those workers.
- Backlinks, hierarchy and note picker searches now carry an explicit reactive dependency on the multi-note privacy epoch inside their asynchronous effects. Revocation therefore clears their retained vectors/tokens, and revocation release triggers a fresh authorized load.
- The editor note-link target registry now starts empty. It no longer performs an unguarded synchronous target lookup during editor construction. Target lookup and WebView registry publication are multi-note-authorized, and revocation actively writes an empty registry into the live editor.
- Global source-candidate search remains explicitly multi-note while current-note source summaries remain note-local.
- Cross-note tag rename/delete retains a multi-note publication lease around the catalog mutation and its aggregate UI publication.
- Search, sort and filter changes now checkpoint the selected source note first through source-note authority and only then acquire a fresh multi-note lease for the aggregate list change.
- Notebook delete/remount handling similarly separates selected-source checkpoint authority from the subsequent multi-note notebook mutation lease.
- Editor-driven aggregate tag/list refreshes wait for fresh multi-note authority rather than directly publishing tag-catalog state from a note-local callback.

## Explicit non-goals

This pass does not redesign the Vault action controller itself. The exact Lock/Unlock/Protect/Remove Encryption orchestration, `vault_action_busy`, selected/unselected action behavior and failure reconciliation remain 2b.42.5.

It also does not redesign automatic backup, complete archive export, shutdown, or library Restore; those remain 2b.42.6. Settings' post-Restore refresh is therefore intentionally left for that pass.

## Security/UX result

A Lock of B may blank/refetch aggregate surfaces because they can contain B-derived plaintext, but it does not revoke or clear unrelated editor A. Aggregate state is reconstructed after the transition from canonical storage under fresh multi-note authority.
