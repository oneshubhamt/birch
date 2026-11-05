# QA — Vault 2b.1–2b.4 Final Objective Closure

## Scope

Targeted closure of the residual objectives found by an independent audit of passes 2b.1–2b.4 against the original Vault 2b contract.

## Targeted source/adversarial coverage

### 2b.2

- coordinator-owned single-note protected mutation guard;
- coordinator-owned multi-note protected mutation guard;
- Restore guard owns the exceptional reader/transition/storage ordering;
- retained post-Restore mutation fails closed if Lock admits first;
- Lock waits while a valid protected mutation guard retains authorization;
- partial multi-note authorization is released on failure;
- source sweep rejects a storage function that manually combines fresh `acquire_note_access` with `lock_operations`/`lock_storage`;
- source sweep rejects low-level transition choreography outside `src/security`.

### 2b.3

- tag mutation publication enters Dioxus state through a publication lease;
- revision-history refresh publication is lease-protected;
- attachment removal holds authorization across the async WebView handoff and decrypted filename/status lifetime;
- note export result/path/status publication is lease-protected;
- editor link-popover selected text/URL capture is lease-protected;
- link insertion carries note-scoped authorization across the async editor handoff;
- selected-note panels bind UI privacy + coordinator privacy + library + note/editor generations;
- raw library/note token-composition methods are private to `ui/privacy.rs`;
- Restore pre-token cannot be reused after persistence; post-Restore recapture preserves the original local generations.

### 2b.4

- protected `remove_tag_from_note` mutates encrypted `MetadataState`;
- global protected tag rename/delete mutate encrypted metadata and ordinary catalog data consistently;
- global tag mutation fails before ordinary mutation if any protected note is locked;
- direct `NoteStore` capability regression exercises the real tag command path;
- tests assert protected `note_tags` residue remains zero;
- existing granular locality, revision, attachment, manifest crash/corruption, GC, and protect/unprotect tests remain in the tree.

## Final available validation

The exact final source tree was checked with:

- `validate-vault-2b1-foundation.sh` — PASS
- `validate-vault-2b2-privacy-authority.sh` — PASS
- `validate-vault-2b3-publication-token.sh` — PASS
- `validate-vault-2b4-granular-cutover.sh` — PASS
- `validate-vault-2b5-consolidation.sh` — PASS
- cumulative E0–E3 static security closure — PASS
- 2a.16 canonical plaintext ownership — PASS
- 2a.17 persistent residue/backup closure — PASS
- 2a.18 UI privacy lifetime — PASS after updating the inherited validator to require the stronger semantic note token
- 2a.19 independent adversarial closure — PASS after updating the inherited validator to require coordinator-owned Restore/retained authorization
- architecture A1–A7 — PASS in static mode
- architecture B1–B6 — PASS in static mode
- editor-document JS contract corpus — PASS, 36 cases
- dependency-free browser/editor/PDF behavior suite — PASS, 36/36
- first-party JavaScript/shell syntax checks embedded in the architecture chain — PASS
- lightweight delimiter sanity scan over every changed Rust production file — PASS

Static architecture scripts were run with their documented `*_STATIC_ONLY=1` switches solely because the native Rust toolchain is absent.

## Native Rust gate — NOT EXECUTED

The environment has no `cargo`, `rustc`, or `rustfmt`.

The project currently discovers **254 Rust regression tests**, but this report does not claim that they compiled or ran.

Required native release gate:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

Then rerun the Vault/security validators on the native tree.

## QA verdict

Within the source/static/dependency-free environment available here, the residual 2b.1–2b.4 objective defects identified by the de-novo audit are closed. Native Rust compilation/test execution remains the sole unexecuted release gate.
