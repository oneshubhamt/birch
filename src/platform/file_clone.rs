use std::{io, path::Path};

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android",
    target_os = "windows"
))]
use std::fs;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(super) fn try_clone_file(source: &Path, destination: &Path) -> io::Result<bool> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    unsafe extern "C" {
        fn clonefile(src: *const std::ffi::c_char, dst: *const std::ffi::c_char, flags: u32)
        -> i32;
    }

    let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path contains a NUL byte",
        )
    })?;
    let destination_bytes = destination.as_os_str().as_bytes();
    let destination_c = CString::new(destination_bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination path contains a NUL byte",
        )
    })?;

    // SAFETY: both pointers reference live, NUL-terminated byte strings for the
    // duration of the call. `clonefile` does not retain either pointer.
    if unsafe { clonefile(source.as_ptr(), destination_c.as_ptr(), 0) } == 0 {
        return Ok(true);
    }

    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::AlreadyExists {
        return Err(error);
    }

    // Unsupported filesystems, cross-volume sources and ordinary clone refusals
    // are capability misses, not import failures. Never leave a partial clone.
    let _ = fs::remove_file(destination);
    Ok(false)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn try_clone_file(source: &Path, destination: &Path) -> io::Result<bool> {
    use std::{fs::OpenOptions, os::fd::AsRawFd};

    // Linux UAPI: _IOW(0x94, 9, int). Android uses the same Linux-kernel API.
    const FICLONE: std::ffi::c_ulong = 0x4004_9409;

    unsafe extern "C" {
        fn ioctl(fd: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
    }

    let input = fs::File::open(source)?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;

    // SAFETY: FICLONE expects the destination fd followed by the readable source
    // fd value. Both files remain open for the complete ioctl call.
    if unsafe { ioctl(output.as_raw_fd(), FICLONE, input.as_raw_fd()) } == 0 {
        return Ok(true);
    }

    drop(output);
    let _ = fs::remove_file(destination);
    Ok(false)
}

#[cfg(target_os = "windows")]
pub(super) fn try_clone_file(source: &Path, destination: &Path) -> io::Result<bool> {
    use std::{
        fs::OpenOptions,
        io::{Read, Seek, SeekFrom, Write},
        os::windows::io::AsRawHandle,
    };

    // ReFS block cloning requires cluster-aligned ranges and each clone request
    // to remain below 4 GiB. 64 KiB is valid for both documented ReFS cluster
    // sizes; only the final sub-64-KiB tail needs a physical copy.
    const CLONE_ALIGNMENT: u64 = 64 * 1024;
    const MAX_CLONE_CHUNK: u64 = 2 * 1024 * 1024 * 1024;
    const FSCTL_DUPLICATE_EXTENTS_TO_FILE: u32 = 0x0009_8344;

    type Handle = *mut std::ffi::c_void;

    #[repr(C)]
    struct DuplicateExtentsData {
        file_handle: Handle,
        source_file_offset: i64,
        target_file_offset: i64,
        byte_count: i64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: Handle,
            control_code: u32,
            input_buffer: *mut std::ffi::c_void,
            input_buffer_size: u32,
            output_buffer: *mut std::ffi::c_void,
            output_buffer_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut std::ffi::c_void,
        ) -> i32;
    }

    let source_len = fs::metadata(source)?.len();
    if source_len < CLONE_ALIGNMENT || source_len > i64::MAX as u64 {
        return Ok(false);
    }

    let mut destination_created = false;
    let clone_result = (|| -> io::Result<bool> {
        let mut input = fs::File::open(source)?;
        let mut output = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(destination)?;
        destination_created = true;
        output.set_len(source_len)?;

        let cloned_len = source_len - (source_len % CLONE_ALIGNMENT);
        let mut offset = 0_u64;
        while offset < cloned_len {
            let byte_count = (cloned_len - offset).min(MAX_CLONE_CHUNK);
            let mut request = DuplicateExtentsData {
                file_handle: input.as_raw_handle(),
                source_file_offset: offset as i64,
                target_file_offset: offset as i64,
                byte_count: byte_count as i64,
            };
            let mut returned = 0_u32;
            // SAFETY: both handles refer to live regular files. The repr(C)
            // request outlives this synchronous DeviceIoControl invocation.
            let succeeded = unsafe {
                DeviceIoControl(
                    output.as_raw_handle(),
                    FSCTL_DUPLICATE_EXTENTS_TO_FILE,
                    (&mut request as *mut DuplicateExtentsData).cast(),
                    std::mem::size_of::<DuplicateExtentsData>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            } != 0;
            if !succeeded {
                return Ok(false);
            }
            offset += byte_count;
        }

        if cloned_len < source_len {
            input.seek(SeekFrom::Start(cloned_len))?;
            output.seek(SeekFrom::Start(cloned_len))?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                output.write_all(&buffer[..read])?;
            }
        }
        output.flush()?;
        Ok(true)
    })();

    match clone_result {
        Ok(true) => Ok(true),
        Ok(false) => {
            if destination_created {
                let _ = fs::remove_file(destination);
            }
            Ok(false)
        }
        Err(error) => {
            if destination_created {
                let _ = fs::remove_file(destination);
            }
            Err(error)
        }
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android",
    target_os = "windows"
)))]
pub(super) fn try_clone_file(_source: &Path, _destination: &Path) -> io::Result<bool> {
    Ok(false)
}
