//! SQLite-backed local storage, split by responsibility.

use std::{
    collections::HashSet,
    env,
    error::Error,
    fmt::{self, Display, Formatter},
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    Connection, OpenFlags, ToSql,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef},
};

use crate::{
    domain::{AttachmentRecoveryReport, Note, NoteId, SourceId},
    editor_document::DocumentValidationError,
    platform::PlatformError,
    security::{
        LibraryPrivacyMutationGuard, LibraryPrivacyTransition, MultiNotePrivacyMutationGuard,
        NotePrivacyMutationGuard, NotePrivacyTransition, PrivacyCoordinator, VaultError,
    },
};

impl ToSql for NoteId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.as_str().to_sql()
    }
}

impl FromSql for NoteId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let value = String::column_result(value)?;
        NoteId::parse(value).map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

impl ToSql for SourceId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.as_str().to_sql()
    }
}

impl FromSql for SourceId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let value = String::column_result(value)?;
        SourceId::parse(value).map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

mod attachment_ingest;
mod capabilities;
mod attachments;
mod atomic_output;
mod backup_repository;
mod backup_support;
mod backups;
mod history;
mod initialization;
mod locations;
mod notebooks;
mod note_candidates;
mod note_hierarchy;
mod note_links;
mod notes;
mod portability;
mod plaintext_staging;
mod portable_bundle;
mod properties;
mod protection;
mod schema;
mod search_query;
mod settings;
mod sources;
mod tags;
mod transport;
pub(crate) mod vault_objects;

pub use locations::StorageLocations;
pub use settings::DEFAULT_TRASH_RETENTION_DAYS;
pub use tags::parse_tags;
pub use transport::{
    AttachmentEphemeralPlaintextGuard, AttachmentPlaintextIngressGuard,
    AttachmentTransportResource, AttachmentTransportSession, AttachmentTransportStore,
    PdfAnnotationRow,
};
pub(crate) use capabilities::{
    AttachmentFileCommandResource, AttachmentStore, EditorStore, ExportStore, LibraryStore,
    NoteExportResource, NoteStore, PrivacyOperationStore, PrivacyPublicationStore, RuntimeStore,
    SettingsStore, VaultStore,
};
pub(crate) use attachment_ingest::{
    AttachmentIngestIntent, PendingAttachmentIngestResource, PreparedAttachmentImport,
};
pub(crate) use attachments::{
    AttachmentImportProfile, AuthorizedDerivedImageAttachment, ProfiledAttachmentImport,
};
pub(crate) use history::{RevisionRestoreAuthorization, RevisionSnapshotResource};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

const SQLITE_BUSY_PRIMARY_CODE: i32 = 5;
const SQLITE_LOCKED_PRIMARY_CODE: i32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RevisionMode {
    Autosave,
    Checkpoint,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoteSaveScope(u8);

impl NoteSaveScope {
    const TITLE_BIT: u8 = 1 << 0;
    const TAGS_BIT: u8 = 1 << 1;
    const STATE_BIT: u8 = 1 << 2;
    const DOCUMENT_BIT: u8 = 1 << 3;

    pub const NONE: Self = Self(0);
    pub const TITLE: Self = Self(Self::TITLE_BIT);
    pub const TAGS: Self = Self(Self::TAGS_BIT);
    pub const STATE: Self = Self(Self::STATE_BIT);
    pub const DOCUMENT: Self = Self(Self::DOCUMENT_BIT);
    pub const ALL: Self = Self(
        Self::TITLE_BIT | Self::TAGS_BIT | Self::STATE_BIT | Self::DOCUMENT_BIT,
    );

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn touches_title(self) -> bool {
        self.0 & Self::TITLE_BIT != 0
    }

    pub const fn touches_tags(self) -> bool {
        self.0 & Self::TAGS_BIT != 0
    }

    pub const fn touches_state(self) -> bool {
        self.0 & Self::STATE_BIT != 0
    }

    pub const fn touches_document(self) -> bool {
        self.0 & Self::DOCUMENT_BIT != 0
    }
}

struct AttachmentOperation {
    id: String,
    operation: String,
    attachment_id: String,
    note_id: NoteId,
    attachment_json: String,
    staging_relative_path: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug)]
