# Rustnote 2a.19 — Independent adversarial E0–E3 closure

Build: **0.12.5 / 2a.19**

## Purpose

2a.19 is the independent closure pass scheduled after 2a.16–2a.18. It starts from the original E0–E3 contracts and treats earlier remediation reports and validators as regression evidence, not as proof. No native Rust toolchain validation is claimed in this pass, by project-owner direction.

The original cryptographic architecture is unchanged: `rustnote-vault` remains the sole production cryptographic authority; AEGIS-256 remains the v1 protected-object/blob cipher with the full 256-bit authentication tag; Argon2id wraps random per-note key material. The previously recorded project-owner decision treating E0 representative-device acceptance as external remains unchanged.

## Defects found by the independent audit

The audit found real closure defects, so source changes were permitted.

### 1. Whole-library restore canonical-blob ownership gap

Complete archive/repository restore could replace an active canonical attachment through the generic atomic file replacement helper. That helper may create `.copy.tmp` / `.previous` siblings whose plaintext lifetime is not represented by the active library's canonical attachment references or the 2a.16 durable staging ledger. A crash or cleanup failure at the wrong transition could therefore leave Rustnote-managed plaintext outside an authoritative cleanup owner.

**Repair:** restore now reserves durable ownership before any incoming-only canonical plaintext exists. Changed bytes are copied to a same-directory managed temporary path, verified, and atomically renamed into an already-owned canonical destination. No restore-specific `.previous` plaintext is created.

### 2. Database replacement could strand old-library plaintext ownership

A successful database switch changes which canonical attachment paths the active SQLite database owns. Merely preinstalling incoming files is insufficient: old-library-only canonical blobs can survive after their old references disappear, while incoming-only blobs need an owner before the switch.

**Repair:** before the database switch, Rustnote seeds the incoming database with cleanup rows for old-library-only canonical blobs and reserves generic owners for incoming-only canonical paths. The SQLite backup write transaction becomes the ownership handoff. On success, seeded old-only paths are retired file-first/owner-second. On post-switch failure, incoming-only paths that the rollback database would not own are retired before rollback is permitted.

### 3. Whole-library restore did not revoke every old plaintext producer/session

The previous restore boundary serialized database replacement but did not constitute a library-wide privacy transition. An ordinary attachment/image/import producer could remain active across replacement, and an unlocked old-library Vault session/materialization could survive after the canonical database had changed.

**Repair:** both complete-archive and repository restore now:

1. stage/validate the source;
2. enter the library-wide plaintext-derivative transition and drain ordinary plaintext producers;
3. enter whole-library Vault revocation and remove old Vault sessions/materializations;
4. take `operation_lock` and perform the final safety snapshot/ownership handoff/install;
5. retain both transition guards through completion.

The Settings restore path also advances the UI privacy epoch, clears the selected note, disposes the live editor and resets the editor/autosave library state before storage replacement begins.

### 4. Protected revision restore authorization ended before UI plaintext lifetime

2a.18 correctly restored protected revisions into the encrypted payload, but the Vault permit could end at the storage boundary while editor replacement, projection/normalization and UI publication still handled decrypted restored state.

**Repair:** protected restore now returns an opaque `RevisionRestoreAuthorization` containing the Vault permit. The application/UI transports it without exposing the permit and retains it until all post-storage editor work and the final privacy-epoch publish/discard decision are complete.

### 5. Revocation/storage lock ordering and conflicting privacy mutations

Extending revision authorization exposed a lock-order problem. Waiting for Vault permits while already holding Storage's `operation_lock` can deadlock if a permit-bearing operation still needs that storage lock for its final authenticated mutation. Moving the drain before `operation_lock` then exposed a second race: two same-note privacy mutations could overlap their revocation guards and serialize only later at storage, allowing one operation (for example Lock) to report success while an already-started conflicting mutation remained queued to run next.

**Repair:** Vault revocation is now an explicit transition. Revocation is published and readers drain before `operation_lock`; its guard remains alive through the storage mutation, blocking Unlock in the gap. Same-note revocation transitions are exclusive, and whole-library revocation is mutually exclusive with all note-level revocations. Multi-note callers acquire note transitions in canonical note-id order.

