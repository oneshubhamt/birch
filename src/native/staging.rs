use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, SystemTime},
};

use super::{
    copy_file_atomically,
    privacy::remove_file as privacy_remove_file,
};

const PDF_EXPORT_TOKEN_BYTES: usize = 16;
const IMAGE_STAGE_TOKEN_BYTES: usize = 16;
const IMAGE_EDIT_STAGE_MAX_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const IMAGE_IMPORT_STAGE_MAX_BYTES: usize = 64 * 1024 * 1024;
const ABANDONED_IMAGE_STAGE_READY_AGE: Duration = Duration::from_secs(10 * 60);
const STALE_PROCESS_STAGE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
enum PlaintextDerivativeOwner {
    Attachment(String),
    Note(String),
}

#[derive(Clone, Debug)]
struct PlaintextDerivativeStage {
    owner: PlaintextDerivativeOwner,
    process_owner: super::temp_owner::ProcessPrivateOwner,
    path: PathBuf,
}

static PLAINTEXT_DERIVATIVE_STAGES: OnceLock<Mutex<HashMap<String, PlaintextDerivativeStage>>> =
    OnceLock::new();

fn derivative_stage_registry() -> &'static Mutex<HashMap<String, PlaintextDerivativeStage>> {
    PLAINTEXT_DERIVATIVE_STAGES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_derivative_stage_registry(
) -> MutexGuard<'static, HashMap<String, PlaintextDerivativeStage>> {
    derivative_stage_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn valid_stage_attachment_id(value: &str) -> bool {
    value.starts_with("att_")
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_stage_note_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn derivative_stage_key(kind: &str, token: &str) -> String {
    format!("{kind}:{token}")
}

fn reserve_derivative_stage(
    key: String,
    owner: PlaintextDerivativeOwner,
    process_owner: super::temp_owner::ProcessPrivateOwner,
    path: PathBuf,
) -> bool {
    let mut stages = lock_derivative_stage_registry();
    match stages.entry(key) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(PlaintextDerivativeStage {
                owner,
                process_owner,
                path,
            });
            true
        }
        std::collections::hash_map::Entry::Occupied(_) => false,
    }
}

fn ensure_derivative_stage_attachment_identity(
    key: &str,
    attachment_id: &str,
) -> super::NativeResult<PathBuf> {
    let stages = lock_derivative_stage_registry();
    let stage = stages
        .get(key)
        .ok_or_else(|| super::NativeError::operation("Sensitive staged derivative is no longer available"))?;
    if stage.owner != PlaintextDerivativeOwner::Attachment(attachment_id.to_string()) {
        return super::native_error("Sensitive staged derivative belongs to a different attachment");
    }
    Ok(stage.path.clone())
}

fn unregister_derivative_stage(key: &str) {
    lock_derivative_stage_registry().remove(key);
}

fn remove_plaintext_file_strict(path: &Path) -> io::Result<()> {
    let removed = privacy_remove_file(path)?;
    if !removed && crate::security::managed_path_is_present(path)? {
        return Err(io::Error::other(format!(
            "registered plaintext path is not a removable file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn remove_registered_stage_plaintext(key: &str, expected_path: &Path) -> io::Result<()> {
    let process_owner = {
        let stages = lock_derivative_stage_registry();
        let stage = stages
            .get(key)
            .ok_or_else(|| io::Error::other("registered plaintext stage disappeared"))?;
        if stage.path != expected_path {
            return Err(io::Error::other(
                "registered plaintext stage path changed during cleanup",
            ));
        }
        stage.process_owner.clone()
    };
    let mut plaintext = crate::security::OwnedPlaintext::process_private(
        process_owner,
        key.to_string(),
        expected_path.to_path_buf(),
    )
    .map_err(io::Error::other)?;
    plaintext.remove_file()
}

fn cleanup_registered_stage_after_failure(key: &str, path: &Path) {
    // Keep the registry entry if secure deletion fails. That intentionally
    // leaves the derivative discoverable by the next Vault transition rather
    // than orphaning plaintext after an error path.
    if remove_registered_stage_plaintext(key, path).is_ok() {
        unregister_derivative_stage(key);
    }
}

/// Fail-closed targeted cleanup used while a note's plaintext-derivative gate is
/// exclusively held by the Vault protection transition.
pub(crate) fn discard_staged_derivatives_for_attachments(
    attachment_ids: &HashSet<String>,
) -> super::NativeResult<usize> {
    if attachment_ids.is_empty() {
        return Ok(0);
    }
    let targets = {
        let stages = lock_derivative_stage_registry();
        stages
            .iter()
            .filter(|(_, stage)| {
                matches!(
                    &stage.owner,
                    PlaintextDerivativeOwner::Attachment(attachment_id)
                        if attachment_ids.contains(attachment_id)
                )
            })
            .map(|(key, stage)| (key.clone(), stage.path.clone()))
            .collect::<Vec<_>>()
    };

    let mut removed = 0;
    for (key, path) in targets {
        remove_registered_stage_plaintext(&key, &path).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not remove protected attachment staging residue: {error}"
            ))
        })?;
        unregister_derivative_stage(&key);
        removed += 1;
    }
    Ok(removed)
}

