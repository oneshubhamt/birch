//! Crash-durable ownership for Rustnote-created output staging.
//!
//! The explicit final destination is outside this transaction's cleanup
//! responsibility. Every Rustnote-created hidden sibling used to prepare or
//! replace it is different: an immutable recovery record is fsynced in durable
//! application state before such plaintext can exist. The record itself is
//! locked for the live transaction, so another Rustnote process recovers only
//! work whose owner process has actually disappeared.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::security::OwnedPlaintext;

#[cfg(not(any(test, feature = "internal-test-support")))]
const OUTPUT_JOURNAL_NAMESPACE: &str = "output-transfers";
const OUTPUT_SNAPSHOT_NAMESPACE: &str = "output-snapshots";
const OUTPUT_LEDGER_LOCK_FILE: &str = ".ledger.lock";
const OUTPUT_RECORD_CHECKSUM_DOMAIN: &[u8] = b"rustnote-output-record-v1\0";
const OUTPUT_JOURNAL_VERSION: u32 = 1;
const MAX_OUTPUT_JOURNAL_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum OutputKind {
    File,
    Directory,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OutputRecord {
    version: u32,
    operation_id: String,
    kind: OutputKind,
    destination: PathBuf,
    temporary: PathBuf,
    previous: Option<PathBuf>,
    had_previous: bool,
    checksum: String,
}

#[derive(Serialize)]
struct OutputRecordChecksum<'a> {
    version: u32,
    operation_id: &'a str,
    kind: OutputKind,
    destination: &'a Path,
    temporary: &'a Path,
    previous: Option<&'a Path>,
    had_previous: bool,
}

impl OutputRecord {
    fn new(
        operation_id: String,
        kind: OutputKind,
        destination: PathBuf,
        temporary: PathBuf,
        previous: Option<PathBuf>,
        had_previous: bool,
    ) -> io::Result<Self> {
        let mut record = Self {
            version: OUTPUT_JOURNAL_VERSION,
            operation_id,
            kind,
            destination,
            temporary,
            previous,
            had_previous,
            checksum: String::new(),
        };
        record.checksum = record_checksum(&record)?;
        Ok(record)
    }
}

pub(crate) struct ExternalFileOutputTransaction {
    record: OutputRecord,
    journal_path: PathBuf,
    journal_lock: Option<File>,
    temporary: OwnedPlaintext,
    predecessor: Option<OwnedPlaintext>,
    finished: bool,
    preserve_for_recovery: bool,
}

