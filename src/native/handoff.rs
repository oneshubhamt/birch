use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::attachment_media::preferred_extension;

use sha2::{Digest, Sha256};

use super::{copy_file_atomically, existing_path};

pub fn open_managed_file(
    handoff_root: &Path,
    path: &Path,
    file_name: &str,
    mime_type: Option<&str>,
) -> super::NativeResult<()> {
    open_managed_file_with_source_key(handoff_root, path, file_name, mime_type, None)
}

/// Open plaintext that belongs to a note but does not yet have a stable content
/// hash (for example a pending attachment ingest). The note-scoped handoff key
/// lets Encrypt remove this Rustnote-managed copy deterministically later.
pub(crate) fn open_managed_file_for_note(
    handoff_root: &Path,
    path: &Path,
    file_name: &str,
    mime_type: Option<&str>,
    note_id: &str,
) -> super::NativeResult<()> {
    let source_key = note_handoff_source_key(note_id);
    open_managed_file_with_source_key(
        handoff_root,
        path,
        file_name,
        mime_type,
        Some(&source_key),
    )
}

fn open_managed_file_with_source_key(
    handoff_root: &Path,
    path: &Path,
    file_name: &str,
    mime_type: Option<&str>,
    source_key: Option<&str>,
) -> super::NativeResult<()> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = handoff_root;
        let _ = source_key;
        // The mobile shell can expose the canonical blob read-only through a native
        // content URI/share mechanism, so do not duplicate multi-gigabyte files here.
        let path = existing_path(path)?;
        return crate::platform::open_external_file(&path, Some(file_name), mime_type).map_err(Into::into);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if crate::security::is_vault_runtime_path(path) {
            let path = existing_path(path)?;
            return crate::platform::open_external_file(&path, Some(file_name), mime_type)
                .map_err(Into::into);
        }
        let path = materialize_handoff_path(handoff_root, path, file_name, mime_type, source_key)?;
        crate::platform::open_external_file(&path, Some(file_name), mime_type).map_err(Into::into)
    }
}

pub fn reveal_managed_file(
    handoff_root: &Path,
    path: &Path,
    file_name: &str,
    mime_type: Option<&str>,
) -> super::NativeResult<()> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = handoff_root;
        // Sharing likewise delegates the canonical blob to the sandbox-aware shell.
        let path = existing_path(path)?;
        return crate::platform::reveal_external_file(&path, Some(file_name), mime_type).map_err(Into::into);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if crate::security::is_vault_runtime_path(path) {
            let path = existing_path(path)?;
            return crate::platform::reveal_external_file(&path, Some(file_name), mime_type)
                .map_err(Into::into);
        }
        let path = materialize_handoff_path(handoff_root, path, file_name, mime_type, None)?;
        crate::platform::reveal_external_file(&path, Some(file_name), mime_type).map_err(Into::into)
    }
}

/// Copy a managed attachment to a user-selected destination without mutating the
/// canonical blob. The destination replacement is atomic within its directory.
pub fn save_managed_file(path: &Path, destination: &Path) -> super::NativeResult<()> {
    let source = existing_path(path)?;
    copy_file_atomically(&source, destination, "attachment")
}

/// Best-effort startup cleanup for library-scoped handoff copies left by crashed Rustnote processes.
pub fn cleanup_stale_attachment_handoffs(handoff_root: &Path) -> super::NativeResult<usize> {
    super::temp_owner::cleanup_session_directories(handoff_root, "attachment handoffs")
}

/// Remove Rustnote-managed handoff copies whose canonical source key is now
/// being protected. Ordinary attachment blobs use their SHA-256 filename as the
/// handoff source key, so E3 can invalidate only the affected plaintext copies
/// instead of deleting unrelated handoffs.
pub(crate) fn note_handoff_source_key(note_id: &str) -> String {
    let digest = Sha256::digest(note_id.as_bytes());
    let suffix = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("note-{suffix}")
}

pub(crate) fn discard_handoffs_for_source_keys(
    handoff_root: &Path,
    source_keys: &HashSet<String>,
) -> super::NativeResult<usize> {
    if source_keys.is_empty() {
        return Ok(0);
    }
    let process_directories = match fs::read_dir(handoff_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return super::native_error(format!("Could not inspect attachment handoffs: {error}")),
    };
    let mut removed = 0;
    for process in process_directories {
        let process = process.map_err(|error| {
            super::NativeError::operation(format!("Could not inspect attachment handoffs: {error}"))
        })?;
        if !process.file_type().map_err(|error| {
            super::NativeError::operation(format!("Could not inspect attachment handoffs: {error}"))
        })?.is_dir() {
            continue;
        }
        for source_key in source_keys {
            let directory = process.path().join(source_key);
            match super::privacy::remove_tree(&directory) {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return super::native_error(format!(
                        "Could not remove protected attachment handoff {}: {error}",
                        directory.display()
                    ));
                }
            }
        }
        let _ = fs::remove_dir(process.path());
    }
    Ok(removed)
}