pub enum StorageError {
    Database(rusqlite::Error),
    Io(io::Error),
    Serialization(serde_json::Error),
    Archive(zip::result::ZipError),
    InvalidBackup(String),
    Validation(String),
    DocumentValidation(DocumentValidationError),
    Platform(PlatformError),
    Vault(VaultError),
    Integrity(String),
    LibraryInUse(PathBuf),
    BackupInUse(PathBuf),
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::Io(error) => write!(formatter, "file error: {error}"),
            Self::Serialization(error) => write!(formatter, "serialization error: {error}"),
            Self::Archive(error) => write!(formatter, "archive error: {error}"),
            Self::InvalidBackup(message) => write!(formatter, "invalid backup: {message}"),
            Self::Validation(message) => write!(formatter, "validation error: {message}"),
            Self::DocumentValidation(error) => write!(formatter, "validation error: {error}"),
            Self::Platform(error) => std::fmt::Display::fmt(error, formatter),
            Self::Vault(error) => write!(formatter, "encryption error: {error}"),
            Self::Integrity(message) => write!(formatter, "integrity error: {message}"),
            Self::LibraryInUse(path) => write!(
                formatter,
                "Rustnote is already using this library. Close the other Rustnote window and try again. Library: {}",
                path.display()
            ),
            Self::BackupInUse(path) => write!(
                formatter,
                "Another Rustnote library is already using this automatic backup folder. Choose a different backup folder or close the other Rustnote window. Backup folder: {}",
                path.display()
            ),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Archive(error) => Some(error),
            Self::DocumentValidation(error) => Some(error),
            Self::Platform(error) => Some(error),
            Self::Vault(error) => Some(error),
            Self::InvalidBackup(_)
            | Self::Validation(_)
            | Self::Integrity(_)
            | Self::LibraryInUse(_)
            | Self::BackupInUse(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

impl From<io::Error> for StorageError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}

impl From<PlatformError> for StorageError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

impl From<VaultError> for StorageError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

impl From<DocumentValidationError> for StorageError {
    fn from(value: DocumentValidationError) -> Self {
        Self::DocumentValidation(value)
    }
}

impl From<zip::result::ZipError> for StorageError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Archive(value)
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug)]
pub(crate) struct NoteSaveFailure {
    pub(crate) note: Box<Note>,
    pub(crate) error: StorageError,
}

impl NoteSaveFailure {
    pub(crate) fn new(note: Note, error: StorageError) -> Self {
        Self {
            note: Box::new(note),
            error,
        }
    }

    pub(crate) fn into_parts(self) -> (Note, StorageError) {
        (*self.note, self.error)
    }
}

pub(crate) type OwnedNoteSaveResult = Result<(Note, bool), NoteSaveFailure>;


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupRepositoryRecoveryReport {
    pub quarantined_manifests: usize,
    pub quarantined_snapshots: usize,
    pub quarantined_other_artifacts: usize,
    pub purged_quarantine_artifacts: usize,
    pub details: Vec<String>,
}

impl BackupRepositoryRecoveryReport {
    pub fn quarantined_artifact_count(&self) -> usize {
        self.quarantined_manifests
            .saturating_add(self.quarantined_snapshots)
            .saturating_add(self.quarantined_other_artifacts)
    }