## Restore ownership handoff

The final complete-restore lifecycle is:

```text
validate archive/repository in crash-owned private restore stage
        ↓
drain library plaintext producers
        ↓
revoke/drain all Vault sessions + tracked decrypted materializations
        ↓
operation_lock
        ↓
create rollback safety snapshot
        ↓
seed incoming DB owners for current-only canonical plaintext
        ↓
reserve current-DB generic owners for incoming-only canonical plaintext
        ↓
reserve same-directory temporary owner
        ↓
copy + hash + verify incoming bytes
        ↓
atomic rename temp → already-owned canonical path
        ↓
SQLite backup transaction switches canonical DB ownership
        ↓
finalize DB/runtime aids + attachment audit
        ↓
retire old-only plaintext under seeded owners
        ↓
release restore stage / rollback snapshot
```

The critical invariant is continuous ownership: at every point a touched plaintext path is owned by an existing canonical database reference, a durable generic staging record, or the process-session restore owner. A failed unlink does not discard the corresponding durable cleanup obligation.

If finalization fails after the database switch, rollback is allowed only after incoming-only canonical blobs that the rollback database would not own have been removed successfully. If that cleanup fails, the restored database stays canonical and the operation fails closed rather than recreating ownerless plaintext.

## Whole-library producer revocation

2a.19 extends the existing note-scoped `PlaintextDerivativeGate` with a library transition. The library transition blocks all new permits and waits until existing derivative permits for all notes reach zero. Complete archive and repository restore hold that guard through replacement.

Vault runtime has an independent whole-library transition because its responsibility is different: it revokes protected read authorization, waits active protected readers, removes registered decrypted paths fail-closed and removes note keys. The two systems stay separate but have one stable ordering for restore:

```text
library derivative transition
→ library Vault revocation
→ Storage operation_lock
```

This avoids both producer publication into the outgoing database and Vault/storage lock inversion.

## Protected revision publication lifetime

Protected revision restore now follows:

```text
storage operation_lock + Vault read permit
        ↓
decrypt selected revision
        ↓
reseal canonical protected payload
        ↓
commit encrypted mutation
        ↓
return restored state + opaque Vault authorization
        ↓
editor replacement / possible remount
        ↓
projection + protected normalization
        ↓
privacy epoch still current?
        ↓
yes: publish while permit still alive
no: discard
        ↓
drop authorization / release permit
```

Lock that starts during this lifetime revokes new reads and waits for the retained permit. Separately, the UI privacy epoch prevents an older async restore task from publishing after Encrypt/Lock/other Vault visibility changes.

## Adversarial matrix

