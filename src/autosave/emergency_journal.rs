use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    build_info::FULL_BUILD_IDENTIFIER,
    editor_projection::derive_editor_projections,
    domain::{Note, NoteId, NoteTitleStyle},
    storage::{Storage, unix_timestamp},
};

use super::{AutosaveEvent, AutosaveEventSender, same_persisted_note};

const JOURNAL_DIRECTORY: &str = "emergency-journal";
const JOURNAL_MANIFEST_FILE: &str = "manifest.json";
const JOURNAL_MANIFEST_TEMP_FILE: &str = "manifest.json.tmp";
const JOURNAL_OWNER_MARKER: &str = ".emergency-journal.plaintext-owner";
const JOURNAL_OWNER_IDENTITY: &str = "emergency-journal-v1";
const JOURNAL_QUARANTINE_DIRECTORY: &str = "quarantine";
const JOURNAL_RECORD_VERSION: u32 = 1;
const JOURNAL_MANIFEST_VERSION: u32 = 1;
const JOURNAL_DEBOUNCE: Duration = Duration::from_millis(1_500);
const JOURNAL_RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_NOTE_JOURNAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_JOURNAL_RECORDS: usize = 100_000;

#[derive(Debug)]
pub(crate) enum EmergencyJournalError {
    Operation(String),
    SchedulerUnavailable,
    ScheduleUnavailable,
    WorkerUnavailable,
    WorkerStopped(&'static str),
}

impl EmergencyJournalError {
    fn new(message: impl Into<String>) -> Self {
        Self::Operation(message.into())
    }
}

impl std::fmt::Display for EmergencyJournalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(message) => formatter.write_str(message),
            Self::SchedulerUnavailable => {
                formatter.write_str("emergency journal scheduler is unavailable")
            }
            Self::ScheduleUnavailable => {
                formatter.write_str("emergency journal schedule is unavailable")
            }
            Self::WorkerUnavailable => {
                formatter.write_str("emergency journal worker is unavailable")
            }
            Self::WorkerStopped(operation) => {
                write!(formatter, "emergency journal worker stopped before {operation}")
            }
        }
    }
}

impl std::error::Error for EmergencyJournalError {}

impl From<String> for EmergencyJournalError {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

type JournalResult<T> = Result<T, EmergencyJournalError>;

fn journal_error<T>(message: impl Into<String>) -> JournalResult<T> {
    Err(EmergencyJournalError::new(message))
}

#[derive(Clone)]
pub(super) struct EmergencyJournal {
    commands: Sender<JournalCommand>,
    scheduled: Arc<Mutex<Option<EmergencyDraft>>>,
    schedule_gate: Arc<Mutex<()>>,
    wake_pending: Arc<AtomicBool>,
}

pub(super) struct StartedEmergencyJournal {
    pub client: EmergencyJournal,
    pub worker: JoinHandle<()>,
}

#[derive(Debug, Default)]
pub(super) struct RecoveryReport {
    pub recovered: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmergencyDraft {
    note_id: NoteId,
    notebook_id: String,
    title: String,
    #[serde(default)]
    title_style: NoteTitleStyle,
    editor_state: Arc<String>,
    tags: Vec<String>,
    pinned: bool,
    archived: bool,
    trashed: bool,
    purge_after: Option<i64>,
    created_at: i64,
    updated_at: i64,
    autosave_version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmergencyDraftRecord {
    version: u32,
    #[serde(default)]
    build_identifier: String,
    draft: EmergencyDraft,
    checksum: String,
}

#[derive(Serialize)]
struct EmergencyDraftRecordRef<'a> {
    version: u32,
    build_identifier: &'static str,
    draft: &'a EmergencyDraft,
    checksum: &'a str,
}

#[derive(Clone, Debug)]
struct PersistedDraft {
    autosave_version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalManifest {
    version: u32,
    #[serde(default)]
    build_identifier: String,
    updated_at: i64,
}



struct ScheduledDraft {
    draft: EmergencyDraft,
    deadline: Instant,
}

struct JournalState {
    pending: Option<ScheduledDraft>,
    persisted: HashMap<NoteId, PersistedDraft>,
}

enum JournalCommand {
    Wake,
    PersistNow(Box<EmergencyDraft>, Sender<JournalResult<()>>),
    Clear(NoteId, u64, Sender<JournalResult<()>>),
    ClearAll(Sender<JournalResult<()>>),
    Shutdown,
}

struct DigestWriter(Sha256);

impl Write for DigestWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl EmergencyDraft {
    fn from_note(note: &Note, autosave_version: u64) -> Self {
        Self {
            note_id: note.id.clone(),
            notebook_id: note.notebook_id.clone(),
            title: note.title.clone(),
            title_style: note.title_style.clone(),
            editor_state: note.document.editor_state.clone(),
            tags: note.tags.clone(),
            pinned: note.pinned,
            archived: note.archived,
            trashed: note.trashed,
            purge_after: note.purge_after,
            created_at: note.created_at,
            updated_at: note.updated_at,
            autosave_version,
        }
    }

