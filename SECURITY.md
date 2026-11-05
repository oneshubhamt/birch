# Security policy — Rustnote/Birch

## Disclosure

This project is in active private development. **Do not publish vulnerabilities or exploit details in public issues.** Use GitHub's private vulnerability reporting / security advisory flow when enabled for this repository, or contact the repository owner privately by an already established channel. Do not include real user notes, decrypted backups, keys or recovery data in bug reports.

## Threat model and limitations

Rustnote maintains a distinction between ordinary SQLite-backed content and Vault-protected content. Locking, decrypted UI lifetime, temporary file ownership, FTS, revisions, backups, and attachment paths are part of the security design, not peripheral implementation details. Refer to the governing `SECURITY_ENCRYPTION_E0_CONTRACT.md` through `SECURITY_ENCRYPTION_E3_CONTRACT.md` and later `VAULT_*` contracts for full context.

At baseline **2b.43.3** the OS secure key-store and backup key authority are implemented at source level but **encrypted backup repository cutover has not occurred**. The current backup writer cannot be presumed to use the new key authority. The baseline implementation report explicitly identifies native macOS compile/Clippy/test as an outstanding gate. Never advertise a security proof on the strength of static validation alone.

Private GitHub visibility helps prevent accidental source disclosure but is not encryption of runtime data. The OS account, system key store, active unlocked process, local backups, and build/distribution pipeline have different threat models. GitHub CI must never receive real production data or keychain secrets.

## Dependency and supply-chain security

Keep lockfiles and exact dependency pins under review. GitHub Actions run with read-only repository permissions and no production secrets. Dependabot generates proposed updates, to be independently reviewed before merging. Releases are **not** automatically signed, notarized or published by this repository's CI.
