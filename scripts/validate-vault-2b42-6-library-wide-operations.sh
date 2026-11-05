#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors="ignore")


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains forbidden marker: {marker}")


def segment(text, start, end):
    i = text.find(start)
    if i < 0:
        raise SystemExit(f"missing segment start: {start}")
    j = text.find(end, i + len(start))
    if j < 0:
        raise SystemExit(f"missing segment end: {end}")
    return text[i:j]

coordinator = read("src/security/privacy_coordinator.rs")
security_mod = read("src/security/mod.rs")
capabilities = read("src/storage/capabilities.rs")
storage_mod = read("src/storage/mod.rs")
app_privacy = read("src/application/privacy.rs")
executor = read("src/application/executor.rs")
context = read("src/application/context.rs")
app_editor = read("src/application/editor.rs")
app_runtime = read("src/application/runtime.rs")
app_export = read("src/application/export.rs")
sync = read("src/ui/synchronization.rs")
app = read("src/ui/app.rs")
shutdown = read("src/ui/shutdown.rs")
settings = read("src/ui/settings.rs")
backups = read("src/storage/backups.rs")
backup_repository = read("src/storage/backup_repository.rs")
ui_portability = read("src/ui/portability.rs")
app_portability = read("src/application/portability.rs")
storage_portability = read("src/storage/portability.rs")
derivative = read("src/security/derivative.rs")
runner = read("scripts/run-browser-behavior-tests.mjs")

# 1. A dedicated non-publication authority exists. Snapshot/durability work must
# not impersonate a UI publication handoff merely to exclude privacy transitions.
require(coordinator, [
    "active_whole_library_operations: usize",
    "whole_library_operation_drained: Condvar",
    "pub(crate) struct WholeLibraryOperationLease",
    "pub(crate) fn begin_whole_library_operation(",
    "state.library_transition.is_some() || !state.note_transitions.is_empty()",
    "checked_add(1)",
    "whole_library_operation_drained.notify_all()",
], "coordinator whole-library operation authority")
require(security_mod, ["WholeLibraryOperationLease"], "security lease export")

# 2. Every transition admission path drains already-started whole-library work.
# This includes the exceptional protected revision-Restore admission path, which
# does not call admit_notes().
require(coordinator, [
    "fn wait_for_whole_library_operations_to_drain",
    "state = self.wait_for_whole_library_operations_to_drain(state);",
    "fn admit_notes(",
    "fn admit_library(",
    "pub(crate) fn begin_protected_note_restore(",
], "transition/library-operation serialization")
if coordinator.count("state = self.wait_for_whole_library_operations_to_drain(state);") < 3:
    raise SystemExit("not every privacy-transition admission path drains whole-library operations")

# 3. The authority crosses storage/application layers as a narrow capability.
require(capabilities, [
    "pub(crate) struct PrivacyOperationStore",
    "pub(crate) fn privacy_operation_store(&self) -> PrivacyOperationStore",
    "pub(crate) fn begin_whole_library_operation(",
], "storage privacy-operation capability")
require(storage_mod, ["PrivacyOperationStore"], "storage privacy-operation export")
require(app_privacy, [
    "pub(crate) struct PrivacyOperationService",
    "pub(crate) struct WholeLibraryOperationLease",
    "pub(crate) fn begin_whole_library_operation(",
], "application privacy-operation service")
require(context, [
    "privacy_operations: PrivacyOperationService",
    "PrivacyOperationService::new(storage.privacy_operation_store())",
    "pub(crate) fn privacy_operations(&self) -> PrivacyOperationService",
], "application services privacy-operation wiring")

# 4. True snapshot/durability APIs are type-gated by the new lease. checkpoint_all
# is deliberately *not* gated: it can perform protected note-local saves, so it
# must finish before the read-side whole-library lease is captured.
require(app_export, [
    "pub(crate) fn export_archive(",
    "_privacy_lease: &WholeLibraryOperationLease",
], "complete archive type gate")
require(app_runtime, [
    "pub(crate) fn maybe_create_automatic_backup(",
    "pub(crate) fn durability_checkpoint(",
], "runtime whole-library type gates")
if app_runtime.count("_privacy_lease: &WholeLibraryOperationLease") < 2:
    raise SystemExit("runtime backup/checkpoint are not both gated by whole-library lease")
require(app_editor, ["pub(crate) fn checkpoint_all(&self)"], "shutdown checkpoint_all API")
forbid(app_editor, ["checkpoint_all(\n        &self,\n        _privacy_lease: &WholeLibraryOperationLease"], "checkpoint_all library-lease coupling")

