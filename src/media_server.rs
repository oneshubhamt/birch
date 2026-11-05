use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use crate::{
    attachment_media::{self, AttachmentMediaKind},
    library::LibraryRuntime,
    storage::AttachmentTransportResource,
};

const TOKEN_BYTES: usize = 32;
const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;
const STREAM_BUFFER_BYTES: usize = 256 * 1024;
const MAX_ACTIVE_CONNECTIONS: usize = 16;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
struct MediaServerState {
    port: u16,
    token: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MediaRoute {
    Preview,
    Compatible(rustnote_media_compat::CompatibilityProfile),
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    range_header: Option<String>,
    content_length: Option<usize>,
    note_id_header: Option<String>,
    buffered_body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageImportStageRoute<'a> {
    Collection,
    Token(&'a str),
}

static MEDIA_SERVER: OnceLock<MediaServerState> = OnceLock::new();
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

pub fn start(libraries: LibraryRuntime) -> io::Result<()> {
    if MEDIA_SERVER.get().is_some() {
        return Ok(());
    }

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = token_hex(rand::random::<[u8; TOKEN_BYTES]>());
    let server_token = token.clone();

    let _ = thread::Builder::new()
        .name("rustnote-media-http".to_string())
        .spawn(move || serve(listener, server_token, libraries))?;

    MEDIA_SERVER
        .set(MediaServerState { port, token })
        .map_err(|_| io::Error::new(io::ErrorKind::AlreadyExists, "media server already started"))
}

pub fn preview_url(
    attachment_id: &str,
    file_name: &str,
    mime_type: Option<&str>,
) -> Option<String> {
    if !matches!(
        attachment_media::kind(file_name, mime_type),
        AttachmentMediaKind::Audio | AttachmentMediaKind::Video
    ) {
        return None;
    }
    media_url(
        attachment_id,
        "preview",
        attachment_media::preview_extension(file_name, mime_type),
    )
}

pub fn compatible_preview_url(
    attachment_id: &str,
    profile: rustnote_media_compat::CompatibilityProfile,
) -> Option<String> {
    media_url(
        attachment_id,
        &format!("compatible-{}", profile.as_str()),
        Some(profile.extension()),
    )
}

/// Authenticated loopback endpoint for browser-originated image bytes. The
/// editor sends only the resulting opaque stage token over the control bridge.
pub fn image_import_stage_url() -> Option<String> {
    let state = MEDIA_SERVER.get()?;
    Some(format!(
        "http://127.0.0.1:{}/stage/{}/image-import",
        state.port, state.token
    ))
}

fn media_url(attachment_id: &str, resource: &str, extension: Option<&str>) -> Option<String> {
    let state = MEDIA_SERVER.get()?;
    let suffix = extension
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    Some(format!(
        "http://127.0.0.1:{}/media/{}/{}/{}{}",
        state.port, state.token, attachment_id, resource, suffix
    ))
}

fn serve(listener: TcpListener, token: String, libraries: LibraryRuntime) {
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else {
            continue;
        };
        if ACTIVE_CONNECTIONS.fetch_add(1, Ordering::AcqRel) >= MAX_ACTIVE_CONNECTIONS {
            ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
            let _ = write_error_response(&mut stream, 503, "Service Unavailable", "Media server busy");
            continue;
        }

        let token = token.clone();
        let libraries = libraries.clone();
        let spawn_result = thread::Builder::new()
            .name("rustnote-media-stream".to_string())
            .spawn(move || {
                let _guard = ActiveConnectionGuard;
                if let Err(error) = handle_connection(&mut stream, &token, &libraries) {
                    #[cfg(debug_assertions)]
                    eprintln!("[rustnote][media-http] connection ended: {error}");
                    #[cfg(not(debug_assertions))]
                    let _ = error;
                }
            });
        if spawn_result.is_err() {
            ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

struct ActiveConnectionGuard;

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct LeasedMediaFile {
    file: File,
    file_len: u64,
    _authorization: crate::security::PlaintextOperationPermit,
}

fn open_leased_media_file(
    resource: &AttachmentTransportResource,
    media_path: &std::path::Path,
) -> io::Result<LeasedMediaFile> {
    // Clone the already-linearized compound authorization before opening the OS
    // descriptor. The descriptor therefore owns both derivative and Vault
    // lifetime authority until streaming completes, independent of caller scope.
    let authorization = resource.authorization.clone();
    let file = File::open(media_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Media file missing"));
    }
    Ok(LeasedMediaFile {
        file,
        file_len: metadata.len(),
        _authorization: authorization,
    })
}

fn handle_connection(
    stream: &mut TcpStream,
    token: &str,
    libraries: &LibraryRuntime,
) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let _ = stream.set_nodelay(true);

    let request = match read_request(stream)? {
        Some(request) => request,
        None => return write_error_response(stream, 400, "Bad Request", "Invalid local request"),
    };

    if let Some(route) = image_import_stage_route(&request.target, token) {
        return handle_image_import_stage_request(stream, &request, route, libraries);
    }

    let method = request.method.as_str();
    if method != "GET" && method != "HEAD" {
        return write_error_response(stream, 405, "Method Not Allowed", "Method not allowed");
    }

    let Some((attachment_id, route)) = media_route(&request.target, token) else {
        return write_error_response(stream, 404, "Not Found", "Media not found");
    };
    let range_header = request.range_header.as_deref();
    let Some(library) = libraries.current() else {
        return write_error_response(
            stream,
            503,
            "Service Unavailable",
            "Rustnote library is not ready",
        );
    };
    let transport = library.transport_store();
    let resource = match transport.attachment_resource(attachment_id) {
        Ok(Some(resource)) => resource,
        Ok(None) => return write_error_response(stream, 404, "Not Found", "Media not found"),
        Err(_) => {
            return write_error_response(
                stream,
                500,
                "Internal Server Error",
                "Media metadata unavailable",
            );
        }
    };
    if !matches!(
        attachment_media::kind(&resource.file_name, resource.mime_type.as_deref()),
        AttachmentMediaKind::Audio | AttachmentMediaKind::Video
    ) {
        return write_error_response(stream, 415, "Unsupported Media Type", "Not a media attachment");
    }

    let (media_path, content_type, cache_use_guard) = match route {
        MediaRoute::Preview => (
            resource.path.clone(),
            safe_media_content_type(&resource.file_name, resource.mime_type.as_deref()),
            None,
        ),
        MediaRoute::Compatible(profile) => match crate::media_compat::acquire_cached_resource(
            &library.paths().media_compat_directory(),
            &resource,
            profile,
        ) {
            Ok(Some((prepared, guard))) => {
                (prepared.path, prepared.content_type.to_string(), Some(guard))
            }
            Ok(None) => {
                return write_error_response(
                    stream,
                    404,
                    "Not Found",
                    "Compatibility rendition not ready",
                );
            }
            Err(error) => {
                eprintln!("[rustnote][media-compat] compatibility cache unavailable: {error}");
                return write_error_response(
                    stream,
                    500,
                    "Internal Server Error",
                    "Compatibility cache unavailable",
                );
            }
        },
    };
    let _cache_use_guard = cache_use_guard;
    serve_media_path(
        stream,
        method,
        range_header,
        &resource,
        &media_path,
        &content_type,
    )
}

fn serve_media_path(
    stream: &mut TcpStream,
    method: &str,
    range_header: Option<&str>,
    resource: &AttachmentTransportResource,
    media_path: &std::path::Path,
    content_type: &str,
) -> io::Result<()> {
    let mut leased_file = match open_leased_media_file(resource, media_path) {
        Ok(file) => file,
        Err(_) => return write_error_response(stream, 404, "Not Found", "Media file missing"),
    };
    let file_len = leased_file.file_len;

    let requested_range = match range_header {
        Some(value) => match parse_single_range(value, file_len) {
            Some(range) => Some(range),
            None => return write_range_not_satisfiable(stream, file_len),
        },
        None => None,
    };

    let (status, reason, start, length, content_range) = if let Some((start, end)) = requested_range {
        let length = end.saturating_sub(start).saturating_add(1);
        (
            206,
            "Partial Content",
            start,
            length,
            Some(format!("bytes {start}-{end}/{file_len}")),
        )
    } else {
        (200, "OK", 0, file_len, None)
    };

    write_media_headers(
        stream,
        status,
        reason,
        content_type,
        length,
        content_range.as_deref(),
    )?;
    if method == "HEAD" || length == 0 {
        return Ok(());
    }

    leased_file.file.seek(SeekFrom::Start(start))?;
    stream_file_range(&mut leased_file.file, stream, length)
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut bytes = Vec::with_capacity(2048);
    let mut buffer = [0_u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            if index > MAX_REQUEST_HEADER_BYTES {
                return Ok(None);
            }
            break index;
        }
        if bytes.len() > MAX_REQUEST_HEADER_BYTES {
            return Ok(None);
        }
    };

    let buffered_body = bytes[(header_end + 4)..].to_vec();
    let head = match std::str::from_utf8(&bytes[..header_end]) {
        Ok(head) => head,
        Err(_) => return Ok(None),
    };
    let mut lines = head.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut request_line = request_line.split_whitespace();
    let Some(method) = request_line.next() else {
        return Ok(None);
    };
    let Some(target) = request_line.next() else {
        return Ok(None);
    };
    let Some(version) = request_line.next() else {
        return Ok(None);
    };
    if request_line.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Ok(None);
    }

    let mut range_header = None;
    let mut content_length = None;
    let mut note_id_header = None;
    let mut transfer_encoding = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Ok(None);
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("range") {
            if range_header.is_some() {
                return Ok(None);
            }
            range_header = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Ok(None);
            }
            let Ok(length) = value.parse::<usize>() else {
                return Ok(None);
            };
            content_length = Some(length);
        } else if name.eq_ignore_ascii_case("x-rustnote-note-id") {
            if note_id_header.is_some() {
                return Ok(None);
            }
            note_id_header = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if transfer_encoding.is_some() {
                return Ok(None);
            }
            transfer_encoding = Some(value.to_ascii_lowercase());
        }
    }
    if transfer_encoding
        .as_deref()
        .is_some_and(|value| value != "identity")
    {
        // Browser File/Blob uploads have a known size and use Content-Length. Keeping
        // the loopback parser length-delimited avoids a second HTTP stack here.
        return Ok(None);
    }

    Ok(Some(HttpRequest {
        method: method.to_string(),
        target: target.to_string(),
        range_header,
        content_length,
        note_id_header,
        buffered_body,
    }))
}

