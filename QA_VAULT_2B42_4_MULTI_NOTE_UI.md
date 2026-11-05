# QA — Vault 2b.42.4 multi-note UI

## Required invariants

1. Lock/revocation of any B invalidates aggregate authorization and retained aggregate plaintext.
2. Aggregate revocation does not clear unrelated `selected_note`/editor A.
3. All Notes/list summaries, pagination cursors, tag catalog/filter labels, backlinks, hierarchy, picker results, note-link target registry and global source candidates cannot remain publishable across the old multi-note generation.
4. Aggregate data reloads only after fresh multi-note authority is available.
5. Search/sort/filter/sidebar aggregate changes do not reuse one aggregate token to checkpoint the selected source note.
6. Current-note source summaries and the editor's note-local state remain governed by the 2b.42.3 note-local model.

## Targeted assistant-side QA

Run:

```bash
./scripts/validate-vault-2b42-1-scoped-publication-core.sh
./scripts/validate-vault-2b42-2-navigation-source-target.sh
./scripts/validate-vault-2b42-3-note-local-ui.sh
./scripts/validate-vault-2b42-4-multi-note-ui.sh
node --test tests/js/multi-note-privacy-contract.test.mjs
```

Then run applicable inherited Vault/E0-E3 validators, syntax checks and the dependency-free browser/PDF runner. The previously known stale Vault-action assertion in `editor-ui-contract.test.mjs` is still a 2b.42.5 concern and must be baseline-compared rather than attributed to 2b.42.4.

## Targeted manual matrix

- Open A and its editor; start Lock B from an aggregate/list surface. A remains usable while All Notes/list/tag catalog is invalidated and later repopulated.
- Keep Backlinks A open; Lock B. Backlink rows disappear/fail closed, then reload after transition. A editor remains present.
- Keep Hierarchy A open; Lock B. Parent/child summaries and picker candidates fail closed and reload.
- Open a note picker/link picker; Lock B. Candidate results clear and cannot stale-republish.
- Open Sources A in Attach Existing mode; Lock B. Global source candidates clear/reload while A's current-note source summaries remain governed only by A's privacy lifetime.
- With note-link targets available in editor A, Lock B. The in-memory target vector and live WebView target registry clear, then repopulate under fresh aggregate authority.
- Keep a tag filter selected; Lock the note that supplied that protected-only tag. The tag label is removed from aggregate filter/catalog state and cannot remain visible from the old generation.

## Native gate

On macOS after installing this checkpoint:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
./scripts/validate-vault-2b42-1-scoped-publication-core.sh
./scripts/validate-vault-2b42-2-navigation-source-target.sh
./scripts/validate-vault-2b42-3-note-local-ui.sh
./scripts/validate-vault-2b42-4-multi-note-ui.sh
node --test tests/js/multi-note-privacy-contract.test.mjs
```

Native Rust formatting/compile/Clippy/tests remain mandatory because the assistant environment has no Rust toolchain.

## Observed assistant-side result

- 2b.42.1 scoped publication core guard: PASS.
- 2b.42.2 navigation source/target guard: PASS.
- 2b.42.3 note-local UI guard: PASS.
- 2b.42.4 multi-note UI guard: PASS.
- Dedicated multi-note JS contract: 8/8 PASS.
- 2b.11 atomic privacy-authority guard: PASS.
- Independent 2b.6 I1-I7 guard: PASS.
- 2b.12 AEGIS/privacy-disclosure guard: PASS when invoked through `bash` in this environment (the script's `zsh` interpreter is unavailable here).
- E0-E3 2a.16 canonical plaintext ownership guard: PASS.
- E0-E3 2a.17 persistent residue/backup guard: PASS.
- Shell syntax: 80/80 scripts PASS under their available interpreter, with `bash -n` used as the syntax fallback for zsh-shebang scripts because zsh is not installed.
- JS/MJS syntax: 34/34 PASS with `node --check`.
- Changed Rust delimiter/lexical structural audit: 8/8 PASS (`app.rs`, `editor.rs`, `filter_popover.rs`, `note_hierarchy.rs`, `note_links.rs`, `note_picker.rs`, `notes_header.rs`, `sidebar.rs`).
- Dependency-free browser/PDF runner: 60/61 PASS. The sole failure is the same stale Vault-action assertion present in the untouched 2b.42.3 baseline (baseline 52/53); it expects the removed global `wait_for_note_list_publication_token` helper and remains assigned to 2b.42.5.

Native Rust formatting/compile/Clippy/tests were not executable because no Rust toolchain is installed in the assistant environment. They remain mandatory before native acceptance.