/// Fail-closed cleanup for browser-originated plaintext that has been staged for
/// a specific note but has not yet become a canonical attachment.
pub(crate) fn discard_staged_derivatives_for_note(note_id: &str) -> super::NativeResult<usize> {
    if !valid_stage_note_id(note_id) {
        return super::native_error("Invalid note identity for sensitive staging cleanup");
    }
    let targets = {
        let stages = lock_derivative_stage_registry();
        stages
            .iter()
            .filter(|(_, stage)| {
                matches!(&stage.owner, PlaintextDerivativeOwner::Note(owner) if owner == note_id)
            })
            .map(|(key, stage)| (key.clone(), stage.path.clone()))
            .collect::<Vec<_>>()
    };

    let mut removed = 0;
    for (key, path) in targets {
        remove_registered_stage_plaintext(&key, &path).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not remove protected note staging residue: {error}"
            ))
        })?;
        unregister_derivative_stage(&key);
        removed += 1;
    }
    Ok(removed)
}

#[derive(Debug)]
pub enum PdfExportStagingError {
    InvalidToken,
    InvalidPdf,
    Prepare(io::Error),
    Create(io::Error),
    Stage(io::Error),
    AllocationFailed,
    Discard(io::Error),
    Unavailable,
    Save(super::NativeError),
}

impl std::fmt::Display for PdfExportStagingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => formatter.write_str("Invalid staged PDF export token"),
            Self::InvalidPdf => formatter.write_str("Exported data is not a PDF"),
            Self::Prepare(error) => {
                write!(formatter, "Could not prepare PDF export staging: {error}")
            }
            Self::Create(error) => {
                write!(formatter, "Could not create PDF export staging file: {error}")
            }
            Self::Stage(error) => write!(formatter, "Could not stage annotated PDF: {error}"),
            Self::AllocationFailed => {
                formatter.write_str("Could not allocate PDF export staging file")
            }
            Self::Discard(error) => {
                write!(formatter, "Could not discard staged PDF export: {error}")
            }
            Self::Unavailable => formatter.write_str("Staged PDF export is no longer available"),
            Self::Save(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for PdfExportStagingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Prepare(error)
            | Self::Create(error)
            | Self::Stage(error)
            | Self::Discard(error) => Some(error),
            Self::Save(error) => Some(error),
            Self::InvalidToken
            | Self::InvalidPdf
            | Self::AllocationFailed
            | Self::Unavailable => None,
        }
    }
}

/// Fail-closed startup cleanup for staged PDF exports left by crashed Rustnote processes.
/// Callers must stop startup if this returns an error so stale plaintext is not ignored.
pub fn cleanup_stale_pdf_exports() -> super::NativeResult<usize> {
    cleanup_sensitive_stage_root(&pdf_export_root()?, "PDF exports")
}

/// Fail-closed startup cleanup for archive restore staging left by an older
/// Rustnote process. Restore children live beneath an OS-lock-owned process
/// session, so cleanup claims only abandoned owners and never races a live one.
pub fn cleanup_stale_archive_restore_stages() -> super::NativeResult<usize> {
    cleanup_sensitive_stage_root(&archive_restore_root()?, "archive restore staging")
}

pub(crate) fn archive_restore_process_root() -> super::NativeResult<PathBuf> {
    let root = archive_restore_root()?;
    super::temp_owner::process_session_root(&root, "archive restore staging")
}

fn archive_restore_root() -> super::NativeResult<PathBuf> {
    super::private_temp::rustnote_temp_directory("archive-restore").map_err(|error| {
        super::NativeError::operation(format!(
            "Could not prepare archive restore staging: {error}"
        ))
    })
}