    fn into_recovered_note(self, current: &Note) -> JournalResult<Note> {
        let projections = derive_editor_projections(self.editor_state.as_str()).map_err(|error| {
            format!(
                "could not regenerate projections for emergency draft {}: {error}",
                self.note_id
            )
        })?;
        Ok(Note {
            id: self.note_id,
            protection: crate::domain::NoteProtectionState::Unprotected,
            notebook_id: self.notebook_id,
            title: self.title,
            title_style: self.title_style.normalized(),
            document: std::sync::Arc::new(crate::domain::NoteDocument::new(
                projections.markdown,
                self.editor_state,
                projections.plain_text,
            )),
            tags: self.tags,
            attachment_names: current.attachment_names.clone(),
            pinned: self.pinned,
            archived: self.archived,
            trashed: self.trashed,
            purge_after: self.purge_after,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl EmergencyJournal {
    pub(super) fn start(
        data_directory: PathBuf,
        events: AutosaveEventSender,
    ) -> JournalResult<StartedEmergencyJournal> {
        let namespace = journal_plaintext_namespace(&data_directory)?;
        ensure_private_directory(namespace.root())?;
        let (sender, receiver) = mpsc::channel();
        let scheduled = Arc::new(Mutex::new(None));
        let schedule_gate = Arc::new(Mutex::new(()));
        let wake_pending = Arc::new(AtomicBool::new(false));
        let scheduled_for_worker = scheduled.clone();
        let wake_for_worker = wake_pending.clone();
        let worker = thread::Builder::new()
            .name("rustnote-emergency-journal".to_string())
            .spawn(move || {
                journal_worker(
                    data_directory,
                    receiver,
                    events,
                    scheduled_for_worker,
                    wake_for_worker,
                )
            })
            .map_err(|error| format!("could not start the emergency journal worker: {error}"))?;
        Ok(StartedEmergencyJournal {
            client: Self {
                commands: sender,
                scheduled,
                schedule_gate,
                wake_pending,
            },
            worker,
        })
    }

    pub(super) fn schedule(&self, note: &Note, version: u64) -> JournalResult<()> {
        // Protected-note plaintext must never enter the disk emergency journal.
        // E2 notes are read-only, but keep this guard at the journal boundary so
        // later editable encrypted notes cannot regress into plaintext recovery.
        if note.protection.is_protected() {
            return self.clear(&note.id, version);
        }
        // Scheduling is serialized so a displaced draft cannot be overwritten by
        // another caller while it is synchronously spooled to the worker.
        let _schedule_guard = self
            .schedule_gate
            .lock()
            .map_err(|_| EmergencyJournalError::SchedulerUnavailable)?;
        let draft = EmergencyDraft::from_note(note, version);
        let draft_note_id = draft.note_id.clone();
        let draft_version = draft.autosave_version;
        let mut draft = Some(draft);
        let displaced = {
            let mut scheduled = self
                .scheduled
                .lock()
                .map_err(|_| EmergencyJournalError::ScheduleUnavailable)?;
            match scheduled.as_ref() {
                Some(current)
                    if current.note_id == draft_note_id
                        && current.autosave_version > draft_version =>
                {
                    return Ok(());
                }
                Some(current) if current.note_id != draft_note_id => scheduled.take(),
                _ => {
                    *scheduled = draft.take();
                    None
                }
            }
        };

        let mut spool_error = None;
        if let Some(displaced) = displaced {
            // A second dirty note is spooled before the new one is retained in
            // memory. The journal therefore keeps at most one full draft in its
            // client-side debounce slot. If spooling fails, the worker retains
            // the displaced note as its sole retry while this note is still
            // handed off, so neither draft is silently discarded.
            if let Err(error) = self.persist_draft_now(displaced) {
                spool_error = Some(error);
            }
            let mut scheduled = self
                .scheduled
                .lock()
                .map_err(|_| EmergencyJournalError::ScheduleUnavailable)?;
            *scheduled = draft.take();
        }

        if !self.wake_pending.swap(true, Ordering::AcqRel)
            && self.commands.send(JournalCommand::Wake).is_err()
        {
            self.wake_pending.store(false, Ordering::Release);
            return Err(EmergencyJournalError::WorkerUnavailable);
        }
        match spool_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(super) fn persist_now(&self, note: &Note, version: u64) -> JournalResult<()> {
        if note.protection.is_protected() {
            return self.clear(&note.id, version);
        }
        self.persist_draft_now(EmergencyDraft::from_note(note, version))
    }

    fn persist_draft_now(&self, draft: EmergencyDraft) -> JournalResult<()> {
        let (sender, receiver) = mpsc::channel();
        self.commands
            .send(JournalCommand::PersistNow(Box::new(draft), sender))
            .map_err(|_| EmergencyJournalError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| EmergencyJournalError::WorkerStopped("preserving the draft"))?
    }

    pub(super) fn clear(&self, note_id: &NoteId, version: u64) -> JournalResult<()> {
        let (sender, receiver) = mpsc::channel();
        self.commands
            .send(JournalCommand::Clear(note_id.clone(), version, sender))
            .map_err(|_| EmergencyJournalError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| EmergencyJournalError::WorkerStopped("clearing the draft"))?
    }

    pub(super) fn clear_all(&self) -> JournalResult<()> {
        let (sender, receiver) = mpsc::channel();
        self.commands
            .send(JournalCommand::ClearAll(sender))
            .map_err(|_| EmergencyJournalError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| EmergencyJournalError::WorkerStopped("clearing the library journal"))?
    }

    pub(super) fn shutdown(&self) {
        let _ = self.commands.send(JournalCommand::Shutdown);
    }
}

pub(super) fn recover(storage: &Storage) -> JournalResult<RecoveryReport> {
    let data_directory = storage.data_directory();
    let namespace = journal_plaintext_namespace(data_directory)?;
    let directory = namespace.root().to_path_buf();
    ensure_private_directory(&directory)?;

    // A protected note's journal filenames are deterministic from its note ID.
    // Purge every known root/tmp/previous/quarantine record before parsing any
    // candidate so corrupt protected plaintext can never be moved into quarantine.
    // Cleanup is fail-closed: startup stops if Rustnote cannot remove such residue.
    discard_protected_journal_records(storage, &directory)?;

    let mut report = RecoveryReport::default();
    for paths in collect_recovery_candidate_groups(&directory)? {
        recover_candidate_group(storage, &directory, paths, &mut report)?;
    }

    // Defence in depth against any protected record created or exposed during
    // recovery: leave neither root journal files nor quarantine copies behind.
    discard_protected_journal_records(storage, &directory)?;

    if let Err(error) = write_manifest(&directory) {
        report.warnings.push(format!(
            "Could not refresh the emergency journal manifest after recovery: {error}"
        ));
    }
    sync_directory(&directory)?;
    Ok(report)
}

fn recover_candidate_group(
    storage: &Storage,
    directory: &Path,
    paths: Vec<PathBuf>,
    report: &mut RecoveryReport,
) -> JournalResult<()> {
    let mut valid_paths = Vec::new();
    let mut newest = None::<EmergencyDraftRecord>;

    for path in paths {
        match read_draft_record(&path) {
            Ok(record) => {
                let replace = newest.as_ref().is_none_or(|current| {
                    (current.draft.updated_at, current.draft.autosave_version)
                        <= (record.draft.updated_at, record.draft.autosave_version)
                });
                if replace {
                    newest = Some(record);
                }
                valid_paths.push(path);
            }
            Err(error) => {
                let quarantine_result = quarantine_file(directory, &path, "corrupt");
                report.warnings.push(match quarantine_result {
                    Ok(target) => format!(
                        "Ignored corrupt emergency journal file {}: {error}. It was moved to {}.",
                        path.display(),
                        target.display()
                    ),
                    Err(quarantine_error) => format!(
                        "Ignored corrupt emergency journal file {}: {error}. It could not be quarantined: {quarantine_error}",
                        path.display()
                    ),
                });
            }
        }
    }

    let Some(newest) = newest else {
        return Ok(());
    };
    let note_id = newest.draft.note_id.clone();
    let current = match storage.get_note(&note_id) {
        Ok(Some(note)) => note,
        Ok(None) => {
            quarantine_paths(directory, &valid_paths, "missing-note", &mut report.warnings);
            report.warnings.push(format!(
                "Emergency draft {note_id} references a missing note and was quarantined."
            ));
            return Ok(());
        }
        Err(error) => {
            return journal_error(format!(
                "could not inspect emergency draft {note_id}: {error}"
            ));
        }
    };

    if current.protection.is_protected() {
        // A known plaintext recovery record for a protected note is part of the
        // E3 privacy boundary. Do not continue startup after merely warning that
        // Rustnote failed to remove it: keep recovery fail-closed and retry on
        // the next open instead of knowingly leaving app-managed plaintext.
        discard_protected_recovery_records(&valid_paths)?;
        report.warnings.push(format!(
            "Discarded an obsolete plaintext emergency draft for encrypted note {note_id}."
        ));
        return Ok(());
    }

    if current.trashed {
        quarantine_paths(directory, &valid_paths, "trashed-note", &mut report.warnings);
        report.warnings.push(format!(
            "Emergency draft {note_id} was not recovered because the note is in Trash and is read-only."
        ));
        return Ok(());
    }

    let recovered = match newest.draft.into_recovered_note(&current) {
        Ok(note) => note,
        Err(error) => {
            report.warnings.push(format!(
                "Could not project emergency draft {note_id}: {error}. The valid journal record was retained for a later recovery attempt."
            ));
            return Ok(());
        }
    };
    if current.updated_at <= recovered.updated_at && !same_persisted_note(&current, &recovered) {
        if let Err(error) = storage.save_note_checkpoint_owned(recovered) {
            report.warnings.push(format!(
                "Could not recover emergency draft {note_id}: {error}. The valid journal record was retained for a later recovery attempt."
            ));
            return Ok(());
        }
        report.recovered += 1;
    }

    for path in valid_paths {
        if let Err(error) = privacy_remove_file_if_present(&path) {
            report.warnings.push(error.to_string());
        }
    }
    Ok(())
}

fn journal_worker(
    data_directory: PathBuf,
    commands: Receiver<JournalCommand>,
    events: AutosaveEventSender,
    scheduled: Arc<Mutex<Option<EmergencyDraft>>>,
    wake_pending: Arc<AtomicBool>,
) {
    let directory = journal_directory(&data_directory);
    let mut state = JournalState {
        pending: None,
        persisted: HashMap::new(),
    };

    loop {
        let command = if let Some(timeout) = next_journal_timeout(&state) {
            match commands.recv_timeout(timeout) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => {
                    persist_due_drafts(&directory, &mut state, &events);
                    None
                }
                Err(RecvTimeoutError::Disconnected) => {
                    drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
                    persist_all_pending(&directory, &mut state, &events);
                    break;
                }
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => {
                    drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
                    persist_all_pending(&directory, &mut state, &events);
                    break;
                }
            }
        };

        let Some(command) = command else {
            continue;
        };
        match command {
            JournalCommand::Wake => {
                wake_pending.store(false, Ordering::Release);
                drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
            }
            JournalCommand::PersistNow(draft, reply) => {
                drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
                let result = persist_draft_immediately(&directory, &mut state, *draft, &events);
                let _ = reply.send(result);
            }
            JournalCommand::Clear(note_id, version, reply) => {
                drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
                let result = clear_note(&directory, &mut state, &note_id, version);
                let _ = reply.send(result);
            }
            JournalCommand::ClearAll(reply) => {
                if let Ok(mut incoming) = scheduled.lock() {
                    *incoming = None;
                }
                wake_pending.store(false, Ordering::Release);
                state.pending = None;
                state.persisted.clear();
                let result = reset_journal_storage(&data_directory);
                let _ = reply.send(result);
            }
            JournalCommand::Shutdown => {
                drain_scheduled_drafts(&directory, &scheduled, &mut state, &events);
                persist_all_pending(&directory, &mut state, &events);
                break;
            }
        }
    }
}

fn drain_scheduled_drafts(
    directory: &Path,
    scheduled: &Mutex<Option<EmergencyDraft>>,
    state: &mut JournalState,
    events: &AutosaveEventSender,
) {
    let incoming = match scheduled.lock() {
        Ok(mut scheduled) => scheduled.take(),
        Err(_) => {
            let _ = events.send(AutosaveEvent::Failed(
                "Emergency journal schedule is unavailable.".to_string(),
            ));
            return;
        }
    };
    if let Some(draft) = incoming {
        schedule_draft(directory, state, draft, events);
    }
}

fn schedule_draft(
    directory: &Path,
    state: &mut JournalState,
    draft: EmergencyDraft,
    events: &AutosaveEventSender,
) -> bool {
    if state
        .persisted
        .get(&draft.note_id)
        .is_some_and(|current| current.autosave_version > draft.autosave_version)
    {
        return false;
    }

    if let Some(current) = state.pending.take() {
        if current.draft.note_id == draft.note_id {
            if current.draft.autosave_version > draft.autosave_version {
                state.pending = Some(current);
                return false;
            }
        } else if let Err(error) = persist_scheduled_draft(directory, state, current) {
            let _ = events.send(AutosaveEvent::Failed(format!(
                "Could not preserve displaced emergency draft before scheduling {}: {error}",
                draft.note_id
            )));
            return match write_draft_record(directory, &draft) {
                Ok(persisted) => {
                    state.persisted.insert(draft.note_id, persisted);
                    true
                }
                Err(new_error) => {
                    let _ = events.send(AutosaveEvent::Failed(format!(
                        "Could not preserve emergency draft {}: {new_error}",
                        draft.note_id
                    )));
                    false
                }
            };
        }
    }

    state.pending = Some(ScheduledDraft {
        draft,
        deadline: Instant::now() + JOURNAL_DEBOUNCE,
    });
    true
}

fn next_journal_timeout(state: &JournalState) -> Option<Duration> {
    state
        .pending
        .as_ref()
        .map(|scheduled| scheduled.deadline.saturating_duration_since(Instant::now()))
}

fn persist_due_drafts(
    directory: &Path,
    state: &mut JournalState,
    events: &AutosaveEventSender,
) {
    let due = state
        .pending
        .as_ref()
        .is_some_and(|scheduled| scheduled.deadline <= Instant::now());
    if !due {
        return;
    }
    let Some(scheduled) = state.pending.take() else {
        return;
    };
    let note_id = scheduled.draft.note_id.clone();
    if let Err(error) = persist_scheduled_draft(directory, state, scheduled) {
        if let Some(pending) = state.pending.as_mut() {
            pending.deadline = Instant::now() + JOURNAL_RETRY_DELAY;
        }
        let _ = events.send(AutosaveEvent::Failed(format!(
            "Could not preserve emergency draft {note_id}: {error}"
        )));
    }
}

fn persist_all_pending(
    directory: &Path,
    state: &mut JournalState,
    events: &AutosaveEventSender,
) {
    let Some(scheduled) = state.pending.take() else {
        return;
    };
    let note_id = scheduled.draft.note_id.clone();
    if let Err(error) = persist_scheduled_draft(directory, state, scheduled) {
        let _ = events.send(AutosaveEvent::Failed(format!(
            "Could not preserve emergency draft {note_id} during shutdown: {error}"
        )));
    }
}

fn persist_scheduled_draft(
    directory: &Path,
    state: &mut JournalState,
    scheduled: ScheduledDraft,
) -> JournalResult<()> {
    let note_id = scheduled.draft.note_id.clone();
    match write_draft_record(directory, &scheduled.draft) {
        Ok(persisted) => {
            state.persisted.insert(note_id, persisted);
            Ok(())
        }
        Err(error) => {
            state.pending = Some(scheduled);
            Err(error)
        }
    }
}

fn persist_draft_immediately(
    directory: &Path,
    state: &mut JournalState,
    draft: EmergencyDraft,
    events: &AutosaveEventSender,
) -> JournalResult<()> {
    if state
        .persisted
        .get(&draft.note_id)
        .is_some_and(|current| current.autosave_version > draft.autosave_version)
    {
        return Ok(());
    }

    if let Some(current) = state.pending.take() {
        if current.draft.note_id == draft.note_id {
            if current.draft.autosave_version > draft.autosave_version {
                state.pending = Some(current);
                return Ok(());
            }
        } else if let Err(error) = persist_scheduled_draft(directory, state, current) {
            let _ = events.send(AutosaveEvent::Failed(format!(
                "Could not preserve another pending emergency draft: {error}"
            )));
            // The displaced draft remains as the sole in-memory retry. Persist
            // this draft directly so both notes are protected without retaining
            // both complete editor states in memory after this call returns.
            let persisted = write_draft_record(directory, &draft)?;
            state.persisted.insert(draft.note_id, persisted);
            return Ok(());
        }
    }

    match write_draft_record(directory, &draft) {
        Ok(persisted) => {
            state.persisted.insert(draft.note_id, persisted);
            Ok(())
        }
        Err(error) => {
            state.pending = Some(ScheduledDraft {
                draft,
                deadline: Instant::now() + JOURNAL_RETRY_DELAY,
            });
            Err(error)
        }
    }
}

fn clear_note(
    directory: &Path,
    state: &mut JournalState,
    note_id: &NoteId,
    version: u64,
) -> JournalResult<()> {
    if state.pending.as_ref().is_some_and(|scheduled| {
        &scheduled.draft.note_id == note_id && scheduled.draft.autosave_version <= version
    }) {
        state.pending = None;
    }
    let has_newer_pending = state.pending.as_ref().is_some_and(|scheduled| {
        &scheduled.draft.note_id == note_id && scheduled.draft.autosave_version > version
    });
    let has_newer_persisted = state
        .persisted
        .get(note_id)
        .is_some_and(|entry| entry.autosave_version > version);
    if has_newer_pending || has_newer_persisted {
        return Ok(());
    }

    state.persisted.remove(note_id);
    let paths = draft_paths(directory, note_id);
    let mut removed_file = false;
    for path in [&paths.final_path, &paths.temporary_path, &paths.previous_path] {
        removed_file |= privacy_remove_file_if_present(path)?;
    }
    removed_file |= discard_quarantined_records_for_note(directory, note_id)?;
    if removed_file {
        sync_directory(directory)?;
    }
    Ok(())
}

/// Best-effort content overwrite before unlinking a Rustnote-managed recovery
/// file. Modern SSD/COW filesystems can retain historical physical blocks, so
/// this is not claimed as physical-media erasure; it does ensure Rustnote does
/// not intentionally leave a readable journal file behind.
fn privacy_remove_file_if_present(path: &Path) -> JournalResult<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return journal_error(format!(
                "could not inspect emergency-journal plaintext {}: {error}",
                path.display()
            ));
        }
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return journal_error(format!(
                "could not securely discard {}: path is not a removable journal file",
                path.display()
            ));
        }
        Ok(_) => {}
    }
    let namespace = journal_plaintext_namespace_for_path(path)?;
    let mut owned = namespace.adopt_existing(path.to_path_buf()).map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not resume emergency-journal plaintext ownership: {error}"
        ))
    })?;
    owned.remove_file().map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not securely discard {}: {error}",
            path.display()
        ))
    })?;
    Ok(true)
}

