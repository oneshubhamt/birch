use std::{
    fmt::{self, Display, Formatter},
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;

use crate::application::editor::{
    EditorFlushTicket, EditorPersistenceMode, EditorService, ProjectedEditorDocument,
    apply_projected_document, project_editor_document,
};
use crate::application::errors::{DataAccessError, EditorError};
use crate::application::executor::{
    ApplicationExecutionError, ApplicationExecutionTurn, ApplicationExecutor, ExecutionFailure,
};
use crate::application::persistence::NoteChangeScope;
use crate::application::privacy::{
    PrivacyOperationService, PrivacyPublicationService, WholeLibraryOperationLease,
};
use crate::domain::{Note, NoteId};
use crate::editor_bridge::{
    EditorBridgeDecodeError, EditorDocumentUpdate, decode_editor_document_update,
};
use crate::editor_document::DocumentValidationError;

use super::eval_bridge;
use super::live_editor::live_editor_sync_script_for_generation;
use super::privacy::{PrivacyPublicationLease, PrivacyPublicationToken, UiPrivacyEpoch};

#[derive(Clone)]
pub(super) struct EditorSynchronizationContext {
    service: EditorService,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    privacy_epoch: UiPrivacyEpoch,
    publication_privacy: PrivacyPublicationService,
    list_generation: Signal<u64>,
    note_selection_generation: Signal<u64>,
}

impl EditorSynchronizationContext {
    pub(super) fn new(
        service: EditorService,
        selected_note: Signal<Option<Note>>,
        editor_session: Signal<Option<EditorSession>>,
        privacy_epoch: UiPrivacyEpoch,
        publication_privacy: &PrivacyPublicationService,
        list_generation: Signal<u64>,
        note_selection_generation: Signal<u64>,
    ) -> Self {
        Self {
            service,
            selected_note,
            editor_session,
            privacy_epoch,
            publication_privacy: publication_privacy.clone(),
            list_generation,
            note_selection_generation,
        }
    }

    fn capture_publication(&self) -> Option<PrivacyPublicationToken> {
        if let Some(note_id) = self
            .selected_note
            .peek()
            .as_ref()
            .map(|note| note.id.clone())
        {
            self.privacy_epoch.capture_note_publication(
                &self.publication_privacy,
                &note_id,
                self.note_selection_generation,
            )
        } else {
            self.privacy_epoch
                .capture_multi_note_publication_with_lifecycle(
                    &self.publication_privacy,
                    self.list_generation,
                    self.note_selection_generation,
                )
        }
    }
}

pub(super) async fn synchronize_current_editor_scoped(
    context: &EditorSynchronizationContext,
    mode: SynchronizationMode,
) -> SynchronizationResult<Option<PrivacyPublicationToken>> {
    loop {
        let Some(token) = context.capture_publication() else {
            tokio::time::sleep(Duration::from_millis(25)).await;
            continue;
        };
        let result = synchronize_current_editor(
            context.service.clone(),
            context.selected_note,
            context.editor_session,
            mode,
            token.clone(),
        )
        .await;
        if !token.is_publishable() {
            continue;
        }
        result?;
        return Ok(Some(token));
    }
}

/// Two-stage privacy boundary for operations that consume the whole durable library.
///
/// The caller must already own the serialized application-executor turn. That
/// scheduling boundary is intentionally acquired before privacy authority, so a
/// whole-library lease can never wait in the executor queue behind a Restore or
/// another serialized operation. Stage 1 synchronizes only the live source
/// editor under source-note authority. Stage 2 then acquires a fresh
/// coordinator-owned whole-library operation lease. If selection changes while
/// a privacy transition delays stage 2, the new source editor is synchronized
/// before another lease is attempted.
pub(super) async fn synchronize_then_begin_whole_library_operation(
    _operation_turn: &ApplicationExecutionTurn,
    context: &EditorSynchronizationContext,
    operation_privacy: &PrivacyOperationService,
    mode: SynchronizationMode,
) -> SynchronizationResult<WholeLibraryOperationLease> {
    loop {
        synchronize_current_editor_scoped(context, mode).await?;

        let note_selection_generation = context.note_selection_generation;
        let synchronized_selection_generation = *note_selection_generation.peek();
        if let Some(lease) = begin_whole_library_operation_for_synchronized_selection(
            operation_privacy,
            note_selection_generation,
            synchronized_selection_generation,
        )
        .await?
        {
            return Ok(lease);
        }
    }
}

/// Stage-2 authority capture shared by whole-library workflows that have
/// already synchronized their source state. Returning `None` means the selected
/// source changed and the caller must re-run its synchronization stage before
/// attempting library authority again.
pub(super) async fn begin_whole_library_operation_for_synchronized_selection(
    operation_privacy: &PrivacyOperationService,
    note_selection_generation: Signal<u64>,
    synchronized_selection_generation: u64,
) -> SynchronizationResult<Option<WholeLibraryOperationLease>> {
    let mut authority_was_delayed = false;
    loop {
        if *note_selection_generation.peek() != synchronized_selection_generation {
            return Ok(None);
        }
        match operation_privacy.begin_whole_library_operation()? {
            Some(lease) => {
                if *note_selection_generation.peek() != synchronized_selection_generation
                    || authority_was_delayed
                {
                    // A source selection change obviously invalidates stage 1.
                    // Less obviously, any admitted privacy transition that made
                    // stage 2 wait also creates an interval in which the user may
                    // have edited the same selected note. Drop the freshly won
                    // lease and make the caller re-synchronize once, rather than
                    // snapshotting an editor checkpoint that predates the wait.
                    drop(lease);
                    return Ok(None);
                }
                return Ok(Some(lease));
            }
            None => {
                authority_was_delayed = true;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorSession {
    pub(super) note_id: NoteId,
    pub(super) generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SynchronizationMode {
    Persist,
    Checkpoint,
    CheckpointSilent,
}

impl SynchronizationMode {
    fn persistence_mode(self) -> EditorPersistenceMode {
        match self {
            Self::Persist => EditorPersistenceMode::Persist,
            Self::Checkpoint => EditorPersistenceMode::Checkpoint,
            Self::CheckpointSilent => EditorPersistenceMode::CheckpointSilent,
        }
    }
}

#[derive(Debug)]
pub(super) enum SynchronizationError {
    Editor(EditorError),
    Projection(DocumentValidationError),
    Bridge(EditorBridgeDecodeError),
    Execution(ApplicationExecutionError),
    DataAccess(DataAccessError),
    LiveEditorUnavailable,
    LiveEditorNotReady,
    EditorChanged,
    SelectionChangedBeforeSynchronization,
    SelectionChangedDuringSynchronization,
    SessionChangedDuringSynchronization,
    PrivacyAuthorizationChanged,
}

impl Display for SynchronizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Editor(error) => Display::fmt(error, formatter),
            Self::Projection(error) => Display::fmt(error, formatter),
            Self::Bridge(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
            Self::DataAccess(error) => Display::fmt(error, formatter),
            Self::LiveEditorUnavailable => {
                formatter.write_str("could not synchronize the live editor")
            }
            Self::LiveEditorNotReady => formatter.write_str(
                "the live editor did not become ready while saving; reopen the note and retry",
            ),
            Self::EditorChanged => {
                formatter.write_str("the editor changed while synchronization was in progress")
            }
            Self::SelectionChangedBeforeSynchronization => {
                formatter.write_str("the selected note changed before synchronization began")
            }
            Self::SelectionChangedDuringSynchronization => formatter
                .write_str("the selected note changed while synchronization was in progress"),
            Self::SessionChangedDuringSynchronization => formatter
                .write_str("the editor session changed while synchronization was in progress"),
            Self::PrivacyAuthorizationChanged => formatter
                .write_str("privacy authorization changed while synchronization was in progress"),
        }
    }
}

impl std::error::Error for SynchronizationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Editor(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::Bridge(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::DataAccess(error) => Some(error),
            Self::LiveEditorUnavailable
            | Self::LiveEditorNotReady
            | Self::EditorChanged
            | Self::SelectionChangedBeforeSynchronization
            | Self::SelectionChangedDuringSynchronization
            | Self::SessionChangedDuringSynchronization
            | Self::PrivacyAuthorizationChanged => None,
        }
    }
}

impl From<EditorError> for SynchronizationError {
    fn from(error: EditorError) -> Self {
        Self::Editor(error)
    }
}

impl From<DataAccessError> for SynchronizationError {
    fn from(error: DataAccessError) -> Self {
        Self::DataAccess(error)
    }
}

impl From<DocumentValidationError> for SynchronizationError {
    fn from(error: DocumentValidationError) -> Self {
        Self::Projection(error)
    }
}

impl From<EditorBridgeDecodeError> for SynchronizationError {
    fn from(error: EditorBridgeDecodeError) -> Self {
        Self::Bridge(error)
    }
}

impl ExecutionFailure for SynchronizationError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

type SynchronizationResult<T> = Result<T, SynchronizationError>;

#[derive(Debug)]
pub(super) struct DraftFlushResult {
    pub(super) pending_warning: Option<String>,
}

impl DraftFlushResult {
    pub(super) fn completion_message(&self, success: &str) -> String {
        match self.pending_warning.as_deref() {
            Some(warning) => format!("{success}, but {warning}"),
            None => success.to_string(),
        }
    }
}

pub(super) type DraftFlushTicket = EditorFlushTicket;

struct CapturedEditorDocument {
    selected: Option<Note>,
    session: Option<EditorSession>,
    editor_state: Option<String>,
}

/// Submit the autosave command on the UI thread without waiting for storage.
/// The returned ticket is completed by `complete_draft_flush` off-thread.
pub(super) fn begin_draft_flush_scoped(
    service: &EditorService,
    note: Note,
    mode: SynchronizationMode,
    scope: NoteChangeScope,
) -> SynchronizationResult<DraftFlushTicket> {
    service
        .begin_note_flush(note, mode.persistence_mode(), scope)
        .map_err(SynchronizationError::from)
}

pub(super) async fn complete_draft_flush(
    ticket: SynchronizationResult<DraftFlushTicket>,
) -> SynchronizationResult<DraftFlushResult> {
    let ticket = ticket?;
    let result = ApplicationExecutor::read("note save", move || ticket.wait())
        .await
        .map_err(SynchronizationError::from)?;
    Ok(DraftFlushResult {
        pending_warning: result.pending_warning,
    })
}

pub(super) type ProjectedEditorDocumentUpdate = ProjectedEditorDocument;

pub(super) fn project_editor_update(
    change: EditorDocumentUpdate,
) -> SynchronizationResult<ProjectedEditorDocumentUpdate> {
    project_editor_document(change.editor_state).map_err(SynchronizationError::from)
}

pub(super) async fn project_editor_update_async(
    change: EditorDocumentUpdate,
) -> SynchronizationResult<ProjectedEditorDocumentUpdate> {
    ApplicationExecutor::read("editor projection", move || project_editor_update(change)).await
}

pub(super) fn apply_editor_update(note: &mut Note, change: ProjectedEditorDocumentUpdate) -> bool {
    apply_projected_document(note, change)
}

/// Explicit durability boundary for every action that can leave, export, or
/// replace the current note. It reads ProseMirror directly, verifies the live
/// editor generation, merges that document into the latest persisted Rust
/// draft, then flushes/checkpoints through the autosave worker.
pub(super) async fn synchronize_current_editor(
    service: EditorService,
    mut selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    mode: SynchronizationMode,
    publication_token: PrivacyPublicationToken,
) -> SynchronizationResult<Option<Arc<Note>>> {
    ensure_publication_current(&publication_token)?;
    let captured =
        capture_live_editor_document(selected_note, editor_session, &publication_token).await?;
    let Some(selected) = captured.selected else {
        let flushed = flush_without_selected_note(service, mode).await?;
        ensure_publication_current(&publication_token)?;
        return Ok(flushed);
    };
    let note_id = selected.id.clone();
    let expected_session = captured.session;
    let persistence_mode = mode.persistence_mode();
    let editor_state = captured.editor_state;
    let saved = ApplicationExecutor::read("editor synchronization", move || {
        service.synchronize_document(selected, editor_state, persistence_mode)
    })
    .await
    .map_err(SynchronizationError::from)?;
    ensure_publication_current(&publication_token)?;

    verify_active_note(
        &note_id,
        expected_session.as_ref(),
        selected_note,
        editor_session,
    )?;
    let _publication_lease = acquire_publication_lease(&publication_token)?;
    selected_note.set(Some(saved.as_ref().clone()));
    Ok(Some(saved))
}

// The browser bridge itself allows up to eight seconds for runtime readiness.
// Match that installation contract here so a privacy-driven editor remount is
// treated as a transient mount gap rather than a false save failure.
const EDITOR_SESSION_STABILIZATION_ATTEMPTS: usize = 320;
const EDITOR_SESSION_STABILIZATION_DELAY: Duration = Duration::from_millis(25);
const EDITOR_CAPTURE_RETRIES: usize = 3;

async fn capture_live_editor_document(
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    publication_token: &PrivacyPublicationToken,
) -> SynchronizationResult<CapturedEditorDocument> {
    let mut last_transient = None;
    for attempt in 0..EDITOR_CAPTURE_RETRIES {
        match capture_live_editor_document_once(selected_note, editor_session, publication_token)
            .await
        {
            Ok(captured) => return Ok(captured),
            Err(error @ SynchronizationError::SessionChangedDuringSynchronization)
            | Err(error @ SynchronizationError::SelectionChangedBeforeSynchronization)
            | Err(error @ SynchronizationError::EditorChanged) => {
                last_transient = Some(error);
                if attempt + 1 < EDITOR_CAPTURE_RETRIES {
                    tokio::time::sleep(EDITOR_SESSION_STABILIZATION_DELAY).await;
                    continue;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_transient.unwrap_or(SynchronizationError::SessionChangedDuringSynchronization))
}

async fn capture_live_editor_document_once(
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    publication_token: &PrivacyPublicationToken,
) -> SynchronizationResult<CapturedEditorDocument> {
    ensure_publication_current(publication_token)?;
    let selected = selected_note.read().clone();
    let Some(selected_note_value) = selected.as_ref() else {
        return Ok(CapturedEditorDocument {
            selected,
            session: None,
            editor_state: None,
        });
    };

    let captured_session = stable_editor_session_for_selected_note(
        selected_note_value,
        selected_note,
        editor_session,
        publication_token,
    )
    .await?;
    let update = match captured_session.as_ref() {
        Some(session) => {
            let sync_script = live_editor_sync_script_for_generation(session.generation);
            let serialized = eval_bridge::join_idempotent::<String>(&sync_script)
                .await
                .map_err(|_| SynchronizationError::LiveEditorUnavailable)?;
            ensure_publication_current(publication_token)?;
            if editor_session.read().as_ref() != captured_session.as_ref() {
                return Err(SynchronizationError::SessionChangedDuringSynchronization);
            }
            if serialized.trim().is_empty() {
                return Err(SynchronizationError::LiveEditorNotReady);
            }
            let update = decode_editor_document_update(&serialized)?;
            if update.generation != session.generation {
                return Err(SynchronizationError::EditorChanged);
            }
            Some(update)
        }
        None => None,
    };

    verify_active_note(
        &selected_note_value.id,
        captured_session.as_ref(),
        selected_note,
        editor_session,
    )?;
    ensure_publication_current(publication_token)?;

    Ok(CapturedEditorDocument {
        selected,
        session: captured_session,
        editor_state: update.map(|change| change.editor_state),
    })
}

async fn stable_editor_session_for_selected_note(
    selected: &Note,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    publication_token: &PrivacyPublicationToken,
) -> SynchronizationResult<Option<EditorSession>> {
    if selected.protection.is_locked() {
        return Ok(None);
    }

    for attempt in 0..EDITOR_SESSION_STABILIZATION_ATTEMPTS {
        ensure_publication_current(publication_token)?;
        if selected_note
            .read()
            .as_ref()
            .is_none_or(|current| current.id != selected.id)
        {
            return Err(SynchronizationError::SelectionChangedBeforeSynchronization);
        }
        if let Some(session) = editor_session.read().clone()
            && session.note_id == selected.id
        {
            return Ok(Some(session));
        }
        if attempt + 1 < EDITOR_SESSION_STABILIZATION_ATTEMPTS {
            tokio::time::sleep(EDITOR_SESSION_STABILIZATION_DELAY).await;
        }
    }

    Err(SynchronizationError::LiveEditorNotReady)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NoteMutationSynchronization {
    pub(super) selection_generation: u64,
    pub(super) target_selected: bool,
}

/// Durability barrier for a UI action that will immediately perform an
/// authoritative mutation outside the autosave worker. The returned selection
/// snapshot is captured at the same no-await boundary as the completed
/// checkpoint, so the caller can revoke the target UI without reopening an
/// unselected -> live-selected durability gap.
pub(super) async fn synchronize_for_note_mutation(
    service: EditorService,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    note_selection_generation: Signal<u64>,
    target_note_id: NoteId,
    publication_token: PrivacyPublicationToken,
) -> SynchronizationResult<NoteMutationSynchronization> {
    loop {
        // A target-note mutation must never drag an unrelated selected editor
        // into the target's privacy scope. If B becomes selected while its
        // silent storage checkpoint is running, loop and checkpoint the newly
        // live B editor before returning the revocation-boundary snapshot.
        let target_is_selected = selected_note
            .peek()
            .as_ref()
            .is_some_and(|note| note.id == target_note_id);
        if target_is_selected {
            let synchronized = synchronize_current_editor(
                service.clone(),
                selected_note,
                editor_session,
                SynchronizationMode::CheckpointSilent,
                publication_token.clone(),
            )
            .await?;
            if !synchronized
                .as_ref()
                .is_some_and(|note| note.id == target_note_id)
            {
                return Err(SynchronizationError::SelectionChangedDuringSynchronization);
            }
            ensure_publication_current(&publication_token)?;

            let selection_generation = *note_selection_generation.peek();
            let target_selected = selected_note
                .peek()
                .as_ref()
                .is_some_and(|note| note.id == target_note_id);
            if !target_selected {
                // A navigation completed after the live checkpoint. Re-evaluate
                // the target rather than returning a stale selected-source
                // assertion to the Vault controller.
                continue;
            }
            return Ok(NoteMutationSynchronization {
                selection_generation,
                target_selected,
            });
        }

        let mutation_service = service.clone();
        let checkpoint_note_id = target_note_id.clone();
        ApplicationExecutor::read("target-note checkpoint", move || {
            mutation_service.checkpoint_note_silent(&checkpoint_note_id)
        })
        .await
        .map_err(SynchronizationError::from)?;
        ensure_publication_current(&publication_token)?;

        let selection_generation = *note_selection_generation.peek();
        let target_selected = selected_note
            .peek()
            .as_ref()
            .is_some_and(|note| note.id == target_note_id);
        if target_selected {
            // B acquired a live editor while the unselected checkpoint was in
            // flight. Its canonical storage checkpoint is no longer sufficient;
            // loop through the live-editor path before any privacy revocation.
            continue;
        }
        return Ok(NoteMutationSynchronization {
            selection_generation,
            target_selected,
        });
    }
}

fn acquire_publication_lease(
    publication_token: &PrivacyPublicationToken,
) -> SynchronizationResult<PrivacyPublicationLease> {
    publication_token
        .begin_publication()
        .ok_or(SynchronizationError::PrivacyAuthorizationChanged)
}

fn ensure_publication_current(
    publication_token: &PrivacyPublicationToken,
) -> SynchronizationResult<()> {
    if publication_token.is_publishable() {
        Ok(())
    } else {
        Err(SynchronizationError::PrivacyAuthorizationChanged)
    }
}

fn verify_active_note(
    note_id: &NoteId,
    expected_session: Option<&EditorSession>,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
) -> SynchronizationResult<()> {
    if selected_note
        .read()
        .as_ref()
        .is_none_or(|note| &note.id != note_id)
    {
        return Err(SynchronizationError::SelectionChangedDuringSynchronization);
    }
    if editor_session.read().as_ref() != expected_session {
        return Err(SynchronizationError::SessionChangedDuringSynchronization);
    }
    Ok(())
}

async fn flush_without_selected_note(
    service: EditorService,
    mode: SynchronizationMode,
) -> SynchronizationResult<Option<Arc<Note>>> {
    let persistence_mode = mode.persistence_mode();
    ApplicationExecutor::read("editor synchronization", move || {
        service.flush_pending(persistence_mode)
    })
    .await
    .map_err(SynchronizationError::from)
}
