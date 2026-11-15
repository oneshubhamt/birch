use std::{
    env,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

use crate::app_paths::configuration_directory;

use super::atomic_output::AtomicFileOutput;
use super::attachments::sync_parent_directory;
use super::{StorageError, StorageResult, new_id};

const STORAGE_LOCATIONS_VERSION: u32 = 1;
const STORAGE_LOCATIONS_FILE: &str = "storage-locations.json";
const MAX_STORAGE_LOCATIONS_BYTES: u64 = 64 * 1024;
const BACKUP_LOCATION_HISTORY_VERSION: u32 = 3;
const BACKUP_LOCATION_HISTORY_FILE: &str = "backup-location-history.json";
const BACKUP_LOCATION_HISTORY_LOCK_FILE: &str = ".backup-location-history.lock";
const MAX_BACKUP_LOCATION_HISTORY_BYTES: u64 = 1024 * 1024;
const LIBRARY_IDENTITY_FILE: &str = ".rustnote-library-id";
const MAX_LIBRARY_IDENTITY_BYTES: u64 = 256;

static BACKUP_HISTORY_PROCESS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageLocations {
    pub data_directory: PathBuf,
    pub backup_directory: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StorageLocationsFile {
    version: u32,
    data_directory: PathBuf,
    backup_directory: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BackupLocationHistoryFile {
    version: u32,
    libraries: Vec<BackupLocationHistoryEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BackupLocationHistoryEntry {
    library_id: String,
    data_directory: PathBuf,
    backup_directories: Vec<PathBuf>,
    pending_privacy_reset_directories: Vec<PathBuf>,
}

impl From<StorageLocations> for StorageLocationsFile {
    fn from(value: StorageLocations) -> Self {
        Self {
            version: STORAGE_LOCATIONS_VERSION,
            data_directory: value.data_directory,
            backup_directory: value.backup_directory,
        }
    }
}

impl TryFrom<StorageLocationsFile> for StorageLocations {
    type Error = StorageError;

    fn try_from(value: StorageLocationsFile) -> Result<Self, Self::Error> {
        if value.version != STORAGE_LOCATIONS_VERSION {
            return Err(StorageError::Validation(format!(
                "unsupported storage-location configuration version {}",
                value.version
            )));
        }
        validate_nonempty_path(&value.data_directory, "library")?;
        validate_nonempty_path(&value.backup_directory, "backup")?;
        validate_storage_location_relationship(&value.data_directory, &value.backup_directory)?;
        Ok(Self {
            data_directory: value.data_directory,
            backup_directory: value.backup_directory,
        })
    }
}

fn validate_platform_storage_ready() -> StorageResult<()> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    crate::platform::ensure_mobile_platform_ready().map_err(StorageError::from)?;
    Ok(())
}

pub(super) fn resolved_storage_locations() -> StorageResult<StorageLocations> {
    validate_platform_storage_ready()?;
    let mut locations =
        read_configured_storage_locations()?.unwrap_or_else(default_storage_locations);
    if let Some(custom) = nonempty_environment_path("RUSTNOTE_DATA_DIR") {
        locations.data_directory = custom;
    }
    if let Some(custom) = nonempty_environment_path("RUSTNOTE_BACKUP_DIR") {
        locations.backup_directory = custom;
    }
    validate_storage_location_relationship(&locations.data_directory, &locations.backup_directory)?;
    Ok(locations)
}

pub(super) fn configured_storage_locations() -> StorageResult<StorageLocations> {
    validate_platform_storage_ready()?;
    read_configured_storage_locations()
        .map(|configured| configured.unwrap_or_else(default_storage_locations))
}

pub(super) fn default_storage_locations() -> StorageLocations {
    let documents = documents_directory();
    StorageLocations {
        data_directory: documents.join("Rustnote"),
        backup_directory: documents.join("Rustnote Backups"),
    }
}

pub(super) fn save_storage_locations(
    data_directory: PathBuf,
    backup_directory: PathBuf,
) -> StorageResult<StorageLocations> {
    validate_platform_storage_ready()?;
    validate_nonempty_path(&data_directory, "library")?;
    validate_nonempty_path(&backup_directory, "backup")?;
    validate_storage_location_relationship(&data_directory, &backup_directory)?;
    fs::create_dir_all(&data_directory)?;
    let data_directory = fs::canonicalize(data_directory)?;
    let pending_backup_reset = if let Some(library_id) =
        read_library_identity_if_present(&data_directory)?
    {
        require_pending_backup_location_available(&library_id, &data_directory, &backup_directory)?
    } else {
        false
    };
    if !pending_backup_reset {
        fs::create_dir_all(&backup_directory)?;
    }
    let backup_directory = fs::canonicalize(backup_directory)?;
    validate_disjoint_directories(&data_directory, &backup_directory)?;
    validate_directory_writable(&data_directory, "library")?;
    if !pending_backup_reset {
        validate_directory_writable(&backup_directory, "backup")?;
    }

    let locations = StorageLocations {
        data_directory,
        backup_directory,
    };
    // Backup privacy ownership is recorded by `Storage::at_with_backup`, where
    // Rustnote has the canonical library identity and active directory locks.
    // Configuration alone must never guess that two paths represent one library.
    let payload = serde_json::to_vec_pretty(&StorageLocationsFile::from(locations.clone()))?;
    let path = storage_locations_path();
    let mut output = AtomicFileOutput::create(&path)?;
    output.writer()?.write_all(&payload)?;
    output.commit(|temporary| {
        let parsed = read_storage_locations_file(temporary)?;
        let verified = StorageLocations::try_from(parsed)?;
        if verified != locations {
            return Err(StorageError::Integrity(
                "storage-location configuration changed during verification".to_string(),
            ));
        }
        Ok(())
    })?;
    Ok(locations)
}

pub(super) fn reset_storage_locations() -> StorageResult<()> {
    validate_platform_storage_ready()?;
    let path = storage_locations_path();
    match fs::remove_file(&path) {
        Ok(()) => sync_parent_directory(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read_library_identity_if_present(
    data_directory: &Path,
) -> StorageResult<Option<String>> {
    let path = data_directory.join(LIBRARY_IDENTITY_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_LIBRARY_IDENTITY_BYTES
            {
                return Err(StorageError::Integrity(
                    "Rustnote library identity is invalid".to_string(),
                ));
            }
            let value = fs::read_to_string(&path)?;
            validate_library_id(value.trim()).map(|value| Some(value.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn ensure_library_identity(data_directory: &Path) -> StorageResult<String> {
    with_backup_history_lock(|| {
        ensure_library_identity_with_history_at(data_directory, &backup_location_history_path())
    })
}

fn ensure_library_identity_with_history_at(
    data_directory: &Path,
    history_path: &Path,
) -> StorageResult<String> {
    let historical_id = historical_library_id_at(history_path, data_directory)?;
    if let Some(current_id) = read_library_identity_if_present(data_directory)? {
        if let Some(historical_id) = historical_id
            && historical_id != current_id
            && pristine_bootstrap_library_directory(data_directory)?
        {
            write_library_identity(data_directory, &historical_id)?;
            return Ok(historical_id);
        }
        return Ok(current_id);
    }

    let library_id = historical_id.unwrap_or_else(|| new_id("library"));
    write_library_identity(data_directory, &library_id)?;
    Ok(library_id)
}

fn read_current_backup_history_at(
    path: &Path,
) -> StorageResult<Option<Vec<BackupLocationHistoryEntry>>> {
    let Some(history) = read_backup_location_history_file(path)? else {
        return Ok(None);
    };
    if history.version != BACKUP_LOCATION_HISTORY_VERSION {
        return Err(StorageError::Validation(format!(
            "unsupported backup-location history version {}",
            history.version
        )));
    }
    normalize_backup_history(history.libraries).map(Some)
}

fn historical_library_id_at(
    history_path: &Path,
    data_directory: &Path,
) -> StorageResult<Option<String>> {
    let Some(libraries) = read_current_backup_history_at(history_path)? else {
        return Ok(None);
    };
    let data_directory = normalize_directory_for_comparison(data_directory)?;
    Ok(libraries
        .into_iter()
        .find(|entry| entry.data_directory == data_directory)
        .map(|entry| entry.library_id))
}

fn pristine_bootstrap_library_directory(data_directory: &Path) -> StorageResult<bool> {
    for entry in fs::read_dir(data_directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Ok(false);
        };
        let file_type = entry.file_type()?;
        if name == LIBRARY_IDENTITY_FILE {
            if !file_type.is_file() {
                return Ok(false);
            }
            continue;
        }
        if matches!(name, ".rustnote-instance.lock" | ".rustnote-directory.lock")
            || name.starts_with(".rustnote-instance.lock-")
            || name.starts_with(".rustnote-directory.lock-")
        {
            if !file_type.is_file() {
                return Ok(false);
            }
            continue;
        }
        if name == "attachments" {
            if !file_type.is_dir() || fs::read_dir(entry.path())?.next().transpose()?.is_some() {
                return Ok(false);
            }
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

fn write_library_identity(data_directory: &Path, library_id: &str) -> StorageResult<()> {
    validate_library_id(library_id)?;
    let path = data_directory.join(LIBRARY_IDENTITY_FILE);
    let mut output = AtomicFileOutput::create(&path)?;
    output.writer()?.write_all(library_id.as_bytes())?;
    output.commit(|temporary| {
        let value = fs::read_to_string(temporary)?;
        validate_library_id(value.trim()).map(|_| ())
    })
}

fn validate_library_id(value: &str) -> StorageResult<&str> {
    if value.len() < 16
        || value.len() > 128
        || !value.starts_with("library_")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(StorageError::Integrity(
            "Rustnote library identity is malformed".to_string(),
        ));
    }
    Ok(value)
}

pub(super) fn validate_storage_location_ownership(
    library_id: &str,
    locations: &StorageLocations,
) -> StorageResult<()> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        validate_storage_location_ownership_at(
            &backup_location_history_path(),
            library_id,
            locations,
        )
    })
}

fn validate_storage_location_ownership_at(
    path: &Path,
    library_id: &str,
    locations: &StorageLocations,
) -> StorageResult<()> {
    let Some(libraries) = read_current_backup_history_at(path)? else {
        return Ok(());
    };

    let data_directory = normalize_directory_for_comparison(&locations.data_directory)?;
    let backup_directory = normalize_directory_for_comparison(&locations.backup_directory)?;
    for entry in libraries {
        if entry.library_id == library_id {
            if entry.data_directory != data_directory {
                return Err(StorageError::Integrity(
                    "Rustnote library identity is associated with a different data directory"
                        .to_string(),
                ));
            }
            continue;
        }
        if entry.backup_directories.contains(&backup_directory) {
            return Err(StorageError::Validation(format!(
                "automatic backup folder is already associated with another Rustnote library: {}",
                backup_directory.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn remember_storage_location(
    library_id: &str,
    locations: &StorageLocations,
) -> StorageResult<()> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        let path = backup_location_history_path();
        // Revalidate while holding the same inter-process lock as the
        // read/modify/write update. This makes ownership publication one
        // serialized transaction across concurrently opened libraries.
        validate_storage_location_ownership_at(&path, library_id, locations)?;
        remember_storage_location_at(&path, library_id, locations)
    })
}

fn remember_storage_location_at(
    path: &Path,
    library_id: &str,
    locations: &StorageLocations,
) -> StorageResult<()> {
    validate_library_id(library_id)?;
    let mut libraries = read_current_backup_history_at(path)?.unwrap_or_default();
    let data_directory = normalize_directory_for_comparison(&locations.data_directory)?;
    let backup_directory = normalize_directory_for_comparison(&locations.backup_directory)?;

    if let Some(entry) = libraries
        .iter_mut()
        .find(|entry| entry.library_id == library_id)
    {
        if entry.data_directory != data_directory {
            return Err(StorageError::Integrity(
                "Rustnote library identity is associated with a different data directory"
                    .to_string(),
            ));
        }
        if !entry.backup_directories.contains(&backup_directory) {
            entry.backup_directories.push(backup_directory);
        }
    } else {
        libraries.push(BackupLocationHistoryEntry {
            library_id: library_id.to_string(),
            data_directory,
            backup_directories: vec![backup_directory],
            pending_privacy_reset_directories: Vec::new(),
        });
    }
    write_backup_location_history_at(path, normalize_backup_history(libraries)?)
}

fn write_backup_location_history_at(
    path: &Path,
    libraries: Vec<BackupLocationHistoryEntry>,
) -> StorageResult<()> {
    let payload = serde_json::to_vec_pretty(&BackupLocationHistoryFile {
        version: BACKUP_LOCATION_HISTORY_VERSION,
        libraries,
    })?;
    if payload.len() as u64 > MAX_BACKUP_LOCATION_HISTORY_BYTES {
        return Err(StorageError::Validation(
            "backup-location privacy history is unexpectedly large".to_string(),
        ));
    }
    let mut output = AtomicFileOutput::create(path)?;
    output.writer()?.write_all(&payload)?;
    output.commit(|temporary| {
        let parsed = read_backup_location_history_file(temporary)?.ok_or_else(|| {
            StorageError::Integrity(
                "backup-location privacy history disappeared during verification".to_string(),
            )
        })?;
        if parsed.version != BACKUP_LOCATION_HISTORY_VERSION {
            return Err(StorageError::Integrity(
                "backup-location privacy history changed during verification".to_string(),
            ));
        }
        normalize_backup_history(parsed.libraries).map(|_| ())
    })
}

pub(super) fn begin_backup_privacy_reset_for(
    library_id: &str,
    data_directory: &Path,
) -> StorageResult<Vec<PathBuf>> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        let path = backup_location_history_path();
        let Some(mut libraries) = read_current_backup_history_at(&path)? else {
            return Ok(Vec::new());
        };
        let data_directory = normalize_directory_for_comparison(data_directory)?;
        let Some(entry) = libraries
            .iter_mut()
            .find(|entry| entry.library_id == library_id)
        else {
            return Ok(Vec::new());
        };
        if entry.data_directory != data_directory {
            return Err(StorageError::Integrity(
                "Rustnote library identity is associated with a different data directory"
                    .to_string(),
            ));
        }
        let backup_directories = entry.backup_directories.clone();
        for backup_directory in &backup_directories {
            if !entry
                .pending_privacy_reset_directories
                .contains(backup_directory)
            {
                entry
                    .pending_privacy_reset_directories
                    .push(backup_directory.clone());
            }
        }
        write_backup_location_history_at(&path, libraries)?;
        Ok(backup_directories)
    })
}

pub(super) fn clear_backup_privacy_reset_required_for(
    library_id: &str,
    data_directory: &Path,
    backup_directory: &Path,
) -> StorageResult<()> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        let path = backup_location_history_path();
        let Some(mut libraries) = read_current_backup_history_at(&path)? else {
            return Ok(());
        };
        let data_directory = normalize_directory_for_comparison(data_directory)?;
        let backup_directory = normalize_directory_for_comparison(backup_directory)?;
        if let Some(entry) = libraries
            .iter_mut()
            .find(|entry| entry.library_id == library_id)
        {
            if entry.data_directory != data_directory {
                return Err(StorageError::Integrity(
                    "Rustnote library identity is associated with a different data directory"
                        .to_string(),
                ));
            }
            entry
                .pending_privacy_reset_directories
                .retain(|path| path != &backup_directory);
        }
        write_backup_location_history_at(&path, libraries)
    })
}

pub(super) fn backup_privacy_reset_required_for(
    library_id: &str,
    data_directory: &Path,
    backup_directory: &Path,
) -> StorageResult<bool> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        let path = backup_location_history_path();
        let Some(libraries) = read_current_backup_history_at(&path)? else {
            return Ok(false);
        };
        let data_directory = normalize_directory_for_comparison(data_directory)?;
        let backup_directory = normalize_directory_for_comparison(backup_directory)?;
        let Some(entry) = libraries
            .into_iter()
            .find(|entry| entry.library_id == library_id)
        else {
            return Ok(false);
        };
        if entry.data_directory != data_directory {
            return Err(StorageError::Integrity(
                "Rustnote library identity is associated with a different data directory"
                    .to_string(),
            ));
        }
        Ok(entry
            .pending_privacy_reset_directories
            .contains(&backup_directory))
    })
}

pub(super) fn require_pending_backup_location_available(
    library_id: &str,
    data_directory: &Path,
    backup_directory: &Path,
) -> StorageResult<bool> {
    if !backup_privacy_reset_required_for(library_id, data_directory, backup_directory)? {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(backup_directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StorageError::Validation(format!(
                "this backup location still requires Vault privacy cleanup and is currently unavailable: {}",
                backup_directory.display()
            ))
        } else {
            StorageError::Io(error)
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StorageError::Integrity(format!(
            "this backup location still requires Vault privacy cleanup and is not a real directory: {}",
            backup_directory.display()
        )));
    }
    Ok(true)
}

pub(super) fn pending_backup_privacy_reset_count_for(
    library_id: &str,
    data_directory: &Path,
) -> StorageResult<usize> {
    validate_library_id(library_id)?;
    with_backup_history_lock(|| {
        let path = backup_location_history_path();
        let Some(libraries) = read_current_backup_history_at(&path)? else {
            return Ok(0);
        };
        let data_directory = normalize_directory_for_comparison(data_directory)?;
        let Some(entry) = libraries
            .into_iter()
            .find(|entry| entry.library_id == library_id)
        else {
            return Ok(0);
        };
        if entry.data_directory != data_directory {
            return Err(StorageError::Integrity(
                "Rustnote library identity is associated with a different data directory"
                    .to_string(),
            ));
        }
        Ok(entry.pending_privacy_reset_directories.len())
    })
}

fn with_backup_history_lock<T>(operation: impl FnOnce() -> StorageResult<T>) -> StorageResult<T> {
    let process_lock = BACKUP_HISTORY_PROCESS_LOCK.get_or_init(|| Mutex::new(()));
    let _process_guard = process_lock.lock().map_err(|_| {
        StorageError::Integrity("backup-location history lock is poisoned".to_string())
    })?;

    let lock_path = configuration_directory().join(BACKUP_LOCATION_HISTORY_LOCK_FILE);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&lock_path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(StorageError::Integrity(
            "backup-location history lock is invalid".to_string(),
        ));
    }
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file.lock().map_err(|error| {
        StorageError::Integrity(format!(
            "could not acquire backup-location history lock: {error}"
        ))
    })?;
    operation()
}

fn normalize_backup_history(
    libraries: Vec<BackupLocationHistoryEntry>,
) -> StorageResult<Vec<BackupLocationHistoryEntry>> {
    let mut normalized = Vec::<BackupLocationHistoryEntry>::with_capacity(libraries.len());
    for entry in libraries {
        validate_library_id(&entry.library_id)?;
        validate_nonempty_path(&entry.data_directory, "library")?;
        let data_directory = normalize_directory_for_comparison(&entry.data_directory)?;

        if normalized.iter().any(|existing| {
            existing.library_id != entry.library_id && existing.data_directory == data_directory
        }) {
            return Err(StorageError::Integrity(
                "Rustnote backup history has multiple library identities for one data directory"
                    .to_string(),
            ));
        }

        let index = if let Some(index) = normalized
            .iter()
            .position(|existing| existing.library_id == entry.library_id)
        {
            index
        } else {
            normalized.push(BackupLocationHistoryEntry {
                library_id: entry.library_id.clone(),
                data_directory: data_directory.clone(),
                backup_directories: Vec::new(),
                pending_privacy_reset_directories: Vec::new(),
            });
            normalized.len() - 1
        };
        if normalized[index].data_directory != data_directory {
            return Err(StorageError::Integrity(
                "Rustnote backup history identity has conflicting data directories".to_string(),
            ));
        }
        for backup_directory in entry.backup_directories {
            validate_nonempty_path(&backup_directory, "backup")?;
            let backup_directory = normalize_directory_for_comparison(&backup_directory)?;
            if normalized
                .iter()
                .enumerate()
                .any(|(other_index, existing)| {
                    other_index != index && existing.backup_directories.contains(&backup_directory)
                })
            {
                return Err(StorageError::Validation(format!(
                    "automatic backup folder is already associated with another Rustnote library: {}",
                    backup_directory.display()
                )));
            }
            let target = &mut normalized[index];
            if !target.backup_directories.contains(&backup_directory) {
                target.backup_directories.push(backup_directory);
            }
        }
        for pending_directory in entry.pending_privacy_reset_directories {
            validate_nonempty_path(&pending_directory, "backup")?;
            let pending_directory = normalize_directory_for_comparison(&pending_directory)?;
            if normalized
                .iter()
                .enumerate()
                .any(|(other_index, existing)| {
                    other_index != index && existing.backup_directories.contains(&pending_directory)
                })
            {
                return Err(StorageError::Validation(format!(
                    "pending backup privacy location is associated with another Rustnote library: {}",
                    pending_directory.display()
                )));
            }
            let target = &mut normalized[index];
            if !target.backup_directories.contains(&pending_directory) {
                target.backup_directories.push(pending_directory.clone());
            }
            if !target
                .pending_privacy_reset_directories
                .contains(&pending_directory)
            {
                target
                    .pending_privacy_reset_directories
                    .push(pending_directory);
            }
        }
    }
    Ok(normalized)
}

fn read_backup_location_history_file(
    path: &Path,
) -> StorageResult<Option<BackupLocationHistoryFile>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_BACKUP_LOCATION_HISTORY_BYTES
            {
                return Err(StorageError::Validation(format!(
                    "backup-location privacy history is invalid: {}",
                    path.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_BACKUP_LOCATION_HISTORY_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BACKUP_LOCATION_HISTORY_BYTES {
        return Err(StorageError::Validation(format!(
            "backup-location privacy history is unexpectedly large: {}",
            path.display()
        )));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(StorageError::from)
}

fn read_configured_storage_locations() -> StorageResult<Option<StorageLocations>> {
    let path = storage_locations_path();
    match fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(StorageError::Validation(format!(
                    "storage-location configuration is not a regular file: {}",
                    path.display()
                )));
            }
            if metadata.len() > MAX_STORAGE_LOCATIONS_BYTES {
                return Err(StorageError::Validation(format!(
                    "storage-location configuration is unexpectedly large: {}",
                    path.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    read_storage_locations_file(&path)
        .and_then(StorageLocations::try_from)
        .map(Some)
}

fn read_storage_locations_file(path: &Path) -> StorageResult<StorageLocationsFile> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_STORAGE_LOCATIONS_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STORAGE_LOCATIONS_BYTES {
        return Err(StorageError::Validation(format!(
            "storage-location configuration is unexpectedly large: {}",
            path.display()
        )));
    }
    serde_json::from_slice(&bytes).map_err(StorageError::from)
}

pub(super) fn validate_disjoint_directories(library: &Path, backup: &Path) -> StorageResult<()> {
    if library == backup || library.starts_with(backup) || backup.starts_with(library) {
        return Err(StorageError::Validation(
            "the library and automatic backup folders must not overlap or contain one another"
                .to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_storage_location_relationship(
    library: &Path,
    backup: &Path,
) -> StorageResult<()> {
    let library = normalize_directory_for_comparison(library)?;
    let backup = normalize_directory_for_comparison(backup)?;
    validate_disjoint_directories(&library, &backup)
}

fn normalize_directory_for_comparison(path: &Path) -> StorageResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut existing = lexical_normalize(&absolute);
    let mut missing = Vec::new();

    loop {
        match fs::canonicalize(&existing) {
            Ok(mut normalized) => {
                for component in missing.iter().rev() {
                    normalized.push(component);
                }
                return Ok(lexical_normalize(&normalized));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name().map(|value| value.to_os_string()) else {
                    return Err(error.into());
                };
                missing.push(name);
                if !existing.pop() {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn validate_directory_writable(path: &Path, label: &str) -> StorageResult<()> {
    let probe = path.join(format!(".rustnote-write-test-{}", new_id("dir")));
    let result = (|| -> StorageResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        file.write_all(b"rustnote")?;
        file.sync_all()?;
        drop(file);
        fs::remove_file(&probe)?;
        sync_parent_directory(&probe)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&probe);
    }
    result.map_err(|error| {
        StorageError::Validation(format!(
            "the selected {label} folder is not writable ({}): {error}",
            path.display()
        ))
    })
}

fn validate_nonempty_path(path: &Path, label: &str) -> StorageResult<()> {
    if path.as_os_str().is_empty() {
        return Err(StorageError::Validation(format!(
            "the {label} folder path is empty"
        )));
    }
    Ok(())
}

fn nonempty_environment_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn storage_locations_path() -> PathBuf {
    configuration_directory().join(STORAGE_LOCATIONS_FILE)
}

fn backup_location_history_path() -> PathBuf {
    configuration_directory().join(BACKUP_LOCATION_HISTORY_FILE)
}

fn documents_directory() -> PathBuf {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Some(documents) = crate::platform::mobile_documents_directory() {
            return documents;
        }
        if let Some(root) = crate::platform::mobile_application_support_directory() {
            return root;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = env::var_os("USERPROFILE") {
            return PathBuf::from(home).join("Documents");
        }
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join("Documents");
    }
    env::temp_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        env::temp_dir().join(format!("rustnote-{label}-{}", new_id("locations")))
    }

    #[test]
    fn backup_history_rejects_precurrent_versions() {
        let root = test_root("backup-history-old-version");
        fs::create_dir_all(&root).expect("test root");
        let history_path = root.join("history.json");
        fs::write(&history_path, br#"{"version":2,"libraries":[]}"#).expect("old history");

        let error = read_current_backup_history_at(&history_path)
            .expect_err("pre-current backup history must not be migrated");
        assert!(
            error
                .to_string()
                .contains("unsupported backup-location history version 2")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backup_history_requires_current_library_identity_shape() {
        let root = test_root("backup-history-current-shape");
        fs::create_dir_all(&root).expect("test root");
        let history_path = root.join("history.json");
        fs::write(
            &history_path,
            br#"{"version":3,"libraries":[{"data_directory":"/tmp/rustnote","backup_directories":[],"pending_privacy_reset_directories":[]}]}"#,
        )
        .expect("history missing library identity");

        read_current_backup_history_at(&history_path)
            .expect_err("current backup history must require library_id");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backup_privacy_history_never_forgets_previous_locations() {
        let root = test_root("backup-location-history");
        let library = root.join("library");
        let other_library = root.join("other-library");
        let first = root.join("first");
        let second = root.join("second");
        let other_backup = root.join("other-backup");
        fs::create_dir_all(&library).expect("library directory");
        fs::create_dir_all(&other_library).expect("other library directory");
        fs::create_dir_all(&first).expect("first backup directory");
        fs::create_dir_all(&second).expect("second backup directory");
        fs::create_dir_all(&other_backup).expect("other backup directory");
        let history_path = root.join("history.json");

        let first_locations = StorageLocations {
            data_directory: library.clone(),
            backup_directory: first.clone(),
        };
        let second_locations = StorageLocations {
            data_directory: library.clone(),
            backup_directory: second.clone(),
        };
        let other_locations = StorageLocations {
            data_directory: other_library.clone(),
            backup_directory: other_backup.clone(),
        };
        let first_library_id = new_id("library");
        let other_library_id = new_id("library");
        remember_storage_location_at(&history_path, &first_library_id, &first_locations)
            .expect("remember first");
        remember_storage_location_at(&history_path, &first_library_id, &second_locations)
            .expect("remember second");
        remember_storage_location_at(&history_path, &other_library_id, &other_locations)
            .expect("remember other library");

        let history = read_backup_location_history_file(&history_path)
            .expect("read history")
            .expect("history exists");
        let normalized = normalize_backup_history(history.libraries).expect("normalize history");
        assert_eq!(normalized.len(), 2);
        let library = fs::canonicalize(&library).expect("canonical library");
        let entry = normalized
            .iter()
            .find(|entry| entry.data_directory == library)
            .expect("library history");
        assert_eq!(entry.backup_directories.len(), 2);
        assert!(
            entry
                .backup_directories
                .contains(&fs::canonicalize(&first).expect("canonical first"))
        );
        assert!(
            entry
                .backup_directories
                .contains(&fs::canonicalize(&second).expect("canonical second"))
        );
        assert!(
            !entry
                .backup_directories
                .contains(&fs::canonicalize(&other_backup).expect("canonical other backup")),
            "backup history must remain scoped to its owning library"
        );

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn deleted_library_recovers_historical_identity_for_the_same_data_path() {
        let root = test_root("library-identity-recovery");
        let library = root.join("library");
        let backup = root.join("backup");
        fs::create_dir_all(&library).expect("library directory");
        fs::create_dir_all(&backup).expect("backup directory");
        let history_path = root.join("history.json");
        let historical_id = new_id("library");
        let locations = StorageLocations {
            data_directory: library.clone(),
            backup_directory: backup,
        };
        remember_storage_location_at(&history_path, &historical_id, &locations)
            .expect("remember historical owner");

        fs::remove_dir_all(&library).expect("delete old library");
        fs::create_dir_all(&library).expect("recreate empty library");
        let recovered = ensure_library_identity_with_history_at(&library, &history_path)
            .expect("recover historical identity");

        assert_eq!(recovered, historical_id);
        validate_storage_location_ownership_at(&history_path, &recovered, &locations)
            .expect("recovered identity must retain backup ownership");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_bootstrap_identity_is_reconciled_only_while_library_is_pristine() {
        let root = test_root("library-bootstrap-reconcile");
        let library = root.join("library");
        let backup = root.join("backup");
        fs::create_dir_all(&library).expect("library directory");
        fs::create_dir_all(&backup).expect("backup directory");
        let history_path = root.join("history.json");
        let historical_id = new_id("library");
        let locations = StorageLocations {
            data_directory: library.clone(),
            backup_directory: backup,
        };
        remember_storage_location_at(&history_path, &historical_id, &locations)
            .expect("remember historical owner");

        let bootstrap_id = new_id("library");
        fs::write(library.join(LIBRARY_IDENTITY_FILE), &bootstrap_id)
            .expect("write failed-bootstrap identity");
        fs::write(library.join(".rustnote-instance.lock"), b"").expect("instance lock shell");
        fs::write(library.join(".rustnote-directory.lock"), b"").expect("directory lock shell");
        fs::create_dir(library.join("attachments")).expect("empty attachments shell");

        let recovered = ensure_library_identity_with_history_at(&library, &history_path)
            .expect("reconcile failed bootstrap");
        assert_eq!(recovered, historical_id);
        assert_ne!(recovered, bootstrap_id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_pristine_library_identity_is_never_rewritten_from_history() {
        let root = test_root("library-identity-no-rewrite");
        let library = root.join("library");
        let backup = root.join("backup");
        fs::create_dir_all(&library).expect("library directory");
        fs::create_dir_all(&backup).expect("backup directory");
        let history_path = root.join("history.json");
        let historical_id = new_id("library");
        let locations = StorageLocations {
            data_directory: library.clone(),
            backup_directory: backup,
        };
        remember_storage_location_at(&history_path, &historical_id, &locations)
            .expect("remember historical owner");

        let different_id = new_id("library");
        fs::write(library.join(LIBRARY_IDENTITY_FILE), &different_id)
            .expect("write current identity");
        fs::write(library.join("rustnote.sqlite3"), b"not-pristine")
            .expect("mark real library state");

        let retained = ensure_library_identity_with_history_at(&library, &history_path)
            .expect("retain non-pristine identity");
        assert_eq!(retained, different_id);
        let error = validate_storage_location_ownership_at(&history_path, &retained, &locations)
            .expect_err("cross-library backup ownership must remain fail-closed");
        assert!(error.to_string().contains("another Rustnote library"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn library_identity_rejects_symlink_substitution() {
        use std::os::unix::fs::symlink;

        let root = test_root("library-identity-symlink");
        let library = root.join("library");
        fs::create_dir_all(&library).expect("library");
        let target = root.join("identity-target");
        fs::write(&target, new_id("library")).expect("identity target");
        symlink(&target, library.join(LIBRARY_IDENTITY_FILE)).expect("identity symlink");

        let error = ensure_library_identity(&library)
            .expect_err("library identity must never follow a substituted symlink");
        assert!(error.to_string().contains("library identity is invalid"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backup_privacy_history_rejects_cross_library_repository_reuse() {
        let root = test_root("backup-location-owner");
        let first_library = root.join("first-library");
        let second_library = root.join("second-library");
        let shared_backup = root.join("shared-backup");
        fs::create_dir_all(&first_library).expect("first library");
        fs::create_dir_all(&second_library).expect("second library");
        fs::create_dir_all(&shared_backup).expect("shared backup");
        let history_path = root.join("history.json");

        let first_library_id = new_id("library");
        let second_library_id = new_id("library");
        remember_storage_location_at(
            &history_path,
            &first_library_id,
            &StorageLocations {
                data_directory: first_library,
                backup_directory: shared_backup.clone(),
            },
        )
        .expect("remember first owner");

        let error = remember_storage_location_at(
            &history_path,
            &second_library_id,
            &StorageLocations {
                data_directory: second_library,
                backup_directory: shared_backup,
            },
        )
        .expect_err("backup repository ownership must not silently transfer between libraries");
        assert!(error.to_string().contains("another Rustnote library"));

        let _ = fs::remove_dir_all(root);
    }
}
