# Rustnote 2b.15 — Transport Visibility + Warning Closure Implementation

## Binary/library privacy boundary
`attachment_protocol.rs` is part of the binary crate while Rustnote storage/security live in the library crate. Build 2b.15 does not make security internals public to resolve this boundary.

Instead, storage transport now exposes two narrow opaque public guards:

- `AttachmentPlaintextIngressGuard` retains the compound plaintext/Vault authorization for note-scoped image ingress. Its underlying `PlaintextOperationPermit` remains private.
- `AttachmentEphemeralPlaintextGuard` owns cleanup for one protected preview namespace. Its underlying `OwnedPlaintext` remains private.

`AttachmentTransportResource::prepare_ephemeral_plaintext_directory` reserves Vault-session plaintext ownership first and then asks `OwnedPlaintext` to create the private directory. The binary no longer calls `native::private_temp` or manipulates `OwnedPlaintext` directly.

## Plaintext ownership
`OwnedPlaintext::create_private_directory` is the authoritative create path for an already-reserved Vault/process-private/durable-namespace directory. It rejects pre-existing destinations and materializes only under an existing trusted parent, preserving ownership-before-plaintext ordering.

## Warning/dead-code cleanup
The native warnings supplied by the macOS build were treated as cleanup objectives rather than ignored:

- unused transport methods exposing `PlaintextOperationPermit` were removed;
- the unused `ExportError::AttachmentNotFound` variant was removed;
- unused best-effort cache wrappers were removed while strict privacy cleanup remains;
- durable ownership marker payloads remain semantically present but use named `_ownership` fields so the compiler understands their intentional retention;
- runtime helpers used only by tests are test-only;
- declarative sensitive-data contract constants/helpers not used by production are test-only and exercised in tests;
- unused attachment capability façade wrappers were removed while authorized production paths remain;
- `ProtectedAttachmentResource` retains only the fields actually required for its lifetime (`path` + Vault permit);
- stale unused transport locals in the binary protocol were removed.

No blanket `#[allow(dead_code)]` or `#[allow(private_interfaces)]` suppression was added.

## Security invariants
The opaque façade strengthens rather than weakens the intended architecture: the binary can request a protected transport operation, but cannot reconstruct coordinator ordering, create an `OwnedPlaintext`, or invoke private-temp creation independently.