/// Fail-closed startup cleanup for browser-originated image staging left by crashed
/// Rustnote processes. Callers must stop startup on error.
pub fn cleanup_stale_image_stages() -> super::NativeResult<usize> {
    Ok(cleanup_sensitive_stage_root(&image_edit_stage_root()?, "image edits")?
        + cleanup_sensitive_stage_root(&image_import_stage_root()?, "image imports")?)
}

fn cleanup_sensitive_stage_root(root: &Path, label: &str) -> super::NativeResult<usize> {
    let mut removed = super::temp_owner::cleanup_session_directories(root, label)?;
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(removed),
        Err(error) => return super::native_error(format!("Could not inspect {label}: {error}")),
    };
    let now = SystemTime::now();

    for entry in entries {
        let entry = entry.map_err(|error| {
            super::NativeError::operation(format!("Could not inspect {label}: {error}"))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            super::NativeError::operation(format!("Could not inspect {label}: {error}"))
        })?;
        // Session directories are handled above using their ownership lock.
        // Retain only the old root-level stray-file cleanup here.
        if !file_type.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                super::NativeError::operation(format!("Could not inspect {label}: {error}"))
            })?;
        let removable = now
            .duration_since(modified)
            .is_ok_and(|age| age >= STALE_PROCESS_STAGE_AGE);
        if !removable {
            continue;
        }

        remove_plaintext_file_strict(&path)
            .map_err(|error| format!("Could not clear {label}: {error}"))?;
        removed += 1;
    }
    Ok(removed)
}

fn image_edit_stage_root() -> super::NativeResult<PathBuf> {
    super::private_temp::rustnote_temp_directory("image-edit")
        .map_err(|error| super::NativeError::operation(format!("Could not prepare image edit staging: {error}")))
}

fn image_import_stage_root() -> super::NativeResult<PathBuf> {
    super::private_temp::rustnote_temp_directory("image-import")
        .map_err(|error| super::NativeError::operation(format!("Could not prepare image import staging: {error}")))
}

fn image_edit_stage_process_owner() -> super::NativeResult<super::temp_owner::ProcessPrivateOwner> {
    let root = image_edit_stage_root()?;
    super::temp_owner::process_session_owner(&root, "image edit staging")
}

fn image_edit_stage_process_root() -> super::NativeResult<PathBuf> {
    Ok(image_edit_stage_process_owner()?.root().to_path_buf())
}

fn image_import_stage_process_owner() -> super::NativeResult<super::temp_owner::ProcessPrivateOwner> {
    let root = image_import_stage_root()?;
    super::temp_owner::process_session_owner(&root, "image import staging")
}

fn image_import_stage_process_root() -> super::NativeResult<PathBuf> {
    Ok(image_import_stage_process_owner()?.root().to_path_buf())
}

fn image_stage_token() -> String {
    rand::random::<[u8; IMAGE_STAGE_TOKEN_BYTES]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_image_stage_token(token: &str) -> bool {
    token.len() == IMAGE_STAGE_TOKEN_BYTES * 2
        && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn image_edit_stage_extension(mime_type: &str) -> super::NativeResult<&'static str> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        _ => super::native_error("Edited image staging only accepts PNG or JPEG"),
    }
}

fn validate_image_edit_stage_bytes(bytes: &[u8], mime_type: &str) -> super::NativeResult<()> {
    if bytes.is_empty() {
        return super::native_error("Edited image output is empty");
    }
    if bytes.len() > IMAGE_EDIT_STAGE_MAX_BYTES {
        return super::native_error("Edited image is larger than the 128 MB edit limit");
    }
    match mime_type {
        "image/png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok(()),
        "image/jpeg" if bytes.starts_with(b"\xFF\xD8\xFF") => Ok(()),
        "image/png" | "image/jpeg" => {
            super::native_error("Edited image data does not match its media type")
        }
        _ => super::native_error("Edited image staging only accepts PNG or JPEG"),
    }
}

fn image_stage_path(
    root: &Path,
    token: &str,
    extension: &str,
    claimed: bool,
) -> super::NativeResult<PathBuf> {
    if !valid_image_stage_token(token) {
        return super::native_error("Invalid staged image token");
    }
    let suffix = if claimed { "claimed" } else { "ready" };
    Ok(root.join(format!("{token}.{suffix}.{extension}")))
}

