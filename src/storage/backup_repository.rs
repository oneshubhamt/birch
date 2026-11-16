use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::build_info::FULL_BUILD_IDENTIFIER;
use crate::domain::{
    BackupRepositorySnapshot, BackupRepositoryStatus, BackupSnapshotKind, CompleteArchiveSummary,
};

use super::attachments::{
    atomic_replace, copy_file_atomically, sha256_file, sync_parent_directory, validate_sha256,
};
use super::backups::{
    DATABASE_ARCHIVE_PATH, StagedCompleteArchive, discard_restore_stage_directory,
};
use super::{Storage, StorageError, StorageResult, new_id, unix_timestamp};

const REPOSITORY_FORMAT_VERSION: u32 = 2;
const SNAPSHOT_MANIFEST_VERSION: u32 = 1;
const INDEX_FILE_NAME: &str = "repository.json";
// The authoritative repository owner lives beside, never inside, the
// recursively deletable repository tree so cleanup authority survives removal.
const BACKUP_NAMESPACE_MARKER_FILE_NAME: &str = ".rustnote-backup-repository-owner";
const REPOSITORY_DIRECTORY_NAME: &str = "Rustnote Repository";
const DEFAULT_BACKUP_BUDGET_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const MIN_BACKUP_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BACKUP_BUDGET_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_AUTOMATIC_SNAPSHOTS: usize = 7;
const MAX_SAFETY_SNAPSHOTS: usize = 3;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const REPOSITORY_METADATA_RESERVE_BYTES: u64 = 64 * 1024;
const BACKUP_BUDGET_SETTING: &str = "automatic_backup_budget_bytes";
const QUARANTINE_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_QUARANTINE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_QUARANTINE_DETAILS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryBlobReference {
    sha256: String,
    byte_size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositorySnapshotManifest {
    format_version: u32,
    build_identifier: String,
    snapshot_id: String,
    kind: BackupSnapshotKind,
    created_at: i64,
    database_sha256: String,
    database_bytes: u64,
    blobs: Vec<RepositoryBlobReference>,
    notebook_count: usize,
    note_count: usize,
    attachment_count: usize,
    revision_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositorySnapshotIndexEntry {
    snapshot_id: String,
    kind: BackupSnapshotKind,
    created_at: i64,
    database_bytes: u64,
    blob_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupRepositoryIndex {
    format_version: u32,
    library_id: String,
    last_attempt_at: Option<i64>,
    last_result: String,
    snapshots: Vec<RepositorySnapshotIndexEntry>,
}

impl BackupRepositoryIndex {
    fn empty(library_id: &str) -> Self {
        Self {
            format_version: REPOSITORY_FORMAT_VERSION,
            library_id: library_id.to_string(),
            last_attempt_at: None,
            last_result: "No automatic backup has run yet.".to_string(),
            snapshots: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct InvalidRepositoryArtifact {
    manifest_path: PathBuf,
    snapshot_id: Option<String>,
    reason: String,
}

#[derive(Debug, Default)]
struct RepositoryManifestLoad {
    valid: Vec<RepositorySnapshotManifest>,
    invalid: Vec<InvalidRepositoryArtifact>,
}

#[derive(Debug)]
struct RepositoryQuarantineEntry {
    path: PathBuf,
    created_at: i64,
    byte_size: u64,
}

#[derive(Clone, Debug)]
struct RepositoryLayout {
    root: PathBuf,
    blobs: PathBuf,
    snapshots: PathBuf,
    manifests: PathBuf,
    staging: PathBuf,
    quarantine: PathBuf,
    index: PathBuf,
}

impl RepositoryLayout {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            blobs: root.join("blobs"),
            snapshots: root.join("snapshots"),
            manifests: root.join("manifests"),
            staging: root.join(".staging"),
            quarantine: root.join("quarantine"),
            index: root.join(INDEX_FILE_NAME),
        }
    }

    fn ensure(&self) -> StorageResult<()> {
        for directory in [
            &self.root,
            &self.blobs,
            &self.snapshots,
            &self.manifests,
            &self.staging,
            &self.quarantine,
        ] {
            ensure_repository_directory(directory)?;
        }
        Ok(())
    }

    fn snapshot_path(&self, snapshot_id: &str) -> PathBuf {
        self.snapshots.join(format!("{snapshot_id}.sqlite3"))
    }

    fn manifest_path(&self, snapshot_id: &str) -> PathBuf {
        self.manifests.join(format!("{snapshot_id}.json"))
    }

    fn blob_path(&self, sha256: &str) -> PathBuf {
        self.blobs.join(sha256)
    }
}

fn ensure_repository_directory(path: &Path) -> StorageResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Integrity(format!(
                    "automatic backup repository path is not a real directory: {}",
                    path.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Integrity(format!(
                    "automatic backup repository path resolved unsafely: {}",
                    path.display()
                )));
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn validate_snapshot_id(value: &str) -> StorageResult<()> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(StorageError::InvalidBackup(
            "backup snapshot has an invalid identifier".to_string(),
        ));
    }
    Ok(())
}

fn system_time_to_unix_timestamp(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn directory_file_bytes(root: &Path) -> StorageResult<u64> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StorageError::Io(error)),
            };
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let metadata = match entry.metadata() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(StorageError::Io(error)),
                };
                total = total.checked_add(metadata.len()).ok_or_else(|| {
                    StorageError::Integrity("backup repository size overflow".to_string())
                })?;
            }
        }
    }
    Ok(total)
}

fn copy_repository_blob_atomically(
    source: &Path,
    destination: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> StorageResult<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut transaction = crate::native::output::ExternalFileOutputTransaction::begin(destination)?;
    let (actual_sha256, actual_bytes) = transaction.copy_and_hash_from(source)?;
    if actual_sha256 != expected_sha256 || actual_bytes != expected_bytes {
        return Err(StorageError::Integrity(format!(
            "active attachment blob {expected_sha256} is missing or corrupt"
        )));
    }
    transaction.commit()?;
    Ok(())
}

fn repository_blob_metadata_matches(path: &Path, expected_bytes: u64) -> StorageResult<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StorageError::Integrity(format!(
            "backup blob destination {} is not a regular file",
            path.display()
        )));
    }
    Ok(metadata.len() == expected_bytes)
}

fn repository_blob_matches(
    path: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> StorageResult<bool> {
    if !repository_blob_metadata_matches(path, expected_bytes)? {
        return Ok(false);
    }
    let (actual_sha256, actual_bytes) = sha256_file(path)?;
    Ok(actual_sha256 == expected_sha256 && actual_bytes == expected_bytes)
}

fn ensure_repository_blob(
    source: &Path,
    destination: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> StorageResult<()> {
    // Repository blob names are content hashes and committed blobs are immutable.
    // Routine snapshot creation therefore trusts an existing regular file of the
    // expected size. Full checksum verification remains on inspect/restore paths.
    if repository_blob_metadata_matches(destination, expected_bytes)? {
        return Ok(());
    }
    copy_repository_blob_atomically(source, destination, expected_sha256, expected_bytes)
}

fn read_bounded(path: &Path, limit: u64) -> StorageResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(StorageError::InvalidBackup(format!(
            "{} is not safe bounded repository metadata",
            path.display()
        )));
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| {
        StorageError::InvalidBackup(
            "repository metadata is too large for this platform".to_string(),
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    BufReader::new(File::open(path)?).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn validate_manifest_metadata(manifest: &RepositorySnapshotManifest) -> StorageResult<()> {
    if manifest.format_version != SNAPSHOT_MANIFEST_VERSION {
        return Err(StorageError::InvalidBackup(format!(
            "unsupported backup repository snapshot version {}",
            manifest.format_version
        )));
    }
    validate_snapshot_id(&manifest.snapshot_id)?;
    validate_sha256(&manifest.database_sha256)?;
    if manifest.database_bytes == 0 {
        return Err(StorageError::InvalidBackup(
            "backup snapshot database is empty".to_string(),
        ));
    }
    let mut previous = None::<&str>;
    for blob in &manifest.blobs {
        validate_sha256(&blob.sha256)?;
        if previous.is_some_and(|value| value >= blob.sha256.as_str()) {
            return Err(StorageError::InvalidBackup(
                "backup snapshot blob list is not unique and sorted".to_string(),
            ));
        }
        previous = Some(&blob.sha256);
    }
    Ok(())
}

fn manifests_equivalent(
    left: &RepositorySnapshotManifest,
    right: &RepositorySnapshotManifest,
) -> bool {
    left.database_sha256 == right.database_sha256 && left.blobs == right.blobs
}

fn manifest_usage(manifests: &[RepositorySnapshotManifest]) -> StorageResult<u64> {
    let mut bytes = REPOSITORY_METADATA_RESERVE_BYTES;
    let mut blobs = HashMap::<&str, u64>::new();
    for manifest in manifests {
        bytes = bytes.checked_add(manifest.database_bytes).ok_or_else(|| {
            StorageError::Integrity("backup repository size overflow".to_string())
        })?;
        bytes = bytes
            .checked_add(
                u64::try_from(serde_json::to_vec_pretty(manifest)?.len()).map_err(|_| {
                    StorageError::Integrity("backup manifest size overflow".to_string())
                })?,
            )
            .ok_or_else(|| {
                StorageError::Integrity("backup repository size overflow".to_string())
            })?;
        for blob in &manifest.blobs {
            match blobs.insert(blob.sha256.as_str(), blob.byte_size) {
                Some(previous) if previous != blob.byte_size => {
                    return Err(StorageError::Integrity(format!(
                        "backup blob {} has conflicting sizes",
                        blob.sha256
                    )));
                }
                _ => {}
            }
        }
    }
    for size in blobs.into_values() {
        bytes = bytes.checked_add(size).ok_or_else(|| {
            StorageError::Integrity("backup repository size overflow".to_string())
        })?;
    }
    Ok(bytes)
}

fn retained_manifests(
    mut manifests: Vec<RepositorySnapshotManifest>,
    budget_bytes: u64,
    protected_snapshot: Option<&str>,
) -> StorageResult<Vec<RepositorySnapshotManifest>> {
    manifests.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.snapshot_id.cmp(&left.snapshot_id))
    });

    let protected = protected_snapshot.and_then(|snapshot_id| {
        manifests
            .iter()
            .find(|manifest| manifest.snapshot_id == snapshot_id)
            .cloned()
    });
    let mut automatic = 0_usize;
    let mut safety = 0_usize;
    let mut kept = Vec::new();
    for manifest in manifests.iter().cloned() {
        let allowed = match manifest.kind {
            BackupSnapshotKind::Automatic => {
                automatic += 1;
                automatic <= MAX_AUTOMATIC_SNAPSHOTS
            }
            BackupSnapshotKind::Safety => {
                safety += 1;
                safety <= MAX_SAFETY_SNAPSHOTS
            }
        };
        if allowed {
            kept.push(manifest);
        }
    }
    if let Some(protected) = protected
        && !kept
            .iter()
            .any(|manifest| manifest.snapshot_id == protected.snapshot_id)
    {
        if let Some(index) = kept
            .iter()
            .rposition(|manifest| manifest.kind == protected.kind)
        {
            kept.remove(index);
        }
        kept.push(protected);
        kept.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.snapshot_id.cmp(&left.snapshot_id))
        });
    }

    while kept.len() > 1 && manifest_usage(&kept)? > budget_bytes {
        let removable = kept
            .iter()
            .enumerate()
            .rev()
            .find(|(_, manifest)| protected_snapshot != Some(manifest.snapshot_id.as_str()))
            .map(|(index, _)| index);
        let Some(index) = removable else {
            break;
        };
        kept.remove(index);
    }
    Ok(kept)
}

