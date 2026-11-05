# Rustnote 2b.28 — Implementation

Build 2b.28 closes the binary-test compile error reported by native Cargo for the two protected PDF lifetime tests in `src/attachment_protocol.rs`.

Both fixture destructures now bind `AttachmentTransportStore` as `transport`, matching the existing explicit `drop(transport)` at the end of each test. This preserves the intended lifetime: the transport/store clone remains alive for the test and is released before the temporary library directory is removed.

No production Rust path, privacy transition, PDF transport behavior, encryption behavior, schema, or persistence contract is changed.

Inherited validators are extended only to recognize 2b.28 as the same current architecture generation as 2b.27.