impl ExternalFileOutputTransaction {
    pub(crate) fn begin(destination: &Path) -> io::Result<Self> {
        let destination = absolute_path(destination)?;
        let parent = destination.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "output destination has no parent")
        })?;
        validate_output_parent(parent)?;
        let had_previous = validate_existing_file_destination(&destination)?;

        let operation_id = output_token();
        let temporary = parent.join(format!(".rustnote-output-{operation_id}.tmp"));
        let previous = parent.join(format!(".rustnote-output-{operation_id}.previous"));
        require_path_absent(&temporary)?;
        require_path_absent(&previous)?;
        let record = OutputRecord::new(
            operation_id.clone(),
            OutputKind::File,
            destination,
            temporary.clone(),
            had_previous.then_some(previous.clone()),
            had_previous,
        )?;
        let (journal_path, journal_lock) = persist_record(&record)?;
        let temporary = OwnedPlaintext::durable_external_output_reserved(
            temporary,
            operation_id.clone(),
        );
        let predecessor = had_previous.then(|| {
            OwnedPlaintext::durable_external_output_reserved(previous, operation_id)
        });
        Ok(Self {
            record,
            journal_path,
            journal_lock: Some(journal_lock),
            temporary,
            predecessor,
            finished: false,
            preserve_for_recovery: false,
        })
    }

    pub(crate) fn temporary_path(&self) -> &Path {
        self.temporary.path()
    }

    pub(crate) fn create_file(&mut self) -> io::Result<File> {
        self.temporary.create_file()
    }

    pub(crate) fn copy_from(&mut self, source: &Path) -> io::Result<bool> {
        self.temporary.copy_from(source)
    }

    pub(crate) fn copy_and_hash_from(&mut self, source: &Path) -> io::Result<(String, u64)> {
        self.temporary.copy_and_hash_from(source)
    }

    pub(crate) fn commit(mut self) -> io::Result<()> {
        if !self.temporary.path_entry_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "output staging disappeared before commit",
            ));
        }

        // A destination that did not exist when the transaction was admitted
        // must still be absent at publication. This protects unrelated external
        // writes that race Rustnote after begin(); Drop/recovery then discards
        // only Rustnote's owned temporary and leaves the external file intact.
        if self.predecessor.is_none() && path_exists(&self.record.destination)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output destination appeared after transaction admission",
            ));
        }

        if let Some(predecessor) = self.predecessor.as_mut() {
            predecessor.capture_external_predecessor(&self.record.destination)?;
            if let Err(error) = sync_directory(parent_of(&self.record.destination)?) {
                self.preserve_for_recovery = true;
                return Err(error);
            }
        }

        self.temporary.publish_external_output(&self.record.destination)?;
        if let Err(error) = sync_directory(parent_of(&self.record.destination)?) {
            self.preserve_for_recovery = true;
            return Err(error);
        }

        if let Some(predecessor) = self.predecessor.as_mut() {
            if let Err(error) = predecessor.remove_file() {
                self.preserve_for_recovery = true;
                return Err(error);
            }
            if let Err(error) = sync_directory(parent_of(&self.record.destination)?) {
                self.preserve_for_recovery = true;
                return Err(error);
            }
        }

        self.finish_transaction()
    }

    fn finish_transaction(&mut self) -> io::Result<()> {
        self.journal_lock.take();
        if let Err(error) = retire_journal(&self.journal_path) {
            self.preserve_for_recovery = true;
            return Err(error);
        }
        self.finished = true;
        Ok(())
    }

    fn reconcile_on_drop(&mut self) {
        if self.finished || self.preserve_for_recovery {
            return;
        }
        if reconcile_record_state(&self.record, &self.journal_path).is_err() {
            self.preserve_for_recovery = true;
            return;
        }
        self.journal_lock.take();
        if retire_journal(&self.journal_path).is_ok() {
            self.finished = true;
        } else {
            self.preserve_for_recovery = true;
        }
    }
}

impl Drop for ExternalFileOutputTransaction {
    fn drop(&mut self) {
        self.reconcile_on_drop();
    }
}

pub(crate) struct ExternalDirectoryOutputTransaction {
    record: OutputRecord,
    journal_path: PathBuf,
    journal_lock: Option<File>,
    temporary: OwnedPlaintext,
    finished: bool,
    preserve_for_recovery: bool,
}

impl ExternalDirectoryOutputTransaction {
    pub(crate) fn begin(destination: &Path) -> io::Result<Self> {
        let destination = absolute_path(destination)?;
        let parent = parent_of(&destination)?;
        validate_output_parent(parent)?;
        if path_exists(&destination)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output directory destination already exists",
            ));
        }
        let operation_id = output_token();
        let temporary = parent.join(format!(
            ".rustnote-output-{operation_id}.dir.tmp"
        ));
        require_path_absent(&temporary)?;
        let record = OutputRecord::new(
            operation_id.clone(),
            OutputKind::Directory,
            destination,
            temporary.clone(),
            None,
            false,
        )?;
        let (journal_path, journal_lock) = persist_record(&record)?;
        let temporary = OwnedPlaintext::durable_external_output_reserved(temporary, operation_id);
        let mut transaction = Self {
            record,
            journal_path,
            journal_lock: Some(journal_lock),
            temporary,
            finished: false,
            preserve_for_recovery: false,
        };
        transaction.temporary.create_external_output_directory()?;
        Ok(transaction)
    }

    pub(crate) fn temporary_path(&self) -> &Path {
        self.temporary.path()
    }

    pub(crate) fn commit(mut self) -> io::Result<PathBuf> {
        // Directory outputs never replace an existing destination. If another
        // actor created it after begin(), preserve that external tree and let
        // transaction cleanup remove only Rustnote's owned staging directory.
        if path_exists(&self.record.destination)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output directory appeared after transaction admission",
            ));
        }
        self.temporary
            .publish_external_output(&self.record.destination)?;
        if let Err(error) = sync_directory(parent_of(&self.record.destination)?) {
            self.preserve_for_recovery = true;
            return Err(error);
        }
        let destination = self.record.destination.clone();
        self.finish_transaction()?;
        Ok(destination)
    }

    fn finish_transaction(&mut self) -> io::Result<()> {
        self.journal_lock.take();
        if let Err(error) = retire_journal(&self.journal_path) {
            self.preserve_for_recovery = true;
            return Err(error);
        }
        self.finished = true;
        Ok(())
    }

    fn reconcile_on_drop(&mut self) {
        if self.finished || self.preserve_for_recovery {
            return;
        }
        if reconcile_record_state(&self.record, &self.journal_path).is_err() {
            self.preserve_for_recovery = true;
            return;
        }
        self.journal_lock.take();
        if retire_journal(&self.journal_path).is_ok() {
            self.finished = true;
        } else {
            self.preserve_for_recovery = true;
        }
    }
}