fn discard_protected_recovery_records(paths: &[PathBuf]) -> JournalResult<()> {
    for path in paths {
        let _ = privacy_remove_file_if_present(path)?;
    }
    Ok(())
}

fn discard_protected_journal_records(
    storage: &Storage,
    directory: &Path,
) -> JournalResult<()> {
    let note_ids = storage
        .protected_note_ids_for_privacy_recovery()
        .map_err(|error| {
            EmergencyJournalError::Operation(format!(
                "could not enumerate encrypted notes for emergency-journal privacy recovery: {error}"
            ))
        })?;
    for note_id in note_ids {
        let paths = draft_paths(directory, &note_id);
        discard_protected_recovery_records(&[
            paths.final_path,
            paths.temporary_path,
            paths.previous_path,
        ])?;
        let _ = discard_quarantined_records_for_note(directory, &note_id)?;
    }
    Ok(())
}

fn discard_quarantined_records_for_note(
    directory: &Path,
    note_id: &NoteId,
) -> JournalResult<bool> {
    let quarantine = directory.join(JOURNAL_QUARANTINE_DIRECTORY);
    let entries = match fs::read_dir(&quarantine) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return journal_error(format!(
                "could not inspect emergency journal quarantine {}: {error}",
                quarantine.display()
            ));
        }
    };
    let prefix = format!("{}.", draft_file_name(note_id));
    let mut removed = false;
    for entry in entries {
        let entry = entry.map_err(|error| {
            EmergencyJournalError::Operation(format!(
                "could not inspect emergency journal quarantine {}: {error}",
                quarantine.display()
            ))
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&prefix) {
            removed |= privacy_remove_file_if_present(&entry.path())?;
        }
    }
    if removed {
        sync_directory(&quarantine)?;
    }
    Ok(removed)
}

