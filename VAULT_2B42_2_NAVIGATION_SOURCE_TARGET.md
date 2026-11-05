# Rustnote Vault 2b.42.2 — Navigation source/target separation

## Scope

This subpass changes only navigation/publication orchestration. It does not migrate all note-local UI surfaces, all aggregate surfaces, Vault-action transitions, backup/Restore, profiling, or VACUUM behavior.

## Governing rule

A navigation request must never use one privacy authorization for both the note being left and the note being entered.

```text
source A authorization
→ checkpoint A

fresh target C authorization
→ load C
→ freshness check
→ target publication lease
→ publish C
```

Aggregate navigation state has a third, independent authority:

```text
source note synchronization
→ fresh multi-note authorization/lease
→ change section/notebook/list state
```

## Implementation

### Ordinary target navigation

`src/ui/note_list_data.rs::select_note` advances `note_selection_generation` before starting. It then:

1. synchronizes the current source editor under source-note authority;
2. aborts if the request was superseded or source synchronization failed;
3. captures fresh note-local authority for the requested target;
4. loads the target from canonical storage;
5. rechecks both selection generation and target authorization;
6. acquires a target publication lease;
7. publishes the target note.

A stale target result is retried under fresh authority instead of being published.

### No-current-note edge

When no note is currently selected, source synchronization retains the existing pending-autosave durability barrier. The helper uses multi-note authority because the pending autosave queue may belong to a previously selected note whose identity is no longer represented by `selected_note`.

### List reconciliation

List/page reconciliation no longer loads and carries a full fallback `Note` under aggregate list authority. It carries only `Option<NoteId>`. A chosen fallback target re-enters `select_note`, so its canonical note load/publication is note-authorized.

### Section/notebook navigation

`switch_library_section` and sidebar navigation now use `synchronize_then_begin_multi_note_publication_when_privacy_ready`:

1. synchronize source under source-note authority;
2. acquire a new multi-note authorization/lease;
3. mutate aggregate navigation state and reload the list.

The obsolete helper that began publication on the source token itself was removed.

### Navigation entry points

Ordinary card/list navigation, editor note-link opening, backlinks opening, and index/keyboard navigation funnel through `select_note`. Note pickers used for link/hierarchy selection remain selection-only and do not publish `selected_note` directly.

### Create-note compile/authority closure

The 2b.42.1 checkpoint contained a stale reference to the removed `wait_for_note_list_publication_token`. Because Create Note also creates a new selection target, this subpass closes that compile hole without reusing source authority:

1. source note is checkpointed;
2. note C is created once;
3. fresh C note authority publishes C;
4. fresh multi-note authority updates section/search/filter/list state.

This is intentionally limited to source/target publication separation; broader Create Note/Vault concurrency remains part of later adversarial passes.

## Explicitly not changed

- note-local feature cache/reactivity classification — 2b.42.3;
- aggregate feature classification — 2b.42.4;
- Vault-action controller — 2b.42.5;
- backup/archive/shutdown/Restore — 2b.42.6;
- profiling and VACUUM — 2b.42.7/2b.42.8.