impl Drop for ExternalDirectoryOutputTransaction {
    fn drop(&mut self) {
        self.reconcile_on_drop();
    }
}

pub(crate) fn copy_file_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    let mut transaction = ExternalFileOutputTransaction::begin(destination)?;
    let _ = transaction.copy_from(source)?;
    transaction.commit()
}

/// Reserve a process-private file before a native/SQLite producer materializes
/// it. The entire session directory is OS-lock owned and is scrubbed on the
/// next startup after process death, so SQLite sidecars inherit the same prior
/// cleanup authority without creating a second output ledger.
pub(crate) fn reserve_process_private_snapshot() -> io::Result<OwnedPlaintext> {
    let root = output_snapshot_root()?;
    let owner = super::temp_owner::process_session_owner(&root, "archive export snapshots")
        .map_err(|error| io::Error::other(error.to_string()))?;
    let token = output_token();
    let path = owner.root().join(format!("snapshot-{token}.sqlite3"));
    OwnedPlaintext::process_private(owner, format!("archive-snapshot-{token}"), path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

/// Fail-closed process-start recovery for abandoned external output staging.
/// Live transactions are skipped by their OS-held journal lock. Corrupt records
/// are never deleted because untrusted metadata is not permission to mutate an
/// arbitrary external path.
pub fn recover_stale_output_transactions() -> super::NativeResult<usize> {
    let snapshot_root = output_snapshot_root().map_err(|error| {
        super::NativeError::operation(format!(
            "Could not prepare archive export snapshot staging: {error}"
        ))
    })?;
    let mut recovered = super::temp_owner::cleanup_session_directories(
        &snapshot_root,
        "archive export snapshot staging",
    )?;

    let root = journal_root().map_err(|error| {
        super::NativeError::operation(format!("Could not prepare output recovery ledger: {error}"))
    })?;
    let _ledger_lock = lock_output_ledger(&root).map_err(|error| {
        super::NativeError::operation(format!("Could not lock output recovery ledger: {error}"))
    })?;
    let entries = fs::read_dir(&root).map_err(|error| {
        super::NativeError::operation(format!("Could not inspect output recovery ledger: {error}"))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            super::NativeError::operation(format!("Could not inspect output recovery ledger: {error}"))
        })?;
        if entry.file_name().to_str() == Some(OUTPUT_LEDGER_LOCK_FILE) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            super::NativeError::operation(format!("Could not inspect output recovery record: {error}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return super::native_error(
                "Output recovery ledger contains an unexpected non-file entry",
            );
        }
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            return super::native_error("Output recovery ledger contains an unknown record");
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| {
                super::NativeError::operation(format!(
                    "Could not open output recovery record {}: {error}",
                    path.display()
                ))
            })?;
        match lock_file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => continue,
            Err(fs::TryLockError::Error(error)) => {
                return super::native_error(format!(
                    "Could not lock output recovery record {}: {error}",
                    path.display()
                ));
            }
        }

        let record = read_record(&path).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not validate output recovery record {}: {error}",
                path.display()
            ))
        })?;
        if !output_parent_available(&record).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not inspect output recovery parent for {}: {error}",
                path.display()
            ))
        })? {
            // The destination may live on removable/offline storage. Absence of
            // the mount is not proof that Rustnote's hidden sibling is absent.
            // Keep the durable record and retry on a later startup/re-export.
            continue;
        }
        reconcile_record_state(&record, &path).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not reconcile abandoned output transaction: {error}"
            ))
        })?;
        drop(lock_file);
        retire_journal_locked(&path, &root).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not retire recovered output transaction: {error}"
            ))
        })?;
        recovered += 1;
    }
    Ok(recovered)
}

