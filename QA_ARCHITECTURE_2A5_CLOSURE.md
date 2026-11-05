# Rustnote 2a.5 — A7/B1 validator closure

Build: **0.12.5 / 2a.5**

This is a deliberately narrow architecture/tooling closure pass. It changes no editor behavior, storage schema, encryption format, attachment semantics, search behavior, or user-facing feature.

## Objectives

1. Make `rust-toolchain.toml` the declarative authority for the Rust QA components required by the pinned toolchain.
2. Remove the remaining `src/ui/live_editor.rs -> crate::media_server` implementation dependency by routing the image-import staging URL through `AttachmentService`.
3. Update A7 to validate the declarative toolchain contract rather than brittle shell-script implementation text.
4. Re-run A0–A7, B1 and Security E0–E3 static acceptance without weakening any architecture guard.

## Resulting boundaries

```text
rust-toolchain.toml
  -> rustfmt + clippy provisioning authority

live_editor.rs
  -> AttachmentService
  -> media_server
```

`live_editor.rs` must not import or name `crate::media_server`.

## Acceptance

```bash
RUSTNOTE_A7_STATIC_ONLY=1 ./scripts/validate-architecture-a7.sh
RUSTNOTE_B1_STATIC_ONLY=1 ./scripts/validate-architecture-b1.sh
RUSTNOTE_E0_STATIC_ONLY=1 ./scripts/validate-security-e0.sh
RUSTNOTE_E1_STATIC_ONLY=1 ./scripts/validate-security-e1.sh
RUSTNOTE_E2_STATIC_ONLY=1 ./scripts/validate-security-e2.sh
RUSTNOTE_E3_STATIC_ONLY=1 ./scripts/validate-security-e3.sh
```

On a Rust-capable development machine also run the complete A7/B1 and workspace Cargo QA.
