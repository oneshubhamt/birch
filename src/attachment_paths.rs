//! Canonical attachment-blob path helpers shared by storage and preview serving.

/// Returns true when `value` is Rustnote's canonical lowercase SHA-256 identity.
pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// Builds Rustnote's canonical sharded attachment path for a SHA-256 digest.
///
/// The returned form is `blobs/<first-two-hex>/<lowercase-sha256>`.
pub fn canonical_blob_relative_path(sha256: &str) -> Option<String> {
    if !is_valid_sha256(sha256) {
        return None;
    }
    Some(format!("blobs/{}/{}", &sha256[..2], sha256))
}

/// Builds the disposable image-rendition cache directory for a canonical
/// attachment SHA-256. Cached renditions are implementation data, not attachments.
pub fn image_rendition_cache_relative_directory(sha256: &str) -> Option<String> {
    if !is_valid_sha256(sha256) {
        return None;
    }
    Some(format!(
        "{}/{sha256}",
        crate::library_paths::IMAGE_RENDITION_RELATIVE_ROOT
    ))
}

/// Builds the full-resolution WebView-compatible cache path for an image whose
/// canonical source is not directly decodable by the WebView.
pub fn compatible_image_cache_relative_path(sha256: &str) -> Option<String> {
    image_rendition_cache_relative_directory(sha256)
        .map(|directory| format!("{directory}/compatible-full.png"))
}

/// Builds the single screen-sized rendition used for large static raster images.
pub fn screen_image_cache_relative_path(sha256: &str) -> Option<String> {
    image_rendition_cache_relative_directory(sha256)
        .map(|directory| format!("{directory}/screen.png"))
}
