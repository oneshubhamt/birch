# Rustnote 2b.24 — Tag Mutation + Transport Lifetime Closure Plan

## Audit findings

The 2b.23 serial native suite now advances through the previously stalled media and protected attachment roundtrip tests. It exposes three ordinary tag failures and stalls at `protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it`.

The tag failures share one production regression: global `rename_tag` / `delete_tag` always call `begin_protected_notes_mutation`, but the privacy coordinator intentionally rejects an empty protected-note set. Ordinary-only libraries therefore fail before the tag transaction begins.

The transport stall is test-only: the test holds a protected transport resource, admits Lock, then calls the normal blocking `attachment_resource` path to observe revocation. That read waits for Lock to clear while Lock waits for the retained resource, creating a circular test wait.

The separate `protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only` failure has no panic detail in the interrupted full-suite log. Its production path is therefore not changed speculatively; the test gets descriptive invariant messages so a targeted native run can identify the exact failing checkpoint.

## Objectives

1. Preserve global tag serialization and fail-closed protected-note authorization.
2. Allow global tag rename/delete when the library has zero protected notes, without opening an ordinary→protected race.
3. Preserve complete protected-note authorization when protected notes exist.
4. Replace the transport test's blocking revocation probe with the existing nonblocking test-only Vault-state inspection.
5. Preserve transport-resource and Lock lifetimes unchanged in production.
6. Improve the protected metadata-locality test diagnostics without changing its behavior.
7. Add a 2b.24 static regression guard, run inherited validators and dependency-free behavioral QA, and package with exact archive verification.