fn prune_abandoned_ready_image_stages(
    process_owner: &super::temp_owner::ProcessPrivateOwner,
) -> super::NativeResult<usize> {
    let root = process_owner.root();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return super::native_error(format!("Could not inspect image staging: {error}")),
    };
    // Registered derivatives have an explicit owner and lifecycle. Never age
    // them out behind the registry's back; Vault transitions and consumers are
    // responsible for retiring them. This opportunistic pruning is only for
    // unregistered ready files left by an interrupted local staging attempt.
    let registered_paths = {
        let stages = lock_derivative_stage_registry();
        stages
            .values()
            .map(|stage| stage.path.clone())
            .collect::<HashSet<_>>()
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(|error| {
            super::NativeError::operation(format!(
                "Could not inspect image staging entry: {error}"
            ))
        })?;
        let path = entry.path();
        if registered_paths.contains(&path) {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            super::NativeError::operation(format!(
                "Could not inspect image staging entry type: {error}"
            ))
        })?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.contains(".ready.") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                super::NativeError::operation(format!(
                    "Could not inspect image staging age: {error}"
                ))
            })?;
        let Ok(age) = now.duration_since(modified) else { continue };
        if age < ABANDONED_IMAGE_STAGE_READY_AGE {
            continue;
        }
        let mut plaintext = crate::security::OwnedPlaintext::process_private(
            process_owner.clone(),
            "abandoned-image-stage".to_string(),
            path,
        )
        .map_err(super::NativeError::operation)?;
        plaintext.remove_file().map_err(|error| {
            super::NativeError::operation(format!(
                "Could not remove abandoned image staging: {error}"
            ))
        })?;
        removed += 1;
    }
    Ok(removed)
}

#[cfg(test)]
fn stage_image_reader(
    root: &Path,
    mut reader: impl Read,
    byte_len: usize,
    max_bytes: usize,
    extension: &str,
    label: &str,
) -> super::NativeResult<String> {
    if byte_len == 0 {
        return super::native_error(format!("{label} is empty"));
    }
    if byte_len > max_bytes {
        return super::native_error(format!("{label} is too large"));
    }
    super::private_temp::ensure_private_directory(root)
        .map_err(|error| format!("Could not prepare {label}: {error}"))?;
    for _ in 0..8 {
        let token = image_stage_token();
        let path = root.join(format!("{token}.ready.{extension}"));
        let file = super::private_temp::create_private_file(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return super::native_error(format!("Could not create {label} file: {error}"));
            }
        };
        // Process-local staging is disposable. Canonical attachment storage performs
        // durable sync when it consumes the claimed file.
        let mut limited = (&mut reader).take(byte_len as u64 + 1);
        let copied = match io::copy(&mut limited, &mut file).and_then(|copied| {
            file.flush()?;
            Ok(copied)
        }) {
            Ok(copied) => copied,
            Err(error) => {
                if let Err(cleanup) = remove_plaintext_file_strict(&path) {
                    return super::native_error(format!(
                        "Could not stage {label}: {error}; cleanup failed: {cleanup}"
                    ));
                }
                return super::native_error(format!("Could not stage {label}: {error}"));
            }
        };
        if copied != byte_len as u64 {
            if let Err(cleanup) = remove_plaintext_file_strict(&path) {
                return super::native_error(format!(
                    "Could not stage {label}: expected {byte_len} bytes but received {copied}; cleanup failed: {cleanup}"
                ));
            }
            return super::native_error(format!(
                "Could not stage {label}: expected {byte_len} bytes but received {copied}"
            ));
        }
        return Ok(token);
    }
    super::native_error(format!("Could not allocate {label} file"))
}

