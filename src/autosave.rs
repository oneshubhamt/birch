use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    domain::{Note, NoteId},
    storage::{NoteSaveScope, Storage, StorageError},
};

mod emergency_journal;

use emergency_journal::EmergencyJournal;

const AUTOSAVE_DELAY: Duration = Duration::from_millis(450);

#[derive(Debug)]
pub(crate) enum AutosaveError {
    Storage(StorageError),
    Journal(emergency_journal::EmergencyJournalError),
    WorkerUnavailable,
    WorkerStopped(&'static str),
    StateUnavailable(&'static str),
    ListenerAlreadyActive,
    Operation(String),
    Context {
        context: &'static str,
        source: Box<AutosaveError>,
    },
    Combined {
        primary: Box<AutosaveError>,
        additional: Box<AutosaveError>,
    },
}

impl AutosaveError {
    pub(crate) fn context(self, context: &'static str) -> Self {
        Self::Context {
            context,
            source: Box::new(self),
        }
    }
}

impl std::fmt::Display for AutosaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => std::fmt::Display::fmt(error, formatter),
            Self::Journal(error) => std::fmt::Display::fmt(error, formatter),
            Self::Operation(message) => formatter.write_str(message),
            Self::WorkerUnavailable => formatter.write_str("autosave worker is unavailable"),
            Self::WorkerStopped(operation) => {
                write!(formatter, "autosave worker stopped before {operation}")
            }
            Self::StateUnavailable(state) => write!(formatter, "{state} is unavailable"),
            Self::ListenerAlreadyActive => {
                formatter.write_str("autosave event listener is already active")
            }
            Self::Context { context, source } => write!(formatter, "{context}: {source}"),
            Self::Combined { primary, additional } => {
                write!(formatter, "{primary}; {additional}")
            }
        }
    }
}

impl std::error::Error for AutosaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::Combined { primary, .. } => Some(primary.as_ref()),
            Self::WorkerUnavailable
            | Self::WorkerStopped(_)
            | Self::StateUnavailable(_)
            | Self::ListenerAlreadyActive
            | Self::Operation(_) => None,
        }
    }
}

impl From<StorageError> for AutosaveError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<String> for AutosaveError {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

impl From<emergency_journal::EmergencyJournalError> for AutosaveError {
    fn from(error: emergency_journal::EmergencyJournalError) -> Self {
        Self::Journal(error)
    }
}

pub(crate) type AutosaveResult<T> = Result<T, AutosaveError>;

fn autosave_error<T>(message: impl Into<String>) -> AutosaveResult<T> {
    Err(AutosaveError::Operation(message.into()))
}

#[derive(Clone, Debug)]
pub(crate) enum AutosaveEvent {
    Saved(Arc<Note>, u64),
    Recovered(usize),
    Failed(String),
}

pub(super) type AutosaveEventSender = UnboundedSender<AutosaveEvent>;

enum AutosaveCommand {
    Schedule(Note, u64, NoteSaveScope),
    Flush(Note, u64, NoteSaveScope, bool, bool, Sender<ExplicitFlushOutcome>),
    FlushPending(bool, bool, Sender<AutosaveResult<Option<Arc<Note>>>>),
    CheckpointNote(
        NoteId,
        u64,
        bool,
        Sender<AutosaveResult<Option<Arc<Note>>>>,
    ),
    CheckpointAll(Sender<AutosaveResult<()>>),
    DiscardNote(NoteId, u64, Sender<AutosaveResult<()>>),
    BlockVaultNote(NoteId, u64, Sender<AutosaveResult<()>>),
    UnblockVaultNote(NoteId, Sender<AutosaveResult<()>>),
    ResetLibrary(Sender<AutosaveResult<()>>),
    Shutdown,
}

#[derive(Clone)]
pub(crate) struct AutosaveController {
    inner: Arc<AutosaveInner>,
}

struct AutosaveInner {
    commands: Sender<AutosaveCommand>,
    events: Mutex<Option<UnboundedReceiver<AutosaveEvent>>>,
    note_versions: Mutex<HashMap<NoteId, u64>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    journal: EmergencyJournal,
    journal_worker: Mutex<Option<JoinHandle<()>>>,
}

struct PendingDraft {
    note: Note,
    version: u64,
    scope: NoteSaveScope,
}

#[derive(Clone, Copy)]
struct SaveNoteOptions {
    checkpoint: bool,
    emit_saved_event: bool,
    version: u64,
    scope: NoteSaveScope,
}

struct AutosaveSaveFailure {
    note: Box<Note>,
    error: AutosaveError,
}

impl AutosaveSaveFailure {
    fn new(note: Note, error: AutosaveError) -> Self {
        Self {
            note: Box::new(note),
            error,
        }
    }