fn image_import_stage_route<'a>(
    target: &'a str,
    expected_token: &str,
) -> Option<ImageImportStageRoute<'a>> {
    let path = target.split('?').next()?;
    let mut segments = path.strip_prefix('/')?.split('/');
    if segments.next()? != "stage" || segments.next()? != expected_token {
        return None;
    }
    if segments.next()? != "image-import" {
        return None;
    }
    let stage_token = segments.next();
    if segments.next().is_some() {
        return None;
    }
    match stage_token {
        None | Some("") => Some(ImageImportStageRoute::Collection),
        Some(token) => Some(ImageImportStageRoute::Token(token)),
    }
}

fn handle_image_import_stage_request(
    stream: &mut TcpStream,
    request: &HttpRequest,
    route: ImageImportStageRoute<'_>,
    libraries: &LibraryRuntime,
) -> io::Result<()> {
    if request.method == "OPTIONS" {
        return write_stage_response(stream, 204, "No Content", "", "");
    }

    match (request.method.as_str(), route) {
        ("POST", ImageImportStageRoute::Collection) => {
            let Some(note_id_header) = request.note_id_header.as_deref() else {
                return write_stage_error(
                    stream,
                    400,
                    "Bad Request",
                    "Image staging is missing its note identity",
                );
            };
            let note_id = match crate::domain::NoteId::parse(note_id_header.to_string()) {
                Ok(note_id) => note_id,
                Err(_) => {
                    return write_stage_error(
                        stream,
                        400,
                        "Bad Request",
                        "Image staging has an invalid note identity",
                    );
                }
            };
            let Some(library) = libraries.current() else {
                return write_stage_error(
                    stream,
                    503,
                    "Service Unavailable",
                    "Rustnote library is not ready",
                );
            };
            let transport = library.transport_store();
            let _plaintext_permit = match transport.plaintext_attachment_ingress_permit(&note_id) {
                Ok(permit) => permit,
                Err(_) => {
                    return write_stage_error(
                        stream,
                        403,
                        "Forbidden",
                        "Images cannot be staged for this note",
                    );
                }
            };

            let Some(content_length) = request.content_length else {
                return write_stage_error(
                    stream,
                    411,
                    "Length Required",
                    "Image staging requires a known byte length",
                );
            };
            if content_length == 0 {
                return write_stage_error(
                    stream,
                    400,
                    "Bad Request",
                    "Image import staging is empty",
                );
            }
            if content_length > crate::native::staging::IMAGE_IMPORT_STAGE_MAX_BYTES {
                return write_stage_error(
                    stream,
                    413,
                    "Payload Too Large",
                    "Image import staging is too large",
                );
            }
            if request.buffered_body.len() > content_length {
                return write_stage_error(
                    stream,
                    400,
                    "Bad Request",
                    "Image staging request has an invalid body length",
                );
            }

            let buffered_len = request.buffered_body.len();
            let remaining = content_length - buffered_len;
            let prefix = io::Cursor::new(request.buffered_body.as_slice());
            let tail = stream.take(remaining as u64);
            let reader = prefix.chain(tail);
            let token = match crate::native::staging::stage_image_import_reader(
                note_id.as_str(),
                reader,
                content_length,
            ) {
                Ok(token) => token,
                Err(error) => {
                    return write_stage_error(stream, 400, "Bad Request", &error.to_string());
                }
            };
            let body = serde_json::json!({"version": 1, "token": token}).to_string();
            write_stage_response(stream, 200, "OK", "application/json; charset=utf-8", &body)
        }
        ("DELETE", ImageImportStageRoute::Token(stage_token)) => {
            match crate::native::staging::discard_staged_image_import(stage_token) {
                Ok(()) => write_stage_response(stream, 204, "No Content", "", ""),
                Err(error) => write_stage_error(stream, 400, "Bad Request", &error.to_string()),
            }
        }
        _ => write_stage_error(stream, 405, "Method Not Allowed", "Method not allowed"),
    }
}