fn reset_journal_storage(data_directory: &Path) -> JournalResult<()> {
    // Establish the durable owner *outside* the journal tree before removing or
    // recreating any managed plaintext. A partial recursive deletion can never
    // delete this authority ahead of surviving journal bytes.
    let namespace = journal_plaintext_namespace(data_directory)?;
    let directory = namespace.root().to_path_buf();
    namespace.remove_tree_strict().map_err(|error| {
        EmergencyJournalError::new(format!("could not remove {}: {error}", directory.display()))
    })?;
    ensure_private_directory(&directory)?;
    write_manifest(&directory)?;
    sync_directory(data_directory)
}

fn write_draft_record(
    directory: &Path,
    draft: &EmergencyDraft,
) -> JournalResult<PersistedDraft> {
    validate_draft(draft)?;
    if estimated_draft_bytes(draft) > MAX_NOTE_JOURNAL_BYTES {
        return journal_error(format!(
            "draft {} exceeds the {} MiB per-note recovery limit",
            draft.note_id,
            MAX_NOTE_JOURNAL_BYTES / (1024 * 1024)
        ));
    }
    ensure_private_directory(directory)?;
    let checksum = draft_checksum(draft)?;
    let record = EmergencyDraftRecordRef {
        version: JOURNAL_RECORD_VERSION,
        build_identifier: FULL_BUILD_IDENTIFIER,
        draft,
        checksum: &checksum,
    };
    let paths = draft_paths(directory, &draft.note_id);
    write_json_file(&paths.temporary_path, &record)?;
    let byte_size = fs::metadata(&paths.temporary_path)
        .map_err(|error| format!("could not inspect {}: {error}", paths.temporary_path.display()))?
        .len();
    if byte_size > MAX_NOTE_JOURNAL_BYTES {
        let _ = remove_file_if_present(&paths.temporary_path);
        return journal_error(format!(
            "draft {} exceeds the {} MiB per-note recovery limit",
            draft.note_id,
            MAX_NOTE_JOURNAL_BYTES / (1024 * 1024)
        ));
    }
    replace_temporary_file(
        &paths.temporary_path,
        &paths.final_path,
        &paths.previous_path,
    )?;
    Ok(PersistedDraft {
        autosave_version: draft.autosave_version,
    })
}