fn reconcile_record_state(record: &OutputRecord, journal_path: &Path) -> io::Result<()> {
    validate_record(record, journal_path)?;
    if !output_parent_available(record)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "output destination parent is unavailable; recovery ownership must be preserved",
        ));
    }
    match record.kind {
        OutputKind::File => reconcile_file_record(record),
        OutputKind::Directory => reconcile_directory_record(record),
    }
}

fn reconcile_file_record(record: &OutputRecord) -> io::Result<()> {
    let destination_exists = path_exists(&record.destination)?;
    let temporary_exists = path_exists(&record.temporary)?;
    let previous_exists = match record.previous.as_ref() {
        Some(previous) => path_exists(previous)?,
        None => false,
    };

    if !record.had_previous {
        if temporary_exists {
            remove_owned_output_file(&record.temporary, &record.operation_id)?;
        }
        // Destination + no temporary means publication completed. Neither path
        // means the transaction never published anything (or the external user
        // subsequently removed the completed destination); both are safe states.
        return Ok(());
    }

    let previous = record.previous.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "replacement transaction has no predecessor",
        )
    })?;
    match (destination_exists, temporary_exists, previous_exists) {
        // Preparation never displaced the user's original destination.
        (true, true, false) => remove_owned_output_file(&record.temporary, &record.operation_id),
        // Publication completed; only the Rustnote-hidden predecessor remains.
        (true, false, true) => remove_owned_output_file(previous, &record.operation_id),
        // Crash after preserving the original but before publishing the new file.
        (false, true, true) => {
            restore_owned_predecessor(previous, &record.destination, &record.operation_id)?;
            remove_owned_output_file(&record.temporary, &record.operation_id)
        }
        // Same rollback state, but the temporary was already removed.
        (false, false, true) => {
            restore_owned_predecessor(previous, &record.destination, &record.operation_id)
        }
        // Either commit completed including predecessor cleanup, or begin()
        // persisted its record before any hidden sibling was materialized.
        (true, false, false) => Ok(()),
        // Three simultaneous files imply external interference. Preserve every
        // byte and the durable journal rather than guessing which file wins.
        (true, true, true) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "output recovery is ambiguous because destination, temporary, and predecessor all exist",
        )),
        (false, true, false) | (false, false, false) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "output recovery lost the previously existing destination",
        )),
    }
}

fn reconcile_directory_record(record: &OutputRecord) -> io::Result<()> {
    let destination_exists = path_exists(&record.destination)?;
    let temporary_exists = path_exists(&record.temporary)?;
    match (destination_exists, temporary_exists) {
        (false, true) | (true, true) => {
            // A successful same-parent rename cannot leave both paths behind.
            // Therefore destination + temporary is external interference before
            // publication, not an ambiguous Rustnote commit state. Keep the
            // external destination and discard only Rustnote-owned staging.
            remove_owned_output_tree(&record.temporary, &record.operation_id)
        }
        (true, false) | (false, false) => Ok(()),
    }
}

fn restore_owned_predecessor(
    previous: &Path,
    destination: &Path,
    operation_id: &str,
) -> io::Result<()> {
    let mut owned = OwnedPlaintext::durable_external_output_existing(
        previous.to_path_buf(),
        operation_id.to_string(),
    );
    owned.publish_external_output(destination)?;
    sync_directory(parent_of(destination)?)
}