fn write_stage_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {status} {reason}\r\n")?;
    if !content_type.is_empty() {
        write!(stream, "Content-Type: {content_type}\r\n")?;
    }
    write!(
        stream,
        concat!(
            "Content-Length: {}\r\n",
            "Access-Control-Allow-Origin: *\r\n",
            "Access-Control-Allow-Methods: POST, DELETE, OPTIONS\r\n",
            "Access-Control-Allow-Headers: Content-Type, X-Rustnote-Note-Id\r\n",
            "Access-Control-Allow-Private-Network: true\r\n",
            "Cache-Control: no-store\r\n",
            "Referrer-Policy: no-referrer\r\n",
            "X-Content-Type-Options: nosniff\r\n",
            "Cross-Origin-Resource-Policy: cross-origin\r\n",
            "Connection: close\r\n\r\n{}"
        ),
        body.len(),
        body,
    )?;
    stream.flush()
}

fn write_stage_error(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    message: &str,
) -> io::Result<()> {
    let body = serde_json::json!({"error": message}).to_string();
    write_stage_response(stream, status, reason, "application/json; charset=utf-8", &body)
}

fn media_route<'a>(target: &'a str, expected_token: &str) -> Option<(&'a str, MediaRoute)> {
    let path = target.split('?').next()?;
    let mut segments = path.strip_prefix('/')?.split('/');
    if segments.next()? != "media" || segments.next()? != expected_token {
        return None;
    }
    let attachment_id = segments.next()?;
    let resource = segments.next()?;
    if segments.next().is_some() || !valid_attachment_id(attachment_id) {
        return None;
    }
    let route = if resource == "preview" || resource.starts_with("preview.") {
        MediaRoute::Preview
    } else {
        let profile_and_extension = resource.strip_prefix("compatible-")?;
        let (profile_name, extension) = profile_and_extension.split_once('.')?;
        let profile = crate::media_compat::parse_profile(profile_name)?;
        if !extension.eq_ignore_ascii_case(crate::media_compat::profile_extension(profile)) {
            return None;
        }
        MediaRoute::Compatible(profile)
    };
    Some((attachment_id, route))
}