fn write_manifest(directory: &Path) -> JournalResult<()> {
    let manifest = JournalManifest {
        version: JOURNAL_MANIFEST_VERSION,
        build_identifier: FULL_BUILD_IDENTIFIER.to_string(),
        updated_at: unix_timestamp(),
    };
    let final_path = directory.join(JOURNAL_MANIFEST_FILE);
    let temporary_path = directory.join(JOURNAL_MANIFEST_TEMP_FILE);
    let previous_path = directory.join("manifest.json.previous");
    write_json_file(&temporary_path, &manifest)?;
    replace_temporary_file(&temporary_path, &final_path, &previous_path)
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> JournalResult<()> {
    remove_file_if_present(path)?;
    let namespace = journal_plaintext_namespace_for_path(path)?;
    let mut owned = namespace.reserve(path.to_path_buf()).map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not reserve emergency-journal plaintext {}: {error}",
            path.display()
        ))
    })?;
    let file = owned.create_file().map_err(|error| {
        EmergencyJournalError::new(format!("could not create {}: {error}", path.display()))
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("could not encode {}: {error}", path.display()))?;
    writer
        .flush()
        .map_err(|error| format!("could not flush {}: {error}", path.display()))?;
    let file = writer
        .into_inner()
        .map_err(|error| format!("could not finish {}: {}", path.display(), error.into_error()))?;
    file.sync_all().map_err(|error| {
        EmergencyJournalError::new(format!("could not sync {}: {error}", path.display()))
    })
}