    fn into_parts(self) -> (Note, AutosaveError) {
        (*self.note, self.error)
    }
}

#[derive(Debug)]
enum ExplicitFlushOutcome {
    Complete(Arc<Note>),
    PendingFailed {
        saved: Arc<Note>,
        pending_error: AutosaveError,
    },
    TargetFailed {
        target_error: AutosaveError,
    },
    PendingAndTargetFailed {
        pending_error: AutosaveError,
        target_error: AutosaveError,
    },
}

#[derive(Debug)]
pub(crate) struct TargetFlushResult {
    saved: Arc<Note>,
    pending_warning: Option<String>,
}

#[derive(Debug)]
pub(crate) struct TargetFlushTicket {
    receiver: Receiver<ExplicitFlushOutcome>,
}

impl TargetFlushTicket {
    pub(crate) fn wait(self) -> AutosaveResult<TargetFlushResult> {
        self.receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("saving"))?
            .into_target_result()
    }
}

impl TargetFlushResult {
    pub(crate) fn into_parts(self) -> (Arc<Note>, Option<String>) {
        (self.saved, self.pending_warning)
    }
}

impl ExplicitFlushOutcome {
    fn from_results(
        pending_result: AutosaveResult<()>,
        target_result: AutosaveResult<Arc<Note>>,
    ) -> Self {
        match (pending_result, target_result) {
            (Ok(()), Ok(saved)) => Self::Complete(saved),
            (Err(pending_error), Ok(saved)) => Self::PendingFailed {
                saved,
                pending_error,
            },
            (Ok(()), Err(target_error)) => Self::TargetFailed { target_error },
            (Err(pending_error), Err(target_error)) => Self::PendingAndTargetFailed {
                pending_error,
                target_error,
            },
        }
    }

    fn into_result(self) -> AutosaveResult<Arc<Note>> {
        match self {
            Self::Complete(saved) => Ok(saved),
            Self::PendingFailed { pending_error, .. } => {
                Err(AutosaveError::Operation(format!("Earlier pending save failed: {pending_error}")))
            }
            Self::TargetFailed { target_error } => Err(target_error),
            Self::PendingAndTargetFailed {
                pending_error,
                target_error,
            } => Err(combine_flush_errors(pending_error, target_error)),
        }
    }

    fn into_target_result(self) -> AutosaveResult<TargetFlushResult> {
        match self {
            Self::Complete(saved) => Ok(TargetFlushResult {
                saved,
                pending_warning: None,
            }),
            Self::PendingFailed {
                saved,
                pending_error,
            } => Ok(TargetFlushResult {
                saved,
                pending_warning: Some(format!(
                    "an earlier pending note could not be saved: {pending_error}"
                )),
            }),
            Self::TargetFailed { target_error } => Err(target_error),
            Self::PendingAndTargetFailed {
                pending_error,
                target_error,
            } => Err(combine_flush_errors(pending_error, target_error)),
        }
    }
}

fn combine_flush_errors(pending_error: AutosaveError, target_error: AutosaveError) -> AutosaveError {
    AutosaveError::Combined {
        primary: Box::new(AutosaveError::Operation(format!(
            "Earlier pending save failed: {pending_error}"
        ))),
        additional: Box::new(AutosaveError::Operation(format!(
            "target note save also failed: {target_error}"
        ))),
    }
}

impl std::fmt::Debug for AutosaveController {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AutosaveController")
            .finish_non_exhaustive()
    }
}

impl PartialEq for AutosaveController {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for AutosaveController {}

impl AutosaveController {
    pub(crate) fn try_new(storage: Storage) -> AutosaveResult<Self> {
        let recovery = emergency_journal::recover(&storage)?;
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
        if recovery.recovered > 0 {
            let _ = event_sender.send(AutosaveEvent::Recovered(recovery.recovered));
        }
        for warning in recovery.warnings {
            let _ = event_sender.send(AutosaveEvent::Failed(warning));
        }

        let started_journal = EmergencyJournal::start(
            storage.data_directory().to_path_buf(),
            event_sender.clone(),
        )?;
        let journal = started_journal.client.clone();
        let journal_for_worker = journal.clone();
        let worker = match thread::Builder::new()
            .name("rustnote-autosave".to_string())
            .spawn(move || {
                autosave_worker(
                    storage,
                    command_receiver,
                    event_sender,
                    journal_for_worker,
                )
            }) {
            Ok(worker) => worker,
            Err(error) => {
                journal.shutdown();
                let _ = started_journal.worker.join();
                return autosave_error(format!("could not start the autosave worker: {error}"));
            }
        };

        Ok(Self {
            inner: Arc::new(AutosaveInner {
                commands: command_sender,
                events: Mutex::new(Some(event_receiver)),
                note_versions: Mutex::new(HashMap::new()),
                worker: Mutex::new(Some(worker)),
                journal,
                journal_worker: Mutex::new(Some(started_journal.worker)),
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn schedule(&self, note: Note) -> AutosaveResult<()> {
        self.schedule_scoped(note, NoteSaveScope::ALL)
    }

    pub(crate) fn schedule_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
    ) -> AutosaveResult<()> {
        let version = self.advance_note_version(&note.id)?;
        self.inner
            .commands
            .send(AutosaveCommand::Schedule(note, version, scope))
            .map_err(|_| AutosaveError::WorkerUnavailable)
    }

    pub(crate) fn flush_note_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
    ) -> AutosaveResult<Arc<Note>> {
        self.flush_note_outcome_scoped(note, scope, true, true)?.into_result()
    }


    pub(crate) fn flush_live_note_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
    ) -> AutosaveResult<Arc<Note>> {
        self.flush_note_outcome_scoped(note, scope, false, true)?.into_result()
    }


    pub(crate) fn flush_note_silent_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
    ) -> AutosaveResult<Arc<Note>> {
        self.flush_note_outcome_scoped(note, scope, true, false)?.into_result()
    }

    /// Queue an explicit note flush immediately and return a ticket whose
    /// response may be awaited elsewhere. Submitting before offloading the
    /// wait preserves command order across rapid UI actions.
    pub(crate) fn begin_note_target_flush_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
        checkpoint: bool,
        emit_saved_event: bool,
    ) -> AutosaveResult<TargetFlushTicket> {
        let receiver = self.begin_note_flush(note, scope, checkpoint, emit_saved_event)?;
        Ok(TargetFlushTicket { receiver })
    }

