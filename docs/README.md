# Birch documentation index

**Current source:** Rustnote v0.12.6, build 2b.43.3 snapshot in the `birch` repository. This is development documentation, not a security certification or release announcement.

## Living references

| Purpose | Authoritative entry point |
| --- | --- |
| Current implemented/planned state and verification gaps | [Project status](PROJECT_STATUS.md) |
| System ownership and data/security boundaries | [Architecture](ARCHITECTURE.md) |
| Toolchain, local build and execution commands | [Development](DEVELOPMENT.md) |
| CI scope, test evidence and release gates | [QA](QA.md) |
| Contribution and change discipline | [Contributing](../CONTRIBUTING.md) |
| Vulnerability reporting and security limitations | [Security policy](../SECURITY.md) |
| Third-party components, licences and baseline PDF.js caveat | [Third-party notices](../THIRD_PARTY_NOTICES.md) |

The exact editor-document authority is [`editor-spec/README.md`](../editor-spec/README.md), with its adjacent JSON specification and cases. The [editor source map](../editor-src/README.md), [PDF viewer source map](../pdf-src/README.md), and [E0 research benchmark guide](../tools/e0-crypto-bench/README.md) retain their own technical responsibilities.

## Authoritative security and backup contracts

Use the [contract authority index](contracts/README.md) to navigate the E0–E3 and 2b.43 security/backup contracts, their **complete section-by-section source maps**, explicit amendment precedence, and unresolved implementation differences. It does not replace the original byte-exact documents.

## Detailed contracts and historical evidence

E0–E3 security rules remain in [E0](../SECURITY_ENCRYPTION_E0_CONTRACT.md), [E1](../SECURITY_ENCRYPTION_E1_CONTRACT.md), [E2](../SECURITY_ENCRYPTION_E2_CONTRACT.md), and [E3](../SECURITY_ENCRYPTION_E3_CONTRACT.md). The planned encrypted-backup key model and byte format are in [2b.43.1](../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md) and [2b.43.2](../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md), with the staged source implementation documented in [2b.43.3](../BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md). These detailed contracts **are not superseded by a brief overview**.

Pass-specific root records are being collapsed into current-purpose documentation. Batch 3 has already consolidated the Vault/security implementation series; exact deleted originals remain accessible from the immutable baseline links in [DOCUMENT_INVENTORY.md](DOCUMENT_INVENTORY.md). Files that remain at old root paths should be assumed to be temporary validator dependencies or not-yet-consolidated batches, not preferred documentation.

[Initial publication instructions](REPOSITORY_SETUP.md) and [the 2026-11-05 bootstrap QA report](BOOTSTRAP_QA.md) describe **historical repository setup**, not instructions to republish a populated `main` or evidence that current checks have passed.

## Consolidation tracking

- [Frozen inventory of 239 original Markdown files](DOCUMENT_INVENTORY.md)
- [Batch protocol and loss-prevention rules](CONSOLIDATION_PLAN.md)
- [Source-to-destination and outstanding-finding register](CONSOLIDATION_REGISTER.md)

**Interpretation:** The current code describes implemented behavior; a governing contract describes intended behavior; a QA report describes only the tests performed on its stated version and environment. Treat conflicts as findings, not as permission to silently change the contract.