# 5. Serialized scheduling is an explicit pre-privacy boundary. The executor
# turn is RAII-owned and ordinary execute() reuses the same mechanism, so there
# is no second queue implementation for whole-library callers.
require(executor, [
    "pub(crate) struct ApplicationExecutionTurn",
    "pub(crate) async fn begin_operation(",
    "pub(crate) async fn stage<T, E, F>(mut self, work: F)",
    "pub(crate) async fn execute<T, E, F>(self, work: F)",
    ".begin_operation(operation)",
    "turn.execute(work).await",
], "serialized pre-privacy executor turn")

# 6. Canonical two-stage UI helper: the already-acquired executor turn is a
# required type input; source-editor scoped sync comes before fresh whole-library
# authority, with resync if the selected source changes while stage 2 waits.
helper = segment(
    sync,
    "pub(super) async fn synchronize_then_begin_whole_library_operation(",
    "#[derive(Clone, Debug, PartialEq, Eq)]",
)
require(helper, [
    "_operation_turn: &ApplicationExecutionTurn",
    "synchronize_current_editor_scoped(",
    "let synchronized_selection_generation = *note_selection_generation.peek();",
    "begin_whole_library_operation_for_synchronized_selection(",
    "return Ok(lease);",
], "two-stage library operation helper")
stage_two = segment(
    sync,
    "pub(super) async fn begin_whole_library_operation_for_synchronized_selection(",
    "#[derive(Clone, Debug, PartialEq, Eq)]",
)
require(stage_two, [
    "let mut authority_was_delayed = false;",
    "operation_privacy.begin_whole_library_operation()?",
    "if *note_selection_generation.peek() != synchronized_selection_generation",
    "|| authority_was_delayed",
    "authority_was_delayed = true;",
    "drop(lease);",
    "return Ok(Some(lease));",
], "fresh whole-library authority stage")
sync_pos = helper.find("synchronize_current_editor_scoped(")
generation_pos = helper.find("let synchronized_selection_generation = *note_selection_generation.peek();")
stage_two_pos = helper.find("begin_whole_library_operation_for_synchronized_selection(")
if min(sync_pos, generation_pos, stage_two_pos) < 0 or not (sync_pos < generation_pos < stage_two_pos):
    raise SystemExit("whole-library helper is not source synchronization -> fresh library authority")
forbid(helper + stage_two, ["begin_publication()"], "whole-library helper publication misuse")

# 7. Automatic backup uses the two-stage helper and carries the lease through
# the actual snapshot call. It no longer uses a multi-note UI publication lease
# as a durability barrier.
auto_backup = segment(app, "let automatic_backup_runtime", "let notebook_name")
require(auto_backup, [
    ".begin_operation(operation)",
    "synchronize_then_begin_whole_library_operation(",
    "&backup_turn",
    "backup_turn.execute(move ||",
    "SynchronizationMode::Persist",
    "maybe_create_automatic_backup(&backup_lease)",
], "automatic backup two-stage wiring")
forbid(auto_backup, [
    "wait_for_multi_note_publication_token",
    "_backup_publication_lease",
], "automatic backup obsolete publication barrier")
if auto_backup.find(".begin_operation(operation)") >= auto_backup.find("synchronize_then_begin_whole_library_operation("):
    raise SystemExit("automatic backup captures privacy authority before executor scheduling")

# 8. Manual complete archive uses the same two stages and type-gated export.
archive = segment(settings, 'title: "Export complete archive"', 'title: "Restore complete archive"')
require(archive, [
    ".begin_operation(operation)",
    "synchronize_then_begin_whole_library_operation(",
    "&operation_turn",
    ".execute(move ||",
    "SynchronizationMode::Persist",
    "export_archive(&archive_lease, &path)",
], "complete archive two-stage wiring")
forbid(archive, ["capture_multi_note_publication(", "begin_publication()"], "archive publication-barrier misuse")
if archive.find(".begin_operation(operation)") >= archive.find("synchronize_then_begin_whole_library_operation("):
    raise SystemExit("complete archive captures privacy authority before executor scheduling")

