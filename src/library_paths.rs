//! Canonical construction of library-scoped derived filesystem paths.
//!
//! Core storage paths (database, attachments, backups) remain owned by `Storage`.
//! This module owns only derived cache/handoff locations so their names and layout
//! are not reconstructed independently by application, protocol, or storage code.

use std::path::{Path, PathBuf};

const CACHE_DIRECTORY_NAME: &str = "cache";
const MEDIA_COMPAT_DIRECTORY_NAME: &str = "media-compat";
const ATTACHMENT_HANDOFF_DIRECTORY_NAME: &str = "attachment-handoff";
pub(crate) const IMAGE_RENDITION_RELATIVE_ROOT: &str = "cache/image-renditions/v2";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPaths {
    data_directory: PathBuf,
}

impl LibraryPaths {
    pub(crate) fn from_data_directory(data_directory: &Path) -> Self {
        Self {
            data_directory: data_directory.to_path_buf(),
        }
    }

    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    pub fn media_compat_directory(&self) -> PathBuf {
        media_compat_directory(&self.data_directory)
    }

    pub fn attachment_handoff_directory(&self) -> PathBuf {
        attachment_handoff_directory(&self.data_directory)
    }

    pub fn image_rendition_directory(&self) -> PathBuf {
        image_rendition_directory(&self.data_directory)
    }
}

pub(crate) fn media_compat_directory(data_directory: &Path) -> PathBuf {
    data_directory.join(MEDIA_COMPAT_DIRECTORY_NAME)
}

pub(crate) fn attachment_handoff_directory(data_directory: &Path) -> PathBuf {
    data_directory
        .join(CACHE_DIRECTORY_NAME)
        .join(ATTACHMENT_HANDOFF_DIRECTORY_NAME)
}

pub(crate) fn image_rendition_directory(data_directory: &Path) -> PathBuf {
    data_directory.join(IMAGE_RENDITION_RELATIVE_ROOT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_library_paths_share_one_canonical_layout() {
        let root = Path::new("/tmp/rustnote-library");
        let paths = LibraryPaths::from_data_directory(root);
        assert_eq!(paths.media_compat_directory(), root.join("media-compat"));
        assert_eq!(
            paths.attachment_handoff_directory(),
            root.join("cache").join("attachment-handoff")
        );
        assert_eq!(
            paths.image_rendition_directory(),
            root.join("cache").join("image-renditions").join("v2")
        );
    }
}
