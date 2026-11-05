use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::domain::ArchiveEntry;

use super::attachments::{
    COPY_BUFFER_SIZE, digest_to_hex, sync_parent_directory, temporary_sibling, validate_sha256,
};
use super::{StorageError, StorageResult, new_id};

pub(super) const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024 * 1024;
pub(super) const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub(super) const MAX_DATABASE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub(super) const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub(super) const MAX_ARCHIVE_FILES: usize = 100_000;
pub(super) const MAX_ARCHIVE_PATH_BYTES: usize = 4_096;
pub(super) const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn validate_zip_structure<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
) -> StorageResult<HashSet<String>> {
    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(StorageError::InvalidBackup(
            "archive contains too many files".to_string(),
        ));
    }
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name().to_string();
        if name.is_empty() || name.len() > MAX_ARCHIVE_PATH_BYTES {
            return Err(StorageError::InvalidBackup(
                "archive contains an invalid path length".to_string(),
            ));
        }
        if file.is_dir() || file.enclosed_name().is_none() || name.contains('\\') {
            return Err(StorageError::InvalidBackup(format!(
                "archive contains an unsafe path: {name}"
            )));
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(StorageError::InvalidBackup(format!(
                "archive contains a symbolic link: {name}"
            )));
        }
        if file.size() > MAX_ENTRY_BYTES {
            return Err(StorageError::InvalidBackup(format!(
                "archive entry is too large: {name}"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(StorageError::InvalidBackup(format!(
                "archive contains duplicate path: {name}"
            )));
        }
    }
    Ok(names)
}

pub(super) fn validate_manifest_entry(entry: &ArchiveEntry) -> StorageResult<()> {
    if entry.path.is_empty() || entry.path.len() > MAX_ARCHIVE_PATH_BYTES {
        return Err(StorageError::InvalidBackup(
            "manifest contains an invalid path length".to_string(),
        ));
    }
    if entry.byte_size > MAX_ENTRY_BYTES {
        return Err(StorageError::InvalidBackup(format!(
            "manifest entry is too large: {}",
            entry.path
        )));
    }
    validate_sha256(&entry.sha256)?;
    let valid_path = entry.path == "library.sqlite3"
        || entry
            .path
            .strip_prefix("blobs/")
            .is_some_and(|hash| hash == entry.sha256 && hash.len() == 64);
    if !valid_path {
        return Err(StorageError::InvalidBackup(format!(
            "manifest contains a non-portable path: {}",
            entry.path
        )));
    }
    Ok(())
}

pub(super) fn read_zip_entry_bounded<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    maximum_bytes: u64,
) -> StorageResult<Vec<u8>> {
    let mut file = archive.by_name(name)?;
    if file.size() > maximum_bytes {
        return Err(StorageError::InvalidBackup(format!(
            "archive entry exceeds its supported size: {name}"
        )));
    }
    let capacity = usize::try_from(file.size())
        .map_err(|_| StorageError::InvalidBackup(format!("archive entry is too large: {name}")))?;
    let mut bytes = Vec::with_capacity(capacity);
    let copied = io::copy(
        &mut Read::by_ref(&mut file).take(maximum_bytes + 1),
        &mut bytes,
    )?;
    if copied > maximum_bytes {
        return Err(StorageError::InvalidBackup(format!(
            "archive entry expanded beyond its supported size: {name}"
        )));
    }
    Ok(bytes)
}

pub(super) fn extract_verified_zip_entry<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &str,
    entry: &ArchiveEntry,
    destination: &Path,
) -> StorageResult<()> {
    let mut source = archive.by_name(archive_path)?;
    if source.size() != entry.byte_size || source.size() > MAX_ENTRY_BYTES {
        return Err(StorageError::Integrity(format!(
            "archive metadata mismatch for {archive_path}"
        )));
    }
    let temporary_path = temporary_sibling(destination, "restore-stage.tmp")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?,
    );
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        written = written.checked_add(read as u64).ok_or_else(|| {
            StorageError::InvalidBackup("archive entry size overflow".to_string())
        })?;
        if written > entry.byte_size || written > MAX_ENTRY_BYTES {
            let _ = fs::remove_file(&temporary_path);
            return Err(StorageError::InvalidBackup(format!(
                "archive entry expanded beyond its declared size: {archive_path}"
            )));
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
    }
    output.flush()?;
    let file = output.into_inner().map_err(|error| error.into_error())?;
    file.sync_all()?;
    let checksum = digest_to_hex(&hasher.finalize());
    if written != entry.byte_size || checksum != entry.sha256 {
        let _ = fs::remove_file(&temporary_path);
        return Err(StorageError::Integrity(format!(
            "archive blob {archive_path} failed checksum verification"
        )));
    }
    fs::rename(&temporary_path, destination)?;
    sync_parent_directory(destination)?;
    Ok(())
}

pub(super) fn escape_markdown_link_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' | '[' | ']' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '\r' | '\n' => escaped.push(' '),
            character if character.is_control() => escaped.push(' '),
            character => escaped.push(character),
        }
    }
    escaped
}

pub(super) fn sanitize_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let compact = compact.trim_matches('.');
    if compact.is_empty() {
        return "Untitled".to_string();
    }

    const MAX_COMPONENT_BYTES: usize = 180;
    let mut end = 0;
    for (index, character) in compact.char_indices() {
        let next = index + character.len_utf8();
        if next > MAX_COMPONENT_BYTES {
            break;
        }
        end = next;
    }
    compact[..end].to_string()
}

pub(super) fn unique_directory(parent: &Path, base_name: &str) -> PathBuf {
    let candidate = parent.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    for index in 2..10_000 {
        let candidate = parent.join(format!("{base_name}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base_name}-{}", new_id("export")))
}

#[cfg(test)]
mod tests {
    use super::sanitize_component;

    #[test]
    fn sanitized_export_components_stay_within_utf8_byte_limit() {
        let component = sanitize_component(&"界".repeat(100));
        assert!(component.len() <= 180);
        assert!(!component.is_empty());
    }
}