# 9. Shutdown keeps one serialized turn across three phases:
# source sync -> checkpoint_all note-local saves -> fresh library lease -> raw
# WAL/durability checkpoint. checkpoint_all must never run while holding the
# read-side library lease, otherwise an admitted Lock can form a circular wait.
require(shutdown, [
    "privacy_operations: PrivacyOperationService",
    "executor: ApplicationExecutor",
    ".begin_operation(ApplicationOperation::DurabilityCheckpoint)",
    "synchronize_current_editor_scoped(",
    "let synchronized_selection_generation = *note_selection_generation.peek();",
    ".stage(move ||",
    ".checkpoint_all()",
    "begin_whole_library_operation_for_synchronized_selection(",
    ".execute(move ||",
    "SynchronizationMode::Checkpoint",
    "durability_checkpoint(&library_operation_lease)",
], "shutdown staged library wiring")
forbid(shutdown, [
    "capture_multi_note_publication(",
    "begin_publication()",
    "checkpoint_all(&library_operation_lease)",
    "synchronize_then_begin_whole_library_operation(",
], "shutdown unsafe/obsolete library barrier")
turn_pos = shutdown.find(".begin_operation(ApplicationOperation::DurabilityCheckpoint)")
sync_pos = shutdown.find("synchronize_current_editor_scoped(")
checkpoint_pos = shutdown.find(".checkpoint_all()")
lease_pos = shutdown.find("begin_whole_library_operation_for_synchronized_selection(")
durability_pos = shutdown.find("durability_checkpoint(&library_operation_lease)")
if min(turn_pos, sync_pos, checkpoint_pos, lease_pos, durability_pos) < 0 or not (
    turn_pos < sync_pos < checkpoint_pos < lease_pos < durability_pos
):
    raise SystemExit("shutdown is not executor turn -> source sync -> checkpoint_all -> fresh library lease -> durability")
if app.count("privacy_operations: native_close_services.privacy_operations()") != 1:
    raise SystemExit("native close request is missing or duplicates privacy_operations wiring")
if app.count("privacy_operations: retry_close_services.privacy_operations()") != 1:
    raise SystemExit("retry close request is missing or duplicates privacy_operations wiring")
if app.count("executor: native_close_background.clone()") != 1:
    raise SystemExit("native close request is missing or duplicates executor-turn wiring")
if app.count("executor: retry_close_background.clone()") != 1:
    raise SystemExit("retry close request is missing or duplicates executor-turn wiring")

# 10. The explicit manual whole-library durability checkpoint is standardized on
# the same barrier, so there is no ungated UI durability path left behind.
manual_save = segment(
    app,
    'Key::Character(value) if value.eq_ignore_ascii_case("s") => {',
    'Key::Character(value)',
)
require(manual_save, [
    ".begin_operation(ApplicationOperation::DurabilityCheckpoint)",
    "synchronize_then_begin_whole_library_operation(",
    "&operation_turn",
    ".execute(move ||",
    "SynchronizationMode::Checkpoint",
    "durability_checkpoint(&checkpoint_lease)",
], "manual durability checkpoint barrier")

# 11. Restore keeps its stronger exclusive model: source editor sync; immediate
# library UI revocation; storage-owned library privacy transition; fresh
# post-restore aggregate publication. A read-side operation lease must never be
# held across Restore, or exclusive admission would deadlock.
restore = segment(settings, "let restore_source = pending.source.clone();", '"Restoring…"')
require(restore, [
    ".begin_operation(operation)",
    "synchronize_current_editor_scoped(",
    ".execute(move ||",
    "SynchronizationMode::Checkpoint",
    "begin_library_ui_revocation()",
    "task_selected_note.set(None)",
    "restore_complete_archive(&path)",
    "restore_backup_repository_snapshot(",
    "drop(privacy_revocation);",
    "let _post_restore_publication_lease = loop",
    "capture_multi_note_publication(",
    "token.begin_publication()",
], "Restore strong library boundary")
forbid(restore, ["synchronize_then_begin_whole_library_operation("], "Restore read-lease deadlock")
restore_turn_pos = restore.find(".begin_operation(operation)")
restore_sync_pos = restore.find("synchronize_current_editor_scoped(")
restore_revoke_pos = restore.find("begin_library_ui_revocation()")
restore_execute_pos = restore.find("operation_turn")
if min(restore_turn_pos, restore_sync_pos, restore_revoke_pos) < 0 or not (restore_turn_pos < restore_sync_pos < restore_revoke_pos):
    raise SystemExit("Restore does not acquire executor turn before source synchronization and library revocation")
# Successful and failed Restore both reconstruct the UI under the fresh lease.
if restore.count("onrestored.call(());") < 2:
    raise SystemExit("successful and failed Restore do not both reconstruct UI")

# 12. Storage remains the exclusive Restore authority and persistent snapshots
# retain their existing operation-lock + pending-Vault-operation fail-closed gate.
require(backups, [
    "pub fn export_complete_archive(&self, destination: &Path)",
    "let _operation_guard = self.lock_operations()?;",
    "self.ensure_no_pending_vault_operations_for_snapshot()?;",
    "begin_library_privacy_transition(",
    "LibraryPrivacyTransition::Restore",
], "archive/Restore storage boundary")
require(backup_repository, [
    "pub fn maybe_create_automatic_backup(&self)",
    "let _operation_guard = self.lock_operations()?;",
    "begin_library_privacy_transition(",
    "LibraryPrivacyTransition::Restore",
], "backup repository/Restore storage boundary")