fn remove_owned_output_file(path: &Path, operation_id: &str) -> io::Result<()> {
    let mut owned = OwnedPlaintext::durable_external_output_existing(
        path.to_path_buf(),
        operation_id.to_string(),
    );
    owned.remove_file()?;
    sync_directory(parent_of(path)?)
}

fn remove_owned_output_tree(path: &Path, operation_id: &str) -> io::Result<()> {
    let mut owned = OwnedPlaintext::durable_external_output_existing(
        path.to_path_buf(),
        operation_id.to_string(),
    );
    owned.remove_external_output_tree()?;
    sync_directory(parent_of(path)?)
}

fn persist_record(record: &OutputRecord) -> io::Result<(PathBuf, File)> {
    validate_record_shape(record)?;
    validate_record_checksum(record)?;
    // Serialize before publishing the record. A serialization failure therefore
    // cannot create a corrupt recovery file for an operation that never began.
    let mut bytes = serde_json::to_vec(record).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_OUTPUT_JOURNAL_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output recovery record is too large",
        ));
    }

    let root = journal_root()?;
    let _ledger_lock = lock_output_ledger(&root)?;
    ensure_destination_available_locked(&root, &record.destination)?;
    let path = root.join(format!("{}.json", record.operation_id));
    let persist_result = (|| -> io::Result<File> {
        let mut file = super::private_temp::create_private_file(&path)?;
        // Lock before writing. A concurrent Rustnote startup that observes this
        // just-created inode will skip it instead of parsing a partial record.
        file.lock()?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        sync_directory(&root)?;
        Ok(file)
    })();
    match persist_result {
        Ok(file) => Ok((path, file)),
        Err(error) => {
            // No output sibling can exist before begin() returns. Removing a
            // failed metadata-only record is therefore safe.
            let _ = fs::remove_file(&path);
            let _ = sync_directory(&root);
            Err(error)
        }
    }
}

fn read_record(path: &Path) -> io::Result<OutputRecord> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_OUTPUT_JOURNAL_BYTES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid output recovery record file",
        ));
    }
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_OUTPUT_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OUTPUT_JOURNAL_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output recovery record is too large",
        ));
    }
    let record: OutputRecord = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    validate_record(&record, path)?;
    Ok(record)
}

fn validate_record(record: &OutputRecord, journal_path: &Path) -> io::Result<()> {
    validate_record_shape(record)?;
    validate_record_checksum(record)?;
    let expected = journal_root()?.join(format!("{}.json", record.operation_id));
    if journal_path != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output recovery record identity mismatch",
        ));
    }
    Ok(())
}

fn validate_record_shape(record: &OutputRecord) -> io::Result<()> {
    if record.version != OUTPUT_JOURNAL_VERSION || !valid_token(&record.operation_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid output transaction identity",
        ));
    }
    if !record.destination.is_absolute()
        || !record.temporary.is_absolute()
        || record.previous.as_ref().is_some_and(|path| !path.is_absolute())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output recovery paths must be absolute",
        ));
    }
    let parent = parent_of(&record.destination)?;
    if record.temporary.parent() != Some(parent) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output temporary escaped destination parent",
        ));
    }
    match record.kind {
        OutputKind::File => {
            let expected_temporary =
                parent.join(format!(".rustnote-output-{}.tmp", record.operation_id));
            if record.temporary != expected_temporary {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "output temporary identity mismatch",
                ));
            }
            let expected_previous =
                parent.join(format!(".rustnote-output-{}.previous", record.operation_id));
            match (&record.previous, record.had_previous) {
                (Some(previous), true) if *previous == expected_previous => {}
                (None, false) => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "output predecessor identity mismatch",
                    ));
                }
            }
        }
        OutputKind::Directory => {
            let expected_temporary =
                parent.join(format!(".rustnote-output-{}.dir.tmp", record.operation_id));
            if record.had_previous
                || record.previous.is_some()
                || record.temporary != expected_temporary
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid directory output transaction",
                ));
            }
        }
    }
    Ok(())
}