fn claim_registered_staged_image_file(
    key: &str,
    expected_owner: &PlaintextDerivativeOwner,
    root: &Path,
    token: &str,
    extension: &str,
    max_bytes: usize,
    label: &str,
) -> super::NativeResult<PathBuf> {
    let ready = image_stage_path(root, token, extension, false)?;
    let claimed = image_stage_path(root, token, extension, true)?;

    // The registry lock spans the filesystem rename and ownership-path update.
    // Vault cleanup therefore observes either the old registered path or the
    // new registered path, never an unowned interval between them.
    {
        let mut stages = lock_derivative_stage_registry();
        let stage = stages.get_mut(key).ok_or_else(|| {
            super::NativeError::operation("Sensitive staged derivative is no longer available")
        })?;
        if &stage.owner != expected_owner {
            return super::native_error("Sensitive staged derivative belongs to a different owner");
        }
        if stage.path != ready {
            return super::native_error(format!("Staged {label} is no longer available"));
        }
        if !ready.is_file() {
            return super::native_error(format!("Staged {label} is no longer available"));
        }
        let mut plaintext = crate::security::OwnedPlaintext::process_private(
            stage.process_owner.clone(),
            key.to_string(),
            ready.clone(),
        )
        .map_err(super::NativeError::operation)?;
        plaintext
            .rename_process_private(claimed.clone())
            .map_err(|error| format!("Could not claim staged {label}: {error}"))?;
        // No fallible operation is allowed between rename and this assignment.
        // ProcessPrivateOwner covers both names across process death.
        stage.path = claimed.clone();
    }

    let metadata = match fs::metadata(&claimed) {
        Ok(metadata) => metadata,
        Err(error) => {
            cleanup_registered_stage_after_failure(key, &claimed);
            return super::native_error(format!("Could not inspect staged {label}: {error}"));
        }
    };
    if metadata.len() == 0 || metadata.len() > max_bytes as u64 {
        cleanup_registered_stage_after_failure(key, &claimed);
        return super::native_error(format!("Staged {label} has an invalid size"));
    }
    Ok(claimed)
}

fn discard_staged_image_file(
    process_owner: &super::temp_owner::ProcessPrivateOwner,
    token: &str,
    extensions: &[&str],
    label: &str,
) -> super::NativeResult<()> {
    if !valid_image_stage_token(token) {
        return super::native_error(format!("Invalid staged {label} token"));
    }
    let mut first_error = None;
    for extension in extensions {
        for state in ["ready", "claimed"] {
            let path = process_owner
                .root()
                .join(format!("{token}.{state}.{extension}"));
            let mut plaintext = crate::security::OwnedPlaintext::process_private(
                process_owner.clone(),
                format!("{label}:{token}:{state}:{extension}"),
                path,
            )
            .map_err(super::NativeError::operation)?;
            match plaintext.remove_file() {
                Ok(()) => {}
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
    }
    match first_error {
        Some(error) => super::native_error(format!("Could not discard staged {label}: {error}")),
        None => Ok(()),
    }
}

/// Stage a rendered image edit outside the JS/Dioxus control bridge.
/// The returned token is intentionally small and opaque.
pub fn stage_image_edit(
    attachment_id: &str,
    bytes: &[u8],
    mime_type: &str,
) -> super::NativeResult<String> {
    validate_image_edit_stage_bytes(bytes, mime_type)?;
    if !valid_stage_attachment_id(attachment_id) {
        return super::native_error("Invalid attachment identity for sensitive staging");
    }
    let extension = image_edit_stage_extension(mime_type)?;
    let process_owner = image_edit_stage_process_owner()?;
    let root = process_owner.root().to_path_buf();
    super::private_temp::ensure_private_directory(&root)
        .map_err(|error| format!("Could not prepare image edit staging: {error}"))?;
    prune_abandoned_ready_image_stages(&process_owner)?;

    for _ in 0..8 {
        let token = image_stage_token();
        let path = root.join(format!("{token}.ready.{extension}"));
        let key = derivative_stage_key("image-edit", &token);
        if !reserve_derivative_stage(
            key.clone(),
            PlaintextDerivativeOwner::Attachment(attachment_id.to_string()),
            process_owner.clone(),
            path.clone(),
        ) {
            continue;
        }

        let mut plaintext = crate::security::OwnedPlaintext::process_private(
            process_owner.clone(),
            key.clone(),
            path.clone(),
        )
        .map_err(super::NativeError::operation)?;
        let mut file = match plaintext.create_file() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                cleanup_registered_stage_after_failure(&key, &path);
                continue;
            }
            Err(error) => {
                cleanup_registered_stage_after_failure(&key, &path);
                return super::native_error(format!(
                    "Could not create image edit staging file: {error}"
                ));
            }
        };
        if let Err(error) = file.write_all(bytes).and_then(|()| file.flush()) {
            cleanup_registered_stage_after_failure(&key, &path);
            return super::native_error(format!("Could not stage image edit staging: {error}"));
        }
        return Ok(token);
    }
    super::native_error("Could not allocate image edit staging file")
}