fn valid_attachment_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn parse_single_range(value: &str, file_len: u64) -> Option<(u64, u64)> {
    if file_len == 0 {
        return None;
    }
    let range = value.trim().strip_prefix("bytes=")?;
    if range.contains(',') {
        return None;
    }
    let (start_text, end_text) = range.split_once('-')?;
    if start_text.is_empty() {
        let suffix = end_text.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        let length = suffix.min(file_len);
        return Some((file_len - length, file_len - 1));
    }

    let start = start_text.parse::<u64>().ok()?;
    if start >= file_len {
        return None;
    }
    let end = if end_text.is_empty() {
        file_len - 1
    } else {
        end_text.parse::<u64>().ok()?.min(file_len - 1)
    };
    (start <= end).then_some((start, end))
}

fn safe_media_content_type(file_name: &str, supplied: Option<&str>) -> String {
    let value = attachment_media::resolved_mime_type(file_name, supplied)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if value
        .bytes()
        .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        value
    } else {
        "application/octet-stream".to_string()
    }
}

fn write_media_headers(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    content_length: u64,
    content_range: Option<&str>,
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nAccept-Ranges: bytes\r\nCache-Control: private, no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n"
    )?;
    if let Some(content_range) = content_range {
        write!(stream, "Content-Range: {content_range}\r\n")?;
    }
    write!(stream, "\r\n")?;
    stream.flush()
}