fn record_checksum(record: &OutputRecord) -> io::Result<String> {
    let payload = OutputRecordChecksum {
        version: record.version,
        operation_id: &record.operation_id,
        kind: record.kind,
        destination: &record.destination,
        temporary: &record.temporary,
        previous: record.previous.as_deref(),
        had_previous: record.had_previous,
    };
    let encoded = serde_json::to_vec(&payload).map_err(io::Error::other)?;
    let mut hasher = Sha256::new();
    hasher.update(OUTPUT_RECORD_CHECKSUM_DOMAIN);
    hasher.update(encoded);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_record_checksum(record: &OutputRecord) -> io::Result<()> {
    let expected = record_checksum(record)?;
    if record.checksum != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "output recovery record checksum mismatch",
        ));
    }
    Ok(())
}

#[cfg(not(any(test, feature = "internal-test-support")))]
fn journal_root() -> io::Result<PathBuf> {
    let root = crate::app_paths::durable_configuration_directory()?.join(OUTPUT_JOURNAL_NAMESPACE);
    super::private_temp::ensure_private_directory(&root)?;
    Ok(root)
}

#[cfg(any(test, feature = "internal-test-support"))]
fn journal_root() -> io::Result<PathBuf> {
    // Unit tests must never create recovery metadata in the user's real
    // application configuration directory. A process-scoped OS-temp root also
    // keeps parallel tests isolated from normal Rustnote startup recovery.
    let root = std::env::temp_dir().join(format!(
        "rustnote-output-journal-test-{}",
        std::process::id()
    ));
    super::private_temp::ensure_private_directory(&root)?;
    Ok(root)
}

fn output_snapshot_root() -> io::Result<PathBuf> {
    super::private_temp::rustnote_temp_directory(OUTPUT_SNAPSHOT_NAMESPACE)
}

fn retire_journal(path: &Path) -> io::Result<()> {
    let root = journal_root()?;
    let _ledger_lock = lock_output_ledger(&root)?;
    retire_journal_locked(path, &root)
}

fn retire_journal_locked(path: &Path, root: &Path) -> io::Result<()> {
    if path.parent() != Some(root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output recovery record escaped its ledger",
        ));
    }
    match fs::remove_file(path) {
        Ok(()) => sync_directory(root),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Serialize ledger membership changes, not the potentially long output write
/// itself. An active record then acts as a durable destination reservation: a
/// second process cannot begin another transaction for the same canonical
/// destination until the first record is retired.
fn lock_output_ledger(root: &Path) -> io::Result<File> {
    let path = root.join(OUTPUT_LEDGER_LOCK_FILE);
    let file = loop {
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "output ledger lock is not a regular file",
                    ));
                }
                let file = OpenOptions::new().read(true).write(true).open(&path)?;
                super::private_temp::harden_existing_file(&path)?;
                break file;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match super::private_temp::create_private_file_in_existing_directory(&path) {
                    Ok(file) => break file,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
    };
    file.lock()?;
    Ok(file)
}

fn ensure_destination_available_locked(root: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name().to_str() == Some(OUTPUT_LEDGER_LOCK_FILE) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "output recovery ledger contains an unexpected non-file entry",
            ));
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "output recovery ledger contains an unknown record",
            ));
        }
        let record = read_record(&path)?;
        if record.destination == destination {
            let lock_file = OpenOptions::new().read(true).write(true).open(&path)?;
            match lock_file.try_lock() {
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "another Rustnote output transaction already owns this destination",
                    ));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error),
                Ok(()) => {
                    // The prior owner process is gone (or the failed live
                    // transaction has already released its lock). Reconcile
                    // that exact destination before admitting a retry.
                    reconcile_record_state(&record, &path)?;
                    drop(lock_file);
                    retire_journal_locked(&path, root)?;
                }
            }
        }
    }
    Ok(())
}

fn output_parent_available(record: &OutputRecord) -> io::Result<bool> {
    let parent = parent_of(&record.destination)?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "output destination parent is no longer a real directory",
            ))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn validate_output_parent(parent: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(parent)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output parent is not a real directory",
        ));
    }
    Ok(())
}

fn validate_existing_file_destination(destination: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "existing output destination is not a regular file",
            ))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = parent_of(&absolute)?;
    let file_name = absolute.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output destination has no file name")
    })?;
    // The parent must exist for output preparation anyway. Canonicalizing it
    // collapses `..` and symlink aliases so concurrent reservations cannot
    // evade same-destination exclusion with two spellings of the same path.
    Ok(fs::canonicalize(parent)?.join(file_name))
}