/// Atomically claim a staged edit so a token can be consumed at most once.
pub fn claim_staged_image_edit(
    token: &str,
    mime_type: &str,
    attachment_id: &str,
) -> super::NativeResult<PathBuf> {
    let key = derivative_stage_key("image-edit", token);
    let extension = image_edit_stage_extension(mime_type)?;
    let root = image_edit_stage_process_root()?;
    claim_registered_staged_image_file(
        &key,
        &PlaintextDerivativeOwner::Attachment(attachment_id.to_string()),
        &root,
        token,
        extension,
        IMAGE_EDIT_STAGE_MAX_BYTES,
        "image edit",
    )
}

pub fn discard_staged_image_edit(token: &str) -> super::NativeResult<()> {
    let process_owner = image_edit_stage_process_owner()?;
    let result = discard_staged_image_file(
        &process_owner,
        token,
        &["png", "jpg"],
        "image edit",
    );
    if result.is_ok() {
        unregister_derivative_stage(&derivative_stage_key("image-edit", token));
    }
    result
}

/// Stage a pasted, dropped, or replacement image as raw binary. The file name
/// and media type remain in the small control payload; only bytes use this path.
pub fn stage_image_import(
    note_id: &str,
    bytes: &[u8],
) -> super::NativeResult<String> {
    stage_image_import_reader(note_id, io::Cursor::new(bytes), bytes.len())
}

pub(crate) fn stage_image_import_reader(
    note_id: &str,
    mut reader: impl Read,
    byte_len: usize,
) -> super::NativeResult<String> {
    if !valid_stage_note_id(note_id) {
        return super::native_error("Invalid note identity for image import staging");
    }
    if byte_len == 0 {
        return super::native_error("image import staging is empty");
    }
    if byte_len > IMAGE_IMPORT_STAGE_MAX_BYTES {
        return super::native_error("image import staging is too large");
    }
    let process_owner = image_import_stage_process_owner()?;
    let root = process_owner.root().to_path_buf();
    super::private_temp::ensure_private_directory(&root)
        .map_err(|error| format!("Could not prepare image import staging: {error}"))?;
    prune_abandoned_ready_image_stages(&process_owner)?;

    for _ in 0..8 {
        let token = image_stage_token();
        let path = root.join(format!("{token}.ready.bin"));
        let key = derivative_stage_key("image-import", &token);
        if !reserve_derivative_stage(
            key.clone(),
            PlaintextDerivativeOwner::Note(note_id.to_string()),
            process_owner.clone(),
            path.clone(),
        ) {
            continue;
        }

        let mut plaintext = crate::security::OwnedPlaintext::process_private(
            process_owner.clone(),
            key.clone(),
            path.clone(),
        )
        .map_err(super::NativeError::operation)?;
        let mut file = match plaintext.create_file() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                cleanup_registered_stage_after_failure(&key, &path);
                continue;
            }
            Err(error) => {
                cleanup_registered_stage_after_failure(&key, &path);
                return super::native_error(format!(
                    "Could not create image import staging file: {error}"
                ));
            }
        };
        let mut limited = (&mut reader).take(byte_len as u64 + 1);
        let copied = match io::copy(&mut limited, &mut file).and_then(|copied| {
            file.flush()?;
            Ok(copied)
        }) {
            Ok(copied) => copied,
            Err(error) => {
                cleanup_registered_stage_after_failure(&key, &path);
                return super::native_error(format!("Could not stage image import staging: {error}"));
            }
        };
        if copied != byte_len as u64 {
            cleanup_registered_stage_after_failure(&key, &path);
            return super::native_error(format!(
                "Could not stage image import staging: expected {byte_len} bytes but received {copied}"
            ));
        }
        return Ok(token);
    }
    super::native_error("Could not allocate image import staging file")
}

pub fn claim_staged_image_import(token: &str, note_id: &str) -> super::NativeResult<PathBuf> {
    let key = derivative_stage_key("image-import", token);
    let root = image_import_stage_process_root()?;
    claim_registered_staged_image_file(
        &key,
        &PlaintextDerivativeOwner::Note(note_id.to_string()),
        &root,
        token,
        "bin",
        IMAGE_IMPORT_STAGE_MAX_BYTES,
        "image import",
    )
}

pub fn discard_staged_image_import(token: &str) -> super::NativeResult<()> {
    let process_owner = image_import_stage_process_owner()?;
    let result = discard_staged_image_file(
        &process_owner,
        token,
        &["bin"],
        "image import",
    );
    if result.is_ok() {
        unregister_derivative_stage(&derivative_stage_key("image-import", token));
    }
    result
}

