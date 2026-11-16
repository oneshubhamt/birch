use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, TransactionBehavior, backup::Backup as SqliteBackup,
};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::build_info::FULL_BUILD_IDENTIFIER;
use crate::domain::{ArchiveEntry, ArchiveManifest, CompleteArchiveSummary, NoteId};
use crate::editor_projection::derive_editor_projections;

use super::atomic_output::{AtomicDirectoryOutput, AtomicFileOutput};
use super::attachments::{
    COPY_BUFFER_SIZE, copy_file_atomically, digest_to_hex, sha256_file, sync_parent_directory,
    validate_attachment_blob_identity,
};
use super::backup_support::{
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_DATABASE_BYTES, MAX_EXPANDED_BYTES,
    MAX_MANIFEST_BYTES, escape_markdown_link_label, extract_verified_zip_entry,
    read_zip_entry_bounded, sanitize_component, unique_directory, validate_manifest_entry,
    validate_zip_structure,
};
use super::initialization::is_note_runtime_index;
use super::schema::{BASE_SCHEMA, CURRENT_SCHEMA_VERSION, canonicalize_schema_sql};
use super::tags::split_tags;
use super::{Storage, StorageError, StorageResult, new_id, unix_timestamp};

pub(super) const COMPLETE_ARCHIVE_VERSION: u32 = 2;
pub(super) const AUTO_BACKUP_INTERVAL_SECONDS: i64 = 24 * 60 * 60;

pub(super) const DATABASE_ARCHIVE_PATH: &str = "library.sqlite3";
const DATABASE_BACKUP_PAGES_PER_STEP: i32 = 512;
const DATABASE_BACKUP_PAUSE: Duration = Duration::from_millis(5);
const UNRESOLVED_ATTACHMENT_WORK_SQL: &str = "SELECT
        (SELECT COUNT(*) FROM attachment_operations)
        + (SELECT COUNT(*)
           FROM attachment_ingest_jobs AS jobs
           WHERE NOT EXISTS (
               SELECT 1 FROM attachments AS finalized
               WHERE finalized.id = jobs.attachment_id
                 AND finalized.note_id = jobs.note_id
           ))
        + (SELECT COUNT(*) FROM managed_plaintext_staging)";

struct LimitedWriter<'a, W> {
    inner: &'a mut W,
    remaining: u64,
}

impl<'a, W> LimitedWriter<'a, W> {
    fn new(inner: &'a mut W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<W: Write> Write for LimitedWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive manifest exceeds Rustnote's supported size limit",
            ));
        }
        let written = self.inner.write(buffer)?;
        self.remaining = self.remaining.saturating_sub(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(super) struct StagedCompleteArchive {
    pub(super) directory: PathBuf,
    pub(super) database_path: PathBuf,
    pub(super) summary: CompleteArchiveSummary,
}

#[derive(Debug)]
struct PreparedRestoreBlob {
    sha256: String,
    path: PathBuf,
    cleanup_owner: Option<crate::security::OwnedPlaintext>,
}

fn allocate_restore_stage_directory(prefix: &str) -> StorageResult<PathBuf> {
    let root = crate::native::staging::archive_restore_process_root()
        .map_err(|error| StorageError::Integrity(error.to_string()))?;
    let directory = root.join(format!("{prefix}-{}", new_id("archive")));
    fs::create_dir(&directory)?;
    Ok(directory)
}

pub(super) fn discard_restore_stage_directory(path: &Path) -> StorageResult<()> {
    crate::native::privacy::remove_tree(path).map_err(|error| {
        StorageError::Integrity(format!(
            "sensitive archive restore staging cleanup failed and remains process-owned at {}: {error}",
            path.display()
        ))
    })
}

fn finish_with_restore_stage_cleanup<T>(path: &Path, result: StorageResult<T>) -> StorageResult<T> {
    match (result, discard_restore_stage_directory(path)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(error), Err(cleanup)) => Err(StorageError::Integrity(format!("{error}; {cleanup}"))),
    }
}

#[derive(Debug)]
struct MarkdownExportRecord {
    id: NoteId,
    title: String,
    editor_state: String,
    tags: Vec<String>,
    pinned: bool,
    archived: bool,
    trashed: bool,
    purge_after: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl MarkdownExportRecord {
    fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }
}

#[derive(Debug)]
struct MarkdownAttachmentRecord {
    id: String,
    file_name: String,
    byte_size: i64,
    sha256: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MarkdownExportCounts {
    notes: usize,
    attachments: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkdownNoteExportScope {
    note_id: NoteId,
}

impl MarkdownNoteExportScope {
    fn new(note_id: NoteId) -> Self {
        Self { note_id }
    }

    fn directory_name(&self, now: i64) -> String {
        format!("Rustnote-Note-Markdown-{now}")
    }

    fn expected_counts(&self, connection: &Connection) -> StorageResult<MarkdownExportCounts> {
        let notes: i64 = connection.query_row(
            "SELECT COUNT(*) FROM notes WHERE id = ?1",
            [&self.note_id],
            |row| row.get(0),
        )?;
        let attachments: i64 = connection.query_row(
            "SELECT COUNT(*) FROM attachments WHERE note_id = ?1",
            [&self.note_id],
            |row| row.get(0),
        )?;
        Ok(MarkdownExportCounts {
            notes: usize::try_from(notes).map_err(|_| {
                StorageError::Integrity("note count exceeds this platform".to_string())
            })?,
            attachments: usize::try_from(attachments).map_err(|_| {
                StorageError::Integrity("attachment count exceeds this platform".to_string())
            })?,
        })
    }

    fn ensure_unprotected(&self, connection: &Connection) -> StorageResult<()> {
        if super::protection::protected_note_exists(connection, &self.note_id)? {
            return Err(StorageError::Validation(
                "protected notes cannot be included in plaintext Markdown exports; remove protection first"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

fn is_markdown_attachment_file(path: &Path) -> bool {
    let Some(directory) = path.parent() else {
        return false;
    };
    let Some(stem) = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".attachments"))
    else {
        return false;
    };
    let Some(note_directory) = directory.parent() else {
        return false;
    };
    note_directory.join(format!("{stem}.md")).is_file()
}

fn verify_markdown_export_directory(
    root: &Path,
    expected: MarkdownExportCounts,
) -> StorageResult<MarkdownExportCounts> {
    let mut found = MarkdownExportCounts::default();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(StorageError::Integrity(format!(
                    "Markdown export contains a symbolic link: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                return Err(StorageError::Integrity(format!(
                    "Markdown export contains an unsupported entry: {}",
                    path.display()
                )));
            }
            if is_markdown_attachment_file(&path) {
                found.attachments += 1;
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
                found.notes += 1;
            } else {
                return Err(StorageError::Integrity(format!(
                    "Markdown export contains an unexpected file: {}",
                    path.display()
                )));
            }
        }
    }
    if found != expected {
        return Err(StorageError::Integrity(format!(
            "Markdown export verification count mismatch: expected {expected:?}, found {found:?}"
        )));
    }
    Ok(found)
}

fn markdown_export_record_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarkdownExportRecord> {
    let tag_names: String = row.get(9)?;
    Ok(MarkdownExportRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        editor_state: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        archived: row.get::<_, i64>(4)? != 0,
        trashed: row.get::<_, i64>(5)? != 0,
        purge_after: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        tags: split_tags(&tag_names),
    })
}

fn markdown_attachment_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarkdownAttachmentRecord> {
    Ok(MarkdownAttachmentRecord {
        id: row.get(0)?,
        file_name: row.get(1)?,
        byte_size: row.get(2)?,
        sha256: row.get(3)?,
    })
}

fn stream_file_to_archive(
    archive: &mut ZipWriter<File>,
    source: &Path,
) -> StorageResult<(String, u64)> {
    let mut reader = BufReader::with_capacity(COPY_BUFFER_SIZE, File::open(source)?);
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        written = written
            .checked_add(read as u64)
            .ok_or_else(|| StorageError::Integrity("archive entry size overflow".to_string()))?;
        hasher.update(&buffer[..read]);
        archive.write_all(&buffer[..read])?;
    }
    Ok((digest_to_hex(&hasher.finalize()), written))
}