    fn flush_note_outcome_scoped(
        &self,
        note: Note,
        scope: NoteSaveScope,
        checkpoint: bool,
        emit_saved_event: bool,
    ) -> AutosaveResult<ExplicitFlushOutcome> {
        self.begin_note_flush(note, scope, checkpoint, emit_saved_event)?
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("saving"))
    }

    fn begin_note_flush(
        &self,
        note: Note,
        scope: NoteSaveScope,
        checkpoint: bool,
        emit_saved_event: bool,
    ) -> AutosaveResult<Receiver<ExplicitFlushOutcome>> {
        let version = self.advance_note_version(&note.id)?;
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::Flush(
                note,
                version,
                scope,
                checkpoint,
                emit_saved_event,
                sender,
            ))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        Ok(receiver)
    }

    pub(crate) fn flush_pending(&self) -> AutosaveResult<Option<Arc<Note>>> {
        self.flush_pending_with_checkpoint(true, true)
    }

    pub(crate) fn flush_pending_live(&self) -> AutosaveResult<Option<Arc<Note>>> {
        self.flush_pending_with_checkpoint(false, true)
    }

    pub(crate) fn flush_pending_silent(&self) -> AutosaveResult<Option<Arc<Note>>> {
        self.flush_pending_with_checkpoint(true, false)
    }

    fn flush_pending_with_checkpoint(
        &self,
        checkpoint: bool,
        emit_saved_event: bool,
    ) -> AutosaveResult<Option<Arc<Note>>> {
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::FlushPending(
                checkpoint,
                emit_saved_event,
                sender,
            ))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("flushing"))?
    }


    pub(crate) fn checkpoint_note_silent(&self, note_id: &NoteId) -> AutosaveResult<Option<Arc<Note>>> {
        self.checkpoint_note_with_event(note_id, false)
    }

    fn checkpoint_note_with_event(
        &self,
        note_id: &NoteId,
        emit_saved_event: bool,
    ) -> AutosaveResult<Option<Arc<Note>>> {
        let version = self.advance_note_version(note_id)?;
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::CheckpointNote(
                note_id.clone(),
                version,
                emit_saved_event,
                sender,
            ))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("checkpointing the note"))?
    }

    /// Flush every pending draft and checkpoint every note changed since its
    /// latest checkpoint. This is the explicit durability barrier used before
    /// the native desktop window is allowed to close.
    pub(crate) fn checkpoint_all(&self) -> AutosaveResult<()> {
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::CheckpointAll(sender))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("completing the close checkpoint"))?
    }

    pub(crate) fn discard_note(&self, note_id: &NoteId) -> AutosaveResult<()> {
        let version = self.advance_note_version(note_id)?;
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::DiscardNote(
                note_id.clone(),
                version,
                sender,
            ))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("discarding a pending draft"))?
    }

    /// Establish an authoritative privacy barrier before a vault transition.
    ///
    /// Command ordering guarantees that earlier autosave work is drained or
    /// discarded before this returns. Later stale UI drafts for this note are
    /// rejected by the worker until the barrier is explicitly removed.
    pub(crate) fn block_note_for_vault(&self, note_id: &NoteId) -> AutosaveResult<()> {
        let version = self.advance_note_version(note_id)?;
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::BlockVaultNote(
                note_id.clone(),
                version,
                sender,
            ))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("establishing the vault autosave barrier"))?
    }

    pub(crate) fn unblock_note_after_vault(&self, note_id: &NoteId) -> AutosaveResult<()> {
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::UnblockVaultNote(note_id.clone(), sender))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("releasing the vault autosave barrier"))?
    }

    pub(crate) fn reset_library(&self) -> AutosaveResult<()> {
        {
            let mut versions = self
                .inner
                .note_versions
                .lock()
                .map_err(|_| AutosaveError::StateUnavailable("autosave version tracker"))?;
            for version in versions.values_mut() {
                *version = version.saturating_add(1);
            }
        }
        let (sender, receiver) = mpsc::channel();
        self.inner
            .commands
            .send(AutosaveCommand::ResetLibrary(sender))
            .map_err(|_| AutosaveError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| AutosaveError::WorkerStopped("resetting the library"))?
    }

    pub(crate) fn is_current_note_version(&self, note_id: &NoteId, version: u64) -> bool {
        self.inner
            .note_versions
            .lock()
            .ok()
            .and_then(|versions| versions.get(note_id).copied())
            == Some(version)
    }

    fn advance_note_version(&self, note_id: &NoteId) -> AutosaveResult<u64> {
        let mut versions = self
            .inner
            .note_versions
            .lock()
            .map_err(|_| AutosaveError::StateUnavailable("autosave version tracker"))?;
        let next = versions
            .get(note_id)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        versions.insert(note_id.clone(), next);
        Ok(next)
    }

    pub(crate) fn take_event_receiver(&self) -> AutosaveResult<UnboundedReceiver<AutosaveEvent>> {
        self.inner
            .events
            .lock()
            .map_err(|_| AutosaveError::StateUnavailable("autosave event queue"))?
            .take()
            .ok_or(AutosaveError::ListenerAlreadyActive)
    }
}

