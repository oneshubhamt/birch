# Rustnote 2b.16 — Native warning closure implementation

- Removed the unused `let transport = library.transport_store();` from `serve_pdf_document`.
- No authorization was removed: both full and ranged PDF reads still call `pdf_attachment`, which opens the transport read session and returns a `ManagedAttachment` retaining the complete `AttachmentTransportResource` guard for the request lifetime.
- No cryptography, Vault format, database schema, privacy transition, attachment persistence, or UI behavior changed.
- Build identity advanced to `2b.16`; inherited validators accept the new build as the same security architecture.