fn database_schema_signature(connection: &Connection) -> StorageResult<String> {
    // Runtime indexes are rebuildable query-plan aids, not archive invariants.
    // Use the same authoritative index list that startup/restore installs so a
    // newly added runtime aid cannot silently invalidate backup compatibility.
    let mut statement = connection.prepare(
        "SELECT type, name, tbl_name, COALESCE(sql, '')
         FROM sqlite_schema
         WHERE name NOT GLOB 'sqlite_*'
         ORDER BY type, name, tbl_name",
    )?;
    let mut rows = statement.query([])?;
    let mut hasher = Sha256::new();
    while let Some(row) = rows.next()? {
        let object_type: String = row.get(0)?;
        let name: String = row.get(1)?;
        if object_type == "index" && is_note_runtime_index(&name) {
            continue;
        }

        let values = [
            object_type,
            name,
            row.get::<_, String>(2)?,
            canonicalize_schema_sql(&row.get::<_, String>(3)?),
        ];
        for value in values {
            let length = u64::try_from(value.len()).map_err(|_| {
                StorageError::Integrity("database schema value is too large".to_string())
            })?;
            hasher.update(length.to_le_bytes());
            hasher.update(value.as_bytes());
        }
    }
    Ok(digest_to_hex(&hasher.finalize()))
}

fn expected_schema_signature() -> StorageResult<String> {
    let reference = Connection::open_in_memory()?;
    reference.execute_batch(BASE_SCHEMA)?;
    database_schema_signature(&reference)
}