    pub fn found_corruption(&self) -> bool {
        self.quarantined_artifact_count() > 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeferredMaintenanceReport {
    pub scheduled_trash_deletions: usize,
    pub purged_expired_notes: usize,
    pub pruned_revisions: usize,
    pub attachment_recovery: AttachmentRecoveryReport,
    pub backup_recovery: BackupRepositoryRecoveryReport,
}

impl DeferredMaintenanceReport {
    pub fn has_activity(&self) -> bool {
        self.scheduled_trash_deletions > 0
            || self.purged_expired_notes > 0
            || self.pruned_revisions > 0
            || self.attachment_recovery != AttachmentRecoveryReport::default()
    }
}

#[derive(Clone, Copy)]
enum DirectoryLockKind {
    Library,
    Backup,
}

struct DirectoryLock {
    path: PathBuf,
    _connection: Mutex<Connection>,
}

impl fmt::Debug for DirectoryLock {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryLock")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl DirectoryLock {
    fn acquire(
        directory: &Path,
        file_name: &str,
        kind: DirectoryLockKind,
    ) -> StorageResult<Self> {
        let path = directory.join(file_name);
        let connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::ZERO)?;
        if let Err(error) = connection.execute_batch(
            "PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;
             BEGIN EXCLUSIVE;",
        ) {
            if matches!(
                &error,
                rusqlite::Error::SqliteFailure(details, _)
                    if matches!(
                        details.extended_code & 0xff,
                        SQLITE_BUSY_PRIMARY_CODE | SQLITE_LOCKED_PRIMARY_CODE
                    )
            ) {
                return Err(match kind {
                    DirectoryLockKind::Library => {
                        StorageError::LibraryInUse(directory.to_path_buf())
                    }
                    DirectoryLockKind::Backup => {
                        StorageError::BackupInUse(directory.to_path_buf())
                    }
                });
            }
            return Err(StorageError::Database(error));
        }
        Ok(Self {
            path,
            _connection: Mutex::new(connection),
        })
    }
}

struct DirectoryOwnership {
    path: PathBuf,
    _role_lock: DirectoryLock,
    _common_lock: DirectoryLock,
}

impl fmt::Debug for DirectoryOwnership {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryOwnership")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl DirectoryOwnership {
    fn acquire(directory: &Path, kind: DirectoryLockKind) -> StorageResult<Arc<Self>> {
        let role_lock_file = match kind {
            // Retain the established library lock name so an older running
            // Rustnote build and this build cannot open the same library.
            DirectoryLockKind::Library => ".rustnote-instance.lock",
            DirectoryLockKind::Backup => ".rustnote-backup.lock",
        };
        let role_lock = DirectoryLock::acquire(directory, role_lock_file, kind)?;
        // The shared role lock prevents one new process from using a directory
        // as a library while another uses it as an automatic backup repository.
        let common_lock =
            DirectoryLock::acquire(directory, ".rustnote-directory.lock", kind)?;
        Ok(Arc::new(Self {
            path: directory.to_path_buf(),
            _role_lock: role_lock,
            _common_lock: common_lock,
        }))
    }
}

#[derive(Clone, Debug)]
pub struct Storage {
    database_path: PathBuf,
    data_directory: PathBuf,
    attachments_directory: PathBuf,
    automatic_backup_directory: PathBuf,
    library_id: String,
    privacy_coordinator: PrivacyCoordinator,
    verified_attachment_blobs: Arc<Mutex<HashSet<(String, u64, u128)>>>,
    _data_directory_lock: Arc<DirectoryOwnership>,
    _backup_directory_lock: Arc<DirectoryOwnership>,
}

impl PartialEq for Storage {
    fn eq(&self, other: &Self) -> bool {
        self.database_path == other.database_path
    }
}

impl Eq for Storage {}

impl Storage {
    pub fn open() -> StorageResult<Self> {
        let locations = locations::resolved_storage_locations()?;
        Self::at_with_backup(locations.data_directory, locations.backup_directory)
    }
    pub fn expected_data_directory() -> PathBuf {
        locations::resolved_storage_locations()
            .map(|locations| locations.data_directory)
            .unwrap_or_else(|_| locations::default_storage_locations().data_directory)
    }
    pub fn configured_locations() -> StorageResult<StorageLocations> {
        locations::configured_storage_locations()
    }
    pub fn default_locations() -> StorageLocations {
        locations::default_storage_locations()
    }
    pub fn save_configured_locations(
        data_directory: PathBuf,
        backup_directory: PathBuf,
    ) -> StorageResult<StorageLocations> {
        locations::save_storage_locations(data_directory, backup_directory)
    }
    pub fn reset_configured_locations() -> StorageResult<()> {
        locations::reset_storage_locations()
    }
    pub fn at(data_directory: PathBuf) -> StorageResult<Self> {
        let directory_name = data_directory
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("Rustnote");
        let automatic_backup_directory = data_directory
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{directory_name} Backups"));
        Self::at_with_backup(data_directory, automatic_backup_directory)
    }
    pub fn at_with_backup(
        data_directory: PathBuf,
        automatic_backup_directory: PathBuf,
    ) -> StorageResult<Self> {
        locations::validate_storage_location_relationship(
            &data_directory,
            &automatic_backup_directory,
        )?;
        fs::create_dir_all(&data_directory)?;
        let data_directory = fs::canonicalize(data_directory)?;
        let data_directory_lock =
            DirectoryOwnership::acquire(&data_directory, DirectoryLockKind::Library)?;
        PrivacyCoordinator::cleanup_stale_plaintext().map_err(|error| {
            StorageError::Integrity(format!(
                "Vault temporary plaintext cleanup failed before library open: {error}"
            ))
        })?;
        let library_id = locations::ensure_library_identity(&data_directory)?;

        // A remembered repository with a pending Vault privacy reset may live on
        // removable storage. Never manufacture a missing mount/path and then
        // falsely treat the empty replacement as proof that historical plaintext
        // was invalidated. The real repository must already be present as a real
        // directory before its obligation can be discharged.
        let pending_backup_reset = locations::require_pending_backup_location_available(
            &library_id,
            &data_directory,
            &automatic_backup_directory,
        )?;
        if !pending_backup_reset {
            fs::create_dir_all(&automatic_backup_directory)?;
        }
        let automatic_backup_directory = fs::canonicalize(automatic_backup_directory)?;
        locations::validate_disjoint_directories(
            &data_directory,
            &automatic_backup_directory,
        )?;
        let backup_directory_lock =
            DirectoryOwnership::acquire(&automatic_backup_directory, DirectoryLockKind::Backup)?;
        let storage_locations = StorageLocations {
            data_directory: data_directory.clone(),
            backup_directory: automatic_backup_directory.clone(),
        };

        let attachments_directory = data_directory.join("attachments");
        fs::create_dir_all(&attachments_directory)?;

        let storage = Self {
            database_path: data_directory.join("rustnote.sqlite3"),
            data_directory,
            attachments_directory,
            automatic_backup_directory,
            library_id,
            privacy_coordinator: PrivacyCoordinator::new(),
            verified_attachment_blobs: Arc::new(Mutex::new(HashSet::new())),
            _data_directory_lock: data_directory_lock,
            _backup_directory_lock: backup_directory_lock,
        };
        // Reject a backup folder already associated with another known library
        // before adopting or publishing its external durable repository namespace.
        // A failed open must never claim another library's backup authority.
        locations::validate_storage_location_ownership(storage.library_id(), &storage_locations)?;
        if !storage.discharge_current_backup_privacy_obligation_unlocked()? {
            storage.ensure_backup_repository_identity_unlocked()?;
        }
        // Persist history only after both global path ownership and repository
        // ownership are positively established, but before initialization can
        // run E3 cleanup.
        locations::remember_storage_location(storage.library_id(), &storage_locations)?;
        storage.initialize()?;
        Ok(storage)
    }
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }
    pub fn attachments_directory(&self) -> &Path {
        &self.attachments_directory
    }
    pub fn automatic_backup_directory(&self) -> &Path {
        &self.automatic_backup_directory
    }
    pub(super) fn library_id(&self) -> &str {
        &self.library_id
    }
    pub fn default_archive_path(&self) -> PathBuf {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Documents"))
            .unwrap_or_else(env::temp_dir)
            .join("rustnote-complete.rustnote.zip")
    }
    pub(super) fn lock_operations(&self) -> StorageResult<MutexGuard<'_, ()>> {
        self.privacy_coordinator
            .lock_storage()
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_protected_note_restore(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<crate::security::ProtectedNoteRestoreGuard<'_>>> {
        self.privacy_coordinator
            .begin_protected_note_restore(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_retained_protected_note_mutation(
        &self,
        access: &crate::security::VaultNoteAccess,
    ) -> StorageResult<Option<crate::security::RetainedProtectedNoteMutationGuard<'_>>> {
        self.privacy_coordinator
            .begin_retained_protected_note_mutation(access)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_protected_note_mutation(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<crate::security::ProtectedNoteMutationGuard<'_>>> {
        self.privacy_coordinator
            .begin_protected_note_mutation(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_protected_notes_mutation(
        &self,
        note_ids: &[NoteId],
    ) -> StorageResult<Option<crate::security::ProtectedNotesMutationGuard<'_>>> {
        self.privacy_coordinator
            .begin_protected_notes_mutation(note_ids)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_note_privacy_transition(
        &self,
        note_id: &NoteId,
        transition: NotePrivacyTransition,
    ) -> StorageResult<NotePrivacyMutationGuard<'_>> {
        self.privacy_coordinator
            .begin_note_transition(note_id, transition)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_multi_note_privacy_transition(
        &self,
        note_ids: &[NoteId],
        transition: NotePrivacyTransition,
    ) -> StorageResult<MultiNotePrivacyMutationGuard<'_>> {
        self.privacy_coordinator
            .begin_note_transitions(note_ids, transition)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_library_privacy_transition(
        &self,
        transition: LibraryPrivacyTransition,
    ) -> StorageResult<LibraryPrivacyMutationGuard<'_>> {
        self.privacy_coordinator
            .begin_library_transition(transition)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn acquire_plaintext_operation_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<crate::security::PlaintextOperationPermit> {
        self.privacy_coordinator
            .acquire_plaintext_operation(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn acquire_multi_note_plaintext_operation(
        &self,
        note_ids: &[NoteId],
    ) -> StorageResult<crate::security::MultiNotePlaintextOperationPermit> {
        self.privacy_coordinator
            .acquire_multi_note_plaintext_operation(note_ids)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_plaintext_operation_mutation<'a>(
        &'a self,
        permit: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Option<crate::security::PlaintextOperationMutationGuard<'a>>> {
        self.privacy_coordinator
            .begin_plaintext_operation_mutation(permit)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_plaintext_derivative_cleanup(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<crate::security::PlaintextDerivativeCleanupGuard<'_>> {
        self.privacy_coordinator
            .begin_plaintext_derivative_cleanup(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn begin_library_plaintext_derivative_cleanup(
        &self,
    ) -> StorageResult<crate::security::PlaintextDerivativeCleanupGuard<'_>> {
        self.privacy_coordinator
            .begin_library_plaintext_derivative_cleanup()
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }
    fn configure_connection(connection: &Connection) -> StorageResult<()> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA wal_autocheckpoint = 1000;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }

    pub(super) fn connection(&self) -> StorageResult<Connection> {
        let connection = Connection::open(&self.database_path)?;
        Self::configure_connection(&connection)?;
        Ok(connection)
    }

    pub(super) fn read_only_connection(&self) -> StorageResult<Connection> {
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA query_only = ON;")?;
        Ok(connection)
    }

    /// Force all committed WAL frames through SQLite's full checkpoint path.
    /// Routine autosaves use `synchronous = NORMAL`; explicit save and close
    /// barriers call this method when the user expects stronger durability.
    pub fn durability_checkpoint(&self) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        let connection = self.connection()?;
        connection.execute_batch("PRAGMA synchronous = FULL;")?;
        let (busy, log_frames, checkpointed_frames): (i64, i64, i64) = connection.query_row(
            "PRAGMA wal_checkpoint(FULL)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if busy != 0 {
            return Err(StorageError::Integrity(
                "SQLite could not complete the full WAL checkpoint because the database remained busy"
                    .to_string(),
            ));
        }
        if log_frames >= 0 && checkpointed_frames < log_frames {
            return Err(StorageError::Integrity(format!(
                "SQLite checkpointed {checkpointed_frames} of {log_frames} WAL frame(s)"
            )));
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "internal-test-support"))]
#[doc(hidden)]
pub fn lock_protected_note_for_test(
    storage: &Storage,
    note_id: &NoteId,
) -> StorageResult<Note> {
    storage.lock_protected_note(note_id)
}

#[cfg(any(test, feature = "internal-test-support"))]
#[doc(hidden)]
pub fn protected_note_accepts_vault_reads_for_test(
    storage: &Storage,
    note_id: &NoteId,
) -> StorageResult<bool> {
    storage
        .privacy_coordinator
        .contains_note_key(note_id)
        .map_err(|message| StorageError::Integrity(message.to_string()))
}


#[cfg(any(test, feature = "internal-test-support"))]
#[doc(hidden)]
pub fn protected_transport_test_fixture(
    label: &str,
    file_name: &str,
    mime_type: Option<&str>,
    payload: &[u8],
) -> (
    Storage,
    AttachmentTransportStore,
    AttachmentTransportResource,
    NoteId,
    String,
    PathBuf,
) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = env::temp_dir().join(format!(
        "rustnote-transport-fixture-{label}-{}-{unique}",
        std::process::id()
    ));
    let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
        .expect("test storage should initialize");
    let note = storage.create_note(None).expect("create note");
    let source = root.join(file_name);
    fs::write(&source, payload).expect("write transport fixture");
    let attachment = storage
        .add_attachment_file_as(&note.id, file_name, mime_type, &source)
        .expect("add transport fixture attachment");
    storage
        .protect_note_with_password_and_public_label(
            &note.id,
            b"correct horse battery staple",
            "Locked note",
        )
        .expect("protect transport fixture note");
    let transport = storage.transport_store();
    let resource = transport
        .attachment_resource(&attachment.id)
        .expect("protected transport read")
        .expect("protected transport resource");
    (
        storage,
        transport,
        resource,
        note.id,
        attachment.id,
        root,
    )
}

pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub(super) fn new_id(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{timestamp:x}_{counter:x}")
}

fn new_uuid_v4_string() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

pub(super) fn new_note_id() -> NoteId {
    NoteId::parse(new_uuid_v4_string()).expect("generated UUID note identifier must be valid")
}

pub(super) fn new_source_id() -> SourceId {
    SourceId::parse(new_uuid_v4_string()).expect("generated UUID source identifier must be valid")
}

pub(super) fn new_citation_id() -> String {
    new_uuid_v4_string()
}