| Attack | 2a.19 result |
| --- | --- |
| failed unlink at managed plaintext transitions | **PASS** — security-sensitive staging/Vault cleanup keeps owner/session until deletion succeeds; restore rollback refuses owner loss |
| rename succeeds / owner update fails | **PASS** — image registry rename updates under the registry lock; restore temporary rename targets an already-owned canonical path and only releases the now-absent temp owner afterward |
| owner update succeeds / filesystem operation fails | **PASS** — durable/process owner remains and recovery/retry is fail-closed |
| process death around attachment/image/import staging | **PASS** — inherited 2a.16 durable ledger or process-session owner covers abandoned state |
| process death during whole-library restore | **PASS** at the plaintext-ownership boundary — pre-switch paths remain under outgoing/generic owners; destination SQLite backup uses one destination write transaction; post-switch old-only cleanup rows are already in the incoming DB |
| Encrypt racing attachment/image/import staging | **PASS** — note derivative transition blocks new producers, drains prior permits, retires staging and then takes storage mutation lock |
| Lock racing protected async UI operations | **PASS** — Vault permits span protected attachment commands, revision previews and now revision restore publication; UI privacy epoch rejects stale publication |
| concurrent protected attachment readers/materialization | **PASS** — existing read leases/materialization tracking remain authoritative and Lock waits for them |
| hard delete / Trash purge during ingestion | **PASS** — derivative transition drains producers before owner-backed cleanup and deletion; eligibility is revalidated |
| startup with abandoned staging | **PASS** — generic durable stages and app-private process-session restore stages are recovered before normal use |
| corrupt managed staging record/path | **PASS / fail-closed** — path/kind validation failure prevents normal startup rather than guessing or deleting outside approved namespaces |
| offline backup → Encrypt → restart → reconnect | **PASS** — durable repository privacy obligation survives until Rustnote positively reacquires and resets the remembered repository |
| historical `.orphaned` duplicate | **PASS** — protection performs strict hash-targeted orphan quarantine cleanup; inherited migration handles pre-2a.17 residue |
| protected → unprotected → protected cycle | **PASS** — protected-payload and residue regression remains present; conflicting same-note/library privacy transitions are rejected fail-closed |
| revision restore while Lock begins | **PASS** — retained authorization + revocation-before-storage lock order removes stale publication and lock inversion |
| stale Backlinks / note-picker/search local result | **PASS** — 2a.18 privacy epoch remains the central invalidation gate |
| stale protected attachment filename / Save-As dialog after Lock | **PASS within Rustnote boundary** — command holds both derivative and Vault lifetime permits through dialog/native operation; deliberate external copies remain outside revocation scope |
| bundle import interrupted at lifecycle transitions | **PASS** — staged archive and extracted blobs remain under durable 2a.16 ownership until transactional transfer or cleanup |
| WAL / FTS / ordinary revision / autosave residue | **PASS** — protected transition rebuild/scrub, WAL truncation, revision-table deletion and protected-journal fail-closed recovery remain intact |
| direct SQLite writer sweep | **PASS** — no production SQLite connection constructor exists outside `src/storage` |
| direct filesystem writer sweep | **PASS with classified boundaries** — security-sensitive library plaintext writers use canonical/durable/Vault/process owners; remaining generic atomic writers are external egress, ordinary disposable caches, backup metadata/snapshots, or process-owned temporary namespaces |

## Direct writer sweep

The sweep searched production Rust sources for SQLite connection constructors and filesystem creation/write/copy/rename helpers rather than inspecting only the repaired functions.

SQLite connection authority remains confined to `src/storage`. Application/UI/native modules receive storage capabilities/resources rather than opening the canonical database directly.

Filesystem writers outside the durable library-staging ledger were reclassified against the E3 contract:

- `native::temp_owner`, Vault runtime and archive-restore staging are process-session owned with startup crash recovery;
- emergency journal has deterministic note ownership and protected-note pre-quarantine/fail-closed recovery;
- protected attachment handoff avoids persistent ordinary handoff caching and retains Vault authorization;
- ordinary rendition/media-compat caches are scrubbed/disabled at protected transitions;
- explicitly user-selected export/Save-As destinations are external egress and intentionally outside Lock revocation;
- backup repository mutable metadata/snapshot maintenance is governed by the backup privacy-obligation barrier rather than note staging ownership.

No new unmanaged production plaintext namespace was identified by this sweep.

## E0–E3 assessment

**E0:** source/threat/KDF/format contract remains intact. 2a.19 inherits the explicit project-owner treatment of representative-device performance acceptance as external evidence; no benchmark is fabricated here.

**E1:** **PASS at source architecture level.** No cryptographic primitive/format changes were made and direct production crypto remains isolated in `rustnote-vault`.

**E2:** **PASS at source architecture/functional-contract level.** Protected note/attachment persistence remains encrypted, and protected revision restore remains a Vault-aware encrypted mutation without repopulating ordinary `note_revisions`/FTS/tag plaintext.

**E3:** **PASS at independent source-level closure level after the defects above were repaired.** The audit did not find a remaining Rustnote-managed plaintext lifetime/residue bypass in the requested adversarial surfaces.

## Closure verdict

**2a.19 closes the requested independent E0–E3 source-level adversarial pass.**

This verdict is deliberately scoped. Per project-owner instruction, this environment does not claim Cargo/rustc/rustfmt/Clippy/native Rust execution. The retained external E0 representative-device acceptance premise is unchanged. Within those explicit exclusions, the final source has continuous ownership/revocation rules for the attacked filesystem, backup, UI, revision, restore and concurrency surfaces, and no further architectural security repair is indicated before moving to the next planned workstream.