impl Drop for AutosaveInner {
    fn drop(&mut self) {
        let _ = self.commands.send(AutosaveCommand::Shutdown);
        if let Ok(mut worker) = self.worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
        self.journal.shutdown();
        if let Ok(mut worker) = self.journal_worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
    }
}

fn autosave_worker(
    storage: Storage,
    commands: Receiver<AutosaveCommand>,
    events: AutosaveEventSender,
    journal: EmergencyJournal,
) {
    let mut pending = VecDeque::<PendingDraft>::new();
    let mut dirty_notes = HashSet::<NoteId>::new();
    let mut vault_blocked_notes = HashSet::<NoteId>::new();
    let mut debounce_armed = false;

    loop {
        let command = if !pending.is_empty() && debounce_armed {
            match commands.recv_timeout(AUTOSAVE_DELAY) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => {
                    let _ = flush_pending_queue(
                        &storage,
                        &mut pending,
                        &mut dirty_notes,
                        &events,
                        &journal,
                        false,
                        true,
                    );
                    // Keep failed work in memory and in its per-note emergency
                    // journal, but wait for a new edit or explicit flush before retrying.
                    debounce_armed = false;
                    None
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = flush_pending_queue(
                        &storage,
                        &mut pending,
                        &mut dirty_notes,
                        &events,
                        &journal,
                        true,
                        true,
                    );
                    break;
                }
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => {
                    let _ = flush_pending_queue(
                        &storage,
                        &mut pending,
                        &mut dirty_notes,
                        &events,
                        &journal,
                        true,
                        true,
                    );
                    break;
                }
            }
        };

        let Some(command) = command else {
            continue;
        };

        match command {
            AutosaveCommand::Schedule(note, version, scope) => {
                if vault_autosave_is_blocked(&storage, &vault_blocked_notes, &note.id) {
                    remove_note(&mut pending, &note.id);
                    dirty_notes.remove(&note.id);
                    let _ = journal.clear(&note.id, version);
                    debounce_armed = !pending.is_empty();
                    continue;
                }
                schedule_journal_or_report(&storage, &journal, &note, version, &events);
                let switching_notes = pending
                    .back()
                    .is_some_and(|current| current.note.id != note.id);
                if switching_notes {
                    let _ = flush_pending_queue(
                        &storage,
                        &mut pending,
                        &mut dirty_notes,
                        &events,
                        &journal,
                        true,
                        true,
                    );
                }
                dirty_notes.insert(note.id.clone());
                queue_latest(&mut pending, note, version, scope);
                debounce_armed = true;
            }
            AutosaveCommand::Flush(note, version, scope, checkpoint, emit_saved_event, reply) => {
                if vault_autosave_is_blocked(&storage, &vault_blocked_notes, &note.id) {
                    remove_note(&mut pending, &note.id);
                    dirty_notes.remove(&note.id);
                    let _ = journal.clear(&note.id, version);
                    let _ = reply.send(ExplicitFlushOutcome::TargetFailed {
                        target_error: AutosaveError::Operation(
                            "note autosave is blocked by encryption state".to_string(),
                        ),
                    });
                    debounce_armed = !pending.is_empty();
                    continue;
                }
                schedule_journal_or_report(&storage, &journal, &note, version, &events);
                let target_scope = take_note(&mut pending, &note.id)
                    .map(|pending| pending.scope.union(scope))
                    .unwrap_or(scope);
                let pending_result = flush_pending_queue(
                    &storage,
                    &mut pending,
                    &mut dirty_notes,
                    &events,
                    &journal,
                    true,
                    emit_saved_event,
                )
                .map(|_| ());

                let target_result = match save_note(
                    &storage,
                    note,
                    &mut dirty_notes,
                    &events,
                    &journal,
                    SaveNoteOptions {
                        checkpoint,
                        emit_saved_event,
                        version,
                        scope: target_scope,
                    },
                ) {
                    Ok(saved) => Ok(saved),
                    Err(failure) => {
                        let (note, message) = failure.into_parts();
                        queue_latest(&mut pending, note, version, target_scope);
                        Err(message)
                    }
                };
                let outcome = ExplicitFlushOutcome::from_results(pending_result, target_result);
                debounce_armed = !pending.is_empty();
                let _ = reply.send(outcome);
            }
            AutosaveCommand::FlushPending(checkpoint, emit_saved_event, reply) => {
                let result = flush_pending_queue(
                    &storage,
                    &mut pending,
                    &mut dirty_notes,
                    &events,
                    &journal,
                    checkpoint,
                    emit_saved_event,
                );
                debounce_armed = !pending.is_empty();
                let _ = reply.send(result);
            }
            AutosaveCommand::CheckpointNote(note_id, version, emit_saved_event, reply) => {
                if vault_autosave_is_blocked(&storage, &vault_blocked_notes, &note_id) {
                    remove_note(&mut pending, &note_id);
                    dirty_notes.remove(&note_id);
                    let _ = journal.clear(&note_id, version);
                    let result = storage
                        .get_note(&note_id)
                        .map(|note| note.map(Arc::new))
                        .map_err(AutosaveError::Storage);
                    let _ = reply.send(result);
                    debounce_armed = !pending.is_empty();
                    continue;
                }
                let pending_draft = take_note(&mut pending, &note_id);
                let (note_result, checkpoint_scope) = match pending_draft {
                    Some(draft) => (Ok(Some(draft.note)), draft.scope),
                    None => (
                        storage
                            .get_note(&note_id)
                            .map_err(AutosaveError::Storage),
                        NoteSaveScope::NONE,
                    ),
                };
                let result = match note_result {
                    Ok(Some(note)) => {
                        schedule_journal_or_report(&storage, &journal, &note, version, &events);
                        match save_note(
                            &storage,
                            note,
                            &mut dirty_notes,
                            &events,
                            &journal,
                            SaveNoteOptions {
                                checkpoint: true,
                                emit_saved_event,
                                version,
                                scope: checkpoint_scope,
                            },
                        ) {
                            Ok(saved) => Ok(Some(saved)),
                            Err(failure) => {
                                let (note, message) = failure.into_parts();
                                queue_latest(
                                    &mut pending,
                                    note,
                                    version,
                                    checkpoint_scope,
                                );
                                Err(message)
                            }
                        }
                    }
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                };
                let result = match result {
                    Ok(saved) => flush_pending_queue(
                        &storage,
                        &mut pending,
                        &mut dirty_notes,
                        &events,
                        &journal,
                        true,
                        emit_saved_event,
                    )
                    .map(|_| saved),
                    Err(error) => Err(error),
                };
                debounce_armed = !pending.is_empty();
                let _ = reply.send(result);
            }
            AutosaveCommand::CheckpointAll(reply) => {
                let result = checkpoint_all_work(
                    &storage,
                    &mut pending,
                    &mut dirty_notes,
                    &events,
                    &journal,
                );
                debounce_armed = !pending.is_empty();
                let _ = reply.send(result);
            }
            AutosaveCommand::DiscardNote(note_id, version, reply) => {
                remove_note(&mut pending, &note_id);
                dirty_notes.remove(&note_id);
                let result = journal.clear(&note_id, version).map_err(|error| {
                    AutosaveError::Journal(error).context("could not clear the note's emergency journal")
                });
                debounce_armed = !pending.is_empty();
                let _ = reply.send(result);
            }
            AutosaveCommand::BlockVaultNote(note_id, version, reply) => {
                vault_blocked_notes.insert(note_id.clone());
                remove_note(&mut pending, &note_id);
                dirty_notes.remove(&note_id);
                let result = journal.clear(&note_id, version).map_err(|error| {
                    AutosaveError::Journal(error)
                        .context("could not clear the note's emergency journal for encryption")
                });
                debounce_armed = !pending.is_empty();
                let _ = reply.send(result);
            }
            AutosaveCommand::UnblockVaultNote(note_id, reply) => {
                let result = match storage.note_has_pending_vault_operation(&note_id) {
                    Ok(false) => {
                        vault_blocked_notes.remove(&note_id);
                        Ok(())
                    }
                    Ok(true) => Err(AutosaveError::Operation(
                        "note autosave remains blocked while encryption state is changing"
                            .to_string(),
                    )),
                    Err(error) => Err(AutosaveError::Storage(error)),
                };
                let _ = reply.send(result);
            }
            AutosaveCommand::ResetLibrary(reply) => {
                pending.clear();
                dirty_notes.clear();
                vault_blocked_notes.clear();
                debounce_armed = false;
                let result = journal.clear_all().map_err(AutosaveError::from);
                let _ = reply.send(result);
            }
            AutosaveCommand::Shutdown => {
                // A failed checkpoint deliberately leaves its per-note journal and
                // dirty tracking intact for startup recovery or diagnostic reporting.
                let _ = checkpoint_all_work(
                    &storage,
                    &mut pending,
                    &mut dirty_notes,
                    &events,
                    &journal,
                );
                break;
            }
        }
    }
}

