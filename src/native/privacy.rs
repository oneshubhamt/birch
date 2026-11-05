//! Privacy-focused cleanup for Rustnote-owned disposable plaintext.
//!
//! This module is an application-level residue-cleanup aid, not a physical
//! secure-erase primitive. SSD wear levelling, copy-on-write filesystems and
//! snapshots can retain historical blocks; whole-device encryption remains the
//! physical-media boundary.

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Full-file overwrite is deliberately bounded. Rewriting multi-gigabyte media
/// would impose a large cost without providing a truthful SSD/COW erasure
/// guarantee. Small/medium Rustnote-owned plaintext is overwritten before
/// unlinking; larger files are unlinked.
const OVERWRITE_MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn remove_file(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        fs::remove_file(path)?;
        return Ok(true);
    }
    if !file_type.is_file() {
        return Ok(false);
    }

    let may_overwrite = metadata.len() <= OVERWRITE_MAX_BYTES && !has_multiple_hard_links(&metadata);
    if may_overwrite
        && metadata.len() > 0
        && let Ok(mut file) = fs::OpenOptions::new().write(true).open(path)
    {
        let zeros = [0_u8; 64 * 1024];
        let mut remaining = metadata.len();
        while remaining > 0 {
            let count = usize::try_from(remaining.min(zeros.len() as u64)).unwrap_or(zeros.len());
            if file.write_all(&zeros[..count]).is_err() {
                break;
            }
            remaining = remaining.saturating_sub(count as u64);
        }
        let _ = file.flush();
        let _ = file.sync_all();
    }
    fs::remove_file(path)?;
    Ok(true)
}

#[cfg(unix)]
fn has_multiple_hard_links(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() > 1
}

#[cfg(not(unix))]
fn has_multiple_hard_links(_metadata: &fs::Metadata) -> bool {
    // Stable Rust does not expose a portable hard-link count on every target.
    // Be conservative: skip overwrite when we cannot prove this path is the only
    // link, then unlink only this directory entry.
    true
}

pub(crate) fn remove_tree(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        let _ = remove_file(path)?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            remove_tree(&entry.path())?;
        } else {
            let _ = remove_file(&entry.path())?;
        }
    }
    fs::remove_dir(path)
}
