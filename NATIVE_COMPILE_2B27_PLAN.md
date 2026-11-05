# Rustnote 2b.27 — Main PDF lifetime closure plan

## Audit findings

1. The serial native suite completes all 277 library tests and then stalls in `attachment_protocol::pdf_transport_tests::protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops`.
2. That test uses `transport.attachment_resource(...)` after Lock admission as a revocation probe. The read correctly waits behind the admitted transition while Lock correctly waits for the still-live PDF response lease, creating a test-only circular wait.
3. The immediately following `protected_pdf_file_handle_keeps_lock_waiting_until_descriptor_drops` contains the same blocking probe and would be vulnerable to the same deadlock.
4. The nonblocking Vault-state probe already used by the passing library media/transport tests is currently only compiled under `cfg(test)`, so binary tests cannot call it through the library crate.

## Objectives

- Keep production Lock, transport, PDF handle, and Vault lease semantics unchanged.
- Expose the existing inspection helper only for tests/internal-test-support.
- Replace both blocking PDF revocation probes with the nonblocking test-only inspection helper.
- Preserve the assertion that Lock remains incomplete until the protected response/descriptor lifetime ends.
- Use the established 15-second post-release completion bound.
- Re-run inherited E0–E3/Vault static closure and dependency-free behavioral QA.
