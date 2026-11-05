# Rustnote 2b.28 — PDF test transport binding compile closure

## Audit
Native macOS compilation of build 2b.27 failed in the binary test target because two PDF lifetime tests bound the fixture transport as `_transport` while later explicitly calling `drop(transport)`.

The explicit drop is intentional: `AttachmentTransportStore` owns a `Storage` clone and should be released before the temporary library tree is removed. Removing the drop would weaken deterministic test cleanup; suppressing the binding as unused contradicts the actual lifetime use.

## Objectives
1. Restore the transport binding name in both affected PDF lifetime tests.
2. Preserve the explicit `drop(transport)` cleanup ordering.
3. Make no production behavior changes.
4. Admit build 2b.28 through inherited validators without weakening their architecture checks.
5. Add a targeted regression guard for the binding/drop relationship.
6. Re-run static, inherited security, behavioral, syntax, and archive-integrity QA available in this environment.
