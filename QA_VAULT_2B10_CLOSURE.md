# Rustnote Vault 2b.10 — Cross-cutting Invariant Closure QA

## Findings reconstructed independently

The final source audit did not use the 2b.10 validator as its primary proof. It reconstructed the three original failure classes directly.

### I1/I2 namespace marker loss

Verified source ordering:

```text
inspect managed root occupancy
→ inspect durable marker
→ missing marker + nonempty root fails
→ marker creation is unreachable in that state
```

`adopt_existing_with_proof()` has exactly one production call, in automatic-backup one-way migration after the old inline owner and library identity are verified. Journal/media/rendition code contains no exceptional adoption call.

### I5 global protected status lifetime

Verified:

- `NoteEditor` has exactly one ordinary-note bypass to its outer status handler;
- protected status captures the fixed note publication scope at the central editor boundary;
- actual global publication uses `PrivacyPublicationToken::publish()` and therefore a coordinator publication lease;
- the toast retains the token;
- render state rechecks token validity;
- UI/library/note generations are reactive dependencies;
- every privacy generation change clears any existing toast, covering Protect reclassification as well as protected Lock/revocation.

There is exactly one global `ToastMessage` construction path.

### I6 protected sync queue

Verified:

- there is exactly one `INSERT INTO sync_queue` writer;
- its guarded upsert/delete callers classify protection before `serde_json` serialization;
- protected classification deletes stale rows and returns without payload creation;
- attachment recovery classification includes `attachment_operations` so ordinary remove tombstones remain possible after row deletion;
- Protection still removes any already-existing protected note/attachment queue rows;
- `SensitiveNoteData` still classifies protected sync queue rows as forbidden.

## Added adversarial/non-regression Rust coverage

- durable namespace fresh establishment rejects nonempty unowned root;
- explicit proven adoption is separate;
- protected note and attachment upsert/delete cannot enter `sync_queue`;
- ordinary note and attachment upsert/delete remains enabled.

The existing UI lifetime static/adversarial guards were strengthened for the protected-status publisher, retained token, reactive validity, and privacy-transition toast clearing.

## Runnable frozen-tree regression gates

The final runnable gate was executed after source freeze. All of the following passed in their documented dependency-free/static modes:

- Architecture A1-A7;
- A1-A4 cumulative closure;
- Architecture B1-B6, including dependency-free browser behavior checks;
- Dioxus 0.7.10 static baseline;
- Security E0-E3 static contracts;
- E0-E3 ownership 2a.16;
- E0-E3 residue 2a.17;
- E0-E3 UI lifetime 2a.18;
- E0-E3 adversarial closure 2a.19;
- Vault 2b.1 through 2b.10.

## Native release gate not executable here

This environment has no Rust toolchain, so the following are not claimed as passed:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

The new `remote-sync` regressions require the all-features native test gate.

## Frozen-tree independent result

A separate source audit, outside the accumulated validators, reconstructed all three 2b.10 failure classes after the final code change. It passed. The combined all-in-one gate timed out while recursively entering 2b.3, but the remaining 2b.3-2b.10 validators were then run individually and all passed; no assertion failure was reported by the timed-out wrapper.