fn parent_of(path: &Path) -> io::Result<&Path> {
    path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })
}

fn path_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn require_path_absent(path: &Path) -> io::Result<()> {
    if path_exists(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Rustnote output staging path already exists",
        ));
    }
    Ok(())
}

fn valid_token(token: &str) -> bool {
    token.len() == 32
        && token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn output_token() -> String {
    rand::random::<[u8; 16]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustnote-output-test-{label}-{}", output_token()))
    }

    #[test]
    fn file_transaction_restores_previous_after_abandonment_before_publish() {
        let root = root("rollback");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        fs::write(&destination, b"old").expect("old");
        let mut transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let mut file = transaction.create_file().expect("owned file");
        file.write_all(b"new").expect("write");
        file.sync_all().expect("sync");
        drop(file);
        transaction
            .predecessor
            .as_mut()
            .expect("predecessor")
            .capture_external_predecessor(&transaction.record.destination)
            .expect("preserve previous");
        drop(transaction);
        assert_eq!(fs::read(&destination).expect("restored"), b"old");
        assert_eq!(fs::read_dir(&root).expect("entries").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_transaction_commits_and_retires_hidden_predecessor() {
        let root = root("commit");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        fs::write(&destination, b"old").expect("old");
        let mut transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let mut file = transaction.create_file().expect("owned file");
        file.write_all(b"new").expect("write");
        file.sync_all().expect("sync");
        drop(file);
        transaction.commit().expect("commit");
        assert_eq!(fs::read(&destination).expect("new"), b"new");
        assert_eq!(fs::read_dir(&root).expect("entries").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_rolls_back_preserved_predecessor_and_partial_temporary() {
        let root = root("recover-predecessor");
        fs::create_dir(&root).expect("root");
        let destination = absolute_path(&root.join("note.txt")).expect("absolute destination");
        let operation_id = output_token();
        let temporary = root.join(format!(".rustnote-output-{operation_id}.tmp"));
        let previous = root.join(format!(".rustnote-output-{operation_id}.previous"));
        fs::write(&temporary, b"partial-new").expect("temporary");
        fs::write(&previous, b"old").expect("previous");
        let record = OutputRecord::new(
            operation_id,
            OutputKind::File,
            destination.clone(),
            absolute_path(&temporary).expect("absolute temporary"),
            Some(absolute_path(&previous).expect("absolute previous")),
            true,
        )
        .expect("record");
        reconcile_file_record(&record).expect("reconcile");
        assert_eq!(fs::read(destination).expect("restored"), b"old");
        assert!(!temporary.exists());
        assert!(!previous.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn directory_transaction_owns_tree_before_children_exist() {
        let root = root("directory");
        fs::create_dir(&root).expect("root");
        let destination = root.join("Export");
        let transaction = ExternalDirectoryOutputTransaction::begin(&destination).expect("begin");
        fs::write(transaction.temporary_path().join("note.md"), b"body").expect("child");
        let final_path = transaction.commit().expect("commit");
        assert_eq!(fs::read(final_path.join("note.md")).expect("body"), b"body");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_transactions_for_same_destination_fail_closed() {
        let root = root("same-destination");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        let transaction = ExternalFileOutputTransaction::begin(&destination).expect("first begin");

        let error = ExternalFileOutputTransaction::begin(&destination)
            .err()
            .expect("second begin must fail");
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);

        drop(transaction);
        ExternalFileOutputTransaction::begin(&destination)
            .expect("destination reservation released after transaction cleanup");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn same_destination_aliases_share_one_reservation() {
        let root = root("destination-alias");
        fs::create_dir(&root).expect("root");
        let subdirectory = root.join("sub");
        fs::create_dir(&subdirectory).expect("subdirectory");
        let destination = root.join("note.txt");
        let alias = subdirectory.join("..").join("note.txt");
        let transaction = ExternalFileOutputTransaction::begin(&destination).expect("first begin");

        let error = ExternalFileOutputTransaction::begin(&alias)
            .err()
            .expect("alias begin must fail");
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);

        drop(transaction);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_reconciles_released_stale_destination_reservation() {
        let root = root("stale-retry");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        let mut transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let stale_temporary = transaction.record.temporary.clone();
        let mut file = transaction.create_file().expect("owned file");
        file.write_all(b"partial").expect("write");
        file.sync_all().expect("sync");
        drop(file);
        transaction.preserve_for_recovery = true;
        drop(transaction);

        let retry = ExternalFileOutputTransaction::begin(&destination)
            .expect("retry should reconcile stale predecessor transaction");
        assert!(!stale_temporary.exists());
        drop(retry);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_output_parent_preserves_recovery_record() {
        let root = root("offline-parent");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        let mut transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let journal_path = transaction.journal_path.clone();
        let temporary_name = transaction
            .temporary_path()
            .file_name()
            .expect("temporary name")
            .to_os_string();
        let mut file = transaction.create_file().expect("owned file");
        file.write_all(b"secret").expect("write");
        file.sync_all().expect("sync");
        drop(file);

        let detached = root.with_file_name(format!(
            "{}-detached",
            root.file_name().expect("root name").to_string_lossy()
        ));
        fs::rename(&root, &detached).expect("simulate offline parent");
        drop(transaction);
        assert!(journal_path.is_file(), "offline parent must retain recovery ownership");
        assert!(detached.join(&temporary_name).is_file(), "hidden plaintext still exists offline");

        fs::rename(&detached, &root).expect("reconnect parent");
        let retry = ExternalFileOutputTransaction::begin(&destination)
            .expect("retry reconciles preserved offline transaction");
        assert!(!root.join(&temporary_name).exists());
        drop(retry);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn new_file_destination_race_preserves_external_file() {
        let root = root("file-destination-race");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        let mut transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let mut file = transaction.create_file().expect("owned file");
        file.write_all(b"rustnote").expect("write");
        file.sync_all().expect("sync");
        drop(file);
        fs::write(&destination, b"external").expect("external race");

        let error = transaction.commit().expect_err("race must fail closed");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&destination).expect("external file"), b"external");
        assert_eq!(fs::read_dir(&root).expect("entries").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn directory_destination_race_preserves_external_tree() {
        let root = root("directory-destination-race");
        fs::create_dir(&root).expect("root");
        let destination = root.join("Export");
        let transaction = ExternalDirectoryOutputTransaction::begin(&destination).expect("begin");
        fs::write(transaction.temporary_path().join("note.md"), b"rustnote").expect("child");
        fs::create_dir(&destination).expect("external race");
        fs::write(destination.join("external.txt"), b"external").expect("external child");

        let error = transaction.commit().expect_err("race must fail closed");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(destination.join("external.txt")).expect("external child"),
            b"external"
        );
        assert_eq!(fs::read_dir(&root).expect("entries").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn output_record_checksum_detects_valid_json_path_corruption() {
        let root = root("record-corruption");
        fs::create_dir(&root).expect("root");
        let destination = root.join("note.txt");
        let transaction = ExternalFileOutputTransaction::begin(&destination).expect("begin");
        let mut record = read_record(&transaction.journal_path).expect("record");
        record.destination = absolute_path(&root.join("other.txt")).expect("other destination");
        let mut bytes = serde_json::to_vec(&record).expect("encode corrupt record");
        bytes.push(b'\n');
        fs::write(&transaction.journal_path, bytes).expect("write corrupt record");

        let error = read_record(&transaction.journal_path).expect_err("checksum must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("checksum"));

        // Restore the valid record so Drop can reconcile and retire its own
        // transaction rather than intentionally leaving a corrupt test record.
        let mut valid = transaction.record.clone();
        valid.checksum = record_checksum(&valid).expect("checksum");
        let mut valid_bytes = serde_json::to_vec(&valid).expect("encode valid record");
        valid_bytes.push(b'\n');
        fs::write(&transaction.journal_path, valid_bytes).expect("restore record");
        drop(transaction);
        let _ = fs::remove_dir_all(root);
    }
}
