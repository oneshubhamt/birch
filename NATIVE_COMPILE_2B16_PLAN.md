# Rustnote 2b.16 — Native warning closure plan

## Audit
The first native `cargo check --workspace --all-targets --all-features --locked` to reach successful compilation on build 2b.15 reported one remaining warning in the binary crate: `serve_pdf_document` created an `AttachmentTransportStore` local that was never read.

Inspection confirms the local is genuinely redundant. The function re-authorizes reads through `pdf_attachment(library, attachment_id)`, whose transport session obtains and retains the canonical `AttachmentTransportResource` authorization guard. Removing the unused local does not remove authorization, change request lifetime, or alter PDF transport behavior.

## Objectives
1. Remove the single compiler-reported unused local instead of suppressing or underscore-renaming it.
2. Preserve the protected PDF authorization/lifetime path unchanged.
3. Advance build identity and inherited static validators without changing product behavior.
4. Re-run targeted Vault/E0–E3/static regression checks available without Cargo.