/// Remove every plaintext handoff payload for this library while preserving a
/// live session's ownership-lock artifacts. E3 uses this when older protected
/// libraries cannot identify historical plaintext hashes safely.
pub(crate) fn discard_all_handoffs(handoff_root: &Path) -> super::NativeResult<usize> {
    let sessions = match fs::read_dir(handoff_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return super::native_error(format!(
                "Could not inspect attachment handoffs: {error}"
            ));
        }
    };
    let mut removed = 0;
    for session in sessions {
        let session = session.map_err(|error| {
            super::NativeError::operation(format!("Could not inspect attachment handoffs: {error}"))
        })?;
        if !session.file_type().map_err(|error| {
            super::NativeError::operation(format!("Could not inspect attachment handoffs: {error}"))
        })?.is_dir() {
            continue;
        }
        let session_path = session.path();
        let mut has_owner_artifact = false;
        let payloads = match fs::read_dir(&session_path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return super::native_error(format!(
                    "Could not inspect attachment handoff session: {error}"
                ));
            }
        };
        for payload in payloads {
            let payload = payload.map_err(|error| {
                super::NativeError::operation(format!("Could not inspect attachment handoff session: {error}"))
            })?;
            if super::temp_owner::is_owner_artifact_name(&payload.file_name()) {
                has_owner_artifact = true;
                continue;
            }
            let path = payload.path();
            let file_type = payload.file_type().map_err(|error| {
                super::NativeError::operation(format!("Could not inspect attachment handoff payload: {error}"))
            })?;
            let result = if file_type.is_dir() {
                super::privacy::remove_tree(&path)
            } else {
                super::privacy::remove_file(&path).map(|_| ())
            };
            match result {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return super::native_error(format!(
                        "Could not clear attachment handoff payload {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        if !has_owner_artifact {
            let _ = fs::remove_dir(&session_path);
        }
    }
    Ok(removed)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn materialize_handoff_path(
    handoff_root: &Path,
    source: &Path,
    file_name: &str,
    mime_type: Option<&str>,
    source_key: Option<&str>,
) -> super::NativeResult<PathBuf> {
    let source = existing_path(source)?;
    let file_name = handoff_file_name(file_name, mime_type);
    let source_key = source_key.unwrap_or_else(|| {
        source
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("attachment")
    });
    let process_owner =
        super::temp_owner::process_session_owner(handoff_root, "attachment handoffs")?;
    let directory = process_owner.root().join(source_key);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare attachment handoff: {error}"))?;
    let destination = directory.join(file_name);
    if crate::security::managed_path_is_present(&destination)
        .map_err(|error| super::NativeError::operation(format!(
            "Could not inspect attachment handoff destination: {error}"
        )))?
    {
        let mut prior = crate::security::OwnedPlaintext::process_private(
            process_owner.clone(),
            format!("handoff-refresh-{source_key}"),
            destination.clone(),
        )
        .map_err(super::NativeError::operation)?;
        prior
            .remove_file()
            .map_err(|error| super::NativeError::operation(format!(
                "Could not refresh attachment handoff: {error}"
            )))?;
    }
    let mut owned = crate::security::OwnedPlaintext::process_private(
        process_owner,
        format!("handoff-{source_key}"),
        destination,
    )
    .map_err(super::NativeError::operation)?;
    owned
        .copy_from(&source)
        .map_err(|error| super::NativeError::operation(format!(
            "Could not prepare attachment handoff: {error}"
        )))?;
    owned
        .into_transferred_path()
        .map_err(super::NativeError::operation)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn handoff_file_name(file_name: &str, mime_type: Option<&str>) -> String {
    let base = Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment");
    let candidate = if Path::new(base).extension().is_some() {
        base.to_string()
    } else {
        match preferred_extension(mime_type) {
            Some(extension) => format!("{base}.{extension}"),
            None => base.to_string(),
        }
    };
    portable_handoff_file_name(&candidate)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn portable_handoff_file_name(file_name: &str) -> String {
    const MAX_CHARS: usize = 180;

    let sanitized = file_name
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized
        .trim()
        .trim_end_matches([' ', '.']);
    let sanitized = if sanitized.is_empty() || matches!(sanitized, "." | "..") {
        "attachment"
    } else {
        sanitized
    };

    let path = Path::new(sanitized);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty());
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment");
    let reserved = matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    );
    let stem = if reserved { format!("_{stem}") } else { stem.to_string() };

    let extension_chars = extension.map_or(0, |value| value.chars().count() + 1);
    let stem_limit = MAX_CHARS.saturating_sub(extension_chars).max(1);
    let mut stem = stem.chars().take(stem_limit).collect::<String>();
    if stem.is_empty() {
        stem.push_str("attachment");
    }
    match extension {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_handoff_scrub_preserves_live_session_owner() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-handoff-owner-test-{}",
            rand::random::<u64>()
        ));
        let owner = super::super::temp_owner::SensitiveTempOwner::acquire(&root, "session-")
            .expect("handoff owner");
        let payload = owner.root().join("source-hash");
        fs::create_dir_all(&payload).expect("payload directory");
        fs::write(payload.join("secret.txt"), b"secret").expect("payload bytes");

        assert_eq!(discard_all_handoffs(&root).expect("scrub handoffs"), 1);
        assert!(!payload.exists());
        assert!(owner.root().is_dir(), "live owner directory must survive E3 scrub");
        assert!(
            fs::read_dir(owner.root())
                .expect("owner directory")
                .flatten()
                .any(|entry| super::super::temp_owner::is_owner_artifact_name(&entry.file_name())),
            "ownership lock must remain published for the live session"
        );

        drop(owner);
        let _ = fs::remove_dir_all(root);
    }
}
