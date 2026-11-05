# A1–A4 Architectural Closure QA

This closure pass finishes four residual boundaries from A1–A4 without introducing new user-facing behavior.

## Automated acceptance

From the Rustnote source directory on macOS:

```bash
./scripts/prepare-rustnote-mac.sh
./scripts/validate-architecture-a1-a4-closure.sh
```

The closure validator runs the complete A4 acceptance chain, which in turn protects A0–A3, then checks the four closure invariants below.

## Closure invariants

1. **Canonical library paths** — media compatibility, attachment handoff, and image-rendition roots have one source of truth.
2. **Narrow storage capabilities** — the live editor uses `AttachmentStore`, the sources panel uses `SourceStore`, and note-list persistence uses `EditorStore`; these surfaces no longer receive unrestricted `Storage`.
3. **Typed editor payloads** — stable Rust→editor command payloads use concrete DTOs rather than `serde_json::Value`.
4. **Note-switch ownership** — `EditorService` owns checkpoint→load; Dioxus keeps browser/session and selection-generation checks.

## Targeted manual regression

- Open Rustnote with the default library and with a configured custom library location. Confirm notes and attachments load normally.
- Import/open/reveal/save attachments, including image, PDF, audio/video, and generic files. Confirm previews and compatible media still resolve.
- Exercise image replacement/editing and verify derived image previews survive restart.
- Open the Sources panel; create/attach/detach a source and add/remove a citation.
- Switch rapidly among several notes while editing. Confirm the note being left is saved and a stale switch never replaces the newest selection.
- Exercise attachment picker/import recovery, image import/edit completion, and note-link insertion to cover the newly typed bridge DTOs.
- Restart Rustnote and confirm the last edits, attachment state, note links, and source/citation state persist.

## Closure criterion

A1–A4 closure is complete when the automated acceptance command passes and the targeted manual regression above shows no behavioral or performance regression.