fn remove_database_files(database_path: &Path) -> StorageResult<()> {
    for suffix in ["", "-journal", "-shm", "-wal"] {
        let path = if suffix.is_empty() {
            database_path.to_path_buf()
        } else {
            let mut value = database_path.as_os_str().to_os_string();
            value.push(suffix);
            PathBuf::from(value)
        };
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    Ok(())
}

impl Storage {
    fn ensure_no_pending_vault_operations_for_snapshot(&self) -> StorageResult<()> {
        let pending: i64 =
            self.connection()?
                .query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                    row.get(0)
                })?;
        if pending != 0 {
            return Err(StorageError::Integrity(format!(
                "cannot create a persistent snapshot while {pending} vault operation(s) require recovery"
            )));
        }
        Ok(())
    }

    fn create_restore_safety_backup_unlocked(&self, reason: &str) -> StorageResult<()> {
        self.create_safety_repository_snapshot_unlocked(reason)
            .map(|_| ())
    }

    pub fn export_complete_archive(&self, destination: &Path) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        self.ensure_export_destination_is_external(destination)?;
        self.export_complete_archive_unlocked(destination)
    }
    pub(super) fn create_database_snapshot(&self, destination: &Path) -> StorageResult<()> {
        // Every persistent database snapshot (automatic/safety backup, complete
        // archive, restore rollback) crosses this boundary. Fail closed while a
        // durable vault transition is unfinished so partially scrubbed state can
        // never be preserved into another artifact. Callers hold operation_lock.
        self.ensure_no_pending_vault_operations_for_snapshot()?;
        if crate::security::managed_path_is_present(destination)? {
            return Err(StorageError::Integrity(
                "database snapshot destination already exists".to_string(),
            ));
        }
        let snapshot_result = (|| -> StorageResult<()> {
            let source = Connection::open_with_flags(
                &self.database_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            source.busy_timeout(Duration::from_secs(5))?;
            let mut target = Connection::open(destination)?;
            {
                let backup = SqliteBackup::new(&source, &mut target)?;
                backup.run_to_completion(
                    DATABASE_BACKUP_PAGES_PER_STEP,
                    DATABASE_BACKUP_PAUSE,
                    None,
                )?;
            }
            // Durable orphan-quarantine owners are valid steady-state live-library
            // cleanup authority, but quarantine itself is deliberately excluded
            // from archives. Strip only those settled owner rows from the snapshot
            // copy; all other operational/plaintext ownership rows remain and are
            // rejected by validate_snapshot_database below.
            super::plaintext_staging::strip_snapshot_excluded_quarantine_ownership(&mut target)?;
            let journal_mode: String =
                target.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
            if !journal_mode.eq_ignore_ascii_case("delete") {
                return Err(StorageError::Integrity(format!(
                    "database snapshot could not leave WAL mode: {journal_mode}"
                )));
            }
            self.validate_snapshot_database(&target)?;
            drop(target);
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(destination)?
                .sync_all()?;
            sync_parent_directory(destination)
        })();
        if snapshot_result.is_err() {
            let _ = remove_database_files(destination);
        }
        snapshot_result
    }

    pub(super) fn open_snapshot_database(path: &Path) -> StorageResult<Connection> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }

    pub(super) fn validate_snapshot_database(&self, connection: &Connection) -> StorageResult<()> {
        let schema_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version != CURRENT_SCHEMA_VERSION {
            return Err(StorageError::InvalidBackup(format!(
                "archive database schema {schema_version} does not match Rustnote schema {CURRENT_SCHEMA_VERSION}"
            )));
        }
        self.verify_integrity_with_connection(connection)?;
        if database_schema_signature(connection)? != expected_schema_signature()? {
            return Err(StorageError::InvalidBackup(
                "archive database schema does not match this Rustnote build".to_string(),
            ));
        }
        // A request-scoped ingest bridge may intentionally remain after its blob and
        // attachment row are fully materialized, until the saved pending document
        // anchor is replaced by the canonical attachment node. Such a state is
        // self-contained in the archive and can be recovered after restore; only
        // ingests without a finalized attachment still represent unresolved bytes.
        let pending_attachment_work: i64 =
            connection.query_row(UNRESOLVED_ATTACHMENT_WORK_SQL, [], |row| row.get(0))?;
        if pending_attachment_work != 0 {
            return Err(StorageError::Integrity(format!(
                "archive database contains {pending_attachment_work} unresolved attachment operation(s) or ingest(s)"
            )));
        }
        let pending_vault_operations: i64 =
            connection.query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                row.get(0)
            })?;
        if pending_vault_operations != 0 {
            return Err(StorageError::Integrity(format!(
                "archive database contains {pending_vault_operations} unfinished vault operation(s)"
            )));
        }
        Ok(())
    }

    pub(super) fn for_each_archive_blob<F>(
        &self,
        connection: &Connection,
        mut visit: F,
    ) -> StorageResult<usize>
    where
        F: FnMut(&str, &str, u64) -> StorageResult<()>,
    {
        let mut statement = connection.prepare(
            "SELECT sha256, byte_size
             FROM (
                 SELECT sha256, byte_size FROM attachments
                 UNION ALL
                 SELECT sha256, byte_size FROM revision_attachment_refs
                 UNION ALL
                 SELECT sha256, byte_size FROM vault_blob_refs
             )
             ORDER BY sha256, byte_size",
        )?;
        let mut rows = statement.query([])?;
        let mut previous: Option<(String, u64)> = None;
        let mut count = 0_usize;
        while let Some(row) = rows.next()? {
            let stored_sha256: String = row.get(0)?;
            let byte_size_value: i64 = row.get(1)?;
            let (relative_path, sha256, byte_size) = validate_attachment_blob_identity(
                &format!("attachment blob {stored_sha256}"),
                &stored_sha256,
                byte_size_value,
            )?;
            if let Some((previous_hash, previous_size)) = &previous
                && previous_hash == &sha256
            {
                if *previous_size != byte_size {
                    return Err(StorageError::Integrity(format!(
                        "attachment blob {sha256} has conflicting metadata"
                    )));
                }
                continue;
            }
            visit(&sha256, &relative_path, byte_size)?;
            previous = Some((sha256, byte_size));
            count = count.checked_add(1).ok_or_else(|| {
                StorageError::Integrity("attachment blob count overflow".to_string())
            })?;
        }
        Ok(count)
    }

    pub(super) fn complete_archive_counts(
        &self,
        connection: &Connection,
    ) -> StorageResult<(usize, usize, usize, usize)> {
        let counts = connection.query_row(
            "SELECT
                (SELECT COUNT(*) FROM notebooks),
                (SELECT COUNT(*) FROM notes),
                (SELECT COUNT(*) FROM attachments),
                (SELECT COUNT(*) FROM note_revisions)",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )?;
        Ok((
            usize::try_from(counts.0).map_err(|_| {
                StorageError::Integrity("notebook count exceeds this platform".to_string())
            })?,
            usize::try_from(counts.1).map_err(|_| {
                StorageError::Integrity("note count exceeds this platform".to_string())
            })?,
            usize::try_from(counts.2).map_err(|_| {
                StorageError::Integrity("attachment count exceeds this platform".to_string())
            })?,
            usize::try_from(counts.3).map_err(|_| {
                StorageError::Integrity("revision count exceeds this platform".to_string())
            })?,
        ))
    }

    pub(super) fn export_complete_archive_unlocked(&self, destination: &Path) -> StorageResult<()> {
        self.verify_integrity()?;
        // Finalized request-scoped ingests are durable recovery bridges, not pending
        // file operations. The archive contains the note anchor, attachment row,
        // blob, and bridge needed to finish canonical insertion after restore.
        let unresolved_operations: i64 =
            self.connection()?
                .query_row(UNRESOLVED_ATTACHMENT_WORK_SQL, [], |row| row.get(0))?;
        if unresolved_operations != 0 {
            return Err(StorageError::Integrity(format!(
                "cannot archive while {unresolved_operations} attachment operation(s) or ingest(s) require recovery"
            )));
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        // Reserve a process-private owner before SQLite can materialize the
        // plaintext snapshot. The owner covers SQLite sidecars too because the
        // whole session directory is crash-recoverable.
        let mut snapshot_owner = crate::native::output::reserve_process_private_snapshot()?;
        let snapshot_path = snapshot_owner.path().to_path_buf();
        if let Err(error) = self.create_database_snapshot(&snapshot_path) {
            let _ = remove_database_files(&snapshot_path);
            return Err(error);
        }
        if let Err(error) = snapshot_owner.verify_materialized() {
            let _ = remove_database_files(&snapshot_path);
            return Err(StorageError::Io(error));
        }

        let export_result = (|| -> StorageResult<()> {
            let snapshot = Self::open_snapshot_database(&snapshot_path)?;
            self.validate_snapshot_database(&snapshot)?;

            // The external-output transaction persists its immutable recovery
            // record before this hidden sibling is created. Replacement of an
            // existing user destination is therefore recoverable across every
            // rename boundary.
            let mut output =
                crate::native::output::ExternalFileOutputTransaction::begin(destination)?;
            let file = output.create_file()?;
            let mut archive = ZipWriter::new(file);
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .large_file(true)
                .unix_permissions(0o600);
            let mut entries = Vec::new();

            archive.start_file(DATABASE_ARCHIVE_PATH, options)?;
            let (database_checksum, database_size) =
                stream_file_to_archive(&mut archive, &snapshot_path)?;
            if database_size > MAX_DATABASE_BYTES {
                return Err(StorageError::Validation(
                    "library database exceeds Rustnote's supported archive size".to_string(),
                ));
            }
            let mut expanded_bytes = database_size;
            entries.push(ArchiveEntry {
                path: DATABASE_ARCHIVE_PATH.to_string(),
                byte_size: database_size,
                sha256: database_checksum.clone(),
            });

            self.for_each_archive_blob(&snapshot, |sha256, relative_path, expected_size| {
                if entries.len() >= MAX_ARCHIVE_FILES.saturating_sub(1) {
                    return Err(StorageError::Validation(
                        "library contains too many attachment blobs for one archive".to_string(),
                    ));
                }
                let source = self.attachments_directory.join(relative_path);
                let archive_path = format!("blobs/{sha256}");
                archive.start_file(&archive_path, options)?;
                let (actual_checksum, actual_size) = stream_file_to_archive(&mut archive, &source)?;
                if actual_checksum != sha256 || actual_size != expected_size {
                    return Err(StorageError::Integrity(format!(
                        "attachment blob {sha256} changed or is corrupt"
                    )));
                }
                expanded_bytes = expanded_bytes.checked_add(actual_size).ok_or_else(|| {
                    StorageError::Validation("expanded archive size overflow".to_string())
                })?;
                if expanded_bytes > MAX_EXPANDED_BYTES {
                    return Err(StorageError::Validation(
                        "expanded archive exceeds Rustnote's supported size limit".to_string(),
                    ));
                }
                entries.push(ArchiveEntry {
                    path: archive_path,
                    byte_size: actual_size,
                    sha256: actual_checksum,
                });
                Ok(())
            })?;

            let manifest = ArchiveManifest {
                format_version: COMPLETE_ARCHIVE_VERSION,
                build_identifier: FULL_BUILD_IDENTIFIER.to_string(),
                created_at: unix_timestamp(),
                database_sha256: database_checksum,
                entries,
            };
            archive.start_file("manifest.json", options)?;
            {
                let mut manifest_writer = LimitedWriter::new(&mut archive, MAX_MANIFEST_BYTES);
                serde_json::to_writer_pretty(&mut manifest_writer, &manifest)?;
                manifest_writer.write_all(b"\n")?;
            }
            let file = archive.finish()?;
            file.sync_all()?;
            let archive_size = file.metadata()?.len();
            drop(file);
            if archive_size > MAX_ARCHIVE_BYTES {
                return Err(StorageError::Validation(
                    "archive exceeds Rustnote's supported size limit".to_string(),
                ));
            }
            output.commit()?;
            Ok(())
        })();

        // Normal cleanup removes SQLite and all possible sidecars while the
        // process-private namespace still owns them. Only after successful
        // removal may the exact snapshot owner retire. If cleanup fails, the
        // session-root authority intentionally survives for process/startup
        // recovery.
        let cleanup_result = remove_database_files(&snapshot_path);
        if cleanup_result.is_ok() {
            let _ = snapshot_owner.remove_file();
        }
        match (export_result, cleanup_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
    pub fn inspect_complete_archive(&self, source: &Path) -> StorageResult<CompleteArchiveSummary> {
        let _operation_guard = self.lock_operations()?;
        let staged = self.stage_complete_archive(source)?;
        let summary = staged.summary.clone();
        discard_restore_stage_directory(&staged.directory)?;
        Ok(summary)
    }

    pub fn restore_complete_archive(&self, source: &Path) -> StorageResult<CompleteArchiveSummary> {
        // Validation/staging is serialized, but Vault revocation must drain
        // outside operation_lock so an already-authorized reader can finish a
        // final storage operation without lock inversion.
        let staged = {
            let _operation_guard = self.lock_operations()?;
            self.stage_complete_archive(source)?
        };
        let summary = staged.summary.clone();
        // Freeze every ordinary plaintext producer before Vault revocation and
        // before the final safety-snapshot/install boundary. Without this gate,
        // a `.incoming` owner could be committed to the outgoing database and
        // then lost at the SQLite snapshot switch while its file survives.
        let _privacy_guard = match self
            .begin_library_privacy_transition(crate::security::LibraryPrivacyTransition::Restore)
        {
            Ok(guard) => guard,
            Err(error) => {
                return finish_with_restore_stage_cleanup(&staged.directory, Err(error));
            }
        };
        let result = self
            .install_staged_complete_archive(&staged)
            .map(|()| summary);
        finish_with_restore_stage_cleanup(&staged.directory, result)
    }

    fn stage_complete_archive(&self, source: &Path) -> StorageResult<StagedCompleteArchive> {
        self.ensure_restore_source_is_external(source)?;
        let archive_bytes = fs::metadata(source)?.len();
        if archive_bytes > MAX_ARCHIVE_BYTES {
            return Err(StorageError::InvalidBackup(
                "archive exceeds Rustnote's supported size limit".to_string(),
            ));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(BufReader::new(file))?;
        let archive_names = validate_zip_structure(&mut archive)?;
        let manifest_bytes =
            read_zip_entry_bounded(&mut archive, "manifest.json", MAX_MANIFEST_BYTES)?;
        let manifest: ArchiveManifest = serde_json::from_slice(&manifest_bytes)?;
        if manifest.format_version != COMPLETE_ARCHIVE_VERSION {
            return Err(StorageError::InvalidBackup(format!(
                "unsupported archive version {}",
                manifest.format_version
            )));
        }

        let entries = manifest
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect::<HashMap<_, _>>();
        if entries.len() != manifest.entries.len() {
            return Err(StorageError::InvalidBackup(
                "manifest contains duplicate paths".to_string(),
            ));
        }

        let mut expected_names = entries
            .keys()
            .map(|name| (*name).to_string())
            .collect::<HashSet<_>>();
        expected_names.insert("manifest.json".to_string());
        if expected_names != archive_names {
            return Err(StorageError::InvalidBackup(
                "archive entries do not exactly match the manifest".to_string(),
            ));
        }

        let mut expanded_bytes = 0_u64;
        for entry in &manifest.entries {
            validate_manifest_entry(entry)?;
            expanded_bytes = expanded_bytes.checked_add(entry.byte_size).ok_or_else(|| {
                StorageError::InvalidBackup("expanded archive size overflow".to_string())
            })?;
            if expanded_bytes > MAX_EXPANDED_BYTES {
                return Err(StorageError::InvalidBackup(
                    "expanded archive exceeds Rustnote's supported size limit".to_string(),
                ));
            }
        }

        let database_entry = entries.get(DATABASE_ARCHIVE_PATH).ok_or_else(|| {
            StorageError::InvalidBackup(format!("manifest does not list {DATABASE_ARCHIVE_PATH}"))
        })?;
        if database_entry.byte_size > MAX_DATABASE_BYTES {
            return Err(StorageError::InvalidBackup(
                "library database exceeds Rustnote's supported size limit".to_string(),
            ));
        }
        if database_entry.sha256 != manifest.database_sha256 {
            return Err(StorageError::Integrity(
                "library database checksum does not match the manifest".to_string(),
            ));
        }

        // The process-session owner exists before this child is created. A hard
        // crash releases its OS lock, allowing fail-closed startup cleanup to
        // claim and remove the entire restore namespace.
        let staging_directory = allocate_restore_stage_directory("restore-stage")?;
        let database_path = staging_directory.join(DATABASE_ARCHIVE_PATH);
        let stage_result = (|| -> StorageResult<CompleteArchiveSummary> {
            extract_verified_zip_entry(
                &mut archive,
                DATABASE_ARCHIVE_PATH,
                database_entry,
                &database_path,
            )?;
            let database = Self::open_snapshot_database(&database_path)?;
            self.validate_snapshot_database(&database)?;
            let counts = self.complete_archive_counts(&database)?;

            let mut unclaimed_entries = entries.clone();
            unclaimed_entries.remove(DATABASE_ARCHIVE_PATH);
            self.for_each_archive_blob(&database, |sha256, _relative_path, expected_size| {
                let archive_path = format!("blobs/{sha256}");
                let entry = unclaimed_entries
                    .remove(archive_path.as_str())
                    .ok_or_else(|| {
                        StorageError::InvalidBackup(format!(
                            "missing manifest entry {archive_path}"
                        ))
                    })?;
                if entry.sha256 != sha256 || entry.byte_size != expected_size {
                    return Err(StorageError::Integrity(format!(
                        "manifest metadata mismatch for {archive_path}"
                    )));
                }
                let staged_path = staging_directory.join("blobs").join(sha256);
                extract_verified_zip_entry(&mut archive, &archive_path, entry, &staged_path)
            })?;
            if !unclaimed_entries.is_empty() {
                return Err(StorageError::InvalidBackup(
                    "manifest contains unreferenced archive entries".to_string(),
                ));
            }

            Ok(CompleteArchiveSummary {
                created_at: manifest.created_at,
                archive_bytes,
                expanded_bytes,
                notebook_count: counts.0,
                note_count: counts.1,
                attachment_count: counts.2,
                revision_count: counts.3,
            })
        })();

        match stage_result {
            Ok(summary) => Ok(StagedCompleteArchive {
                directory: staging_directory,
                database_path,
                summary,
            }),
            Err(error) => finish_with_restore_stage_cleanup(&staging_directory, Err(error)),
        }
    }

    fn copy_database_snapshot_pages(&self, source_path: &Path) -> StorageResult<()> {
        let source = Self::open_snapshot_database(source_path)?;
        let mut destination = self.connection()?;
        {
            let backup = SqliteBackup::new(&source, &mut destination)?;
            backup.run_to_completion(
                DATABASE_BACKUP_PAGES_PER_STEP,
                DATABASE_BACKUP_PAUSE,
                None,
            )?;
        }
        Ok(())
    }

    fn install_database_snapshot_pages(&self, source_path: &Path) -> StorageResult<()> {
        let source = Self::open_snapshot_database(source_path)?;
        self.validate_snapshot_database(&source)?;
        drop(source);
        self.copy_database_snapshot_pages(source_path)
    }

    fn finalize_installed_database_snapshot(&self) -> StorageResult<()> {
        let mut destination = self.connection()?;
        self.validate_snapshot_database(&destination)?;
        self.install_runtime_database_aids(&mut destination)?;
        self.verify_integrity_with_connection(&destination)
    }

    /// Finalize a staged archive that was fully validated before Rustnote added
    /// its own restore-handoff cleanup rows. Those rows intentionally make the
    /// generic "no pending staging" snapshot check false until old-library
    /// plaintext has been retired, so only schema-independent integrity and
    /// runtime aids are re-established here.
    fn finalize_prevalidated_restore_handoff(&self) -> StorageResult<()> {
        let mut destination = self.connection()?;
        self.install_runtime_database_aids(&mut destination)?;
        self.verify_integrity_with_connection(&destination)
    }

    fn restore_database_snapshot(&self, source_path: &Path) -> StorageResult<()> {
        self.install_database_snapshot_pages(source_path)?;
        self.finalize_installed_database_snapshot()
    }

    /// Seed the incoming database with durable cleanup ownership for canonical
    /// plaintext blobs that belong only to the library being replaced. The
    /// SQLite snapshot switch then transfers ownership atomically: before the
    /// switch the old canonical references own the files; after it, these
    /// managed staging rows do.
    fn seed_incoming_cleanup_for_retired_current_blobs(
        &self,
        staged: &StagedCompleteArchive,
    ) -> StorageResult<Vec<String>> {
        let incoming = Self::open_snapshot_database(&staged.database_path)?;
        let mut incoming_hashes = HashSet::new();
        self.for_each_archive_blob(&incoming, |sha256, _, _| {
            incoming_hashes.insert(sha256.to_string());
            Ok(())
        })?;
        drop(incoming);

        let current = self.connection()?;
        let mut retired_paths = Vec::new();
        self.for_each_archive_blob(&current, |sha256, relative_path, _| {
            if !incoming_hashes.contains(sha256) {
                let path = self.attachments_directory.join(relative_path);
                match fs::symlink_metadata(&path) {
                    Ok(_) => retired_paths.push(relative_path.to_string()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(StorageError::Io(error)),
                }
            }
            Ok(())
        })?;
        drop(current);

        if retired_paths.is_empty() {
            return Ok(Vec::new());
        }

        let mut incoming = Connection::open_with_flags(
            &staged.database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        incoming.busy_timeout(Duration::from_secs(5))?;
        incoming.execute_batch("PRAGMA journal_mode=DELETE;")?;
        let transaction = incoming.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stage_ids = Vec::with_capacity(retired_paths.len());
        for relative_path in retired_paths {
            stage_ids.push(
                super::plaintext_staging::ensure_canonical_blob_plaintext_stage_in_transaction(
                    &transaction,
                    &relative_path,
                )?,
            );
        }
        transaction.commit()?;
        Ok(stage_ids)
    }

    fn discard_restored_retired_current_blobs(&self, stage_ids: &[String]) -> StorageResult<()> {
        let mut failures = Vec::new();
        for stage_id in stage_ids {
            let result = self
                .resume_owned_plaintext(stage_id)
                .and_then(|mut owned| self.discard_owned_plaintext(&mut owned));
            if let Err(error) = result {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(StorageError::Integrity(format!(
                "{} old-library plaintext cleanup obligation(s) remain durably owned: {}",
                failures.len(),
                failures.join("; ")
            )))
        }
    }

    /// Prepare every incoming archive blob without exposing a partially copied
    /// canonical file. Replacement bytes are first copied to a same-directory
    /// temporary path that already has durable cleanup ownership, verified, and
    /// only then atomically renamed into place.
    ///
    /// If the current database already references the hash, its canonical
    /// reference owns the destination throughout the rename. Otherwise a generic
    /// canonical cleanup owner is reserved before the destination is touched;
    /// that owner disappears atomically when the incoming SQLite snapshot becomes
    /// canonical.
    fn prepare_restore_blobs_under_current_ownership(
        &self,
        staged: &StagedCompleteArchive,
    ) -> StorageResult<Vec<PreparedRestoreBlob>> {
        let current = self.connection()?;
        let mut current_hashes = HashSet::new();
        self.for_each_archive_blob(&current, |sha256, _, _| {
            current_hashes.insert(sha256.to_string());
            Ok(())
        })?;
        drop(current);

        let database = Self::open_snapshot_database(&staged.database_path)?;
        let mut prepared = Vec::<PreparedRestoreBlob>::new();
        let result =
            self.for_each_archive_blob(&database, |sha256, relative_path, expected_size| {
                let staged_path = staged.directory.join("blobs").join(sha256);
                let final_path = self.attachments_directory.join(relative_path);
                let current_owns_final = current_hashes.contains(sha256);

                let existing = match fs::symlink_metadata(&final_path) {
                    Ok(metadata) => {
                        if metadata.file_type().is_symlink() || !metadata.is_file() {
                            return Err(StorageError::Integrity(format!(
                                "canonical attachment path {} is not a regular file",
                                final_path.display()
                            )));
                        }
                        Some(sha256_file(&final_path)?)
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(StorageError::Io(error)),
                };

                // Incoming-only canonical plaintext needs an owner even when a
                // valid orphan already happens to occupy the canonical path.
                let cleanup_owner = if current_owns_final {
                    None
                } else {
                    Some(self.reserve_canonical_blob_owned_plaintext(relative_path)?)
                };
                prepared.push(PreparedRestoreBlob {
                    sha256: sha256.to_string(),
                    path: final_path.clone(),
                    cleanup_owner,
                });

                if let Some((actual_hash, actual_size)) = existing
                    && actual_hash == sha256
                    && actual_size == expected_size
                {
                    return Ok(());
                }

                let (_temporary_relative, mut temporary_owned) =
                    self.reserve_restore_blob_temporary_owned_plaintext(sha256)?;
                if let Some(parent) = temporary_owned.path().parent()
                    && let Err(error) = fs::create_dir_all(parent)
                {
                    let primary = StorageError::Io(error);
                    return Err(
                        self.cleanup_owned_plaintext_after_error(&mut temporary_owned, primary)
                    );
                }

                let copy_result = temporary_owned.copy_and_hash_from(&staged_path);
                let (actual_hash, actual_size) = match copy_result {
                    Ok(identity) => identity,
                    Err(error) => {
                        return Err(self.cleanup_owned_plaintext_after_error(
                            &mut temporary_owned,
                            error.into(),
                        ));
                    }
                };
                if actual_hash != sha256 || actual_size != expected_size {
                    let error = StorageError::Integrity(format!(
                        "restored attachment blob {sha256} failed verification"
                    ));
                    return Err(
                        self.cleanup_owned_plaintext_after_error(&mut temporary_owned, error)
                    );
                }

                // Prepare the predecessor→successor ownership transfer durably
                // before the same-filesystem rename. A crash can therefore be
                // reconciled whether it occurs before or after filesystem rename.
                if let Err(error) = self.rename_owned_restore_plaintext_to_canonical(
                    &mut temporary_owned,
                    relative_path,
                ) {
                    return Err(
                        self.cleanup_owned_plaintext_after_error(&mut temporary_owned, error)
                    );
                }

                // Keep the predecessor transfer record durable until directory
                // sync succeeds. If sync itself fails, startup recovery retains
                // enough information to reconcile either filesystem outcome.
                sync_parent_directory(&final_path)?;
                self.complete_owned_restore_plaintext_rename(&mut temporary_owned)?;
                Ok(())
            });

        match result {
            Ok(_) => Ok(prepared),
            Err(error) => match self.discard_prepared_restore_blobs(&mut prepared) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(StorageError::Integrity(format!(
                    "{error}; restore plaintext cleanup is incomplete and remains durably owned: {cleanup}"
                ))),
            },
        }
    }

    fn discard_prepared_restore_blobs(
        &self,
        prepared: &mut [PreparedRestoreBlob],
    ) -> StorageResult<()> {
        let mut failures = Vec::new();
        for blob in prepared.iter_mut().rev() {
            let Some(mut owner) = blob.cleanup_owner.take() else {
                continue;
            };
            if let Err(error) = self.discard_owned_plaintext(&mut owner) {
                blob.cleanup_owner = Some(owner);
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(StorageError::Integrity(format!(
                "{} restore blob cleanup obligation(s) remain: {}",
                failures.len(),
                failures.join("; ")
            )))
        }
    }

    fn retire_restore_blobs_before_rollback(
        &self,
        rollback_database: &Path,
        prepared: &[PreparedRestoreBlob],
    ) -> StorageResult<()> {
        if prepared.is_empty() {
            return Ok(());
        }
        let rollback = Self::open_snapshot_database(rollback_database)?;
        let mut reference_query = rollback.prepare(
            "SELECT
                EXISTS(SELECT 1 FROM attachments WHERE sha256 = ?1)
                OR EXISTS(SELECT 1 FROM revision_attachment_refs WHERE sha256 = ?1)
                OR EXISTS(SELECT 1 FROM vault_blob_refs WHERE sha256 = ?1)",
        )?;
        let mut failures = Vec::new();
        for blob in prepared.iter().rev() {
            let referenced =
                reference_query.query_row([&blob.sha256], |row| row.get::<_, i64>(0))?;
            if referenced != 0 {
                continue;
            }
            match crate::native::privacy::remove_file(&blob.path) {
                Ok(true) => {
                    if let Err(error) = sync_parent_directory(&blob.path) {
                        failures.push(error.to_string());
                    }
                }
                Ok(false) => match crate::security::managed_path_is_present(&blob.path) {
                    Ok(false) => {}
                    Ok(true) => failures.push(format!(
                        "canonical attachment path {} could not be retired",
                        blob.path.display()
                    )),
                    Err(error) => failures.push(error.to_string()),
                },
                Err(error) => failures.push(error.to_string()),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(StorageError::Integrity(format!(
                "{} restored plaintext path(s) could not be retired before rollback: {}",
                failures.len(),
                failures.join("; ")
            )))
        }
    }

    pub(super) fn install_staged_complete_archive(
        &self,
        staged: &StagedCompleteArchive,
    ) -> StorageResult<()> {
        self.create_restore_safety_backup_unlocked("complete archive restore")?;

        // The incoming archive intentionally excludes ordinary attachment
        // quarantine. Retire that disposable old-library plaintext while the
        // current database still owns it; otherwise the SQLite snapshot switch
        // would drop its durable owner row while leaving the file behind.
        self.discard_settled_orphan_quarantine_unlocked()?;

        let rollback_directory = allocate_restore_stage_directory("restore-rollback")?;
        let rollback_database = rollback_directory.join(DATABASE_ARCHIVE_PATH);
        let rollback_snapshot = self.create_database_snapshot(&rollback_database);
        if let Err(error) = rollback_snapshot {
            return finish_with_restore_stage_cleanup(&rollback_directory, Err(error));
        }

        // Seed the incoming database before the ownership handoff so old-only
        // canonical blobs can never become unowned at the database switch.
        let retired_current_stage_ids =
            match self.seed_incoming_cleanup_for_retired_current_blobs(staged) {
                Ok(stage_ids) => stage_ids,
                Err(error) => {
                    return finish_with_restore_stage_cleanup(&rollback_directory, Err(error));
                }
            };

        // Preinstall all changed blobs while the old database's durable generic
        // ownership ledger is still canonical. This is the crash-safe half of
        // the ownership handoff.
        let mut prepared = match self.prepare_restore_blobs_under_current_ownership(staged) {
            Ok(prepared) => prepared,
            Err(error) => {
                return finish_with_restore_stage_cleanup(&rollback_directory, Err(error));
            }
        };

        // SQLite's backup transaction is the ownership handoff point. Before it
        // commits, the old managed_plaintext_staging rows own changed paths.
        // After it commits, every archive blob is already materialized and the
        // restored database's canonical references own those same paths.
        // The staged database was validated before the controlled cleanup rows
        // above were inserted. Copy it without reapplying the generic snapshot
        // rule that intentionally rejects any managed plaintext staging.
        if let Err(restore_error) = self.copy_database_snapshot_pages(&staged.database_path) {
            let result = match self.discard_prepared_restore_blobs(&mut prepared) {
                Ok(()) => Err(restore_error),
                Err(cleanup) => Err(StorageError::Integrity(format!(
                    "restore database installation failed ({restore_error}); pre-commit plaintext cleanup is incomplete and remains durably owned: {cleanup}"
                ))),
            };
            return finish_with_restore_stage_cleanup(&rollback_directory, result);
        }

        let restore_result = (|| -> StorageResult<()> {
            self.finalize_prevalidated_restore_handoff()?;
            let audit = self.audit_attachments()?;
            if !audit.is_clean() {
                return Err(StorageError::Integrity(
                    "restored archive has missing or corrupt attachments".to_string(),
                ));
            }
            let connection = self.connection()?;
            super::settings::record_build_diagnostics(&connection)?;
            Ok(())
        })();

        if let Err(restore_error) = restore_result {
            // The restored database now canonically owns every prepared path.
            // Before putting the old database back, retire only blobs that the
            // old database would not own. Failure leaves the restored database
            // in place so no surviving plaintext becomes ownerless.
            if let Err(cleanup_error) =
                self.retire_restore_blobs_before_rollback(&rollback_database, &prepared)
            {
                let failure = StorageError::Integrity(format!(
                    "restore failed ({restore_error}); rollback was not attempted because restored plaintext cleanup failed ({cleanup_error}); use the safety backup in {}",
                    self.automatic_backup_directory.display()
                ));
                return finish_with_restore_stage_cleanup(&rollback_directory, Err(failure));
            }

            if let Err(rollback_error) = self.restore_database_snapshot(&rollback_database) {
                let failure = StorageError::Integrity(format!(
                    "restore failed ({restore_error}) and automatic rollback also failed ({rollback_error}); use the safety backup in {}",
                    self.automatic_backup_directory.display()
                ));
                return finish_with_restore_stage_cleanup(&rollback_directory, Err(failure));
            }
            return finish_with_restore_stage_cleanup(&rollback_directory, Err(restore_error));
        }

        // Final validation has succeeded, so rollback is no longer required.
        // Retire old-library-only canonical plaintext under the cleanup rows
        // that became durable in the restored database at the snapshot switch.
        if let Err(cleanup_error) =
            self.discard_restored_retired_current_blobs(&retired_current_stage_ids)
        {
            let failure = StorageError::Integrity(format!(
                "restore committed but old-library plaintext cleanup is incomplete: {cleanup_error}"
            ));
            return finish_with_restore_stage_cleanup(&rollback_directory, Err(failure));
        }

        finish_with_restore_stage_cleanup(&rollback_directory, Ok(()))
    }

    fn ensure_restore_source_is_external(&self, source: &Path) -> StorageResult<()> {
        let source_metadata = fs::symlink_metadata(source)?;
        if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
            return Err(StorageError::Validation(
                "choose a regular backup file rather than a directory or symbolic link".to_string(),
            ));
        }
        let canonical_source = fs::canonicalize(source)?;
        let canonical_data = fs::canonicalize(&self.data_directory)?;
        if canonical_source.starts_with(&canonical_data) {
            return Err(StorageError::Validation(
                "choose a restore archive outside Rustnote's active data directory".to_string(),
            ));
        }
        Ok(())
    }

    pub fn export_markdown_note(&self, parent: &Path, note_id: &NoteId) -> StorageResult<PathBuf> {
        self.export_markdown_scope_controlled(
            parent,
            MarkdownNoteExportScope::new(note_id.clone()),
            || false,
            |_, _| {},
        )
    }

    fn export_markdown_scope_controlled<C, P>(
        &self,
        parent: &Path,
        scope: MarkdownNoteExportScope,
        is_cancelled: C,
        mut report_progress: P,
    ) -> StorageResult<PathBuf>
    where
        C: Fn() -> bool,
        P: FnMut(usize, usize),
    {
        let _operation_guard = self.lock_operations()?;
        self.ensure_export_destination_is_external(&parent.join(".rustnote-export-check"))?;
        fs::create_dir_all(parent)?;
        let now = unix_timestamp();
        let final_directory = unique_directory(parent, &scope.directory_name(now));
        let output = AtomicDirectoryOutput::create(&final_directory)?;

        let mut connection = self.read_only_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        scope.ensure_unprotected(&transaction)?;
        let expected = scope.expected_counts(&transaction)?;
        if expected.notes == 0 {
            return Err(StorageError::Validation(
                "the note selected for export no longer exists".to_string(),
            ));
        }

        let mut exported = MarkdownExportCounts::default();
        report_progress(0, expected.notes);

        {
            let mut statement = transaction.prepare(
                "SELECT
                    n.id, n.title, n.editor_state,
                    n.pinned, n.archived, n.trashed, n.purge_after,
                    n.created_at, n.updated_at,
                    COALESCE((
                        SELECT group_concat(t.name, ', ')
                        FROM note_tags nt
                        JOIN tags t ON t.id = nt.tag_id
                        WHERE nt.note_id = n.id
                    ), '') AS tag_names
                 FROM notes n
                 WHERE n.id = ?1
                 ORDER BY n.created_at, n.id",
            )?;
            let mut rows = statement.query([&scope.note_id])?;
            while let Some(row) = rows.next()? {
                if is_cancelled() {
                    return Err(StorageError::Validation(
                        "Markdown export was cancelled".to_string(),
                    ));
                }
                let note = markdown_export_record_from_row(row)?;
                exported.attachments += self.export_one_markdown_note(
                    &transaction,
                    output.path(),
                    &note,
                    &is_cancelled,
                )?;
                exported.notes += 1;
                report_progress(exported.notes, expected.notes);
            }
        }

        transaction.commit()?;
        if exported != expected {
            return Err(StorageError::Integrity(format!(
                "Markdown export source count changed: expected {expected:?}, exported {exported:?}"
            )));
        }
        let (path, _) =
            output.commit(|directory| verify_markdown_export_directory(directory, expected))?;
        Ok(path)
    }

    fn export_one_markdown_note<C>(
        &self,
        connection: &Connection,
        export_root: &Path,
        note: &MarkdownExportRecord,
        is_cancelled: &C,
    ) -> StorageResult<usize>
    where
        C: Fn() -> bool,
    {
        let note_directory = export_root.to_path_buf();
        fs::create_dir_all(&note_directory)?;
        let short_id = note
            .id
            .as_str()
            .chars()
            .rev()
            .take(8)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let base_stem = format!("{}-{short_id}", sanitize_component(note.display_title()));
        let stem = if note_directory.join(format!("{base_stem}.md")).exists() {
            format!("{base_stem}-{}", sanitize_component(note.id.as_str()))
        } else {
            base_stem
        };
        let markdown_path = note_directory.join(format!("{stem}.md"));
        let markdown = derive_editor_projections(&note.editor_state)
            .map_err(|error| {
                StorageError::Validation(format!(
                    "could not derive Markdown for note {} during export: {error}",
                    note.id
                ))
            })?
            .markdown;
        let mut markdown_output = AtomicFileOutput::create(&markdown_path)?;
        let writer = markdown_output.writer()?;
        writer.write_all(b"---\n")?;
        writeln!(writer, "rustnote_id: {:?}", note.id.as_str())?;
        writeln!(writer, "created_at: {}", note.created_at)?;
        writeln!(writer, "updated_at: {}", note.updated_at)?;
        writer.write_all(b"tags: ")?;
        serde_json::to_writer(&mut *writer, &note.tags)?;
        writer.write_all(b"\n")?;
        writeln!(writer, "pinned: {}", note.pinned)?;
        writeln!(writer, "archived: {}", note.archived)?;
        writeln!(writer, "trashed: {}", note.trashed)?;
        writeln!(writer, "purge_after: {:?}", note.purge_after)?;
        writer.write_all(b"---\n\n# ")?;
        writer.write_all(note.display_title().as_bytes())?;
        writer.write_all(b"\n\n")?;
        writer.write_all(markdown.as_bytes())?;
        writer.write_all(b"\n")?;

        let mut exported_attachments = 0_usize;
        let mut attachment_statement = connection.prepare(
            "SELECT id, file_name, byte_size, sha256
             FROM attachments
             WHERE note_id = ?1
             ORDER BY created_at, id",
        )?;
        let mut attachment_rows = attachment_statement.query([&note.id])?;
        let mut wrote_attachment_heading = false;
        while let Some(row) = attachment_rows.next()? {
            if is_cancelled() {
                return Err(StorageError::Validation(
                    "Markdown export was cancelled".to_string(),
                ));
            }
            let attachment = markdown_attachment_from_row(row)?;
            if !wrote_attachment_heading {
                writer.write_all(b"\n## Attachments\n\n")?;
                wrote_attachment_heading = true;
            }
            let (relative_path, _, expected_size) = validate_attachment_blob_identity(
                &format!("attachment {}", attachment.id),
                &attachment.sha256,
                attachment.byte_size,
            )?;
            let attachment_directory_name = format!("{stem}.attachments");
            let attachment_directory = note_directory.join(&attachment_directory_name);
            fs::create_dir_all(&attachment_directory)?;
            let short_attachment_id = attachment
                .id
                .chars()
                .rev()
                .take(8)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            let base_name = format!(
                "{short_attachment_id}-{}",
                sanitize_component(&attachment.file_name)
            );
            let exported_name = if attachment_directory.join(&base_name).exists() {
                format!("{}-{base_name}", sanitize_component(&attachment.id))
            } else {
                base_name
            };
            let destination = attachment_directory.join(&exported_name);
            let source = self.attachments_directory.join(relative_path);
            copy_file_atomically(&source, &destination)?;
            let (actual_checksum, actual_size) = sha256_file(&destination)?;
            if actual_checksum != attachment.sha256 || actual_size != expected_size {
                let _ = fs::remove_file(&destination);
                return Err(StorageError::Integrity(format!(
                    "exported attachment {} failed verification",
                    attachment.id
                )));
            }
            let link = PathBuf::from(&attachment_directory_name).join(&exported_name);
            writeln!(
                writer,
                "- [{}]({}) — {} bytes — SHA-256 `{}`",
                escape_markdown_link_label(&attachment.file_name),
                link.to_string_lossy().replace('\\', "/"),
                attachment.byte_size,
                attachment.sha256
            )?;
            exported_attachments += 1;
        }
        drop(attachment_rows);
        drop(attachment_statement);

        markdown_output.commit(|path| {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(StorageError::Integrity(format!(
                    "Markdown note was not written as a regular file: {}",
                    path.display()
                )));
            }
            Ok(())
        })?;
        Ok(exported_attachments)
    }

    pub fn export_note_plain_text(
        &self,
        note_id: &NoteId,
        destination: &Path,
    ) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        self.ensure_export_destination_is_external(destination)?;
        let connection = self.read_only_connection()?;
        if super::protection::protected_note_exists(&connection, note_id)? {
            return Err(StorageError::Validation(
                "protected notes cannot be included in plaintext exports; remove protection first"
                    .to_string(),
            ));
        }
        let note = connection
            .query_row(
                "SELECT title, plain_text FROM notes WHERE id = ?1",
                [note_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((title, plain_text)) = note else {
            return Err(StorageError::Validation(
                "the note selected for export no longer exists".to_string(),
            ));
        };
        let title = title.trim();
        let display_title = if title.is_empty() {
            "Untitled note"
        } else {
            title
        };
        let mut output = AtomicFileOutput::create(destination)?;
        let writer = output.writer()?;
        writer.write_all(display_title.as_bytes())?;
        if !plain_text.trim().is_empty() {
            writer.write_all(b"\n\n")?;
            writer.write_all(plain_text.trim().as_bytes())?;
        }
        writer.write_all(b"\n")?;
        output.commit(|path| {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(StorageError::Integrity(format!(
                    "plain-text export was not written as a regular file: {}",
                    path.display()
                )));
            }
            Ok(())
        })
    }

    pub(super) fn ensure_export_destination_is_external(
        &self,
        destination: &Path,
    ) -> StorageResult<()> {
        let parent = destination.parent().ok_or_else(|| {
            StorageError::Validation("export destination has no parent directory".to_string())
        })?;
        fs::create_dir_all(parent)?;
        let canonical_parent = fs::canonicalize(parent)?;
        let canonical_data = fs::canonicalize(&self.data_directory)?;
        if canonical_parent.starts_with(&canonical_data) {
            return Err(StorageError::Validation(
                "store exports outside Rustnote's active data directory".to_string(),
            ));
        }
        Ok(())
    }

    pub fn verify_integrity(&self) -> StorageResult<()> {
        let connection = self.connection()?;
        self.verify_integrity_with_connection(&connection)
    }
    pub(super) fn verify_integrity_with_connection(
        &self,
        connection: &Connection,
    ) -> StorageResult<()> {
        let result: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Err(StorageError::Integrity(result));
        }
        let foreign_key_problem: Option<String> = connection
            .query_row(
                r#"SELECT 'table=' || f."table"
                          || ', rowid=' || COALESCE(CAST(f.rowid AS TEXT), 'none')
                          || ', parent=' || f.parent
                   FROM pragma_foreign_key_check AS f
                   LIMIT 1"#,
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(problem) = foreign_key_problem {
            return Err(StorageError::Integrity(problem));
        }

        let unrooted_hierarchy_note: Option<NoteId> = connection
            .query_row(
                "WITH RECURSIVE rooted(note_id) AS (
                     SELECT n.id
                     FROM notes n
                     LEFT JOIN note_parent h ON h.child_note_id = n.id
                     WHERE h.child_note_id IS NULL
                     UNION ALL
                     SELECT h.child_note_id
                     FROM note_parent h
                     JOIN rooted r ON h.parent_note_id = r.note_id
                 )
                 SELECT h.child_note_id
                 FROM note_parent h
                 LEFT JOIN rooted r ON r.note_id = h.child_note_id
                 WHERE r.note_id IS NULL
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(note_id) = unrooted_hierarchy_note {
            return Err(StorageError::Integrity(format!(
                "note hierarchy contains a cycle at {note_id}"
            )));
        }

        let missing_or_mismatched_search_row: Option<String> = connection
            .query_row(
                "SELECT n.id
                 FROM notes n
                 LEFT JOIN notes_fts f ON f.rowid = n.fts_rowid
                 WHERE NOT EXISTS (
                           SELECT 1 FROM vault_note_roots root WHERE root.note_id = n.id
                       )
                   AND (f.note_id IS NULL OR f.note_id <> n.id)
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(note_id) = missing_or_mismatched_search_row {
            return Err(StorageError::Integrity(format!(
                "search index is missing or mismatched for note {note_id}"
            )));
        }

        let orphaned_search_row: Option<String> = connection
            .query_row(
                "SELECT f.note_id
                 FROM notes_fts f
                 LEFT JOIN notes n ON n.fts_rowid = f.rowid
                 WHERE n.id IS NULL OR n.id <> f.note_id
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(note_id) = orphaned_search_row {
            return Err(StorageError::Integrity(format!(
                "search index contains an orphaned or mismatched note {note_id}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod export_tests {
    use std::{fs, path::PathBuf};

    use crate::editor_projection::derive_editor_projections;
    use crate::storage::new_id;

    use super::*;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let root = std::env::temp_dir().join(format!("rustnote-{}", new_id(label)));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (storage, root)
    }

    fn save_export_note(storage: &Storage, title: &str) -> crate::domain::Note {
        let mut note = storage.create_note(None).expect("create export test note");
        let editor_state = include_str!("../../assets/welcome-note-editor-state.json").trim();
        let projections = derive_editor_projections(editor_state).expect("project test document");
        note.title = title.to_string();
        note.replace_document(
            projections.markdown,
            editor_state.to_string(),
            projections.plain_text,
        );
        storage
            .save_note_owned(note)
            .expect("save export test note")
    }

    fn add_test_attachment(
        storage: &Storage,
        root: &std::path::Path,
        file_name: &str,
        bytes: &[u8],
    ) -> crate::domain::Attachment {
        let note = storage
            .create_note(None)
            .expect("create attachment test note");
        let source = root.join(format!("source-{file_name}"));
        fs::write(&source, bytes).expect("write attachment source");
        storage
            .add_attachment_file_as(&note.id, file_name, Some("text/plain"), &source)
            .expect("add attachment")
    }

    fn managed_plaintext_stage_count(storage: &Storage) -> i64 {
        storage
            .connection()
            .expect("open test database")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging",
                [],
                |row| row.get(0),
            )
            .expect("count managed plaintext staging")
    }

    #[test]
    fn whole_library_restore_retires_old_only_canonical_plaintext() {
        let (current, current_root) = test_storage("restore-old-only-current");
        let old_attachment = add_test_attachment(
            &current,
            &current_root,
            "old-only.txt",
            b"plaintext owned only by the replaced library",
        );
        let old_path = current
            .attachment_file_path(&old_attachment)
            .expect("resolve old canonical path");
        assert!(old_path.exists());

        let (incoming, incoming_root) = test_storage("restore-old-only-incoming");
        let _incoming_attachment = add_test_attachment(
            &incoming,
            &incoming_root,
            "incoming-only.txt",
            b"plaintext owned by the incoming library",
        );
        let archive = current_root.join("incoming-library.rustnote.zip");
        incoming
            .export_complete_archive(&archive)
            .expect("export incoming archive");

        current
            .restore_complete_archive(&archive)
            .expect("restore incoming library");

        assert!(
            !old_path.exists(),
            "a canonical blob referenced only by the replaced database must not survive restore"
        );
        assert_eq!(
            managed_plaintext_stage_count(&current),
            0,
            "successful restore must discharge every ownership-handoff cleanup row"
        );
        assert!(
            current
                .audit_attachments()
                .expect("audit restored attachments")
                .is_clean()
        );

        drop(incoming);
        drop(current);
        let _ = fs::remove_dir_all(incoming_root);
        let _ = fs::remove_dir_all(current_root);
    }

    #[test]
    fn restore_seeds_old_only_cleanup_in_incoming_database_before_snapshot_switch() {
        let (current, current_root) = test_storage("restore-seed-current");
        let old_attachment = add_test_attachment(
            &current,
            &current_root,
            "retired.txt",
            b"old library canonical plaintext",
        );
        let old_relative =
            crate::attachment_paths::canonical_blob_relative_path(&old_attachment.sha256)
                .expect("canonical relative path");

        let (incoming, incoming_root) = test_storage("restore-seed-incoming");
        let _ = incoming.create_note(None).expect("create incoming note");
        let archive = current_root.join("seed-incoming.rustnote.zip");
        incoming
            .export_complete_archive(&archive)
            .expect("export incoming archive");
        let staged = current
            .stage_complete_archive(&archive)
            .expect("stage incoming archive");

        let stage_ids = current
            .seed_incoming_cleanup_for_retired_current_blobs(&staged)
            .expect("seed incoming cleanup ownership");
        assert_eq!(stage_ids.len(), 1);
        let staged_db = Connection::open(&staged.database_path).expect("open staged database");
        let staged_relative: String = staged_db
            .query_row(
                "SELECT relative_path FROM managed_plaintext_staging WHERE id = ?1",
                [&stage_ids[0]],
                |row| row.get(0),
            )
            .expect("read seeded cleanup row");
        assert_eq!(staged_relative, old_relative);
        assert_eq!(
            managed_plaintext_stage_count(&current),
            0,
            "the active pre-swap database must keep canonical ownership rather than duplicate it"
        );

        drop(staged_db);
        discard_restore_stage_directory(&staged.directory).expect("discard staged archive");
        drop(incoming);
        drop(current);
        let _ = fs::remove_dir_all(incoming_root);
        let _ = fs::remove_dir_all(current_root);
    }

    #[test]
    fn shared_hash_restore_repair_uses_existing_canonical_ownership() {
        let shared_bytes = b"shared attachment plaintext";
        let (current, current_root) = test_storage("restore-shared-current");
        let current_attachment =
            add_test_attachment(&current, &current_root, "shared-current.txt", shared_bytes);
        let current_path = current
            .attachment_file_path(&current_attachment)
            .expect("resolve current shared path");

        let (incoming, incoming_root) = test_storage("restore-shared-incoming");
        let incoming_attachment = add_test_attachment(
            &incoming,
            &incoming_root,
            "shared-incoming.txt",
            shared_bytes,
        );
        assert_eq!(current_attachment.sha256, incoming_attachment.sha256);
        let archive = current_root.join("shared-incoming.rustnote.zip");
        incoming
            .export_complete_archive(&archive)
            .expect("export incoming archive");
        let staged = current
            .stage_complete_archive(&archive)
            .expect("stage incoming archive");

        fs::write(&current_path, b"corrupt old canonical bytes").expect("corrupt shared canonical");
        let prepared = current
            .prepare_restore_blobs_under_current_ownership(&staged)
            .expect("repair shared canonical blob");
        let shared = prepared
            .iter()
            .find(|blob| blob.sha256 == current_attachment.sha256)
            .expect("prepared shared blob");
        assert!(
            shared.cleanup_owner.is_none(),
            "the current database reference must remain the ownership authority for a shared hash"
        );
        assert_eq!(
            managed_plaintext_stage_count(&current),
            0,
            "repairing a currently referenced hash must not create a generic owner that startup could delete"
        );
        let (actual_hash, actual_size) = sha256_file(&current_path).expect("hash repaired blob");
        assert_eq!(actual_hash, current_attachment.sha256);
        assert_eq!(actual_size, shared_bytes.len() as u64);

        discard_restore_stage_directory(&staged.directory).expect("discard staged archive");
        drop(incoming);
        drop(current);
        let _ = fs::remove_dir_all(incoming_root);
        let _ = fs::remove_dir_all(current_root);
    }

    #[test]
    fn note_markdown_export_is_scoped_and_plain_text_is_readable() {
        let (storage, root) = test_storage("note-export");
        let note = save_export_note(&storage, "Export target");
        let _sibling = save_export_note(&storage, "Do not export me");
        let export_parent = root.join("exports");
        fs::create_dir_all(&export_parent).expect("create export parent");

        let markdown_root = storage
            .export_markdown_note(&export_parent, &note.id)
            .expect("export one Markdown note");
        let markdown_files = fs::read_dir(&markdown_root)
            .expect("read Markdown export")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        assert_eq!(markdown_files.len(), 1);
        let markdown = fs::read_to_string(&markdown_files[0]).expect("read Markdown note");
        assert!(markdown.contains("# Export target"));
        assert!(!markdown.contains("Do not export me"));

        let text_path = export_parent.join("export-target.txt");
        storage
            .export_note_plain_text(&note.id, &text_path)
            .expect("export note as plain text");
        let text = fs::read_to_string(&text_path).expect("read plain-text export");
        assert!(text.starts_with("Export target\n\n"));
        assert!(text.contains("Welcome to Rustnote"));

        let _ = fs::remove_dir_all(root);
    }
}
