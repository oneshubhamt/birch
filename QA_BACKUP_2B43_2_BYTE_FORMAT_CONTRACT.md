# QA — Rustnote 2b.43.2 Encrypted Managed Backup Byte-Format Contract

## Scope

2b.43.2 is a contract-only pass. It defines the canonical v3 repository bytes before any encrypted-backup production reader/writer or secure-store integration begins.

Baseline: Rustnote v0.12.6 / build 2b.43 with 2b.43.1 closed.

## Source audit performed

The pass re-read:

- `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`;
- current `src/storage/backup_repository.rs` repository layout, manifest/index structures, deduplication, publication and retention behavior;
- current library-identity grammar;
- current `rustnote-vault` AEGIS-256 object/blob formats, nonce construction, full 256-bit tag and bounded chunk streaming.

The format deliberately preserves useful existing semantics (immutable deduplicated blobs, manifest-last commit, reconstructable operational index, external durable namespace ownership) while replacing plaintext JSON/database/blob repository payloads with a canonical encrypted binary format.

## Decisions verified

- repository v3 / header v1 / keyset v1 / object v1 are fixed;
- all integer byte order and canonical string encodings are fixed;
- repository ID, snapshot ID, library binding and opaque blob ID are exact;
- `repository.header` offsets and 92-byte prefix are exact;
- `RepositoryKeysetV1` is exactly 204 bytes;
- Apple AEGIS envelope is exactly 268 bytes;
- Android AES-GCM envelope is exactly 232 bytes;
- small encrypted object header is exactly 116 bytes;
- bulk encrypted object header is exactly 112 bytes;
- v1 bulk chunk size is exactly 1 MiB;
- nonce/counter and AAD bytes are exact;
- index and manifest plaintext schemas are canonical and bounded;
- manifest publication is the snapshot commit point;
- index remains reconstructable operational metadata;
- pruning/crash recovery use authenticated manifests as authority;
- path/object/header identities are cross-checked after authentication;
- parser/resource limits, overflow checks and trailing-byte rejection are explicit;
- rollback freshness is explicitly outside v3 guarantees;
- serde/bincode/native struct layout are forbidden as repository format authorities.

## Structural vectors

The validator independently recomputes:

```text
library binding for "library_example_001"
= f0363a8464e6a722237fbdfdb98a0f869ef60c0bce8a5049c82267525ff54fdb

SHA-256("abc")
= ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad

HMAC-SHA-256(00..1f, SHA-256("abc"))
= 910ec338f64892dfc0288e343162e996b71c39c67dd1cad13923f2d67a79cff6
```

Ciphertext vectors are intentionally deferred to the implementation pass so deterministic nonce fixtures cannot accidentally become production APIs.

## Production-change boundary

No existing production file is modified by 2b.43.2. The pass adds only:

```text
BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md
QA_BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md
scripts/validate-backup-2b43-2-byte-format.sh
```

The runtime build remains `0.12.6+2b.43`.

## Gate

2b.43.3 may begin only after the contract validator, inherited 2b.43.1 contract validator, baseline/version validator, shell syntax, and applicable static regression closure remain green and a baseline diff proves the contract-only boundary.

## Executed QA results

```text
2b.43.1 key-authority validator            PASS (12/12)
2b.43.2 byte-format validator              PASS (18/18)
v0.12.6 / 2b.43 identity validator         PASS
shell syntax                               PASS (91/91)
JS/MJS syntax                              PASS (54/54)
2b.42 final pre-S1 static closure          PASS
dependency-free behavior/browser suite     PASS (111/111)
baseline comparison                        PASS
  new files                                3
  changed existing files                   0
  missing baseline files                   0
```

The execution environment has no Rust toolchain, so no new Cargo/Clippy/native test claim is made here. Because the pass modifies no production code, the native gate remains inherited from the promoted baseline and will be rerun when encrypted-backup implementation begins.