fn write_range_not_satisfiable(stream: &mut TcpStream, file_len: u64) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nContent-Range: bytes */{file_len}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()
}

fn write_error_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    message: &'static str,
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{message}",
        message.len()
    )?;
    stream.flush()
}

fn stream_file_range(file: &mut File, stream: &mut TcpStream, mut remaining: u64) -> io::Result<()> {
    let mut buffer = vec![0_u8; STREAM_BUFFER_BYTES];
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let read = file.read(&mut buffer[..limit])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "media file ended before the declared content length",
            ));
        }
        stream.write_all(&buffer[..read])?;
        remaining -= read as u64;
    }
    Ok(())
}

fn token_hex(bytes: [u8; TOKEN_BYTES]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(TOKEN_BYTES * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_media_file_keeps_lock_waiting_until_file_handle_drops() {
        use std::{sync::mpsc, thread};

        let (storage, _transport, resource, note_id, _attachment_id, root) =
            crate::storage::protected_transport_test_fixture(
                "protected-media-read-lease",
                "source.mp3",
                Some("audio/mpeg"),
                b"protected media payload",
            );
        assert!(resource.protected);
        let leased_file = open_leased_media_file(&resource, &resource.path)
            .expect("open protected leased media file");
        drop(resource);

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let storage_for_lock = storage.clone();
        let note_for_lock = note_id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            done_tx.send(result).expect("signal lock finish");
        });

        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !crate::storage::protected_note_accepts_vault_reads_for_test(&storage, &note_id)
                .expect("inspect protected-note Vault read state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(revocation_visible, "Lock should publish revocation promptly");
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must wait while the media OS handle retains its vault lease"
        );

        drop(leased_file);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("lock should finish after media handle release")
            .expect("lock result");
        lock_thread.join().expect("join lock thread");

        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn image_import_stage_route_requires_server_token_and_exact_shape() {
        let token = "server-secret";
        assert_eq!(
            image_import_stage_route("/stage/server-secret/image-import", token),
            Some(ImageImportStageRoute::Collection)
        );
        assert_eq!(
            image_import_stage_route("/stage/server-secret/image-import/abc123", token),
            Some(ImageImportStageRoute::Token("abc123"))
        );
        assert_eq!(image_import_stage_route("/stage/wrong/image-import", token), None);
        assert_eq!(image_import_stage_route("/stage/server-secret/image-import/a/b", token), None);
    }
}
