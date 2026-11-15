# Documented source status — Birch/Rustnote

**Application-source baseline:** `main@514b797fb0a6a128050cf75ac2428fc12095106a` (Rustnote v0.12.6, build 2b.43, through 2b.43.3 secure-key-store native fix 3). Changes on `docs-consolidation` are documentation-only. The source inspection below **does not constitute execution of native or behavioral tests**.

| Area | Source checked | Status |
| --- | --- | --- |
| Identity | `Cargo.toml`, `Dioxus.toml` | Rust package `rustnote`, application Rustnote, repository Birch; no product rename or stored-data identifier change |
| Development platform | `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml` | Dioxus 0.7.10, Rust 1.97.1, macOS 12.0 minimum deployment target |
| Ordinary notes and Vault | `src/`, `crates/rustnote-vault/` | Source implementations exist; historical audits do not amount to new independent runtime certification |
| Repository key authority | `src/security/backup_key_authority.rs`, `src/platform/secure_key_store.rs` | Staged keyset/header authority and OS key-store protector source (2b.43.3) |
| Authority integration | `src/security/mod.rs` | Explicitly marked as **staged**, awaiting integration into backup storage |
| Active managed-backup writer | `src/storage/backup_repository.rs` | **Repository format version 2**; the proposed encrypted repository v3 persistence cutover has **not** occurred |
| Encrypted backup v3 | `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`, `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md` | Governing intended key model/byte format; not proof of an active v3 writer |
| CI | `.github/workflows/ci.yml` | Source/browser and native macOS jobs are configured; configured is not synonymous with passing |

**Next integration milestone relative to this baseline:** 2b.43.4, encrypted managed-backup persistence and restore. The [backup authority map](contracts/BACKUP_AUTHORITY.md) and current source explicitly separate staged 2b.43.3 key authority from the still-active v2 repository writer.

## Historical QA and outstanding gates

The 2026-11-05 bootstrap report is retained as Git-only historical evidence in the [archive](ARCHIVE.md). It recorded source/browser checks in its original environment while leaving native macOS compilation, strict Clippy/full Rust tests, actual Keychain/device behavior and dependency-backed ProseMirror verification outstanding. **This documentation project has not rerun those tests, inspected live CI run results, or validated a deployed application.**

The current source is an active-development snapshot, **not a signed or security-certified production release**. The OS key-store, unlocked process, private-data lifetime, backups, and release distribution require separate threat-model checks. There is no automatically configured app signing, notarization, publishing, or migration pipeline. See [QA and release gates](QA.md) and [security policy](../SECURITY.md).