# 13. Settings -> Library portability is a true whole-library export. It must
# acquire executor ownership first, synchronize the live source, then capture
# a fresh whole-library lease and carry that typed capability into the export.
library_portability = ui_portability[ui_portability.find("fn LibraryExportAction("):]
require(library_portability, [
    "background.begin_operation(operation)",
    "synchronize_then_begin_whole_library_operation(",
    "&operation_turn",
    "SynchronizationMode::Persist",
    ".export_library_scope(",
    "&library_lease",
], "library portability whole-library barrier")
forbid(library_portability, ["PortabilityScope::Library"], "library portability generic bypass")
turn_pos = library_portability.find("background.begin_operation(operation)")
sync_pos = library_portability.find("synchronize_then_begin_whole_library_operation(")
export_pos = library_portability.find(".export_library_scope(")
if min(turn_pos, sync_pos, export_pos) < 0 or not turn_pos < sync_pos < export_pos:
    raise SystemExit("library portability is not executor turn -> source sync -> fresh library authority -> export")
require(app_portability, [
    "pub(crate) async fn export_library_scope(",
    "operation_turn: ApplicationExecutionTurn",
    "_privacy_lease: &WholeLibraryOperationLease",
    "PortabilityScope::Library => Err(",
    "library portability export requires whole-library privacy authority",
], "application library portability type gate")
require(capabilities, ["pub(crate) fn export_portable_library_scope("], "storage library portability capability")

# 14. Notebook portability uses note-set scoped plaintext authority, not the
# whole-library lease. Candidate membership is captured before privacy permits,
# then revalidated under the storage lock before any plaintext is emitted.
notebook_export = segment(
    storage_portability,
    "pub(super) fn export_portable_notebook_scope_authorized(",
    "fn export_portable_scope_locked(",
)
require(notebook_export, [
    "let candidate_ids =",
    "acquire_multi_note_plaintext_operation(&candidate_ids)",
    "let _operation_guard = self.lock_operations()?;",
    "let stabilized_ids = portable_scope_note_ids",
    "if stabilized_ids != candidate_ids",
    "ensure_portability_scope_unprotected",
    "export_portable_scope_locked",
], "notebook portability stabilized multi-note lifetime")
permit_pos = notebook_export.find("acquire_multi_note_plaintext_operation(&candidate_ids)")
lock_pos = notebook_export.find("let _operation_guard = self.lock_operations()?;", permit_pos)
recheck_pos = notebook_export.find("let stabilized_ids = portable_scope_note_ids", lock_pos)
export_pos = notebook_export.find("export_portable_scope_locked", recheck_pos)
if min(permit_pos, lock_pos, recheck_pos, export_pos) < 0 or not permit_pos < lock_pos < recheck_pos < export_pos:
    raise SystemExit("notebook portability does not preserve privacy-permit -> storage-lock -> scope-recheck -> export ordering")
require(app_portability, [
    "PortabilityScope::Notebook(notebook_id)",
    "export_portable_notebook_scope(&notebook_id, format, &destination)",
], "application notebook portability routing")

# 15. The multi-note plaintext lifetime is a real coordinator/derivative
# capability and has an adversarial contract proving unrelated notes remain
# concurrent while included-note privacy transitions wait.
require(derivative, [
    "pub(crate) fn acquire_notes(",
    "state.transitioning.contains(note_id)",
], "multi-note derivative acquisition")
require(coordinator, [
    "pub(crate) struct MultiNotePlaintextOperationPermit",
    "pub(crate) fn acquire_multi_note_plaintext_operation(",
    "wait_for_notes_admission_clear(&ids)",
    "plaintext_derivatives.acquire_notes(&ids)",
    "fn multi_note_plaintext_operation_blocks_only_included_transitions()",
], "multi-note plaintext operation authority")
require(security_mod, ["MultiNotePlaintextOperationPermit"], "multi-note plaintext capability export")

# Adversarial coordinator contracts for lock/Restore ordering, including the
# exceptional protected-note Restore path.
require(coordinator, [
    "fn whole_library_operation_serializes_against_note_privacy_transition()",
    "fn whole_library_operation_serializes_against_protected_note_restore()",
    "fn whole_library_operation_serializes_against_library_restore()",
], "whole-library adversarial coordinator tests")
require(runner, ["library-wide-privacy-contract.test.mjs"], "browser behavior runner")

print("2b.42.6 library-wide operations validation passed (15 contracts).")
PY