fn replace_temporary_file(
    temporary_path: &Path,
    destination: &Path,
    previous_path: &Path,
) -> JournalResult<()> {
    let namespace = journal_plaintext_namespace_for_path(temporary_path)?;
    let mut temporary = namespace.adopt_existing(temporary_path.to_path_buf()).map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not resume emergency-journal temporary ownership: {error}"
        ))
    })?;

    #[cfg(unix)]
    {
        let _ = previous_path;
        temporary
            .rename_durable_namespace(destination.to_path_buf())
            .map_err(|error| {
                EmergencyJournalError::new(format!(
                    "could not install {} from {}: {error}",
                    destination.display(),
                    temporary_path.display()
                ))
            })?;
        sync_parent_directory(destination)
    }

    #[cfg(not(unix))]
    {
        remove_file_if_present(previous_path)?;
        if crate::security::managed_path_is_present(destination)
            .map_err(|error| EmergencyJournalError::new(error.to_string()))?
        {
            let mut previous = namespace
                .adopt_existing(destination.to_path_buf())
                .map_err(|error| EmergencyJournalError::new(error.to_string()))?;
            previous
                .rename_durable_namespace(previous_path.to_path_buf())
                .map_err(|error| {
                    EmergencyJournalError::new(format!(
                        "could not preserve previous journal file {}: {error}",
                        destination.display()
                    ))
                })?;
        }
        if let Err(error) = temporary.rename_durable_namespace(destination.to_path_buf()) {
            if crate::security::managed_path_is_present(previous_path)
                .map_err(|error| EmergencyJournalError::new(error.to_string()))?
            {
                if let Ok(mut previous) = namespace.adopt_existing(previous_path.to_path_buf()) {
                    let _ = previous.rename_durable_namespace(destination.to_path_buf());
                }
            }
            return journal_error(format!(
                "could not install {} from {}: {error}",
                destination.display(),
                temporary_path.display()
            ));
        }
        sync_parent_directory(destination)?;
        remove_file_if_present(previous_path)?;
        sync_parent_directory(destination)
    }
}

fn collect_recovery_candidate_groups(directory: &Path) -> JournalResult<Vec<Vec<PathBuf>>> {
    let mut files = fs::read_dir(directory)
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?;
    files.sort_by_key(|entry| entry.file_name());
    if files.len() > MAX_JOURNAL_RECORDS.saturating_mul(3).saturating_add(10) {
        return journal_error(format!(
            "{} contains too many emergency journal files",
            directory.display()
        ));
    }

    let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
    for entry in files {
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?
            .is_file()
        {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(base_name) = draft_candidate_base_name(name) else {
            continue;
        };
        groups.entry(base_name.to_string()).or_default().push(path);
    }
    Ok(groups.into_values().collect())
}

fn read_draft_record(path: &Path) -> JournalResult<EmergencyDraftRecord> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return journal_error("journal entry is not a regular file");
    }
    if metadata.len() > MAX_NOTE_JOURNAL_BYTES {
        return journal_error(format!(
            "journal entry exceeds the {} MiB per-note recovery limit",
            MAX_NOTE_JOURNAL_BYTES / (1024 * 1024)
        ));
    }
    let file = File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let record: EmergencyDraftRecord = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    if record.version != JOURNAL_RECORD_VERSION {
        return journal_error(format!(
            "unsupported emergency journal version {}",
            record.version
        ));
    }
    validate_draft(&record.draft)?;
    let expected_file = draft_file_name(&record.draft.note_id);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if draft_candidate_base_name(name) != Some(expected_file.as_str()) {
        return journal_error("journal filename does not match its note identifier");
    }
    let checksum = draft_checksum(&record.draft)?;
    if checksum != record.checksum {
        return journal_error("journal checksum does not match its draft");
    }
    Ok(record)
}

