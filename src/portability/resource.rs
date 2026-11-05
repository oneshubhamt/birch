use std::{ffi::OsStr, path::{Component, Path, PathBuf}};

/// Parse a user/file supplied local portability reference without touching the filesystem.
///
/// This is the single syntax-level authority shared by Markdown, HTML, and storage. Filesystem
/// containment, symlink policy, and size/hash verification remain storage-owned because they
/// require the selected import root and OS metadata.
pub(crate) fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') || value.contains('\\') {
        return None;
    }
    if value.starts_with('#') || is_remote_or_embedded(value) {
        return None;
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!result.as_os_str().is_empty()).then_some(result)
}

pub(crate) fn is_asset_namespace_name(value: &OsStr) -> bool {
    value
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".assets")
}

pub(crate) fn is_asset_namespace_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(part) => is_asset_namespace_name(part),
        _ => false,
    })
}

pub(crate) fn is_remote_or_embedded(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.contains("://") || lower.starts_with("data:") || lower.starts_with("mailto:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_resource_syntax_is_narrow_and_assets_are_detected_centrally() {
        assert_eq!(safe_relative_path("Topic.assets/map.png"), Some(PathBuf::from("Topic.assets/map.png")));
        assert_eq!(safe_relative_path("../secret.txt"), None);
        assert_eq!(safe_relative_path("https://example.com/a.png"), None);
        assert!(is_asset_namespace_path(Path::new("Topic.assets/reference.md")));
        assert!(!is_asset_namespace_path(Path::new("notes/reference.md")));
    }
}