fn vault_autosave_is_blocked(
    storage: &Storage,
    vault_blocked_notes: &HashSet<NoteId>,
    note_id: &NoteId,
) -> bool {
    vault_blocked_notes.contains(note_id)
        || storage
            .note_autosave_mutation_is_blocked(note_id)
            .unwrap_or(true)
}

fn schedule_journal_or_report(
    storage: &Storage,
    journal: &EmergencyJournal,
    note: &Note,
    version: u64,
    events: &AutosaveEventSender,
) {
    match storage.note_requires_vault_autosave_barrier(&note.id) {
        Ok(true) => {
            let _ = journal.clear(&note.id, version);
            return;
        }
        Ok(false) => {}
        Err(error) => {
            let _ = events.send(AutosaveEvent::Failed(format!(
                "Could not verify note privacy state before emergency journaling: {error}"
            )));
            return;
        }
    }
    if let Err(error) = journal.schedule(note, version) {
        let _ = events.send(AutosaveEvent::Failed(format!(
            "Could not schedule the emergency draft journal: {error}"
        )));
    }
}

fn checkpoint_all_work(
    storage: &Storage,
    pending: &mut VecDeque<PendingDraft>,
    dirty_notes: &mut HashSet<NoteId>,
    events: &AutosaveEventSender,
    journal: &EmergencyJournal,
) -> AutosaveResult<()> {
    let pending_result = flush_pending_queue(
        storage,
        pending,
        dirty_notes,
        events,
        journal,
        true,
        true,
    )
    .map(|_| ());
    let pending_note_ids = pending
        .iter()
        .map(|draft| draft.note.id.clone())
        .collect::<HashSet<_>>();
    let persisted_result =
        checkpoint_persisted_notes(storage, dirty_notes, &pending_note_ids, events);
    match (pending_result, persisted_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(pending_error), Err(persisted_error)) => Err(AutosaveError::Combined {
            primary: Box::new(pending_error),
            additional: Box::new(AutosaveError::Operation(format!(
                "additional checkpoint failures: {persisted_error}"
            ))),
        }),
    }
}

