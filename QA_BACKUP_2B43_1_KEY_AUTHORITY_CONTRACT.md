# QA — 2b.43.1 Encrypted Managed Backup Key Authority Contract

## Result

**PASS — contract/audit pass only. No production backup implementation changed.**

## Baseline audited

```text
Rustnote v0.12.6
build 2b.43
```

## Source audit findings

The current managed repository was independently mapped before the contract was written.

Confirmed current architecture:

- external durable backup namespace owner marker lives beside the recursively deletable repository root;
- repository format is currently v2 and plaintext;
- automatic/safety databases are ordinary `.sqlite3` snapshots;
- attachment repository objects are ordinary copied files keyed by plaintext SHA-256;
- snapshot manifests and repository index are plaintext JSON;
- repository staging can contain a plaintext SQLite snapshot during construction;
- automatic and pre-restore safety snapshots share one repository, deduplication and retention budget;
- Restore stages selected repository database/blobs into the existing process-private restore namespace, then enters the canonical library-wide Restore transition for installation;
- Protect currently publishes durable backup privacy-reset obligations and purges all remembered owned managed repositories because historical snapshots can contain plaintext;
- backup-location history and external durable ownership already fail closed across unavailable/removable locations;
- there is currently no Keychain/Keystore/secure-key-store abstraction in Rustnote.

## Contract decisions verified

| Objective | Decision | Status |
|---|---|---|
| OS root key purpose | Wrap/authenticate repository keysets only; never bulk payload | PASS |
| Key creation/retrieval | Lazy first-use creation only from conclusively fresh state | PASS |
| Temporarily unavailable secure store | Backup/restore unavailable; library remains usable; no mutation/fallback | PASS |
| Deleted key with existing repo | Fail closed; preserve repository; never silently regenerate | PASS |
| Corrupt root/envelope | Integrity failure; preserve; no auto-repair | PASS |
| Repository key model | Independent random purpose-separated keyset per repository | PASS |
| Device binding | Device-local root; repo bound to library/repository identity, not absolute path | PASS |
| Domain separation | Backup-specific object classes + independent keys + authenticated context | PASS |
| Repository format | New clean v3; key envelope v1; backup object v1 | PASS |
| Dedup identity | HMAC-SHA-256 of plaintext digest under repository object-ID key | PASS |
| Key lifetime | Non-Clone/redacted/zeroized repository keyset; no Storage/global cache | PASS |
| Before secure storage available | No managed backup write and no plaintext fallback | PASS |
| Restore | Decrypt only into prior-owned process-private staging, then existing library Restore authority | PASS |
| Restore after key loss | Refuse before plaintext staging; preserve encrypted repo | PASS |
| Legacy formats | Destructive owned privacy cleanup only; no migration/adoption | PASS |
| Existing authorities | Reuse PrivacyCoordinator / OwnedPlaintext / durable namespace ownership | PASS |
| Production implementation in 2b.43.1 | None | PASS |

## Platform feasibility checked

The contract's intended platform semantics are compatible with current platform facilities:

- macOS Keychain supports device-only accessibility levels suitable for background application use after first unlock;
- Android Keystore supports non-exportable keys and enforcement of key-use purposes, with secure hardware backing where available.

Implementation dependency/API choice remains a later pass; 2b.43.1 intentionally does not add a secure-storage dependency.

## Security consequences recorded explicitly

Managed repositories are device-bound in 2b.43. Therefore true OS-root-key loss makes managed snapshots unreadable. Rustnote must preserve the ciphertext and fail closed rather than replacing the key. Portable manual archives remain a separate recovery channel until a future encrypted-portable-recovery design exists.

This tradeoff is intentional and may not be silently changed during implementation.

## No-implementation verification

2b.43.1 adds only contract/QA/validation artifacts. It does not alter:

```text
src/
crates/
Cargo.toml
Cargo.lock
build-id.txt
```

The current plaintext repository remains operational until later passes implement and prove the encrypted cutover. The existing Protect-time repository privacy reset remains mandatory until 2b.43.7 proves the v3 invariant and retires legacy plaintext repositories.

## Closure

All fifteen 2b.43.1 Definition-of-Done items are satisfied at the contract level.

Next authorized pass:

```text
2b.43.2 — byte-level encrypted repository/object format
```

No storage implementation should begin under a different key/recovery/device-binding model without first amending `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md` explicitly.

## Executed pass QA

After writing the contract, the exact pass tree was checked again:

```text
validate-backup-2b43-1-key-contract.sh     PASS — 12/12
v0.12.6 + 2b.43 promotion guard            PASS
shell syntax                               PASS — 90 scripts
JS/MJS syntax                              PASS — 21 files
2b.42 final pre-S1 static closure           PASS
baseline comparison                         PASS
```

Baseline comparison against the promoted `rustnote_v0.12.6_build_2b.43_source.zip` proved:

```text
missing baseline files: 0
changed baseline files: 0
new files: exactly 3
```

The three additions are:

```text
BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md
QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md
scripts/validate-backup-2b43-1-key-contract.sh
```

Therefore 2b.43.1 remains a true contract-only pass.