fn backup_namespace_marker(backup_directory: &Path) -> PathBuf {
    backup_directory.join(BACKUP_NAMESPACE_MARKER_FILE_NAME)
}

fn backup_namespace_identity(library_id: &str) -> String {
    format!("backup-repository:{library_id}")
}

impl Storage {
    fn backup_repository_layout_at(&self, backup_directory: &Path) -> RepositoryLayout {
        RepositoryLayout::new(&backup_directory.join(REPOSITORY_DIRECTORY_NAME))
    }

    fn backup_repository_layout(&self) -> RepositoryLayout {
        self.backup_repository_layout_at(&self.automatic_backup_directory)
    }

    fn establish_backup_repository_namespace_unlocked_at(
        &self,
        backup_directory: &Path,
        allow_fresh_repository: bool,
    ) -> StorageResult<Option<crate::security::DurablePlaintextNamespace>> {
        let parent_metadata = match fs::symlink_metadata(backup_directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(StorageError::Integrity(
                "automatic backup parent is not a real directory".to_string(),
            ));
        }

        let layout = self.backup_repository_layout_at(backup_directory);
        let marker = backup_namespace_marker(backup_directory);
        let marker_exists = match fs::symlink_metadata(&marker) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(StorageError::Integrity(
                        "automatic backup namespace owner marker is invalid".to_string(),
                    ));
                }
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };

        if !marker_exists {
            match fs::symlink_metadata(&layout.root) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if !allow_fresh_repository {
                        return Ok(None);
                    }
                }
                Err(error) => return Err(error.into()),
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                    return Err(StorageError::Integrity(
                        "automatic backup repository root is not a real directory".to_string(),
                    ));
                }
                Ok(_) => {
                    let nonempty = fs::read_dir(&layout.root)?.next().transpose()?.is_some();
                    if nonempty {
                        return if allow_fresh_repository {
                            Err(StorageError::Validation(
                                "automatic backup repository has no durable ownership authority and is not empty"
                                    .to_string(),
                            ))
                        } else {
                            Ok(None)
                        };
                    }
                    if !allow_fresh_repository {
                        // During a pending Vault privacy reset, an empty substitute
                        // mount is not proof that the remembered volume returned.
                        return Ok(None);
                    }
                }
            }
        }

        let namespace_result = if marker_exists {
            crate::security::DurablePlaintextNamespace::open_existing(
                layout.root.clone(),
                marker,
                backup_namespace_identity(&self.library_id),
            )
        } else {
            crate::security::DurablePlaintextNamespace::establish(
                layout.root.clone(),
                marker,
                backup_namespace_identity(&self.library_id),
            )
        };
        namespace_result.map(Some).map_err(|error| {
            StorageError::Integrity(format!(
                "automatic backup namespace ownership could not be established: {error}"
            ))
        })
    }

    /// Establish the canonical durable namespace before any repository
    /// scaffolding, mutable index, snapshot, manifest, blob, staging or
    /// quarantine plaintext can be created or used.
    pub(super) fn ensure_backup_repository_identity_unlocked(&self) -> StorageResult<()> {
        let _namespace = self
            .establish_backup_repository_namespace_unlocked_at(
                &self.automatic_backup_directory,
                true,
            )?
            .ok_or_else(|| {
                StorageError::Validation(
                    "automatic backup location is unavailable; repository ownership cannot be established"
                        .to_string(),
                )
            })?;
        let layout = self.backup_repository_layout();
        layout.ensure()?;
        let manifests = self.load_repository_manifests(&layout)?;
        // `repository.json` is operational metadata, not ownership proof. If it
        // was never published (for example after a crash), rebuild it from the
        // manifests under the already-verified external namespace owner.
        let index_needs_rebuild = match read_bounded(&layout.index, MAX_INDEX_BYTES) {
            Ok(bytes) => serde_json::from_slice::<BackupRepositoryIndex>(&bytes)
                .map(|index| {
                    index.format_version != REPOSITORY_FORMAT_VERSION
                        || index.library_id != self.library_id
                })
                .unwrap_or(true),
            Err(StorageError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => true,
        };
        if index_needs_rebuild {
            self.write_repository_index(
                &layout,
                &manifests,
                None,
                "Automatic backup repository metadata recovered.".to_string(),
            )?;
        }
        Ok(())
    }

    fn verified_backup_repository_layout_unlocked(
        &self,
    ) -> StorageResult<(crate::security::DurablePlaintextNamespace, RepositoryLayout)> {
        let layout = self.backup_repository_layout();
        let namespace = crate::security::DurablePlaintextNamespace::open_existing(
            layout.root.clone(),
            backup_namespace_marker(&self.automatic_backup_directory),
            backup_namespace_identity(&self.library_id),
        )
        .map_err(|error| {
            StorageError::Integrity(format!(
                "automatic backup durable ownership is missing or corrupt: {error}"
            ))
        })?;
        Ok((namespace, layout))
    }

    fn purge_owned_backup_repository_for_vault_unlocked_at(
        &self,
        backup_directory: &Path,
    ) -> StorageResult<Option<crate::security::DurablePlaintextNamespace>> {
        let Some(namespace) =
            self.establish_backup_repository_namespace_unlocked_at(backup_directory, false)?
        else {
            return Ok(None);
        };
        let layout = self.backup_repository_layout_at(backup_directory);

        // The owner marker is outside `layout.root`, so arbitrary partial
        // recursive deletion can never destroy the authority needed for retry.
        namespace.remove_tree_strict().map_err(|error| {
            StorageError::Integrity(format!(
                "automatic backup repository Vault cleanup failed: {error}"
            ))
        })?;
        match fs::symlink_metadata(&layout.root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(StorageError::Integrity(
                    "automatic backup repository still exists after Vault cleanup".to_string(),
                ));
            }
            Err(error) => return Err(error.into()),
        }
        Ok(Some(namespace))
    }

    /// E3 privacy boundary for selective encryption.
    ///
    /// Automatic/safety repository snapshots made before a note is protected may
    /// legitimately contain that note in plaintext. They cannot be surgically
    /// rewritten without turning the backup repository into another sync/history
    /// engine, so a protection transition invalidates the app-managed repository
    /// as one unit. A fresh encrypted-state snapshot may be created normally after
    /// the transition.
    pub(super) fn discharge_current_backup_privacy_obligation_unlocked(
        &self,
    ) -> StorageResult<bool> {
        if !super::locations::backup_privacy_reset_required_for(
            &self.library_id,
            &self.data_directory,
            &self.automatic_backup_directory,
        )? {
            return Ok(false);
        }

        // During a privacy-pending open, do not manufacture a missing namespace
        // on an empty substitute mount. Only the surviving external marker can
        // authorize cleanup of an existing repository.
        if self
            .purge_owned_backup_repository_for_vault_unlocked_at(&self.automatic_backup_directory)?
            .is_none()
        {
            return Err(StorageError::Validation(
                "the privacy-pending backup location is present, but Rustnote cannot prove that the remembered repository is mounted there; refusing to clear its Vault privacy obligation"
                    .to_string(),
            ));
        }
        self.ensure_backup_repository_identity_unlocked()?;
        super::locations::clear_backup_privacy_reset_required_for(
            &self.library_id,
            &self.data_directory,
            &self.automatic_backup_directory,
        )?;
        Ok(true)
    }

    pub(super) fn reset_backup_repository_for_vault_unlocked(&self) -> StorageResult<()> {
        // Every automatic-backup parent Rustnote has used remains part of the E3
        // privacy boundary even after the configured location changes. Publish
        // the durable obligation before touching any repository so offline
        // locations remain pending until positively invalidated.
        let backup_directories = super::locations::begin_backup_privacy_reset_for(
            &self.library_id,
            &self.data_directory,
        )?;
        self.reset_backup_repositories_for_vault_unlocked(&backup_directories)
    }

    fn reset_backup_repositories_for_vault_unlocked(
        &self,
        historical_backup_directories: &[PathBuf],
    ) -> StorageResult<()> {
        let mut backup_directories = historical_backup_directories.to_vec();
        if !backup_directories
            .iter()
            .any(|path| path == &self.automatic_backup_directory)
        {
            backup_directories.push(self.automatic_backup_directory.clone());
        }
        for backup_directory in &backup_directories {
            let metadata = match fs::symlink_metadata(backup_directory) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // Never recreate a vanished removable-volume mount path. The
                    // durable privacy obligation remains pending.
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }

            let _historical_lock = if backup_directory != &self.automatic_backup_directory {
                Some(super::DirectoryOwnership::acquire(
                    backup_directory,
                    super::DirectoryLockKind::Backup,
                )?)
            } else {
                None
            };

            let Some(_namespace) =
                self.purge_owned_backup_repository_for_vault_unlocked_at(backup_directory)?
            else {
                // Parent existence alone is not proof that a removable repository
                // returned. Preserve the durable obligation rather than claiming
                // a substitute path.
                continue;
            };

            // Recreate only the dedicated Rustnote child while the external
            // namespace owner remains durable. The user-selected parent and any
            // unrelated files in it are never recursively deleted.
            let clean_layout = self.backup_repository_layout_at(backup_directory);
            clean_layout.ensure()?;
            self.write_repository_index(
                &clean_layout,
                &[],
                None,
                if backup_directory == &self.automatic_backup_directory {
                    "Automatic backups reset after Rustnote vault protection changed.".to_string()
                } else {
                    "Historical backup repository privacy-clean after Rustnote vault protection."
                        .to_string()
                },
            )?;
            super::locations::clear_backup_privacy_reset_required_for(
                &self.library_id,
                &self.data_directory,
                backup_directory,
            )?;
        }
        Ok(())
    }

    fn current_backup_privacy_reset_pending_unlocked(&self) -> StorageResult<bool> {
        super::locations::backup_privacy_reset_required_for(
            &self.library_id,
            &self.data_directory,
            &self.automatic_backup_directory,
        )
    }

    fn require_current_backup_privacy_ready_unlocked(&self) -> StorageResult<()> {
        if self.current_backup_privacy_reset_pending_unlocked()? {
            return Err(StorageError::Validation(
                "automatic backup repository is unavailable or still requires Vault privacy cleanup; reconnect it and reopen Rustnote before using backups"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn backup_repository_budget_bytes(&self) -> StorageResult<u64> {
        Ok(self
            .setting(BACKUP_BUDGET_SETTING)?
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| (MIN_BACKUP_BUDGET_BYTES..=MAX_BACKUP_BUDGET_BYTES).contains(value))
            .unwrap_or(DEFAULT_BACKUP_BUDGET_BYTES))
    }

    pub fn set_backup_repository_budget_bytes(
        &self,
        budget_bytes: u64,
    ) -> StorageResult<BackupRepositoryStatus> {
        if !(MIN_BACKUP_BUDGET_BYTES..=MAX_BACKUP_BUDGET_BYTES).contains(&budget_bytes) {
            return Err(StorageError::Validation(
                "backup storage budget must be between 512 MB and 1 TB".to_string(),
            ));
        }
        let _operation_guard = self.lock_operations()?;
        let previous_budget = self.backup_repository_budget_bytes()?;
        self.set_setting(BACKUP_BUDGET_SETTING, &budget_bytes.to_string())?;
        if let Err(error) = self.prune_backup_repository_unlocked(None) {
            let _ = self.set_setting(BACKUP_BUDGET_SETTING, &previous_budget.to_string());
            return Err(error);
        }
        self.backup_repository_status_unlocked()
    }

    fn load_repository_index(&self, layout: &RepositoryLayout) -> BackupRepositoryIndex {
        let Ok(bytes) = read_bounded(&layout.index, MAX_INDEX_BYTES) else {
            return BackupRepositoryIndex::empty(&self.library_id);
        };
        let Ok(index) = serde_json::from_slice::<BackupRepositoryIndex>(&bytes) else {
            return BackupRepositoryIndex::empty(&self.library_id);
        };
        if index.format_version == REPOSITORY_FORMAT_VERSION && index.library_id == self.library_id
        {
            index
        } else {
            BackupRepositoryIndex::empty(&self.library_id)
        }
    }

    fn write_repository_index(
        &self,
        layout: &RepositoryLayout,
        manifests: &[RepositorySnapshotManifest],
        last_attempt_at: Option<i64>,
        last_result: String,
    ) -> StorageResult<()> {
        let index = BackupRepositoryIndex {
            format_version: REPOSITORY_FORMAT_VERSION,
            library_id: self.library_id.clone(),
            last_attempt_at,
            last_result,
            snapshots: manifests
                .iter()
                .map(|manifest| RepositorySnapshotIndexEntry {
                    snapshot_id: manifest.snapshot_id.clone(),
                    kind: manifest.kind,
                    created_at: manifest.created_at,
                    database_bytes: manifest.database_bytes,
                    blob_count: manifest.blobs.len(),
                })
                .collect(),
        };
        atomic_replace(&layout.index, &serde_json::to_vec_pretty(&index)?)
    }

    fn quarantine_repository_file(
        &self,
        layout: &RepositoryLayout,
        path: &Path,
        label: &str,
    ) -> StorageResult<PathBuf> {
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown");
        let destination = layout.quarantine.join(format!(
            "{}-{label}-{}-{file_name}",
            unix_timestamp(),
            new_id("artifact")
        ));
        fs::rename(path, &destination)?;
        sync_parent_directory(path)?;
        sync_parent_directory(&destination)?;
        Ok(destination)
    }

    fn load_repository_manifests_with_issues(
        &self,
        layout: &RepositoryLayout,
    ) -> StorageResult<RepositoryManifestLoad> {
        layout.ensure()?;
        let mut loaded = RepositoryManifestLoad::default();
        for entry in fs::read_dir(&layout.manifests)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StorageError::Io(error)),
            };
            if !file_type.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let parsed = read_bounded(&path, MAX_MANIFEST_BYTES)
                .and_then(|bytes| {
                    serde_json::from_slice::<RepositorySnapshotManifest>(&bytes)
                        .map_err(StorageError::from)
                })
                .and_then(|manifest| {
                    validate_manifest_metadata(&manifest)?;
                    let expected = layout.manifest_path(&manifest.snapshot_id);
                    if expected != path {
                        return Err(StorageError::InvalidBackup(
                            "backup manifest filename does not match its snapshot identifier"
                                .to_string(),
                        ));
                    }
                    let database_path = layout.snapshot_path(&manifest.snapshot_id);
                    let database_metadata = fs::metadata(&database_path)?;
                    if !database_metadata.is_file()
                        || database_metadata.len() != manifest.database_bytes
                    {
                        return Err(StorageError::InvalidBackup(
                            "backup manifest does not have a matching database snapshot"
                                .to_string(),
                        ));
                    }
                    Ok(manifest)
                });
            match parsed {
                Ok(manifest) => loaded.valid.push(manifest),
                Err(error) => {
                    let snapshot_id = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .filter(|value| validate_snapshot_id(value).is_ok())
                        .map(str::to_string);
                    loaded.invalid.push(InvalidRepositoryArtifact {
                        manifest_path: path,
                        snapshot_id,
                        reason: error.to_string(),
                    });
                }
            }
        }
        loaded.valid.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.snapshot_id.cmp(&right.snapshot_id))
        });
        loaded
            .invalid
            .sort_by_key(|artifact| artifact.manifest_path.clone());
        Ok(loaded)
    }

    fn load_repository_manifests(
        &self,
        layout: &RepositoryLayout,
    ) -> StorageResult<Vec<RepositorySnapshotManifest>> {
        let loaded = self.load_repository_manifests_with_issues(layout)?;
        if !loaded.invalid.is_empty() {
            return Err(StorageError::InvalidBackup(format!(
                "{} backup manifest(s) are invalid; run storage maintenance before continuing",
                loaded.invalid.len()
            )));
        }
        Ok(loaded.valid)
    }

    fn validate_repository_snapshot_structure(
        &self,
        layout: &RepositoryLayout,
        manifest: &RepositorySnapshotManifest,
    ) -> StorageResult<()> {
        validate_manifest_metadata(manifest)?;
        let database_path = layout.snapshot_path(&manifest.snapshot_id);
        let database_metadata = fs::metadata(&database_path)?;
        if !database_metadata.is_file() || database_metadata.len() != manifest.database_bytes {
            return Err(StorageError::Integrity(format!(
                "backup snapshot {} database does not match its manifest",
                manifest.snapshot_id
            )));
        }
        let database = Self::open_snapshot_database(&database_path)?;
        self.validate_snapshot_database(&database)?;
        let mut expected = manifest
            .blobs
            .iter()
            .map(|blob| (blob.sha256.as_str(), blob.byte_size))
            .collect::<HashMap<_, _>>();
        self.for_each_archive_blob(&database, |sha256, _relative_path, byte_size| {
            let Some(manifest_size) = expected.remove(sha256) else {
                return Err(StorageError::Integrity(format!(
                    "backup snapshot {} is missing blob {sha256} from its manifest",
                    manifest.snapshot_id
                )));
            };
            if manifest_size != byte_size {
                return Err(StorageError::Integrity(format!(
                    "backup blob {sha256} size does not match the snapshot database"
                )));
            }
            let blob_metadata = fs::metadata(layout.blob_path(sha256))?;
            if !blob_metadata.is_file() || blob_metadata.len() != byte_size {
                return Err(StorageError::Integrity(format!(
                    "backup blob {sha256} is missing or has the wrong size"
                )));
            }
            Ok(())
        })?;
        if !expected.is_empty() {
            return Err(StorageError::Integrity(
                "backup manifest contains blobs not referenced by its database".to_string(),
            ));
        }
        Ok(())
    }

    fn verify_repository_snapshot_database_payload(
        &self,
        database_path: &Path,
        manifest: &RepositorySnapshotManifest,
    ) -> StorageResult<()> {
        let (database_hash, database_bytes) = sha256_file(database_path)?;
        if database_hash != manifest.database_sha256 || database_bytes != manifest.database_bytes {
            return Err(StorageError::Integrity(format!(
                "backup snapshot {} database checksum does not match its manifest",
                manifest.snapshot_id
            )));
        }
        Ok(())
    }

    fn verify_repository_snapshot_payload(
        &self,
        database_path: &Path,
        layout: &RepositoryLayout,
        manifest: &RepositorySnapshotManifest,
    ) -> StorageResult<()> {
        self.verify_repository_snapshot_database_payload(database_path, manifest)?;
        for blob in &manifest.blobs {
            if !repository_blob_matches(
                &layout.blob_path(&blob.sha256),
                &blob.sha256,
                blob.byte_size,
            )? {
                return Err(StorageError::Integrity(format!(
                    "backup blob {} is missing or corrupt",
                    blob.sha256
                )));
            }
        }
        Ok(())
    }

    fn verify_repository_snapshot(
        &self,
        layout: &RepositoryLayout,
        manifest: &RepositorySnapshotManifest,
    ) -> StorageResult<()> {
        self.validate_repository_snapshot_structure(layout, manifest)?;
        self.verify_repository_snapshot_payload(
            &layout.snapshot_path(&manifest.snapshot_id),
            layout,
            manifest,
        )
    }

    fn cleanup_incomplete_repository_state(
        &self,
        layout: &RepositoryLayout,
        manifests: &[RepositorySnapshotManifest],
    ) -> StorageResult<()> {
        let loaded = RepositoryManifestLoad {
            valid: manifests.to_vec(),
            invalid: Vec::new(),
        };
        let report = self.cleanup_repository_state(layout, &loaded)?;
        if report.found_corruption() {
            return Err(StorageError::Integrity(
                "unexpected backup artifacts were quarantined during an active backup operation"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn cleanup_repository_state(
        &self,
        layout: &RepositoryLayout,
        loaded: &RepositoryManifestLoad,
    ) -> StorageResult<super::BackupRepositoryRecoveryReport> {
        layout.ensure()?;
        let mut report = super::BackupRepositoryRecoveryReport::default();
        let mut protected_quarantine = HashSet::new();
        for entry in fs::read_dir(&layout.staging)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }

        let valid_manifest_paths = loaded
            .valid
            .iter()
            .map(|manifest| layout.manifest_path(&manifest.snapshot_id))
            .collect::<HashSet<_>>();
        let valid_snapshot_ids = loaded
            .valid
            .iter()
            .map(|manifest| manifest.snapshot_id.as_str())
            .collect::<HashSet<_>>();

        for invalid in &loaded.invalid {
            if invalid.manifest_path.exists() {
                let destination = self.quarantine_repository_file(
                    layout,
                    &invalid.manifest_path,
                    "invalid-manifest",
                )?;
                protected_quarantine.insert(destination.clone());
                report.quarantined_manifests += 1;
                if report.details.len() < MAX_QUARANTINE_DETAILS {
                    report
                        .details
                        .push(format!("{}: {}", destination.display(), invalid.reason));
                }
            }
            if let Some(snapshot_id) = invalid.snapshot_id.as_deref() {
                let snapshot_path = layout.snapshot_path(snapshot_id);
                if snapshot_path.exists() {
                    let destination = self.quarantine_repository_file(
                        layout,
                        &snapshot_path,
                        "invalid-snapshot",
                    )?;
                    protected_quarantine.insert(destination.clone());
                    report.quarantined_snapshots += 1;
                    if report.details.len() < MAX_QUARANTINE_DETAILS {
                        report.details.push(format!(
                            "{}: database snapshot associated with an invalid manifest",
                            destination.display()
                        ));
                    }
                }
            }
        }

        for entry in fs::read_dir(&layout.manifests)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_file() && !valid_manifest_paths.contains(&path) {
                let destination =
                    self.quarantine_repository_file(layout, &path, "unknown-manifest")?;
                protected_quarantine.insert(destination.clone());
                report.quarantined_other_artifacts += 1;
                if report.details.len() < MAX_QUARANTINE_DETAILS {
                    report.details.push(format!(
                        "{}: unrecognized backup manifest artifact",
                        destination.display()
                    ));
                }
            }
        }

        for entry in fs::read_dir(&layout.snapshots)? {
            let entry = entry?;
            let path = entry.path();
            let id = path.file_stem().and_then(|value| value.to_str());
            if entry.file_type()?.is_file()
                && id.is_none_or(|value| !valid_snapshot_ids.contains(value))
            {
                let destination =
                    self.quarantine_repository_file(layout, &path, "orphan-snapshot")?;
                protected_quarantine.insert(destination.clone());
                report.quarantined_snapshots += 1;
                if report.details.len() < MAX_QUARANTINE_DETAILS {
                    report.details.push(format!(
                        "{}: database snapshot has no valid manifest",
                        destination.display()
                    ));
                }
            }
        }

        report.purged_quarantine_artifacts =
            self.prune_repository_quarantine(layout, &protected_quarantine)?;
        let quarantine_present = fs::read_dir(&layout.quarantine)?
            .next()
            .transpose()?
            .is_some();
        if !quarantine_present {
            let referenced_blobs = loaded
                .valid
                .iter()
                .flat_map(|manifest| manifest.blobs.iter().map(|blob| blob.sha256.as_str()))
                .collect::<HashSet<_>>();
            for entry in fs::read_dir(&layout.blobs)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let Some(sha256) = file_name.to_str() else {
                    continue;
                };
                if entry.file_type()?.is_file() && !referenced_blobs.contains(sha256) {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        Ok(report)
    }

    fn prune_repository_quarantine(
        &self,
        layout: &RepositoryLayout,
        protected: &HashSet<PathBuf>,
    ) -> StorageResult<usize> {
        let mut entries = self.repository_quarantine_entries(layout)?;
        let now = unix_timestamp();
        let cutoff = now.saturating_sub(QUARANTINE_RETENTION_SECONDS);
        let mut purged = 0_usize;
        for entry in &entries {
            if entry.created_at < cutoff && !protected.contains(&entry.path) {
                fs::remove_file(&entry.path)?;
                purged += 1;
            }
        }
        entries.retain(|entry| entry.path.exists());
        entries.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.path.cmp(&right.path))
        });
        let mut total = entries.iter().try_fold(0_u64, |sum, entry| {
            sum.checked_add(entry.byte_size).ok_or_else(|| {
                StorageError::Integrity("backup quarantine size overflow".to_string())
            })
        })?;
        for entry in entries {
            if total <= MAX_QUARANTINE_BYTES {
                break;
            }
            if protected.contains(&entry.path) {
                continue;
            }
            fs::remove_file(&entry.path)?;
            total = total.saturating_sub(entry.byte_size);
            purged += 1;
        }
        if purged > 0 {
            sync_parent_directory(&layout.quarantine.join("quarantine-entry"))?;
        }
        Ok(purged)
    }

    fn repository_quarantine_entries(
        &self,
        layout: &RepositoryLayout,
    ) -> StorageResult<Vec<RepositoryQuarantineEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&layout.quarantine)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let metadata = entry.metadata()?;
            let name = entry.file_name();
            let created_at = name
                .to_str()
                .and_then(|value| value.split('-').next())
                .and_then(|value| value.parse::<i64>().ok())
                .or_else(|| {
                    metadata
                        .modified()
                        .ok()
                        .and_then(system_time_to_unix_timestamp)
                })
                .unwrap_or(0);
            entries.push(RepositoryQuarantineEntry {
                path: entry.path(),
                created_at,
                byte_size: metadata.len(),
            });
        }
        Ok(entries)
    }

    pub(super) fn reconcile_backup_repository_unlocked(
        &self,
    ) -> StorageResult<super::BackupRepositoryRecoveryReport> {
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let loaded = self.load_repository_manifests_with_issues(&layout)?;
        self.cleanup_repository_state(&layout, &loaded)
    }

    fn latest_automatic_manifest(
        manifests: &[RepositorySnapshotManifest],
    ) -> Option<&RepositorySnapshotManifest> {
        manifests
            .iter()
            .filter(|manifest| manifest.kind == BackupSnapshotKind::Automatic)
            .max_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.snapshot_id.cmp(&right.snapshot_id))
            })
    }

    fn record_repository_result(
        &self,
        message: String,
        automatic_attempt_at: Option<i64>,
    ) -> StorageResult<()> {
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let manifests = self.load_repository_manifests(&layout)?;
        let previous = self.load_repository_index(&layout);
        self.write_repository_index(
            &layout,
            &manifests,
            automatic_attempt_at.or(previous.last_attempt_at),
            message,
        )
    }

    pub(super) fn create_safety_repository_snapshot_unlocked(
        &self,
        reason: &str,
    ) -> StorageResult<String> {
        let now = unix_timestamp();
        let result = self.create_repository_snapshot_unlocked(BackupSnapshotKind::Safety, now)?;
        let Some(manifest) = result else {
            return Err(StorageError::Integrity(
                "safety backup was unexpectedly skipped".to_string(),
            ));
        };
        let message = format!(
            "Created {} snapshot {} before {reason}.",
            manifest.kind.label(),
            manifest.snapshot_id
        );
        let _ = self.record_repository_result(message, None);
        Ok(manifest.snapshot_id)
    }

    fn create_repository_snapshot_unlocked(
        &self,
        kind: BackupSnapshotKind,
        created_at: i64,
    ) -> StorageResult<Option<RepositorySnapshotManifest>> {
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let existing = self.load_repository_manifests(&layout)?;
        self.cleanup_incomplete_repository_state(&layout, &existing)?;

        let snapshot_id = new_id(match kind {
            BackupSnapshotKind::Automatic => "auto",
            BackupSnapshotKind::Safety => "safety",
        });
        validate_snapshot_id(&snapshot_id)?;
        let stage_directory = layout.staging.join(&snapshot_id);
        fs::create_dir(&stage_directory)?;
        let staged_database = stage_directory.join(DATABASE_ARCHIVE_PATH);
        let build_result = (|| -> StorageResult<Option<RepositorySnapshotManifest>> {
            self.create_database_snapshot(&staged_database)?;
            let database = Self::open_snapshot_database(&staged_database)?;
            self.validate_snapshot_database(&database)?;
            let counts = self.complete_archive_counts(&database)?;
            let (database_sha256, database_bytes) = sha256_file(&staged_database)?;

            let mut blobs = Vec::new();
            self.for_each_archive_blob(&database, |sha256, relative_path, byte_size| {
                let source = self.attachments_directory.join(relative_path);
                let repository_path = layout.blob_path(sha256);
                ensure_repository_blob(&source, &repository_path, sha256, byte_size)?;
                blobs.push(RepositoryBlobReference {
                    sha256: sha256.to_string(),
                    byte_size,
                });
                Ok(())
            })?;
            blobs.sort_by(|left, right| left.sha256.cmp(&right.sha256));

            let manifest = RepositorySnapshotManifest {
                format_version: SNAPSHOT_MANIFEST_VERSION,
                build_identifier: FULL_BUILD_IDENTIFIER.to_string(),
                snapshot_id: snapshot_id.clone(),
                kind,
                created_at,
                database_sha256,
                database_bytes,
                blobs,
                notebook_count: counts.0,
                note_count: counts.1,
                attachment_count: counts.2,
                revision_count: counts.3,
            };
            validate_manifest_metadata(&manifest)?;
            drop(database);

            if kind == BackupSnapshotKind::Automatic
                && Self::latest_automatic_manifest(&existing).is_some_and(|previous| {
                    manifests_equivalent(previous, &manifest)
                        && self
                            .verify_repository_snapshot_database_payload(
                                &layout.snapshot_path(&previous.snapshot_id),
                                previous,
                            )
                            .is_ok()
                })
            {
                return Ok(None);
            }

            if kind == BackupSnapshotKind::Safety {
                // Safety snapshots precede destructive restore/reset work, so retain a
                // complete checksum pass before publishing their manifest. Automatic
                // snapshots use the trusted immutable-blob fast path above; their new
                // blobs were hashed while copying and their database was already
                // validated and hashed from the staged file.
                self.verify_repository_snapshot_payload(&staged_database, &layout, &manifest)?;
            }

            let final_database = layout.snapshot_path(&snapshot_id);
            fs::rename(&staged_database, &final_database)?;
            sync_parent_directory(&final_database)?;
            let manifest_path = layout.manifest_path(&snapshot_id);
            atomic_replace(&manifest_path, &serde_json::to_vec_pretty(&manifest)?)?;
            // Atomic publication does not mutate the verified database/blob bytes.
            // Explicit inspect and restore still perform full checksum verification.
            Ok(Some(manifest))
        })();
        let _ = fs::remove_dir_all(&stage_directory);

        let manifest = match build_result {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(layout.manifest_path(&snapshot_id));
                let _ = fs::remove_file(layout.snapshot_path(&snapshot_id));
                let manifests = self.load_repository_manifests(&layout).unwrap_or(existing);
                let _ = self.cleanup_incomplete_repository_state(&layout, &manifests);
                return Err(error);
            }
        };

        if let Some(created) = &manifest {
            self.prune_backup_repository_unlocked(Some(&created.snapshot_id))?;
        } else {
            let manifests = self.load_repository_manifests(&layout)?;
            self.cleanup_incomplete_repository_state(&layout, &manifests)?;
        }
        Ok(manifest)
    }

    fn automatic_backup_due_unlocked(&self) -> StorageResult<bool> {
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let index = self.load_repository_index(&layout);
        let manifests = self.load_repository_manifests(&layout)?;
        let last = index.last_attempt_at.or_else(|| {
            Self::latest_automatic_manifest(&manifests).map(|manifest| manifest.created_at)
        });
        Ok(last.is_none_or(|value| {
            unix_timestamp().saturating_sub(value) >= super::backups::AUTO_BACKUP_INTERVAL_SECONDS
        }))
    }

    pub fn automatic_backup_due(&self) -> StorageResult<bool> {
        let _operation_guard = self.lock_operations()?;
        self.automatic_backup_due_unlocked()
    }

    pub fn maybe_create_automatic_backup(&self) -> StorageResult<Option<PathBuf>> {
        let now = unix_timestamp();
        let _operation_guard = self.lock_operations()?;
        if !self.automatic_backup_due_unlocked()? {
            return Ok(None);
        }
        let attempt = (|| -> StorageResult<Option<PathBuf>> {
            let created =
                self.create_repository_snapshot_unlocked(BackupSnapshotKind::Automatic, now)?;
            match created {
                Some(manifest) => {
                    let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
                    let path = layout.snapshot_path(&manifest.snapshot_id);
                    self.record_repository_result(
                        format!(
                            "Automatic backup completed: {} note(s), {} attachment record(s), {} shared blob(s).",
                            manifest.note_count,
                            manifest.attachment_count,
                            manifest.blobs.len()
                        ),
                        Some(now),
                    )?;
                    Ok(Some(path))
                }
                None => {
                    self.record_repository_result(
                        "Automatic backup skipped because the durable library is unchanged."
                            .to_string(),
                        Some(now),
                    )?;
                    Ok(None)
                }
            }
        })();
        if let Err(error) = &attempt {
            let _ =
                self.record_repository_result(format!("Automatic backup failed: {error}"), None);
        }
        attempt
    }

    fn prune_backup_repository_unlocked(
        &self,
        protected_snapshot: Option<&str>,
    ) -> StorageResult<()> {
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let manifests = self.load_repository_manifests(&layout)?;
        let budget = self.backup_repository_budget_bytes()?;
        let kept = retained_manifests(manifests.clone(), budget, protected_snapshot)?;
        let protected_missing = protected_snapshot
            .is_some_and(|id| !kept.iter().any(|manifest| manifest.snapshot_id == id));
        let retained_usage = manifest_usage(&kept)?;
        if protected_missing || retained_usage > budget {
            if let Some(id) = protected_snapshot {
                let _ = fs::remove_file(layout.manifest_path(id));
                let _ = fs::remove_file(layout.snapshot_path(id));
                let remaining = self.load_repository_manifests(&layout)?;
                self.cleanup_incomplete_repository_state(&layout, &remaining)?;
            }
            return Err(StorageError::Validation(
                "the newest retained backup cannot fit within the configured storage budget"
                    .to_string(),
            ));
        }

        let kept_ids = kept
            .iter()
            .map(|manifest| manifest.snapshot_id.as_str())
            .collect::<HashSet<_>>();
        for manifest in &manifests {
            if kept_ids.contains(manifest.snapshot_id.as_str()) {
                continue;
            }
            let manifest_path = layout.manifest_path(&manifest.snapshot_id);
            if manifest_path.exists() {
                fs::remove_file(&manifest_path)?;
                sync_parent_directory(&manifest_path)?;
            }
            let snapshot_path = layout.snapshot_path(&manifest.snapshot_id);
            if snapshot_path.exists() {
                fs::remove_file(snapshot_path)?;
            }
        }
        self.cleanup_incomplete_repository_state(&layout, &kept)?;
        let previous = self.load_repository_index(&layout);
        let _ = self.write_repository_index(
            &layout,
            &kept,
            previous.last_attempt_at,
            previous.last_result,
        );
        Ok(())
    }

    /// Read-only privacy status used immediately after a successful Protect.
    /// This intentionally avoids opening/creating the active backup repository: a
    /// disclosure query must not mutate backup state or turn a successful privacy
    /// transition into another repository operation.
    pub(crate) fn pending_backup_privacy_reset_count(&self) -> StorageResult<usize> {
        super::locations::pending_backup_privacy_reset_count_for(
            &self.library_id,
            &self.data_directory,
        )
    }

    pub fn backup_repository_status(&self) -> StorageResult<BackupRepositoryStatus> {
        let _operation_guard = self.lock_operations()?;
        self.backup_repository_status_unlocked()
    }

    fn backup_repository_status_unlocked(&self) -> StorageResult<BackupRepositoryStatus> {
        let pending_privacy_reset_count = super::locations::pending_backup_privacy_reset_count_for(
            &self.library_id,
            &self.data_directory,
        )?;
        if self.current_backup_privacy_reset_pending_unlocked()? {
            return Ok(BackupRepositoryStatus {
                budget_bytes: self.backup_repository_budget_bytes()?,
                last_result: "Backup repository is unavailable or pending Vault privacy cleanup. Reconnect it and reopen Rustnote before using backups.".to_string(),
                pending_privacy_reset_count,
                ..BackupRepositoryStatus::default()
            });
        }

        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        layout.ensure()?;
        let manifests = self.load_repository_manifests(&layout)?;
        let index = self.load_repository_index(&layout);
        let mut blob_sizes = HashMap::<&str, u64>::new();
        let mut database_bytes = 0_u64;
        let mut automatic = 0_usize;
        let mut safety = 0_usize;
        for manifest in &manifests {
            database_bytes = database_bytes
                .checked_add(manifest.database_bytes)
                .ok_or_else(|| StorageError::Integrity("backup size overflow".to_string()))?;
            match manifest.kind {
                BackupSnapshotKind::Automatic => automatic += 1,
                BackupSnapshotKind::Safety => safety += 1,
            }
            for blob in &manifest.blobs {
                blob_sizes.entry(&blob.sha256).or_insert(blob.byte_size);
            }
        }
        let blob_bytes = blob_sizes.values().try_fold(0_u64, |total, size| {
            total
                .checked_add(*size)
                .ok_or_else(|| StorageError::Integrity("backup size overflow".to_string()))
        })?;
        let total_bytes = directory_file_bytes(&layout.root)?;
        let quarantine_entries = self.repository_quarantine_entries(&layout)?;
        let quarantined_artifact_count = quarantine_entries.len();
        let mut quarantined_artifacts = quarantine_entries
            .into_iter()
            .filter_map(|entry| {
                entry
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        quarantined_artifacts.sort();
        quarantined_artifacts.truncate(MAX_QUARANTINE_DETAILS);
        let snapshots = manifests
            .iter()
            .rev()
            .map(|manifest| {
                let expanded_bytes =
                    manifest
                        .blobs
                        .iter()
                        .try_fold(manifest.database_bytes, |total, blob| {
                            total.checked_add(blob.byte_size).ok_or_else(|| {
                                StorageError::Integrity("backup snapshot size overflow".to_string())
                            })
                        })?;
                Ok(BackupRepositorySnapshot {
                    snapshot_id: manifest.snapshot_id.clone(),
                    kind: manifest.kind,
                    created_at: manifest.created_at,
                    database_bytes: manifest.database_bytes,
                    expanded_bytes,
                    blob_count: manifest.blobs.len(),
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(BackupRepositoryStatus {
            total_bytes,
            blob_bytes,
            database_bytes,
            snapshot_count: manifests.len(),
            automatic_snapshot_count: automatic,
            safety_snapshot_count: safety,
            oldest_snapshot_at: manifests.first().map(|manifest| manifest.created_at),
            newest_snapshot_at: manifests.last().map(|manifest| manifest.created_at),
            budget_bytes: self.backup_repository_budget_bytes()?,
            last_attempt_at: index.last_attempt_at,
            last_result: index.last_result,
            quarantined_artifact_count,
            quarantined_artifacts,
            pending_privacy_reset_count,
            snapshots,
        })
    }

    pub fn inspect_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> StorageResult<CompleteArchiveSummary> {
        let _operation_guard = self.lock_operations()?;
        self.require_current_backup_privacy_ready_unlocked()?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        let manifests = self.load_repository_manifests(&layout)?;
        let manifest = manifests
            .into_iter()
            .find(|manifest| manifest.snapshot_id == snapshot_id)
            .ok_or_else(|| {
                StorageError::InvalidBackup("backup snapshot was not found".to_string())
            })?;
        self.verify_repository_snapshot(&layout, &manifest)?;
        let expanded_bytes =
            manifest
                .blobs
                .iter()
                .try_fold(manifest.database_bytes, |total, blob| {
                    total.checked_add(blob.byte_size).ok_or_else(|| {
                        StorageError::Integrity("backup snapshot size overflow".to_string())
                    })
                })?;
        Ok(CompleteArchiveSummary {
            created_at: manifest.created_at,
            archive_bytes: expanded_bytes,
            expanded_bytes,
            notebook_count: manifest.notebook_count,
            note_count: manifest.note_count,
            attachment_count: manifest.attachment_count,
            revision_count: manifest.revision_count,
        })
    }

    pub fn restore_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> StorageResult<CompleteArchiveSummary> {
        let staged = {
            let _operation_guard = self.lock_operations()?;
            self.stage_backup_repository_snapshot_unlocked(snapshot_id)?
        };
        let summary = staged.summary.clone();
        let _privacy_guard = match self
            .begin_library_privacy_transition(crate::security::LibraryPrivacyTransition::Restore)
        {
            Ok(guard) => guard,
            Err(error) => {
                return match discard_restore_stage_directory(&staged.directory) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(StorageError::Integrity(format!("{error}; {cleanup}"))),
                };
            }
        };
        let result = self.install_staged_complete_archive(&staged);
        match (result, discard_restore_stage_directory(&staged.directory)) {
            (Ok(()), Ok(())) => Ok(summary),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(cleanup)) => Err(cleanup),
            (Err(error), Err(cleanup)) => {
                Err(StorageError::Integrity(format!("{error}; {cleanup}")))
            }
        }
    }

    fn stage_backup_repository_snapshot_unlocked(
        &self,
        snapshot_id: &str,
    ) -> StorageResult<StagedCompleteArchive> {
        self.require_current_backup_privacy_ready_unlocked()?;
        validate_snapshot_id(snapshot_id)?;
        let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;
        let manifests = self.load_repository_manifests(&layout)?;
        let manifest = manifests
            .into_iter()
            .find(|manifest| manifest.snapshot_id == snapshot_id)
            .ok_or_else(|| {
                StorageError::InvalidBackup("backup snapshot was not found".to_string())
            })?;
        self.verify_repository_snapshot(&layout, &manifest)?;

        let restore_root = crate::native::staging::archive_restore_process_root()
            .map_err(|error| StorageError::Integrity(error.to_string()))?;
        let staging_directory =
            restore_root.join(format!("restore-stage-{}", new_id("repository")));
        fs::create_dir(&staging_directory)?;
        let database_path = staging_directory.join(DATABASE_ARCHIVE_PATH);
        let stage_result = (|| -> StorageResult<()> {
            copy_file_atomically(&layout.snapshot_path(snapshot_id), &database_path)?;
            for blob in &manifest.blobs {
                let destination = staging_directory.join("blobs").join(&blob.sha256);
                copy_file_atomically(&layout.blob_path(&blob.sha256), &destination)?;
            }
            Ok(())
        })();
        if let Err(error) = stage_result {
            return match discard_restore_stage_directory(&staging_directory) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(StorageError::Integrity(format!("{error}; {cleanup}"))),
            };
        }
        let expanded_bytes =
            manifest
                .blobs
                .iter()
                .try_fold(manifest.database_bytes, |total, blob| {
                    total.checked_add(blob.byte_size).ok_or_else(|| {
                        StorageError::Integrity("backup snapshot size overflow".to_string())
                    })
                })?;
        Ok(StagedCompleteArchive {
            directory: staging_directory,
            database_path,
            summary: CompleteArchiveSummary {
                created_at: manifest.created_at,
                archive_bytes: expanded_bytes,
                expanded_bytes,
                notebook_count: manifest.notebook_count,
                note_count: manifest.note_count,
                attachment_count: manifest.attachment_count,
                revision_count: manifest.revision_count,
            },
        })
    }
}

#[cfg(test)]
mod vault_backup_location_tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustnote-{label}-{}", new_id("backup")))
    }

    #[test]
    fn repository_snapshot_restore_round_trips_note_and_attachment_through_library_restore() {
        let root = temp_root("repository-restore-roundtrip");
        let data = root.join("library");
        let backups = root.join("backups");
        let storage = Storage::at_with_backup(data.clone(), backups.clone()).expect("storage");

        let mut note = storage.create_note(None).expect("create note");
        note.title = "repository snapshot title".to_string();
        let note = storage.save_note_owned(note).expect("save snapshot note");

        let source = root.join("snapshot-attachment.txt");
        let attachment_bytes = b"repository snapshot attachment bytes";
        fs::write(&source, attachment_bytes).expect("write attachment source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "snapshot-attachment.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add snapshot attachment");

        storage
            .maybe_create_automatic_backup()
            .expect("create repository snapshot")
            .expect("first automatic snapshot");
        let status = storage
            .backup_repository_status()
            .expect("repository status after snapshot");
        let snapshot_id = status
            .snapshots
            .iter()
            .find(|snapshot| snapshot.kind == BackupSnapshotKind::Automatic)
            .expect("automatic snapshot")
            .snapshot_id
            .clone();

        let mut changed = storage
            .get_note(&note.id)
            .expect("read current note")
            .expect("current note exists");
        changed.title = "mutated after snapshot".to_string();
        storage.save_note_owned(changed).expect("save mutation");
        storage
            .remove_attachment(&attachment.id)
            .expect("remove current attachment");
        assert!(
            storage
                .list_attachments(&note.id)
                .expect("list mutated attachments")
                .is_empty(),
            "precondition: live library must differ from the selected snapshot",
        );

        storage
            .restore_backup_repository_snapshot(&snapshot_id)
            .expect("restore repository snapshot");

        let restored_note = storage
            .get_note(&note.id)
            .expect("read restored note")
            .expect("restored note exists");
        assert_eq!(restored_note.title, "repository snapshot title");

        let restored_attachments = storage
            .list_attachments(&note.id)
            .expect("list restored attachments");
        assert_eq!(restored_attachments.len(), 1);
        assert_eq!(restored_attachments[0].id, attachment.id);
        assert_eq!(restored_attachments[0].file_name, "snapshot-attachment.txt");
        let restored_path = storage
            .attachment_file_path(&restored_attachments[0])
            .expect("restored attachment path");
        assert_eq!(
            fs::read(restored_path).expect("read restored attachment"),
            attachment_bytes
        );
        assert!(
            storage
                .audit_attachments()
                .expect("audit restored attachments")
                .is_clean(),
            "repository restore must leave canonical attachment ownership consistent",
        );

        drop(storage);
        let reopened =
            Storage::at_with_backup(data, backups).expect("reopen restored repository library");
        let reopened_note = reopened
            .get_note(&note.id)
            .expect("read reopened restored note")
            .expect("reopened restored note exists");
        assert_eq!(reopened_note.title, "repository snapshot title");
        assert_eq!(
            reopened
                .list_attachments(&note.id)
                .expect("list reopened restored attachments")
                .len(),
            1,
            "restart verification must not find an unowned orphan left by Restore",
        );

        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_sanitization_does_not_hide_active_plaintext_staging() {
        let root = temp_root("snapshot-active-plaintext-stage");
        let storage = Storage::at_with_backup(root.join("library"), root.join("backups"))
            .expect("storage");
        let note = storage.create_note(None).expect("create note");
        let staging_relative_path = format!(".incoming/{}.tmp", new_id("q6-stage"));
        let mut owner = storage
            .reserve_attachment_owned_plaintext(&note.id, &staging_relative_path)
            .expect("reserve active plaintext staging");

        let error = storage
            .create_safety_repository_snapshot_unlocked("Q6 active plaintext staging")
            .expect_err("active managed plaintext staging must block snapshots");
        assert!(
            error
                .to_string()
                .contains("unresolved attachment operation"),
            "unexpected snapshot rejection: {error}"
        );

        storage
            .discard_owned_plaintext(&mut owner)
            .expect("retire active staging owner");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_reset_purges_historical_owned_repository_without_deleting_parent() {
        let root = temp_root("historical-backup-reset");
        let data = root.join("library");
        let current_backup = root.join("current-backups");
        let historical_backup = root.join("historical-backups");

        // Establish the historical repository through Storage itself so its
        // ownership metadata positively identifies the same library.
        let historical_storage = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect("historical storage");
        let historical_repository = historical_backup.join(REPOSITORY_DIRECTORY_NAME);
        fs::write(
            historical_repository.join("plaintext.sqlite3"),
            b"sensitive plaintext",
        )
        .expect("historical plaintext");
        fs::write(historical_backup.join("keep-me.txt"), b"user file").expect("parent user file");
        drop(historical_storage);

        let storage = Storage::at_with_backup(data, current_backup.clone()).expect("storage");
        storage
            .reset_backup_repositories_for_vault_unlocked(std::slice::from_ref(&historical_backup))
            .expect("vault repository reset");

        assert!(
            historical_repository.is_dir(),
            "historical cleanup should leave only a minimal owned clean-state repository",
        );
        assert!(
            !historical_repository.join("plaintext.sqlite3").exists(),
            "historical plaintext must be removed",
        );
        assert!(historical_backup.join("keep-me.txt").exists());
        assert!(current_backup.join(REPOSITORY_DIRECTORY_NAME).is_dir());

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn offline_historical_repository_keeps_privacy_obligation_until_reconnected() {
        let root = temp_root("offline-backup-obligation");
        let data = root.join("library");
        let historical_backup = root.join("external-backups");
        let offline_storage = root.join("external-backups-offline");
        let current_backup = root.join("current-backups");

        let historical = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect("historical storage");
        let historical_repository = historical_backup.join(REPOSITORY_DIRECTORY_NAME);
        let plaintext = historical_repository.join("pre-encryption-plaintext.sqlite3");
        fs::write(&plaintext, b"historical plaintext").expect("historical plaintext");
        let library_id = historical.library_id().to_string();
        drop(historical);

        fs::rename(&historical_backup, &offline_storage).expect("simulate unplugged volume");
        let current =
            Storage::at_with_backup(data.clone(), current_backup).expect("current storage");
        current
            .reset_backup_repository_for_vault_unlocked()
            .expect("local reset succeeds while old volume is offline");
        assert!(
            super::super::locations::backup_privacy_reset_required_for(
                &library_id,
                &data,
                &historical_backup,
            )
            .expect("pending obligation"),
            "offline historical repository must remain privacy-pending",
        );
        assert!(
            offline_storage
                .join(REPOSITORY_DIRECTORY_NAME)
                .join("pre-encryption-plaintext.sqlite3")
                .is_file(),
            "offline repository bytes must remain untouched until the volume returns",
        );
        assert_eq!(
            current
                .backup_repository_status()
                .expect("backup status")
                .pending_privacy_reset_count,
            1,
            "settings status must expose the unresolved historical repository",
        );
        drop(current);

        let settings_error =
            Storage::save_configured_locations(data.clone(), historical_backup.clone())
                .expect_err("settings must not recreate a missing privacy-pending mount path");
        assert!(
            settings_error
                .to_string()
                .contains("requires Vault privacy cleanup")
        );
        assert!(!historical_backup.exists());

        let unavailable = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect_err("pending missing backup location must not be recreated locally");
        assert!(
            unavailable
                .to_string()
                .contains("requires Vault privacy cleanup")
        );
        assert!(!historical_backup.exists());

        fs::create_dir(&historical_backup).expect("substitute mount path");
        let substitute = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect_err(
                "an empty substitute parent is not proof that the remembered volume returned",
            );
        assert!(substitute.to_string().contains("cannot prove"));
        assert!(
            !historical_backup.join(REPOSITORY_DIRECTORY_NAME).exists(),
            "failed proof must not manufacture a clean repository",
        );
        fs::remove_dir_all(&historical_backup).expect("remove substitute path");

        fs::rename(&offline_storage, &historical_backup).expect("reconnect volume");
        let reopened = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect("reconnected repository should be privacy-reset before use");
        assert!(
            !historical_backup
                .join(REPOSITORY_DIRECTORY_NAME)
                .join("pre-encryption-plaintext.sqlite3")
                .exists(),
            "reconnected repository must be invalidated before normal use",
        );
        assert!(
            !super::super::locations::backup_privacy_reset_required_for(
                reopened.library_id(),
                &data,
                &historical_backup,
            )
            .expect("cleared obligation"),
            "successful invalidation must retire the durable privacy obligation",
        );

        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn current_repository_disappearance_stays_pending_and_cannot_be_recreated_in_place() {
        let root = temp_root("current-backup-disappears");
        let data = root.join("library");
        let current_backup = root.join("external-backups");
        let offline_storage = root.join("external-backups-offline");

        let storage =
            Storage::at_with_backup(data.clone(), current_backup.clone()).expect("storage");
        let repository = current_backup.join(REPOSITORY_DIRECTORY_NAME);
        fs::write(
            repository.join("pre-encryption-plaintext.sqlite3"),
            b"historical plaintext",
        )
        .expect("repository plaintext");
        let library_id = storage.library_id().to_string();

        fs::rename(&current_backup, &offline_storage).expect("simulate unplugged current volume");
        storage
            .reset_backup_repository_for_vault_unlocked()
            .expect("local Vault transition must not depend on backup availability");

        assert!(
            super::super::locations::backup_privacy_reset_required_for(
                &library_id,
                &data,
                &current_backup,
            )
            .expect("pending current obligation"),
            "vanished current repository must remain privacy-pending",
        );
        assert!(
            !current_backup.exists(),
            "Vault reset must never recreate a vanished removable-volume mount path",
        );
        assert!(
            offline_storage
                .join(REPOSITORY_DIRECTORY_NAME)
                .join("pre-encryption-plaintext.sqlite3")
                .is_file(),
            "offline repository bytes remain until the real volume returns",
        );

        let status = storage
            .backup_repository_status()
            .expect("safe pending status");
        assert_eq!(status.pending_privacy_reset_count, 1);
        assert!(status.last_result.contains("pending Vault privacy cleanup"));
        assert!(
            storage.automatic_backup_due().is_err(),
            "the live Storage instance must not manufacture or use a substitute repository",
        );
        assert!(!current_backup.exists());

        drop(storage);
        fs::rename(&offline_storage, &current_backup).expect("reconnect volume");
        let reopened = Storage::at_with_backup(data.clone(), current_backup.clone())
            .expect("reopened storage must discharge the pending repository first");
        assert!(
            !current_backup
                .join(REPOSITORY_DIRECTORY_NAME)
                .join("pre-encryption-plaintext.sqlite3")
                .exists(),
            "reconnected current repository must be invalidated before normal use",
        );
        assert!(
            !super::super::locations::backup_privacy_reset_required_for(
                reopened.library_id(),
                &data,
                &current_backup,
            )
            .expect("cleared current obligation"),
        );

        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_refuses_nonempty_repository_without_durable_namespace_authority() {
        let root = temp_root("unowned-backup-repository");
        let data = root.join("library");
        let backup = root.join("backups");
        let repository = backup.join(REPOSITORY_DIRECTORY_NAME);
        fs::create_dir_all(&repository).expect("repository");
        fs::write(repository.join("user-file.txt"), b"not rustnote owned").expect("user file");

        let error = Storage::at_with_backup(data, backup)
            .expect_err("nonempty repository without durable ownership must not be claimed");
        assert!(error.to_string().contains("no durable ownership authority"));
        assert!(repository.join("user-file.txt").is_file());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repository_namespace_owner_is_external_and_recovers_missing_index() {
        let root = temp_root("backup-namespace-crash-recovery");
        let data = root.join("library");
        let backup = root.join("backups");
        let storage = Storage::at_with_backup(data.clone(), backup.clone()).expect("storage");
        let repository = backup.join(REPOSITORY_DIRECTORY_NAME);
        let owner = backup_namespace_marker(&backup);
        let index = repository.join(INDEX_FILE_NAME);
        assert!(
            owner.is_file(),
            "durable owner must live outside repository root"
        );
        fs::remove_file(&index).expect("simulate crash before mutable index publication");
        drop(storage);

        let reopened =
            Storage::at_with_backup(data, backup).expect("external owner permits recovery");
        assert!(owner.is_file());
        assert!(index.is_file());
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_reset_fails_closed_for_corrupt_external_repository_owner() {
        let root = temp_root("backup-owner-corrupt-history");
        let data = root.join("library");
        let historical_backup = root.join("historical-backups");
        let current_backup = root.join("current-backups");

        let historical = Storage::at_with_backup(data.clone(), historical_backup.clone())
            .expect("historical storage");
        let historical_repository = historical_backup.join(REPOSITORY_DIRECTORY_NAME);
        fs::write(
            historical_repository.join("plaintext.sqlite3"),
            b"sensitive plaintext",
        )
        .expect("historical plaintext");
        drop(historical);

        let storage = Storage::at_with_backup(data, current_backup).expect("current storage");
        fs::write(
            backup_namespace_marker(&historical_backup),
            b"corrupt-owner",
        )
        .expect("corrupt external owner marker");

        let error = storage
            .reset_backup_repositories_for_vault_unlocked(std::slice::from_ref(&historical_backup))
            .expect_err("corrupt external authority must fail Vault cleanup closed");
        assert!(
            error
                .to_string()
                .contains("namespace ownership could not be established")
        );
        assert!(
            historical_repository.join("plaintext.sqlite3").is_file(),
            "unverifiable repository must be preserved rather than destructively guessed",
        );

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repository_namespace_owner_prevents_cross_library_reuse_even_without_index() {
        let root = temp_root("backup-owner-cross-library");
        let backup = root.join("backups");
        let first_data = root.join("library-a");
        let second_data = root.join("library-b");
        let first = Storage::at_with_backup(first_data, backup.clone()).expect("first storage");
        let repository = backup.join(REPOSITORY_DIRECTORY_NAME);
        let owner = backup_namespace_marker(&backup);
        fs::remove_file(repository.join(INDEX_FILE_NAME)).expect("remove mutable index");
        drop(first);

        let error = Storage::at_with_backup(second_data, backup.clone())
            .expect_err("external namespace owner must prevent cross-library claim");
        assert!(
            error.to_string().contains("different Rustnote library")
                || error
                    .to_string()
                    .contains("namespace ownership could not be established")
                || error.to_string().contains("already associated")
        );
        assert!(owner.is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear() {
        let root = temp_root("backup-partial-purge-recovery");
        let data = root.join("library");
        let backup = root.join("backups");
        let storage = Storage::at_with_backup(data.clone(), backup.clone()).expect("storage");
        let repository = backup.join(REPOSITORY_DIRECTORY_NAME);
        let owner = backup_namespace_marker(&backup);
        let surviving_secret = repository.join("blobs").join("surviving-secret");
        let removed_manifest = repository.join("manifests").join("deleted-first.json");
        let removed_snapshot = repository.join("snapshots").join("deleted-first.sqlite3");
        fs::write(&surviving_secret, b"pre-protection plaintext").expect("secret");
        fs::write(&removed_manifest, b"first manifest deletion").expect("manifest candidate");
        fs::write(&removed_snapshot, b"first snapshot deletion").expect("snapshot candidate");

        super::super::locations::begin_backup_privacy_reset_for(
            storage.library_id(),
            storage.data_directory(),
        )
        .expect("publish privacy obligation");
        fs::remove_file(repository.join(INDEX_FILE_NAME)).expect("simulate deleted index");
        fs::remove_file(&removed_manifest).expect("simulate deleted manifest");
        fs::remove_file(&removed_snapshot).expect("simulate deleted snapshot");
        assert!(
            owner.is_file(),
            "external owner must survive partial repository deletion"
        );
        drop(storage);

        let reopened = Storage::at_with_backup(data.clone(), backup.clone())
            .expect("restart must finish interrupted Vault cleanup");
        assert!(
            !surviving_secret.exists(),
            "remaining plaintext must be purged on restart"
        );
        assert!(
            owner.is_file(),
            "cleanup authority survives repository replacement"
        );
        assert!(
            repository.join(INDEX_FILE_NAME).is_file(),
            "clean repository is recreated"
        );
        assert!(
            !super::super::locations::backup_privacy_reset_required_for(
                reopened.library_id(),
                &data,
                &backup,
            )
            .expect("privacy obligation"),
            "obligation retires only after successful recovery",
        );
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restart_recovers_after_repository_removed_before_privacy_obligation_retirement() {
        let root = temp_root("backup-root-removed-before-obligation-clear");
        let data = root.join("library");
        let backup = root.join("backups");
        let storage = Storage::at_with_backup(data.clone(), backup.clone()).expect("storage");
        let repository = backup.join(REPOSITORY_DIRECTORY_NAME);
        let owner = backup_namespace_marker(&backup);
        fs::write(
            repository.join("plaintext.sqlite3"),
            b"pre-protection plaintext",
        )
        .expect("secret");

        super::super::locations::begin_backup_privacy_reset_for(
            storage.library_id(),
            storage.data_directory(),
        )
        .expect("publish privacy obligation");
        let namespace = storage
            .establish_backup_repository_namespace_unlocked_at(&backup, false)
            .expect("namespace lookup")
            .expect("namespace exists");
        namespace
            .remove_tree_strict()
            .expect("simulate completed tree deletion");
        assert!(!repository.exists());
        assert!(owner.is_file(), "authority must outlive tree deletion");
        drop(storage);

        let reopened = Storage::at_with_backup(data.clone(), backup.clone())
            .expect("restart must reconcile root-absent pending cleanup");
        assert!(repository.join(INDEX_FILE_NAME).is_file());
        assert!(owner.is_file());
        assert!(
            !super::super::locations::backup_privacy_reset_required_for(
                reopened.library_id(),
                &data,
                &backup,
            )
            .expect("privacy obligation"),
        );
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn steady_state_backup_writes_fail_closed_after_namespace_marker_loss() {
        let root = temp_root("backup-owner-loss-live-process");
        let data = root.join("library");
        let backup = root.join("backups");
        let storage = Storage::at_with_backup(data, backup.clone()).expect("storage");
        let marker = backup_namespace_marker(&backup);
        fs::remove_file(&marker).expect("remove durable owner marker");

        let error = storage
            .automatic_backup_due()
            .expect_err("backup operation must not recreate a missing durable owner");
        assert!(
            error
                .to_string()
                .contains("durable ownership is missing or corrupt")
        );
        assert!(
            !marker.exists(),
            "steady-state operation must not manufacture replacement ownership"
        );

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }
}