fn checkpoint_persisted_notes(
    storage: &Storage,
    dirty_notes: &mut HashSet<NoteId>,
    pending_note_ids: &HashSet<NoteId>,
    events: &AutosaveEventSender,
) -> AutosaveResult<()> {
    let note_ids = dirty_notes.iter().cloned().collect::<Vec<_>>();
    let mut failures = Vec::new();
    for note_id in note_ids {
        if pending_note_ids.contains(&note_id) {
            continue;
        }
        let result = storage
            .get_note(&note_id)
            .map_err(AutosaveError::Storage)
            .and_then(|note| match note {
                Some(note) => storage
                    .save_note_owned_with_revision_status_scoped(
                        note,
                        crate::storage::RevisionMode::Checkpoint,
                        NoteSaveScope::NONE,
                    )
                    .map(|_| ())
                    .map_err(|failure| AutosaveError::Storage(failure.error)),
                None => Ok(()),
            });
        match result {
            Ok(()) => {
                dirty_notes.remove(&note_id);
            }
            Err(error) => {
                let message = format!("could not checkpoint note {note_id}: {error}");
                let _ = events.send(AutosaveEvent::Failed(message.clone()));
                failures.push(message);
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        autosave_error(failures.join("; "))
    }
}

fn queue_latest(
    pending: &mut VecDeque<PendingDraft>,
    note: Note,
    version: u64,
    scope: NoteSaveScope,
) {
    let merged_scope = take_note(pending, &note.id)
        .map(|pending| pending.scope.union(scope))
        .unwrap_or(scope);
    pending.push_back(PendingDraft {
        note,
        version,
        scope: merged_scope,
    });
}

fn take_note(pending: &mut VecDeque<PendingDraft>, note_id: &NoteId) -> Option<PendingDraft> {
    let index = pending
        .iter()
        .position(|draft| &draft.note.id == note_id)?;
    pending.remove(index)
}

fn remove_note(pending: &mut VecDeque<PendingDraft>, note_id: &NoteId) {
    let _ = take_note(pending, note_id);
}

fn flush_pending_queue(
    storage: &Storage,
    pending: &mut VecDeque<PendingDraft>,
    dirty_notes: &mut HashSet<NoteId>,
    events: &AutosaveEventSender,
    journal: &EmergencyJournal,
    checkpoint: bool,
    emit_saved_event: bool,
) -> AutosaveResult<Option<Arc<Note>>> {
    let mut last_saved = None;
    let mut failures = Vec::new();
    let queued_at_start = pending.len();
    for _ in 0..queued_at_start {
        let Some(draft) = pending.pop_front() else {
            break;
        };
        let version = draft.version;
        let scope = draft.scope;
        match save_note(
            storage,
            draft.note,
            dirty_notes,
            events,
            journal,
            SaveNoteOptions {
                checkpoint,
                emit_saved_event,
                version,
                scope,
            },
        ) {
            Ok(saved) => last_saved = Some(saved),
            Err(failure) => {
                let (note, message) = failure.into_parts();
                pending.push_back(PendingDraft { note, version, scope });
                failures.push(message);
            }
        }
    }
    if failures.is_empty() {
        Ok(last_saved)
    } else {
        let combined = failures
            .into_iter()
            .reduce(|primary, additional| AutosaveError::Combined {
                primary: Box::new(primary),
                additional: Box::new(additional),
            })
            .expect("non-empty autosave failure collection");
        Err(combined)
    }
}

fn save_note(
    storage: &Storage,
    note: Note,
    dirty_notes: &mut HashSet<NoteId>,
    events: &AutosaveEventSender,
    journal: &EmergencyJournal,
    options: SaveNoteOptions,
) -> Result<Arc<Note>, AutosaveSaveFailure> {
    let SaveNoteOptions {
        checkpoint,
        emit_saved_event,
        version,
        scope,
    } = options;
    let mode = if checkpoint {
        crate::storage::RevisionMode::Checkpoint
    } else {
        crate::storage::RevisionMode::Autosave
    };
    let result = storage.save_note_owned_with_revision_status_scoped(note, mode, scope);
    match result {
        Ok((note, revision_recorded)) => {
            let note = Arc::new(note);
            if checkpoint || revision_recorded {
                dirty_notes.remove(&note.id);
            } else {
                dirty_notes.insert(note.id.clone());
            }
            if let Err(clear_error) = journal.clear(&note.id, version) {
                let message = match journal.persist_now(note.as_ref(), version) {
                    Ok(()) => format!(
                        "The note was saved, but its emergency journal could not be removed: {clear_error}. A current recovery record was retained instead."
                    ),
                    Err(persist_error) => format!(
                        "The note was saved, but its emergency journal could not be removed: {clear_error}; the current recovery record could not be refreshed: {persist_error}"
                    ),
                };
                let _ = events.send(AutosaveEvent::Failed(message));
            }
            if emit_saved_event {
                let _ = events.send(AutosaveEvent::Saved(Arc::clone(&note), version));
            }
            Ok(note)
        }
        Err(failure) => {
            let (note, error) = failure.into_parts();
            let mut autosave_error = AutosaveError::Storage(error);
            match storage.note_requires_vault_autosave_barrier(&note.id) {
                Ok(true) => {
                    let _ = journal.clear(&note.id, version);
                }
                Ok(false) => {
                    if let Err(journal_error) = journal.persist_now(&note, version) {
                        autosave_error = AutosaveError::Combined {
                            primary: Box::new(autosave_error),
                            additional: Box::new(
                                AutosaveError::Journal(journal_error)
                                    .context("emergency draft could not be preserved"),
                            ),
                        };
                    }
                }
                Err(privacy_error) => {
                    let _ = journal.clear(&note.id, version);
                    autosave_error = AutosaveError::Combined {
                        primary: Box::new(autosave_error),
                        additional: Box::new(AutosaveError::Storage(privacy_error)),
                    };
                }
            }
            let _ = events.send(AutosaveEvent::Failed(autosave_error.to_string()));
            Err(AutosaveSaveFailure::new(note, autosave_error))
        }
    }
}

fn same_persisted_note(left: &Note, right: &Note) -> bool {
    left.id == right.id
        && left.notebook_id == right.notebook_id
        && left.title == right.title
        && left.title_style == right.title_style
        && (Arc::ptr_eq(&left.document, &right.document) || left.document == right.document)
        && left.tags == right.tags
        && left.pinned == right.pinned
        && left.archived == right.archived
        && left.trashed == right.trashed
        && left.purge_after == right.purge_after
        && left.created_at == right.created_at
        && left.updated_at == right.updated_at
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::storage::new_id;

    use super::*;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let suffix = new_id(label);
        let root = std::env::temp_dir().join(format!("rustnote-{suffix}"));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (storage, root)
    }

    fn cleanup(controller: AutosaveController, storage: Storage, root: PathBuf) {
        drop(controller);
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn newer_note_versions_make_earlier_save_events_stale() {
        let (storage, root) = test_storage("autosave-version");
        let note = storage.create_note(None).expect("create test note");
        let controller = AutosaveController::try_new(storage.clone()).expect("start autosave");

        controller.schedule(note.clone()).expect("schedule first draft");
        assert!(controller.is_current_note_version(&note.id, 1));

        controller.schedule(note.clone()).expect("schedule second draft");
        assert!(!controller.is_current_note_version(&note.id, 1));
        assert!(controller.is_current_note_version(&note.id, 2));

        controller
            .flush_pending_silent()
            .expect("flush pending autosave work");
        cleanup(controller, storage, root);
    }

    #[test]
    fn switching_notes_flushes_each_pending_identity_without_cross_note_overwrite() {
        let (storage, root) = test_storage("autosave-note-switch");
        let mut first = storage.create_note(None).expect("create first note");
        let mut second = storage.create_note(None).expect("create second note");
        first.title = "First pending title".to_string();
        second.title = "Second pending title".to_string();

        let controller = AutosaveController::try_new(storage.clone()).expect("start autosave");
        controller.schedule(first.clone()).expect("schedule first note");
        controller.schedule(second.clone()).expect("schedule second note");
        controller
            .flush_pending_silent()
            .expect("flush pending autosave work");

        let saved_first = storage
            .get_note(&first.id)
            .expect("read first note")
            .expect("first note should exist");
        let saved_second = storage
            .get_note(&second.id)
            .expect("read second note")
            .expect("second note should exist");
        assert_eq!(saved_first.id, first.id);
        assert_eq!(saved_first.title, "First pending title");
        assert_eq!(saved_second.id, second.id);
        assert_eq!(saved_second.title, "Second pending title");

        cleanup(controller, storage, root);
    }
    #[test]
    fn vault_barrier_prevents_stale_plaintext_emergency_journal_recreation() {
        let (storage, root) = test_storage("autosave-vault-barrier");
        let note = storage.create_note(None).expect("create note");
        let mut stale = note.clone();
        let marker = format!("STALE-VAULT-DRAFT-{}", new_id("marker"));
        stale.title = marker.clone();

        let controller = AutosaveController::try_new(storage.clone()).expect("start autosave");
        controller.schedule(stale.clone()).expect("schedule stale draft");
        controller
            .block_note_for_vault(&note.id)
            .expect("establish vault barrier");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect canonical note");

        // A stale UI task arriving after protection must be dropped by both the
        // worker barrier and the authoritative storage privacy check.
        controller.schedule(stale).expect("queue stale post-protection draft");
        controller
            .flush_pending_silent()
            .expect("drain autosave queue");

        let journal_root = storage.data_directory().join("emergency-journal");
        let mut pending = vec![journal_root];
        while let Some(path) = pending.pop() {
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => panic!("inspect emergency journal: {error}"),
            };
            for entry in entries {
                let entry = entry.expect("journal entry");
                if entry.file_type().expect("journal file type").is_dir() {
                    pending.push(entry.path());
                    continue;
                }
                let bytes = fs::read(entry.path()).expect("read journal artifact");
                assert!(
                    !bytes.windows(marker.len()).any(|window| window == marker.as_bytes()),
                    "protected-note stale plaintext must never re-enter the emergency journal"
                );
            }
        }

        cleanup(controller, storage, root);
    }

    #[test]
    fn unlocked_protected_note_autosaves_and_close_checkpoints_without_plaintext_journal() {
        let (storage, root) = test_storage("autosave-unlocked-protected");
        let note = storage.create_note(None).expect("create note");
        let controller = AutosaveController::try_new(storage.clone()).expect("start autosave");

        controller
            .block_note_for_vault(&note.id)
            .expect("establish protect barrier");
        let mut protected = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        assert!(
            storage
                .note_requires_vault_autosave_barrier(&note.id)
                .expect("plaintext privacy state"),
            "protected plaintext must remain barred from the emergency journal",
        );
        assert!(
            !storage
                .note_has_pending_vault_operation(&note.id)
                .expect("vault transition state"),
            "completed protection must not masquerade as an active Vault transition",
        );
        controller
            .unblock_note_after_vault(&note.id)
            .expect("unlocked protected note re-enters encrypted autosave");

        let explicit_marker = format!("PROTECTED-AUTOSAVE-{}", new_id("marker"));
        protected.title = explicit_marker.clone();
        protected.updated_at = protected.updated_at.saturating_add(1);
        protected = controller
            .flush_note_scoped(protected, NoteSaveScope::TITLE)
            .expect("explicit encrypted-note save")
            .as_ref()
            .clone();

        let close_marker = format!("PROTECTED-CLOSE-{}", new_id("marker"));
        protected.title = close_marker.clone();
        protected.updated_at = protected.updated_at.saturating_add(1);
        controller
            .schedule_scoped(protected, NoteSaveScope::TITLE)
            .expect("schedule encrypted-note autosave");
        controller
            .checkpoint_all()
            .expect("close checkpoint for unlocked protected note");

        storage
            .lock_protected_note(&note.id)
            .expect("lock protected note");
        let reopened = storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock persisted protected note");
        assert_eq!(reopened.title, close_marker);

        let journal_root = storage.data_directory().join("emergency-journal");
        let mut pending = vec![journal_root];
        while let Some(path) = pending.pop() {
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => panic!("inspect emergency journal: {error}"),
            };
            for entry in entries {
                let entry = entry.expect("journal entry");
                if entry.file_type().expect("journal file type").is_dir() {
                    pending.push(entry.path());
                    continue;
                }
                let bytes = fs::read(entry.path()).expect("read journal artifact");
                for marker in [&explicit_marker, &close_marker] {
                    assert!(
                        !bytes
                            .windows(marker.len())
                            .any(|window| window == marker.as_bytes()),
                        "protected-note plaintext must never enter the emergency journal",
                    );
                }
            }
        }

        cleanup(controller, storage, root);
    }

}