fn pdf_export_root() -> super::NativeResult<PathBuf> {
    super::private_temp::rustnote_temp_directory("pdf-export")
        .map_err(|error| super::NativeError::operation(format!("Could not prepare PDF export staging: {error}")))
}

fn pdf_export_process_owner(
) -> Result<super::temp_owner::ProcessPrivateOwner, PdfExportStagingError> {
    let root = pdf_export_root()
        .map_err(|error| PdfExportStagingError::Prepare(io::Error::other(error.to_string())))?;
    super::temp_owner::process_session_owner(&root, "PDF export staging")
        .map_err(|error| PdfExportStagingError::Prepare(io::Error::other(error.to_string())))
}

fn pdf_export_process_root() -> Result<PathBuf, PdfExportStagingError> {
    Ok(pdf_export_process_owner()?.root().to_path_buf())
}

fn pdf_export_token() -> String {
    rand::random::<[u8; PDF_EXPORT_TOKEN_BYTES]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_pdf_export_token(token: &str) -> bool {
    token.len() == PDF_EXPORT_TOKEN_BYTES * 2 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn staged_pdf_export_path(token: &str) -> Result<PathBuf, PdfExportStagingError> {
    if !valid_pdf_export_token(token) {
        return Err(PdfExportStagingError::InvalidToken);
    }
    Ok(pdf_export_process_root()?.join(format!("{token}.pdf")))
}

pub fn stage_pdf_export(
    attachment_id: &str,
    bytes: &[u8],
) -> Result<String, PdfExportStagingError> {
    if !valid_stage_attachment_id(attachment_id) {
        return Err(PdfExportStagingError::InvalidToken);
    }
    if bytes.len() < 5 || !bytes.starts_with(b"%PDF-") {
        return Err(PdfExportStagingError::InvalidPdf);
    }
    let process_owner = pdf_export_process_owner()?;
    let root = process_owner.root().to_path_buf();
    super::private_temp::ensure_private_directory(&root)
        .map_err(PdfExportStagingError::Prepare)?;
    for _ in 0..8 {
        let token = pdf_export_token();
        let path = root.join(format!("{token}.pdf"));
        let key = derivative_stage_key("pdf-export", &token);
        if !reserve_derivative_stage(
            key.clone(),
            PlaintextDerivativeOwner::Attachment(attachment_id.to_string()),
            process_owner.clone(),
            path.clone(),
        ) {
            continue;
        }

        let mut plaintext = crate::security::OwnedPlaintext::process_private(
            process_owner.clone(),
            key.clone(),
            path.clone(),
        )
        .map_err(|message| PdfExportStagingError::Prepare(io::Error::other(message)))?;
        let mut file = match plaintext.create_file() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                cleanup_registered_stage_after_failure(&key, &path);
                continue;
            }
            Err(error) => {
                cleanup_registered_stage_after_failure(&key, &path);
                return Err(PdfExportStagingError::Create(error));
            }
        };
        if let Err(error) = file
            .write_all(bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
        {
            cleanup_registered_stage_after_failure(&key, &path);
            return Err(PdfExportStagingError::Stage(error));
        }
        return Ok(token);
    }
    Err(PdfExportStagingError::AllocationFailed)
}

pub fn discard_staged_pdf_export(token: &str) -> Result<(), PdfExportStagingError> {
    let path = staged_pdf_export_path(token)?;
    let key = derivative_stage_key("pdf-export", token);
    match remove_registered_stage_plaintext(&key, &path) {
        Ok(()) => {
            unregister_derivative_stage(&key);
            Ok(())
        }
        Err(error) => Err(PdfExportStagingError::Discard(error)),
    }
}

pub fn save_staged_pdf_export(
    token: &str,
    attachment_id: &str,
    destination: &Path,
) -> Result<(), PdfExportStagingError> {
    let key = derivative_stage_key("pdf-export", token);
    let source = ensure_derivative_stage_attachment_identity(&key, attachment_id)
        .map_err(|error| PdfExportStagingError::Prepare(io::Error::other(error.to_string())))?;
    if !source.is_file() {
        return Err(PdfExportStagingError::Unavailable);
    }
    copy_file_atomically(&source, destination, "annotated PDF")
        .map_err(PdfExportStagingError::Save)?;
    remove_registered_stage_plaintext(&key, &source).map_err(PdfExportStagingError::Discard)?;
    unregister_derivative_stage(&key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn strict_plaintext_cleanup_does_not_accept_ambiguous_absence() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "rustnote-native-staging-ambiguous-{}",
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).expect("test root");
        let loop_parent = root.join("loop");
        symlink("loop", &loop_parent).expect("symlink loop");
        let path = loop_parent.join("stage.bin");

        assert!(
            remove_plaintext_file_strict(&path).is_err(),
            "ambiguous metadata must fail strict cleanup"
        );
        fs::remove_file(&loop_parent).expect("remove symlink loop");
        remove_plaintext_file_strict(&path).expect("NotFound is conclusive absence");
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn image_reader_stages_exact_declared_bytes() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-image-stage-test-{}",
            rand::random::<u64>()
        ));
        let bytes = b"image-bytes";
        let token = stage_image_reader(
            &root,
            io::Cursor::new(bytes),
            bytes.len(),
            IMAGE_IMPORT_STAGE_MAX_BYTES,
            "bin",
            "image import staging",
        )
        .expect("exact image body should stage");
        let path = root.join(format!("{token}.ready.bin"));
        assert_eq!(
            fs::read(&path).expect("staged image should be readable"),
            bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_reader_rejects_short_body_and_removes_partial_stage() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-image-stage-short-test-{}",
            rand::random::<u64>()
        ));
        let error = stage_image_reader(
            &root,
            io::Cursor::new(b"short"),
            6,
            IMAGE_IMPORT_STAGE_MAX_BYTES,
            "bin",
            "image import staging",
        )
        .expect_err("short image body must fail");
        assert!(error.to_string().contains("expected 6 bytes but received 5"));
        let staged_files = fs::read_dir(&root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0);
        assert_eq!(staged_files, 0, "partial image stage must be removed");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_claim_updates_registered_ownership_to_claimed_path() {
        let note_id = "note_image_claim_owner";
        let token = stage_image_import(note_id, b"image-bytes").expect("stage image import");
        let claimed = claim_staged_image_import(&token, note_id).expect("claim image import");
        let key = derivative_stage_key("image-import", &token);
        let registered = lock_derivative_stage_registry()
            .get(&key)
            .expect("claimed stage remains registered")
            .path
            .clone();
        assert_eq!(registered, claimed);
        assert!(claimed.is_file());
        discard_staged_image_import(&token).expect("discard claimed image");
        assert!(!claimed.exists());
    }

    #[test]
    fn derivative_stage_identity_is_enforced_and_targeted_cleanup_is_scoped() {
        let first = stage_pdf_export("att_stage_first", b"%PDF-1.4\nfirst\n%%EOF\n")
            .expect("first stage");
        let second = stage_pdf_export("att_stage_second", b"%PDF-1.4\nsecond\n%%EOF\n")
            .expect("second stage");
        let mismatch = save_staged_pdf_export(
            &first,
            "att_stage_second",
            &std::env::temp_dir().join(format!("rustnote-stage-mismatch-{}", rand::random::<u64>())),
        )
        .expect_err("cross-attachment stage claim must fail");
        assert!(mismatch.to_string().contains("different attachment"));

        let targets = HashSet::from(["att_stage_first".to_string()]);
        assert_eq!(
            discard_staged_derivatives_for_attachments(&targets).expect("targeted cleanup"),
            1
        );
        assert!(
            ensure_derivative_stage_attachment_identity(
                &derivative_stage_key("pdf-export", &first),
                "att_stage_first",
            )
            .is_err(),
            "targeted attachment stage must be gone",
        );
        assert!(
            ensure_derivative_stage_attachment_identity(
                &derivative_stage_key("pdf-export", &second),
                "att_stage_second",
            )
            .is_ok(),
            "unrelated attachment stage must remain",
        );
        discard_staged_pdf_export(&second).expect("discard second");
    }

    #[test]
    fn pdf_export_staging_errors_preserve_categories_and_messages() {
        let invalid_pdf = stage_pdf_export("att_pdf_export_test", b"not-a-pdf").unwrap_err();
        assert_eq!(invalid_pdf.to_string(), "Exported data is not a PDF");
        assert!(matches!(invalid_pdf, PdfExportStagingError::InvalidPdf));

        let invalid_token = staged_pdf_export_path("../escape").unwrap_err();
        assert_eq!(invalid_token.to_string(), "Invalid staged PDF export token");
        assert!(matches!(invalid_token, PdfExportStagingError::InvalidToken));
    }
}
