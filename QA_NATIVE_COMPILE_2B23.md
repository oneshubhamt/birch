# QA — Rustnote 2b.23 Protected Attachment Roundtrip Lifetime Closure

## Targeted QA

- Verify `protected_note_encrypts_attachment_blob_and_restores_it_losslessly` drops its `ProtectedAttachmentResource` before Unprotect.
- Verify the byte-read assertion remains before the drop.
- Verify no protected attachment resource is reacquired between drop and Unprotect.
- Verify inherited 2b.22, 2b.21 and E0-E3 validators remain green.

## Native gate required on macOS

The authoritative native regression is:

```bash
cargo test --workspace --all-features --locked \
  storage::protection::tests::protected_note_encrypts_attachment_blob_and_restores_it_losslessly \
  -- --exact --nocapture --test-threads=1
```

Then resume the serial full workspace suite.

## Sandbox verification completed

- Targeted 2b.23 lifetime source audit: PASS.
- Shell syntax for all validation scripts: PASS.
- Vault 2b.1 through 2b.10 validators each reached their PASS result on the 2b.23 tree; some validators continue into redundant inherited chains after printing PASS, which can exceed the sandbox command timeout.
- Dependency-free browser/editor/PDF behavioral suite: 36/36 PASS.
- Rust source diff versus 2b.22: only `src/storage/protection.rs`, limited to the test-only resource drop/comment.
- Native Cargo/rustc execution remains unavailable in this sandbox; macOS native regression is authoritative.
