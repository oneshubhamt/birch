use std::{fs, path::Path};

use super::{existing_path, output_token};

const IMAGE_RENDITION_OWNER_MARKER: &str = ".v2.plaintext-owner";
const IMAGE_RENDITION_OWNER_IDENTITY: &str = "image-renditions-v2";

/// Materialize a lossless WebView-compatible full-resolution PNG rendition
/// without modifying the canonical attachment. This variant is for ephemeral
/// protected previews whose enclosing Vault/session owner already owns the
/// complete temporary namespace.
pub fn create_compatible_image_rendition(
    source: &Path,
    destination: &Path,
) -> super::NativeResult<()> {
    create_image_rendition_platform(source, destination, None, None)
}

/// Persistent ordinary-note rendition. The durable namespace marker is
/// established outside `cache_root` before a temporary or final rendition can
/// be created, so crash residue always retains cleanup authority.
pub fn create_persistent_compatible_image_rendition(
    source: &Path,
    destination: &Path,
    cache_root: &Path,
) -> super::NativeResult<()> {
    let owner = persistent_rendition_owner(cache_root).map_err(|error| {
        super::NativeError::operation(format!(
            "Could not establish image rendition plaintext authority: {error}"
        ))
    })?;
    create_image_rendition_platform(source, destination, None, Some(&owner))
}

/// Materialize the single screen-sized PNG rendition used for large static
/// raster images. The canonical source remains untouched. This variant is for
/// ephemeral protected previews already covered by a Vault/session owner.
pub fn create_screen_image_rendition(
    source: &Path,
    destination: &Path,
    max_long_edge: u32,
) -> super::NativeResult<()> {
    if max_long_edge == 0 {
        return super::native_error("Screen image rendition size must be positive");
    }
    create_image_rendition_platform(source, destination, Some(max_long_edge), None)
}

pub fn discard_persistent_image_rendition(
    cache_root: &Path,
    path: &Path,
) -> super::NativeResult<()> {
    let owner = persistent_rendition_owner(cache_root).map_err(|error| {
        super::NativeError::operation(format!(
            "Could not establish image rendition plaintext authority: {error}"
        ))
    })?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return super::native_error(format!(
                "Could not inspect image rendition cache: {error}"
            ));
        }
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return super::native_error("Image rendition cache path is not a regular file");
        }
        Ok(_) => {}
    }
    let mut owned = owner.adopt_existing(path.to_path_buf()).map_err(|error| {
        super::NativeError::operation(format!(
            "Could not resume image rendition plaintext ownership: {error}"
        ))
    })?;
    owned.remove_file().map_err(|error| {
        super::NativeError::operation(format!(
            "Could not discard image rendition cache: {error}"
        ))
    })
}

pub fn create_persistent_screen_image_rendition(
    source: &Path,
    destination: &Path,
    cache_root: &Path,
    max_long_edge: u32,
) -> super::NativeResult<()> {
    if max_long_edge == 0 {
        return super::native_error("Screen image rendition size must be positive");
    }
    let owner = persistent_rendition_owner(cache_root).map_err(|error| {
        super::NativeError::operation(format!(
            "Could not establish image rendition plaintext authority: {error}"
        ))
    })?;
    create_image_rendition_platform(
        source,
        destination,
        Some(max_long_edge),
        Some(&owner),
    )
}

pub(crate) fn persistent_rendition_owner(
    cache_root: &Path,
) -> std::io::Result<crate::security::DurablePlaintextNamespace> {
    let marker_parent = cache_root.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "image rendition cache has no parent",
        )
    })?;
    crate::security::DurablePlaintextNamespace::establish(
        cache_root.to_path_buf(),
        marker_parent.join(IMAGE_RENDITION_OWNER_MARKER),
        IMAGE_RENDITION_OWNER_IDENTITY,
    )
}

fn create_image_rendition_platform(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
    owner: Option<&crate::security::DurablePlaintextNamespace>,
) -> super::NativeResult<()> {
    let source = existing_path(source)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "Invalid image rendition cache destination".to_string())?;
    if let Some(owner) = owner
        && (!destination.starts_with(owner.root()) || destination == owner.root())
    {
        return super::native_error("Image rendition destination escaped its plaintext owner");
    }
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not prepare image rendition cache: {error}"))?;

    let temporary = parent.join(format!(
        ".rustnote-image-{}.{}.png",
        std::process::id(),
        output_token(),
    ));

    if let Some(owner) = owner {
        let mut temporary_owner = owner.reserve(temporary.clone()).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not reserve image rendition plaintext: {error}"
            ))
        })?;
        if let Err(error) = crate::platform::render_image_to_png(&source, &temporary, max_long_edge) {
            let _ = temporary_owner.remove_file();
            return Err(error.into());
        }
        if let Err(error) = temporary_owner.verify_materialized() {
            let _ = temporary_owner.remove_file();
            return super::native_error(format!(
                "Could not secure image rendition output: {error}"
            ));
        }
        validate_rendition_output(&temporary, &mut temporary_owner)?;

        if is_regular_file(destination)? {
            temporary_owner.remove_file().map_err(|error| {
                super::NativeError::operation(format!(
                    "Could not discard unused image rendition: {error}"
                ))
            })?;
            return Ok(());
        }
        match temporary_owner.rename_durable_namespace(destination.to_path_buf()) {
            Ok(()) => Ok(()),
            Err(_) if is_regular_file(destination)? => {
                temporary_owner.remove_file().map_err(|error| {
                    super::NativeError::operation(format!(
                        "Could not discard redundant image rendition: {error}"
                    ))
                })?;
                Ok(())
            }
            Err(error) => {
                let _ = temporary_owner.remove_file();
                super::native_error(format!("Could not publish image rendition: {error}"))
            }
        }
    } else {
        // Protected ephemeral output is already covered by its enclosing
        // Vault/session plaintext owner. Keep this low-level converter free of
        // a second ownership system for that lifetime.
        if let Err(error) = crate::platform::render_image_to_png(&source, &temporary, max_long_edge) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        if let Err(error) = super::private_temp::harden_existing_file(&temporary) {
            let _ = fs::remove_file(&temporary);
            return super::native_error(format!(
                "Could not secure image rendition output: {error}"
            ));
        }
        let metadata = fs::metadata(&temporary)
            .map_err(|error| format!("Image rendition output is missing: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 {
            let _ = fs::remove_file(&temporary);
            return super::native_error("The platform image converter produced an empty image");
        }
        if is_regular_file(destination)? {
            let _ = fs::remove_file(&temporary);
            return Ok(());
        }
        match fs::rename(&temporary, destination) {
            Ok(()) => Ok(()),
            Err(_) if is_regular_file(destination)? => {
                let _ = fs::remove_file(&temporary);
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                super::native_error(format!("Could not publish image rendition: {error}"))
            }
        }
    }
}

fn is_regular_file(path: &Path) -> super::NativeResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(!metadata.file_type().is_symlink() && metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => super::native_error(format!(
            "Could not inspect image rendition destination: {error}"
        )),
    }
}

fn validate_rendition_output(
    path: &Path,
    owner: &mut crate::security::OwnedPlaintext,
) -> super::NativeResult<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Image rendition output is missing: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        let _ = owner.remove_file();
        return super::native_error("The platform image converter produced an empty image");
    }
    Ok(())
}