fn validate_draft(draft: &EmergencyDraft) -> JournalResult<()> {
    if draft.note_id.as_str().trim().is_empty() {
        return journal_error("emergency draft is missing its note identifier");
    }
    if draft.notebook_id.trim().is_empty() {
        return journal_error(format!(
            "emergency draft {} is missing its notebook identifier",
            draft.note_id
        ));
    }
    crate::editor_document::validate_editor_state(&draft.editor_state).map_err(|error| {
        EmergencyJournalError::new(format!(
            "emergency draft {} contains an invalid editor document: {error}",
            draft.note_id
        ))
    })
}

fn draft_checksum(draft: &EmergencyDraft) -> JournalResult<String> {
    let mut writer = DigestWriter(Sha256::new());
    serde_json::to_writer(&mut writer, draft)
        .map_err(|error| format!("could not checksum emergency draft: {error}"))?;
    Ok(digest_to_hex(&writer.0.finalize()))
}

fn estimated_draft_bytes(draft: &EmergencyDraft) -> u64 {
    [
        draft.note_id.as_str().len(),
        draft.notebook_id.len(),
        draft.title.len(),
        draft.editor_state.len(),
        draft.tags.iter().map(String::len).sum::<usize>(),
    ]
    .into_iter()
    .sum::<usize>()
    .saturating_add(16 * 1024) as u64
}

fn digest_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

struct DraftPaths {
    final_path: PathBuf,
    temporary_path: PathBuf,
    previous_path: PathBuf,
}

fn draft_paths(directory: &Path, note_id: &NoteId) -> DraftPaths {
    let file_name = draft_file_name(note_id);
    DraftPaths {
        final_path: directory.join(&file_name),
        temporary_path: directory.join(format!("{file_name}.tmp")),
        previous_path: directory.join(format!("{file_name}.previous")),
    }
}

fn draft_file_name(note_id: &NoteId) -> String {
    let digest = Sha256::digest(note_id.as_str().as_bytes());
    format!("note-{}.json", digest_to_hex(&digest))
}

fn draft_candidate_base_name(name: &str) -> Option<&str> {
    let base = name
        .strip_suffix(".tmp")
        .or_else(|| name.strip_suffix(".previous"))
        .unwrap_or(name);
    let hash = base
        .strip_prefix("note-")?
        .strip_suffix(".json")?;
    (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(base)
}

fn journal_plaintext_namespace(
    data_directory: &Path,
) -> JournalResult<crate::security::DurablePlaintextNamespace> {
    crate::security::DurablePlaintextNamespace::establish(
        journal_directory(data_directory),
        data_directory.join(JOURNAL_OWNER_MARKER),
        JOURNAL_OWNER_IDENTITY,
    )
    .map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not establish emergency-journal plaintext authority: {error}"
        ))
    })
}

fn journal_plaintext_namespace_for_path(
    path: &Path,
) -> JournalResult<crate::security::DurablePlaintextNamespace> {
    let directory = path
        .ancestors()
        .find(|candidate| candidate.file_name().and_then(|name| name.to_str()) == Some(JOURNAL_DIRECTORY))
        .ok_or_else(|| {
            EmergencyJournalError::new("emergency-journal path escaped its durable namespace")
        })?;
    let data_directory = directory.parent().ok_or_else(|| {
        EmergencyJournalError::new("emergency-journal directory has no library parent")
    })?;
    journal_plaintext_namespace(data_directory)
}

fn journal_directory(data_directory: &Path) -> PathBuf {
    data_directory.join(JOURNAL_DIRECTORY)
}

fn quarantine_paths(
    directory: &Path,
    paths: &[PathBuf],
    reason: &str,
    warnings: &mut Vec<String>,
) {
    for path in paths {
        if let Err(error) = quarantine_file(directory, path, reason) {
            warnings.push(format!(
                "Could not quarantine emergency journal file {}: {error}",
                path.display()
            ));
        }
    }
}

fn quarantine_file(directory: &Path, source: &Path, reason: &str) -> JournalResult<PathBuf> {
    let quarantine = directory.join(JOURNAL_QUARANTINE_DIRECTORY);
    ensure_private_directory(&quarantine)?;
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("journal-record");
    let destination = available_quarantine_path(&quarantine, file_name, reason)?;
    let namespace = journal_plaintext_namespace_for_path(source)?;
    let mut owned = namespace.adopt_existing(source.to_path_buf()).map_err(|error| {
        EmergencyJournalError::new(format!(
            "could not resume emergency-journal quarantine ownership: {error}"
        ))
    })?;
    owned
        .rename_durable_namespace(destination.clone())
        .map_err(|error| {
            EmergencyJournalError::new(format!(
                "could not move {} to {}: {error}",
                source.display(),
                destination.display()
            ))
        })?;
    sync_directory(&quarantine)?;
    sync_directory(directory)?;
    Ok(destination)
}

