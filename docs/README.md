# Birch documentation index

**Current source:** Rustnote v0.12.6, build 2b.43.3 snapshot in the `birch` repository. This is development documentation, not a security certification or release announcement.

## Living references

| Purpose | Authoritative entry point |
| --- | --- |
| Current implemented/planned state and verification gaps | [Project status](PROJECT_STATUS.md) |
| System ownership and data/security boundaries | [Architecture](ARCHITECTURE.md) |
| Toolchain, local build and execution commands | [Development](DEVELOPMENT.md) |
| Native compile/Clippy/test evolution and persistent compatibility lessons | [Native toolchain history](development/NATIVE_TOOLCHAIN_HISTORY.md) |
| CI scope, test evidence and release gates | [QA](QA.md) |
| Historical QA results and limitations | [QA evidence ledger](qa/HISTORICAL_EVIDENCE.md) |
| Contribution and change discipline | [Contributing](../CONTRIBUTING.md) |
| Vulnerability reporting and security limitations | [Security policy](../SECURITY.md) |
| Third-party components, licences and baseline PDF.js caveat | [Third-party notices](../THIRD_PARTY_NOTICES.md) |

The exact editor-document authority is [`editor-spec/README.md`](../editor-spec/README.md), with its adjacent JSON specification and cases. The [editor source map](../editor-src/README.md), [PDF viewer source map](../pdf-src/README.md), and [E0 research benchmark guide](../tools/e0-crypto-bench/README.md) retain their own technical responsibilities.

## Authoritative security and backup contracts

Use the [contract authority index](contracts/README.md) to navigate the E0–E3 and 2b.43 security/backup contracts, their **complete section-by-section source maps**, explicit amendment precedence, and unresolved implementation differences. It does not replace the original byte-exact documents.

## Detailed contracts and historical evidence

E0–E3 security rules remain in [E0](../SECURITY_ENCRYPTION_E0_CONTRACT.md), [E1](../SECURITY_ENCRYPTION_E1_CONTRACT.md), [E2](../SECURITY_ENCRYPTION_E2_CONTRACT.md), and [E3](../SECURITY_ENCRYPTION_E3_CONTRACT.md). The planned encrypted-backup key model and byte format are in [2b.43.1](../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md) and [2b.43.2](../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md), with staged 2b.43.3 source status reconciled in the [backup authority map](contracts/BACKUP_AUTHORITY.md). These detailed contracts **are not superseded by a brief overview**.

Pass-specific history is consolidated into the Vault/security, QA and native-toolchain ledgers. Exact deleted originals remain accessible from immutable Git links in the [archive/no-loss map](ARCHIVE.md). Files that remain at pass-numbered root paths are validator compatibility fixtures, not preferred documentation.

Initial repository-publication instructions and the 2026-11-05 bootstrap QA report are preserved as Git-only historical evidence; see the [archive/no-loss map](ARCHIVE.md).

## Historical archive

- [Final archive and 239-file no-loss mapping](ARCHIVE.md)

**Interpretation:** The current code describes implemented behavior; a governing contract describes intended behavior; a QA report describes only the tests performed on its stated version and environment. Treat conflicts as findings, not as permission to silently change the contract.
