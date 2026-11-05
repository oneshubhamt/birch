# Rustnote Vault 2b.8 — Plaintext Authority Closure Plan

Baseline: `v0.12.5 build 2b.7`.

This is a bounded closure pass against the original 2b contract. It does not redesign the Vault object store, PrivacyCoordinator, publication-token model, or encrypted note format.

## De-novo affected-surface audit

The 2b.7 tree still contained three independent Rustnote-managed plaintext lifecycle families outside the canonical ownership authority:

1. **External output preparation** — native Save-As and storage atomic output created hidden sibling `.tmp` / `.previous` resources whose cleanup depended on `Drop`; replacement had no durable crash-recovery record.
2. **Emergency journal transitions** — journal `.tmp`, `.previous`, quarantine, create/rename/delete operations were governed by a journal-local raw-filesystem convention rather than `OwnedPlaintext`.
3. **Persistent derivative caches** — ordinary image renditions and media-compat outputs were materialized/renamed outside typed ownership, and attachment retirement could occur before best-effort derivative cleanup succeeded.

The audit also found two closure hazards in the first WIP implementation and treats them as objectives here:

- output recovery metadata must live in durable application state, not OS temp;
- a second Rustnote process must not recover a transaction still owned by a live process.

## Finite objectives

- [x] **O1 — One crash-durable external-output authority.** Persist an immutable recovery record before any Rustnote-created hidden output sibling can exist; recover `.tmp` / `.previous` across every replace boundary.
- [x] **O2 — Live-process exclusion.** Hold an OS file lock for the complete live output transaction and recover only records whose lock can be acquired.
- [x] **O3 — Typed external staging.** Reserve hidden file/directory/predecessor resources as `OwnedPlaintext` before create/copy/rename; final explicit destinations leave Rustnote cleanup ownership only on publication.
- [x] **O4 — Durable transfer completion.** Parent-directory sync must precede recovery-record retirement; sync/cleanup failure must preserve the durable record for startup retry.
- [x] **O5 — Consolidate output callers.** Native Save-As, `AtomicFileOutput`, `AtomicDirectoryOutput`, storage atomic copy/replace, verified backup copies, and archive snapshot staging use the new authority; remove the old markdown stale-temp scavenger rather than keep a second output cleanup convention.
- [x] **O6 — Emergency journal ownership consolidation.** Establish one durable namespace owner outside the journal tree before journal plaintext can be created; route journal create/rename/delete/quarantine through `OwnedPlaintext`.
- [x] **O7 — Persistent derivative ownership.** Establish durable namespace owners before ordinary rendition/media-cache plaintext is produced; typed rename/delete owns all persistent derivatives.
- [x] **O8 — Derivative retirement ordering.** Strictly remove image/media derivatives before the last canonical attachment plaintext owner is quarantined or retired, so failure retains an existing durable retry anchor.
- [x] **O9 — Startup authority order.** Establish persistent cache owners before producers and recover abandoned output transactions before normal application startup proceeds.
- [x] **O10 — Static/adversarial guard.** Add a focused 2b.8 validator for the prior blind spots and keep inherited 2b.1–2b.7/E0–E3 validators green.
- [x] **O11 — Cleanup/CLEER.** Remove superseded duplicate output cleanup logic and avoid a new repository/service framework.
- [x] **O12 — QA and packaging.** Run all dependency-free/static project validators available here, source-level syntax/scope sweeps, package a clean incremented source ZIP, and explicitly leave native Cargo check/Clippy/tests as a Mac release gate because this sandbox has no Rust toolchain.

## Acceptance

The pass can close source-level only if I1/I2 are no longer violated by the three audited families and no inherited 2b invariant regresses. Native compilation/testing remains mandatory before release certification.
