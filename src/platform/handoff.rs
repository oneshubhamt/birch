use std::path::Path;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
use std::process::Command;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
fn launch(mut command: Command) -> super::PlatformResult<()> {
    let status = command
        .status()
        .map_err(|error| format!("Could not launch the system file handler: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(super::PlatformError::operation(format!(
            "The system file handler exited with status {status}"
        )))
    }
}

#[cfg(target_os = "macos")]
pub(super) fn open_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let mut command = Command::new("open");
    command.arg(path);
    launch(command)
}

#[cfg(target_os = "macos")]
pub(super) fn reveal_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    launch(command)
}

#[cfg(target_os = "windows")]
pub(super) fn open_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_command: i32,
        ) -> *mut std::ffi::c_void;
    }

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `path` is a live NUL-terminated UTF-16 buffer for this synchronous
    // call. Null optional arguments request the user's normal file association.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(super::PlatformError::operation(format!(
            "The Windows file handler could not open the attachment (ShellExecuteW code {result})"
        )))
    }
}

#[cfg(target_os = "windows")]
pub(super) fn reveal_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let mut argument = std::ffi::OsString::from("/select,");
    argument.push(path);
    let mut command = Command::new("explorer.exe");
    command.arg(argument);
    launch(command)
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "ios"))
))]
pub(super) fn open_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    launch(command)
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "ios"))
))]
pub(super) fn reveal_external_file(
    path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    launch(command)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn open_external_file(
    path: &Path,
    file_name: Option<&str>,
    mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let adapter = super::mobile::adapter()
        .ok_or_else(|| "Rustnote's mobile platform adapter has not been registered".to_string())?;
    adapter.open_file(path, file_name.unwrap_or("attachment"), mime_type)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn reveal_external_file(
    path: &Path,
    file_name: Option<&str>,
    mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    let adapter = super::mobile::adapter()
        .ok_or_else(|| "Rustnote's mobile platform adapter has not been registered".to_string())?;
    adapter.reveal_or_share_file(path, file_name.unwrap_or("attachment"), mime_type)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn open_external_file(
    _path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    Err(super::PlatformError::Unsupported(
        "External file opening is not supported on this platform",
    ))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn reveal_external_file(
    _path: &Path,
    _file_name: Option<&str>,
    _mime_type: Option<&str>,
) -> super::PlatformResult<()> {
    Err(super::PlatformError::Unsupported(
        "File reveal/share is not supported on this platform",
    ))
}