fn available_quarantine_path(
    quarantine: &Path,
    file_name: &str,
    reason: &str,
) -> JournalResult<PathBuf> {
    let timestamp = unix_timestamp();
    for suffix in 0_u32..10_000 {
        let name = if suffix == 0 {
            format!("{file_name}.{timestamp}-{reason}")
        } else {
            format!("{file_name}.{timestamp}-{reason}-{suffix}")
        };
        let candidate = quarantine.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    journal_error(format!(
        "could not allocate a unique quarantine path for {file_name}"
    ))
}

fn ensure_private_directory(path: &Path) -> JournalResult<()> {
    fs::create_dir_all(path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }
    Ok(())
}

fn remove_file_if_present(path: &Path) -> JournalResult<bool> {
    privacy_remove_file_if_present(path)
}

fn sync_parent_directory(path: &Path) -> JournalResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> JournalResult<()> {
    let directory = File::open(path)
        .map_err(|error| format!("could not open {} for syncing: {error}", path.display()))?;
    match directory.sync_all() {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => Ok(()),
        Err(error) => journal_error(format!("could not sync {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rustnote-emergency-journal-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn protected_recovery_cleanup_fails_closed_when_plaintext_cannot_be_removed() {
        let root = temp_root("protected-cleanup-failure");
        let blocked = root.join("note-record.json");
        std::fs::create_dir_all(&blocked).expect("create directory at recovery-record path");

        let error = discard_protected_recovery_records(std::slice::from_ref(&blocked))
            .expect_err("a directory cannot be removed as a plaintext journal file");
        assert!(
            error.to_string().contains("could not securely discard"),
            "cleanup failure must propagate instead of becoming a warning"
        );
        assert!(blocked.exists(), "failed cleanup must leave evidence for retry");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_discards_corrupt_protected_records_before_quarantine() {
        let root = temp_root("protected-corrupt-prequarantine");
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("initialize test storage");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let owner = journal_plaintext_namespace(storage.data_directory())
            .expect("establish journal plaintext owner");
        let journal = owner.root().to_path_buf();
        ensure_private_directory(&journal).expect("create journal");
        let paths = draft_paths(&journal, &note.id);
        for path in [&paths.final_path, &paths.temporary_path, &paths.previous_path] {
            std::fs::write(path, b"corrupt protected plaintext journal")
                .expect("write corrupt protected record");
        }

        recover(&storage).expect("startup privacy recovery");
        for path in [&paths.final_path, &paths.temporary_path, &paths.previous_path] {
            assert!(
                !path.exists(),
                "protected journal candidate must be deleted before parsing/quarantine"
            );
        }
        assert!(
            !discard_quarantined_records_for_note(&journal, &note.id)
                .expect("inspect target quarantine"),
            "protected corrupt journal must never enter quarantine"
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_fails_closed_when_protected_root_record_cannot_be_removed() {
        let root = temp_root("protected-root-cleanup-failure");
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("initialize test storage");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let owner = journal_plaintext_namespace(storage.data_directory())
            .expect("establish journal plaintext owner");
        let journal = owner.root().to_path_buf();
        ensure_private_directory(&journal).expect("create journal");
        let paths = draft_paths(&journal, &note.id);
        std::fs::create_dir_all(&paths.final_path)
            .expect("create non-file at protected journal path");

        let error = recover(&storage)
            .expect_err("startup must stop when protected journal cleanup fails");
        assert!(
            error.to_string().contains("could not securely discard"),
            "protected root cleanup failure must propagate"
        );
        assert!(paths.final_path.exists(), "failed cleanup must remain retryable");

        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_removes_legacy_quarantine_for_protected_note() {
        let root = temp_root("protected-quarantine-startup");
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("initialize test storage");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let owner = journal_plaintext_namespace(storage.data_directory())
            .expect("establish journal plaintext owner");
        let journal = owner.root().to_path_buf();
        let quarantine = journal.join(JOURNAL_QUARANTINE_DIRECTORY);
        ensure_private_directory(&quarantine).expect("create quarantine");
        let residue = quarantine.join(format!(
            "{}.123456-corrupt",
            draft_file_name(&note.id)
        ));
        std::fs::write(&residue, b"legacy plaintext recovery residue")
            .expect("write legacy quarantine residue");

        recover(&storage).expect("startup privacy recovery");
        assert!(
            !residue.exists(),
            "startup must remove quarantined plaintext belonging to a protected note"
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quarantine_cleanup_is_scoped_to_the_target_note() {
        let root = temp_root("quarantine-scope");
        let owner = journal_plaintext_namespace(&root)
            .expect("establish journal plaintext owner");
        let journal = owner.root().to_path_buf();
        let quarantine = journal.join(JOURNAL_QUARANTINE_DIRECTORY);
        ensure_private_directory(&quarantine).expect("create quarantine");
        let target = NoteId::parse("note_target").expect("target note id");
        let other = NoteId::parse("note_other").expect("other note id");
        let target_path = quarantine.join(format!("{}.1-corrupt", draft_file_name(&target)));
        let other_path = quarantine.join(format!("{}.1-corrupt", draft_file_name(&other)));
        std::fs::write(&target_path, b"target plaintext").expect("write target");
        std::fs::write(&other_path, b"other plaintext").expect("write other");

        assert!(
            discard_quarantined_records_for_note(&journal, &target)
                .expect("discard target quarantine")
        );
        assert!(!target_path.exists(), "target quarantine must be removed");
        assert!(other_path.exists(), "unrelated quarantine must remain untouched");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn protected_recovery_cleanup_removes_known_plaintext_record() {
        let root = temp_root("protected-cleanup-success");
        let owner = journal_plaintext_namespace(&root)
            .expect("establish managed journal plaintext namespace");
        let journal = owner.root().to_path_buf();
        ensure_private_directory(&journal).expect("create managed journal root");
        let record = journal.join("note-record.json");
        std::fs::write(&record, b"plaintext recovery residue").expect("write recovery residue");

        discard_protected_recovery_records(std::slice::from_ref(&record))
            .expect("discard protected recovery residue");
        assert!(!record.exists(), "protected recovery residue must be removed");

        let _ = std::fs::remove_dir_all(root);
    }
}
