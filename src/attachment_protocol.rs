use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use dioxus_desktop::wry::http::{Request, Response};
use rustnote::{
    attachment_media::{self, AttachmentMediaKind},
    library::LibraryContext,
    storage::{
        AttachmentTransportResource, AttachmentTransportSession, PdfAnnotationRow, StorageError,
    },
};
use serde_json::Value;

const PDF_WHOLE_FILE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const PDF_TRANSPORT_BLOCK_BYTES: u64 = 1024 * 1024;
const PDF_MAX_BLOCK_BYTES: u64 = 2 * 1024 * 1024;
const _: () = assert!(PDF_TRANSPORT_BLOCK_BYTES <= PDF_MAX_BLOCK_BYTES);
const PDF_TRANSPORT_READ_AHEAD_BLOCKS: u64 = 2;
const PDF_READ_SESSION_MAX_ENTRIES: usize = 8;
const PDF_READ_SESSION_IDLE_TTL: Duration = Duration::from_secs(5 * 60);
const INLINE_PREVIEW_RANGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_INLINE_PREVIEW_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ANNOTATION_PAGE_BYTES: usize = 512 * 1024;
const MAX_ANNOTATIONS_PER_PAGE: usize = 1_000;
const MAX_ANNOTATION_INDEX_ENTRIES: usize = 5_000;
const MAX_ANNOTATION_INDEX_TEXT_CHARS: usize = 160;
const MAX_ANNOTATION_INDEX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ANNOTATION_PAGES: usize = 100_000;
const MAX_TEXT_MARK_RECTS: usize = 64;
const ATTACHMENT_PREVIEW_CSP: &str = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; \
     script-src 'none'; object-src 'none'; frame-src 'none'; connect-src 'none'; \
     font-src 'none'; base-uri 'none'; form-action 'none'";
const MAX_TEXT_MARK_TEXT_BYTES: usize = 8 * 1024;
const MAX_TEXT_ANNOTATION_BYTES: usize = 16 * 1024;
const MAX_INK_POINTS: usize = 10_000;
const MAX_SHAPE_POINTS: usize = 2;
const MAX_PDF_PAGE_INDEX: u32 = 1_000_000;
const PDF_RESOURCE_CSP: &str = "default-src 'none'; object-src 'none'; frame-src 'none'; child-src 'none'; media-src 'none'; base-uri 'none'; form-action 'none'; navigate-to 'none'";

const EMPTY_BYTES: &[u8] = &[];
static PROTECTED_PREVIEW_COUNTER: AtomicU64 = AtomicU64::new(1);

const PDF_VIEWER_CSS: &[u8] = include_bytes!("../assets/pdfjs/rustnote-pdf-viewer.css");
const PDF_BOOTSTRAP_JS: &[u8] = include_bytes!("../assets/pdfjs/rustnote-pdf-bootstrap.js");
const PDF_RUNTIME_VIEWER: &[u8] = include_bytes!("../target/rustnote-pdfjs/runtime/viewer.mjs");
const PDF_RUNTIME_WORKER: &[u8] = include_bytes!("../target/rustnote-pdfjs/runtime/pdf.worker.mjs");

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/rustnote-pdfjs/runtime/standard_fonts.rs"
));
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/target/rustnote-pdfjs/runtime/wasm_assets.rs"
));

#[derive(Debug)]
struct PdfReadHandle {
    file: File,
    _transport_guard: AttachmentTransportResource,
    #[cfg(windows)]
    cursor_lock: Mutex<()>,
}

impl PdfReadHandle {
    fn open(record: &ManagedAttachment) -> io::Result<Self> {
        Ok(Self {
            file: File::open(&record.path)?,
            _transport_guard: record._resource_guard.clone(),
            #[cfg(windows)]
            cursor_lock: Mutex::new(()),
        })
    }
}

#[derive(Debug)]
struct PdfReadSession {
    path: PathBuf,
    file_len: u64,
    last_used: Instant,
}

#[derive(Default)]
struct PdfReadSessionCache {
    sessions: HashMap<String, PdfReadSession>,
}

static PDF_READ_SESSIONS: OnceLock<Mutex<PdfReadSessionCache>> = OnceLock::new();

fn cached_pdf_read_session(attachment_id: &str, expected_path: Option<&Path>) -> Option<u64> {
    let cache = PDF_READ_SESSIONS.get_or_init(|| Mutex::new(PdfReadSessionCache::default()));
    let now = Instant::now();
    let mut guard = cache.lock().ok()?;
    guard.sessions.retain(|_, session| {
        now.saturating_duration_since(session.last_used) <= PDF_READ_SESSION_IDLE_TTL
    });
    if expected_path.is_some_and(|path| {
        guard
            .sessions
            .get(attachment_id)
            .is_some_and(|session| session.path != path)
    }) {
        guard.sessions.remove(attachment_id);
        return None;
    }
    let session = guard.sessions.get_mut(attachment_id)?;
    session.last_used = now;
    Some(session.file_len)
}

fn create_pdf_read_session(attachment_id: &str, path: &Path) -> io::Result<u64> {
    // Cache only non-sensitive session metadata. File handles are intentionally
    // request-scoped so neither Lock nor Encrypt can leave an open plaintext PDF
    // descriptor alive after the authorized request completes.
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "PDF file missing"));
    }
    let file_len = metadata.len();
    let now = Instant::now();
    let cache = PDF_READ_SESSIONS.get_or_init(|| Mutex::new(PdfReadSessionCache::default()));
    if let Ok(mut guard) = cache.lock() {
        guard.sessions.retain(|_, session| {
            now.saturating_duration_since(session.last_used) <= PDF_READ_SESSION_IDLE_TTL
        });
        if guard.sessions.len() >= PDF_READ_SESSION_MAX_ENTRIES
            && let Some(oldest_id) = guard
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(id, _)| id.clone())
        {
            guard.sessions.remove(&oldest_id);
        }
        guard.sessions.insert(
            attachment_id.to_owned(),
            PdfReadSession {
                path: path.to_path_buf(),
                file_len,
                last_used: now,
            },
        );
    }
    Ok(file_len)
}

fn pdf_read_session(attachment_id: &str, path: &Path) -> io::Result<u64> {
    if let Some(file_len) = cached_pdf_read_session(attachment_id, Some(path)) {
        return Ok(file_len);
    }
    create_pdf_read_session(attachment_id, path)
}

fn release_pdf_read_session(attachment_id: &str) {
    let Some(cache) = PDF_READ_SESSIONS.get() else {
        return;
    };
    if let Ok(mut guard) = cache.lock() {
        guard.sessions.remove(attachment_id);
    }
}

#[cfg(unix)]
fn read_pdf_exact_at(file: &PdfReadHandle, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.file.read_exact_at(bytes, offset)
}

#[cfg(windows)]
fn read_pdf_exact_at(file: &PdfReadHandle, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    let _guard = file
        .cursor_lock
        .lock()
        .map_err(|_| io::Error::other("PDF read lock was poisoned"))?;
    let mut read = 0_usize;
    while read < bytes.len() {
        let count = file
            .file
            .seek_read(&mut bytes[read..], offset.saturating_add(read as u64))?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PDF read ended early",
            ));
        }
        read += count;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn read_pdf_exact_at(file: &PdfReadHandle, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
    let mut clone = file.file.try_clone()?;
    clone.seek(SeekFrom::Start(offset))?;
    clone.read_exact(bytes)
}

#[derive(Debug)]
struct ManagedAttachment {
    path: PathBuf,
    sha256: String,
    file_name: String,
    mime_type: Option<String>,
    image_pixel_width: Option<i64>,
    image_pixel_height: Option<i64>,
    image_format: Option<String>,
    image_animated: Option<bool>,
    note_trashed: bool,
    protected: bool,
    _resource_guard: AttachmentTransportResource,
}

#[derive(Debug, Clone, Copy)]
struct ProtocolError {
    status: u16,
    message: &'static str,
}

impl ProtocolError {
    const fn new(status: u16, message: &'static str) -> Self {
        Self { status, message }
    }

    fn into_response(self) -> Response<Cow<'static, [u8]>> {
        protocol_error(self.status, self.message)
    }
}

#[derive(Clone, Copy)]
struct FileServePolicy {
    max_range_bytes: u64,
    max_full_bytes: Option<u64>,
    require_range: bool,
    clamp_oversized_ranges: bool,
}

#[derive(Clone, Copy)]
struct ParsedByteRange {
    start: u64,
    end: u64,
    clamp_from_end: bool,
}

const INLINE_PREVIEW_SERVE_POLICY: FileServePolicy = FileServePolicy {
    max_range_bytes: INLINE_PREVIEW_RANGE_BYTES,
    max_full_bytes: Some(MAX_INLINE_PREVIEW_BYTES),
    require_range: false,
    clamp_oversized_ranges: true,
};

const COMPATIBLE_IMAGE_PREVIEW_SERVE_POLICY: FileServePolicy = FileServePolicy {
    max_range_bytes: INLINE_PREVIEW_RANGE_BYTES,
    max_full_bytes: Some(128 * 1024 * 1024),
    require_range: false,
    clamp_oversized_ranges: true,
};

const SCREEN_IMAGE_PREVIEW_SERVE_POLICY: FileServePolicy = FileServePolicy {
    max_range_bytes: INLINE_PREVIEW_RANGE_BYTES,
    max_full_bytes: Some(64 * 1024 * 1024),
    require_range: false,
    clamp_oversized_ranges: true,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttachmentPreviewVariant {
    Original,
    CompatibleImage,
    ScreenImage,
}

pub(crate) fn attachment_asset_response(
    request: Request<Vec<u8>>,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    let route_path = attachment_asset_route_path(request.uri().path());
    if route_path.starts_with("/image-edit-stage/") {
        return image_edit_stage_response(&request, route_path, library);
    }
    if route_path == "/image-import-stage" || route_path.starts_with("/image-import-stage/") {
        return image_import_stage_response(&request, route_path, library);
    }
    if !matches!(request.method().as_str(), "GET" | "HEAD") {
        return protocol_error(405, "Method not allowed");
    }
    let Some((attachment_id, variant)) = attachment_preview_route(route_path) else {
        return protocol_error(400, "Invalid attachment preview path");
    };
    let Some(library) = library else {
        return protocol_error(503, "Rustnote library is not ready");
    };
    let record = match managed_attachment(library, attachment_id) {
        Ok(record) => record,
        Err(error) => return error.into_response(),
    };
    let media_kind = attachment_media::kind(&record.file_name, record.mime_type.as_deref());
    if variant != AttachmentPreviewVariant::Original {
        if media_kind != AttachmentMediaKind::Image {
            return protocol_error(415, "Image renditions are only available for images");
        }
        return match variant {
            AttachmentPreviewVariant::CompatibleImage => {
                compatible_image_preview_response(&request, library, &record)
            }
            AttachmentPreviewVariant::ScreenImage => {
                if !rustnote::image_rendition::needs_screen_rendition(
                    record.image_pixel_width,
                    record.image_pixel_height,
                    record.image_format.as_deref(),
                    record.image_animated,
                ) {
                    return protocol_error(404, "Screen image rendition is not required");
                }
                screen_image_preview_response(&request, library, &record)
            }
            AttachmentPreviewVariant::Original => unreachable!(),
        };
    }
    let policy = match media_kind {
        AttachmentMediaKind::Image | AttachmentMediaKind::Audio | AttachmentMediaKind::Video => {
            INLINE_PREVIEW_SERVE_POLICY
        }
        AttachmentMediaKind::Pdf | AttachmentMediaKind::File => {
            return protocol_error(415, "Attachment type is not available for inline preview");
        }
    };
    let content_type =
        attachment_media::resolved_mime_type(&record.file_name, record.mime_type.as_deref())
            .unwrap_or_else(|| "application/octet-stream".to_string());
    serve_file(
        &request,
        &record.path,
        &content_type,
        // Attachment bytes may later cross the ordinary -> Vault boundary. Never
        // ask the embedded WebView to persist a second plaintext copy that Rustnote
        // cannot target during E3 cleanup. Native rendition caches remain the
        // performance layer for ordinary notes.
        "private, no-store",
        policy,
    )
}

fn image_edit_stage_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    let resource = route_path.trim_start_matches("/image-edit-stage/");
    if resource.is_empty() || resource.contains('/') {
        return attachment_json_error(400, "Invalid image edit staging path");
    }

    match request.method().as_str() {
        "POST" => {
            if !valid_attachment_id(resource) {
                return attachment_json_error(400, "Invalid image attachment identifier");
            }
            let Some(library) = library else {
                return attachment_json_error(503, "Rustnote library is not ready");
            };
            let record = match managed_attachment(library, resource) {
                Ok(record) => record,
                Err(error) => return attachment_json_error(error.status, error.message),
            };
            if record.note_trashed {
                return attachment_json_error(
                    403,
                    "Image editing is unavailable while the note is in Trash",
                );
            }
            if attachment_media::kind(&record.file_name, record.mime_type.as_deref())
                != AttachmentMediaKind::Image
            {
                return attachment_json_error(415, "The selected attachment is not an image");
            }
            let hinted_mime_type = request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .map(str::trim)
                .unwrap_or("");
            let detected_mime_type = if request.body().starts_with(b"\x89PNG\r\n\x1a\n") {
                "image/png"
            } else if request.body().starts_with(b"\xFF\xD8\xFF") {
                "image/jpeg"
            } else {
                return attachment_json_error(415, "Rendered image format is unsupported");
            };
            if !hinted_mime_type.is_empty() && hinted_mime_type != detected_mime_type {
                return attachment_json_error(
                    400,
                    "Rendered image media type does not match its data",
                );
            }
            let token = match rustnote::native::staging::stage_image_edit(
                resource,
                request.body(),
                detected_mime_type,
            ) {
                Ok(token) => token,
                Err(_) => return attachment_json_error(400, "Could not stage edited image"),
            };
            let body = serde_json::json!({"version": 1, "token": token})
                .to_string()
                .into_bytes();
            attachment_json_response(200, Cow::Owned(body))
        }
        "DELETE" => match rustnote::native::staging::discard_staged_image_edit(resource) {
            Ok(()) => attachment_json_response(204, Cow::Borrowed(EMPTY_BYTES)),
            Err(_) => attachment_json_error(400, "Could not discard staged image edit"),
        },
        _ => attachment_json_error(405, "Method not allowed"),
    }
}

fn image_import_stage_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    let resource = route_path
        .trim_start_matches("/image-import-stage")
        .trim_start_matches('/');
    match request.method().as_str() {
        "POST" => {
            if !resource.is_empty() {
                return attachment_json_error(400, "Invalid image import staging path");
            }
            let Some(note_id_header) = request
                .headers()
                .get("x-rustnote-note-id")
                .and_then(|value| value.to_str().ok())
            else {
                return attachment_json_error(400, "Image staging is missing its note identity");
            };
            let note_id = match rustnote::domain::NoteId::parse(note_id_header.to_string()) {
                Ok(note_id) => note_id,
                Err(_) => {
                    return attachment_json_error(
                        400,
                        "Image staging has an invalid note identity",
                    );
                }
            };
            let Some(library) = library else {
                return attachment_json_error(503, "Rustnote library is not ready");
            };
            let transport = library.transport_store();
            let _plaintext_permit = match transport.plaintext_attachment_ingress_permit(&note_id) {
                Ok(permit) => permit,
                Err(_) => {
                    return attachment_json_error(403, "Images cannot be staged for this note");
                }
            };
            let hinted_mime_type = request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .map(str::trim)
                .unwrap_or("");
            if !hinted_mime_type.is_empty()
                && hinted_mime_type != "application/octet-stream"
                && !hinted_mime_type.starts_with("image/")
            {
                return attachment_json_error(415, "Image import staging only accepts image data");
            }
            let token = match rustnote::native::staging::stage_image_import(
                note_id.as_str(),
                request.body(),
            ) {
                Ok(token) => token,
                Err(error) => return attachment_json_error(400, &error.to_string()),
            };
            let body = serde_json::json!({"version": 1, "token": token})
                .to_string()
                .into_bytes();
            attachment_json_response(200, Cow::Owned(body))
        }
        "DELETE" => {
            if resource.is_empty() || resource.contains('/') {
                return attachment_json_error(400, "Invalid image import staging token");
            }
            match rustnote::native::staging::discard_staged_image_import(resource) {
                Ok(()) => attachment_json_response(204, Cow::Borrowed(EMPTY_BYTES)),
                Err(_) => attachment_json_error(400, "Could not discard staged image import"),
            }
        }
        _ => attachment_json_error(405, "Method not allowed"),
    }
}

fn attachment_json_response(status: u16, body: Cow<'static, [u8]>) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap_or_else(|_| protocol_error(500, "Could not serve image staging response"))
}

fn attachment_json_error(status: u16, message: &str) -> Response<Cow<'static, [u8]>> {
    attachment_json_response(status, json_error(message))
}

fn regular_file_exists(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
}

fn compatible_image_preview_response(
    request: &Request<Vec<u8>>,
    library: &LibraryContext,
    record: &ManagedAttachment,
) -> Response<Cow<'static, [u8]>> {
    if record.protected {
        let (preview_root, cache_path) = ephemeral_protected_preview_path(record, "compatible");
        let mut preview_guard = match record
            ._resource_guard
            .prepare_ephemeral_plaintext_directory(preview_root.clone())
        {
            Ok(Some(guard)) => guard,
            _ => return protocol_error(500, "Protected image preview staging failed"),
        };
        if rustnote::native::rendition::create_compatible_image_rendition(&record.path, &cache_path)
            .is_err()
            || !cache_path.is_file()
            || imagesize::size(&cache_path).is_err()
        {
            return protocol_error(415, "A compatible image preview could not be created");
        }
        let response = serve_file(
            request,
            &cache_path,
            "image/png",
            "private, no-store",
            COMPATIBLE_IMAGE_PREVIEW_SERVE_POLICY,
        );
        if preview_guard.remove().is_err() {
            return protocol_error(500, "Protected image preview cleanup failed");
        }
        return response;
    }
    let Some(cache_relative_path) =
        rustnote::attachment_paths::compatible_image_cache_relative_path(&record.sha256)
    else {
        return protocol_error(400, "Invalid attachment identity");
    };
    let cache_path = library.paths().data_directory().join(cache_relative_path);

    let cached_is_valid = fs::symlink_metadata(&cache_path).is_ok_and(|metadata| {
        !metadata.file_type().is_symlink() && metadata.is_file() && metadata.len() > 0
    }) && imagesize::size(&cache_path).is_ok();
    if !cached_is_valid {
        let _ = rustnote::native::rendition::discard_persistent_image_rendition(
            &library.paths().image_rendition_directory(),
            &cache_path,
        );
        if rustnote::native::rendition::create_persistent_compatible_image_rendition(
            &record.path,
            &cache_path,
            &library.paths().image_rendition_directory(),
        )
        .is_err()
        {
            return protocol_error(415, "A compatible image preview could not be created");
        }
        if !regular_file_exists(&cache_path) || imagesize::size(&cache_path).is_err() {
            let _ = rustnote::native::rendition::discard_persistent_image_rendition(
                &library.paths().image_rendition_directory(),
                &cache_path,
            );
            return protocol_error(500, "Compatible image preview is invalid");
        }
        prune_image_rendition_cache(library, &cache_path);
    }

    serve_file(
        request,
        &cache_path,
        "image/png",
        "private, no-store",
        COMPATIBLE_IMAGE_PREVIEW_SERVE_POLICY,
    )
}

fn screen_image_preview_response(
    request: &Request<Vec<u8>>,
    library: &LibraryContext,
    record: &ManagedAttachment,
) -> Response<Cow<'static, [u8]>> {
    if record.protected {
        let (preview_root, cache_path) = ephemeral_protected_preview_path(record, "screen");
        let mut preview_guard = match record
            ._resource_guard
            .prepare_ephemeral_plaintext_directory(preview_root.clone())
        {
            Ok(Some(guard)) => guard,
            _ => return protocol_error(500, "Protected image preview staging failed"),
        };
        if rustnote::native::rendition::create_screen_image_rendition(
            &record.path,
            &cache_path,
            rustnote::image_rendition::SCREEN_LONG_EDGE,
        )
        .is_err()
        {
            return protocol_error(415, "A screen image preview could not be created");
        }
        let valid_output = cache_path.is_file()
            && imagesize::size(&cache_path).is_ok_and(|size| {
                size.width > 0
                    && size.height > 0
                    && size.width.max(size.height)
                        <= rustnote::image_rendition::SCREEN_LONG_EDGE as usize
            });
        if !valid_output {
            return protocol_error(500, "Screen image rendition is invalid");
        }
        let response = serve_file(
            request,
            &cache_path,
            "image/png",
            "private, no-store",
            SCREEN_IMAGE_PREVIEW_SERVE_POLICY,
        );
        if preview_guard.remove().is_err() {
            return protocol_error(500, "Protected image preview cleanup failed");
        }
        return response;
    }
    let Some(cache_relative_path) =
        rustnote::attachment_paths::screen_image_cache_relative_path(&record.sha256)
    else {
        return protocol_error(400, "Invalid attachment identity");
    };
    let cache_path = library.paths().data_directory().join(cache_relative_path);

    let cached_is_valid = fs::symlink_metadata(&cache_path).is_ok_and(|metadata| {
        !metadata.file_type().is_symlink() && metadata.is_file() && metadata.len() > 0
    }) && imagesize::size(&cache_path).is_ok_and(|size| {
        size.width.max(size.height) <= rustnote::image_rendition::SCREEN_LONG_EDGE as usize
    });
    if !cached_is_valid {
        let _ = rustnote::native::rendition::discard_persistent_image_rendition(
            &library.paths().image_rendition_directory(),
            &cache_path,
        );
        if rustnote::native::rendition::create_persistent_screen_image_rendition(
            &record.path,
            &cache_path,
            &library.paths().image_rendition_directory(),
            rustnote::image_rendition::SCREEN_LONG_EDGE,
        )
        .is_err()
        {
            return protocol_error(415, "A screen image rendition could not be created");
        }
        let valid_output = regular_file_exists(&cache_path)
            && imagesize::size(&cache_path).is_ok_and(|size| {
                size.width > 0
                    && size.height > 0
                    && size.width.max(size.height)
                        <= rustnote::image_rendition::SCREEN_LONG_EDGE as usize
            });
        if !valid_output {
            let _ = rustnote::native::rendition::discard_persistent_image_rendition(
                &library.paths().image_rendition_directory(),
                &cache_path,
            );
            return protocol_error(500, "Screen image rendition is invalid");
        }
        prune_image_rendition_cache(library, &cache_path);
    }

    serve_file(
        request,
        &cache_path,
        "image/png",
        "private, no-store",
        SCREEN_IMAGE_PREVIEW_SERVE_POLICY,
    )
}

fn ephemeral_protected_preview_path(
    record: &ManagedAttachment,
    variant: &str,
) -> (PathBuf, PathBuf) {
    let sequence = PROTECTED_PREVIEW_COUNTER.fetch_add(1, Ordering::Relaxed);
    let parent = record.path.parent().unwrap_or_else(|| Path::new("."));
    let root = parent.join(format!(
        ".rustnote-protected-preview-{}-{sequence}",
        std::process::id()
    ));
    let path = root.join(format!("{variant}.png"));
    (root, path)
}

fn prune_image_rendition_cache(library: &LibraryContext, protected_path: &Path) {
    let root = library.paths().image_rendition_directory();
    let Ok(directories) = fs::read_dir(&root) else {
        return;
    };

    let mut total_bytes = 0_u64;
    let mut entries = Vec::new();
    for directory in directories.flatten() {
        let Ok(file_type) = directory.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(directory.path()) else {
            continue;
        };
        for file in files.flatten() {
            let Ok(metadata) = file.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let size = metadata.len();
            total_bytes = total_bytes.saturating_add(size);
            let recency = metadata
                .accessed()
                .or_else(|_| metadata.modified())
                .unwrap_or(UNIX_EPOCH);
            entries.push((recency, file.path(), size));
        }
    }
    if total_bytes <= rustnote::image_rendition::CACHE_BUDGET_BYTES {
        return;
    }

    entries.sort_by_key(|(recency, _, _)| *recency);
    for (_, path, size) in entries {
        if total_bytes <= rustnote::image_rendition::CACHE_BUDGET_BYTES {
            break;
        }
        if path == protected_path {
            continue;
        }
        if rustnote::native::rendition::discard_persistent_image_rendition(&root, &path).is_ok() {
            total_bytes = total_bytes.saturating_sub(size);
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

pub(crate) fn pdf_asset_response(
    request: Request<Vec<u8>>,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    let method = request.method().as_str();
    let Some(path) = pdf_asset_route_path(request.uri().path()) else {
        return pdf_response(
            404,
            "text/plain; charset=utf-8",
            Cow::Borrowed(b"Not found"),
        );
    };

    if path.starts_with("/annotations/") {
        return pdf_annotation_response(&request, path, library);
    }
    if path.starts_with("/annotation-index/") {
        return pdf_annotation_index_response(&request, path, library);
    }
    if path.starts_with("/annotation-pages/") {
        return pdf_annotation_pages_response(&request, path, library);
    }
    if path.starts_with("/export/") {
        return pdf_export_stage_response(&request, path, library);
    }
    if path.starts_with("/session/") {
        if method != "DELETE" {
            return pdf_response(
                405,
                "text/plain; charset=utf-8",
                Cow::Borrowed(b"Method not allowed"),
            );
        }
        let attachment_id = path.trim_start_matches("/session/");
        if !valid_attachment_id(attachment_id) {
            return pdf_protocol_error(400, "Invalid attachment identifier");
        }
        release_pdf_read_session(attachment_id);
        return pdf_response(204, "text/plain; charset=utf-8", Cow::Borrowed(EMPTY_BYTES));
    }
    if !matches!(method, "GET" | "HEAD") {
        return pdf_response(
            405,
            "text/plain; charset=utf-8",
            Cow::Borrowed(b"Method not allowed"),
        );
    }

    match path {
        "/runtime/legacy/viewer.mjs" => pdf_module_response(method, PDF_RUNTIME_VIEWER),
        "/runtime/legacy/pdf.worker.mjs" => pdf_module_response(method, PDF_RUNTIME_WORKER),
        "/viewer.css" => pdf_response(
            200,
            "text/css; charset=utf-8",
            if method == "HEAD" {
                Cow::Borrowed(EMPTY_BYTES)
            } else {
                Cow::Borrowed(PDF_VIEWER_CSS)
            },
        ),
        "/bootstrap.js" => pdf_response(
            200,
            "text/javascript; charset=utf-8",
            if method == "HEAD" {
                Cow::Borrowed(EMPTY_BYTES)
            } else {
                Cow::Borrowed(PDF_BOOTSTRAP_JS)
            },
        ),
        _ if path.starts_with("/runtime/legacy/wasm/") => {
            let name = path.trim_start_matches("/runtime/legacy/wasm/");
            if name.is_empty() || name.contains('/') || name.contains('\\') {
                return pdf_response(
                    404,
                    "text/plain; charset=utf-8",
                    Cow::Borrowed(b"Not found"),
                );
            }
            let content_type = if name.ends_with(".wasm") {
                "application/wasm"
            } else if name.ends_with(".js") {
                "text/javascript; charset=utf-8"
            } else {
                "application/octet-stream"
            };
            match pdf_wasm_asset(name) {
                Some(asset) => pdf_response(
                    200,
                    content_type,
                    if method == "HEAD" {
                        Cow::Borrowed(EMPTY_BYTES)
                    } else {
                        Cow::Borrowed(asset)
                    },
                ),
                None => pdf_response(
                    404,
                    "text/plain; charset=utf-8",
                    Cow::Borrowed(b"Not found"),
                ),
            }
        }
        _ if path.starts_with("/runtime/legacy/standard_fonts/") => {
            let name = path.trim_start_matches("/runtime/legacy/standard_fonts/");
            if name.is_empty() || name.contains('/') || name.contains('\\') {
                return pdf_response(
                    404,
                    "text/plain; charset=utf-8",
                    Cow::Borrowed(b"Not found"),
                );
            }
            match pdf_standard_font_asset(name) {
                Some(font) => pdf_response(
                    200,
                    "application/octet-stream",
                    if method == "HEAD" {
                        Cow::Borrowed(EMPTY_BYTES)
                    } else {
                        Cow::Borrowed(font)
                    },
                ),
                None => pdf_response(
                    404,
                    "text/plain; charset=utf-8",
                    Cow::Borrowed(b"Not found"),
                ),
            }
        }
        _ if path.starts_with("/metadata/") => {
            if method != "GET" {
                return pdf_response(
                    405,
                    "application/json; charset=utf-8",
                    json_error("Method not allowed"),
                );
            }
            let attachment_id = path.trim_start_matches("/metadata/");
            let Some(library) = library else {
                return pdf_protocol_error(503, "Rustnote library is not ready");
            };
            let record = match pdf_attachment(library, attachment_id) {
                Ok(record) => record,
                Err(error) => return pdf_protocol_error(error.status, error.message),
            };
            pdf_metadata_response(attachment_id, &record.path)
        }
        _ if path.starts_with("/document/") => {
            if method != "GET" {
                return pdf_response(
                    405,
                    "text/plain; charset=utf-8",
                    Cow::Borrowed(b"Method not allowed"),
                );
            }
            let attachment_id = path.trim_start_matches("/document/");
            if !valid_attachment_id(attachment_id) {
                return pdf_protocol_error(400, "Invalid attachment identifier");
            }
            let Some(library) = library else {
                return pdf_protocol_error(503, "Rustnote library is not ready");
            };
            serve_pdf_document(&request, attachment_id, library)
        }
        _ => pdf_response(
            404,
            "text/plain; charset=utf-8",
            Cow::Borrowed(b"Not found"),
        ),
    }
}

fn pdf_annotation_pages_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    if !matches!(request.method().as_str(), "GET" | "HEAD") {
        return pdf_response(
            405,
            "application/json; charset=utf-8",
            json_error("Method not allowed"),
        );
    }
    let attachment_id = route_path.trim_start_matches("/annotation-pages/");
    if !valid_attachment_id(attachment_id) || attachment_id.contains('/') {
        return pdf_response(
            400,
            "application/json; charset=utf-8",
            json_error("Invalid annotation pages path"),
        );
    }
    let Some(library) = library else {
        return pdf_response(
            503,
            "application/json; charset=utf-8",
            json_error("Rustnote library is not ready"),
        );
    };
    let transport = library.transport_store();
    let session = match transport.read_session() {
        Ok(session) => session,
        Err(_) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Attachment database unavailable"),
            );
        }
    };
    let _attachment_guard = match pdf_attachment_with_session(&session, attachment_id) {
        Ok(record) => record,
        Err(_) => {
            return pdf_response(
                404,
                "application/json; charset=utf-8",
                json_error("PDF unavailable"),
            );
        }
    };
    let pages = match session.list_pdf_annotation_pages(attachment_id, MAX_ANNOTATION_PAGES) {
        Ok(pages) => pages,
        Err(_) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Could not read annotation pages"),
            );
        }
    };
    let body = serde_json::json!({"version": 1, "pages": pages})
        .to_string()
        .into_bytes();
    pdf_response(
        200,
        "application/json; charset=utf-8",
        if request.method().as_str() == "HEAD" {
            Cow::Borrowed(EMPTY_BYTES)
        } else {
            Cow::Owned(body)
        },
    )
}

fn pdf_export_stage_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    if request.method().as_str() != "POST" {
        return pdf_response(
            405,
            "application/json; charset=utf-8",
            json_error("Method not allowed"),
        );
    }
    let attachment_id = route_path.trim_start_matches("/export/");
    if !valid_attachment_id(attachment_id) || attachment_id.contains('/') {
        return pdf_response(
            400,
            "application/json; charset=utf-8",
            json_error("Invalid export path"),
        );
    }
    let Some(library) = library else {
        return pdf_response(
            503,
            "application/json; charset=utf-8",
            json_error("Rustnote library is not ready"),
        );
    };
    // Keep the attachment resource itself alive through native export staging;
    // it owns the compound plaintext/Vault authorization for this full scope.
    let _record = match pdf_attachment_content(library, attachment_id) {
        Ok(record) => record,
        Err(_) => {
            return pdf_response(
                404,
                "application/json; charset=utf-8",
                json_error("PDF unavailable"),
            );
        }
    };
    let token = match rustnote::native::staging::stage_pdf_export(attachment_id, request.body()) {
        Ok(token) => token,
        Err(_) => {
            return pdf_response(
                400,
                "application/json; charset=utf-8",
                json_error("Could not stage annotated PDF"),
            );
        }
    };
    let body = serde_json::json!({"version": 1, "token": token})
        .to_string()
        .into_bytes();
    pdf_response(200, "application/json; charset=utf-8", Cow::Owned(body))
}

fn pdf_annotation_index_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    if !matches!(request.method().as_str(), "GET" | "HEAD") {
        return pdf_response(
            405,
            "application/json; charset=utf-8",
            json_error("Method not allowed"),
        );
    }
    let attachment_id = route_path.trim_start_matches("/annotation-index/");
    if !valid_attachment_id(attachment_id) || attachment_id.contains('/') {
        return pdf_response(
            400,
            "application/json; charset=utf-8",
            json_error("Invalid annotation index path"),
        );
    }
    let Some(library) = library else {
        return pdf_response(
            503,
            "application/json; charset=utf-8",
            json_error("Rustnote library is not ready"),
        );
    };
    let transport = library.transport_store();
    let session = match transport.read_session() {
        Ok(session) => session,
        Err(_) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Attachment database unavailable"),
            );
        }
    };
    let _attachment_guard = match pdf_attachment_with_session(&session, attachment_id) {
        Ok(record) => record,
        Err(_) => {
            return pdf_response(
                404,
                "application/json; charset=utf-8",
                json_error("PDF unavailable"),
            );
        }
    };
    let rows = match session.pdf_annotation_rows(attachment_id, MAX_ANNOTATION_INDEX_SOURCE_BYTES) {
        Ok(rows) => rows,
        Err(_) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Could not read PDF annotation index"),
            );
        }
    };
    let json = match read_pdf_annotation_index(rows) {
        Ok(json) => json,
        Err(()) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Could not read PDF annotation index"),
            );
        }
    };
    pdf_response(
        200,
        "application/json; charset=utf-8",
        if request.method().as_str() == "HEAD" {
            Cow::Borrowed(EMPTY_BYTES)
        } else {
            Cow::Owned(json.into_bytes())
        },
    )
}

fn read_pdf_annotation_index(rows: Vec<PdfAnnotationRow>) -> Result<String, ()> {
    let mut summaries = Vec::new();
    let mut source_bytes = 0_usize;
    let mut truncated = false;

    for row in rows {
        let page_index = row.page_index;
        let json = row.annotations_json;
        source_bytes = source_bytes.saturating_add(json.len());
        if source_bytes > MAX_ANNOTATION_INDEX_SOURCE_BYTES {
            truncated = true;
            break;
        }
        let Ok(payload) = serde_json::from_str::<Value>(&json) else {
            continue;
        };
        let Some(annotations) = payload.get("annotations").and_then(Value::as_array) else {
            continue;
        };
        for annotation in annotations {
            if summaries.len() >= MAX_ANNOTATION_INDEX_ENTRIES {
                truncated = true;
                break;
            }
            let Some(summary) = annotation_index_summary(page_index, annotation) else {
                continue;
            };
            summaries.push(summary);
        }
        if truncated {
            break;
        }
    }

    serde_json::to_string(&serde_json::json!({
        "version": 1,
        "annotations": summaries,
        "truncated": truncated,
    }))
    .map_err(|_| ())
}

fn annotation_index_summary(page_index: u32, annotation: &Value) -> Option<Value> {
    let object = annotation.as_object()?;
    let id = object.get("id")?.as_str()?;
    let kind = object.get("kind")?.as_str()?;
    if !valid_annotation_id(id) {
        return None;
    }
    let (x, y) = annotation_anchor(object)?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(MAX_ANNOTATION_INDEX_TEXT_CHARS)
        .collect::<String>();
    Some(serde_json::json!({
        "id": id,
        "kind": kind,
        "pageIndex": page_index,
        "x": x,
        "y": y,
        "text": text,
    }))
}

fn annotation_anchor(object: &serde_json::Map<String, Value>) -> Option<(f64, f64)> {
    match object.get("kind")?.as_str()? {
        "highlight" | "underline" | "strike" => {
            let rect = object.get("rects")?.as_array()?.first()?.as_array()?;
            Some((rect.first()?.as_f64()?, rect.get(1)?.as_f64()?))
        }
        "note" | "text" => Some((object.get("x")?.as_f64()?, object.get("y")?.as_f64()?)),
        "ink" | "shape" => {
            let point = object.get("points")?.as_array()?.first()?.as_array()?;
            Some((point.first()?.as_f64()?, point.get(1)?.as_f64()?))
        }
        _ => None,
    }
}

fn pdf_annotation_response(
    request: &Request<Vec<u8>>,
    route_path: &str,
    library: Option<&LibraryContext>,
) -> Response<Cow<'static, [u8]>> {
    let Some((attachment_id, page_index)) = parse_annotation_path(route_path) else {
        return pdf_response(
            400,
            "application/json; charset=utf-8",
            json_error("Invalid annotation path"),
        );
    };
    let Some(library) = library else {
        return pdf_response(
            503,
            "application/json; charset=utf-8",
            json_error("Rustnote library is not ready"),
        );
    };
    let transport = library.transport_store();
    let read_only = matches!(request.method().as_str(), "GET" | "HEAD");
    let session = match if read_only {
        transport.read_session()
    } else {
        transport.write_session()
    } {
        Ok(session) => session,
        Err(_) => {
            return pdf_response(
                500,
                "application/json; charset=utf-8",
                json_error("Attachment database unavailable"),
            );
        }
    };
    let record = match pdf_attachment_with_session(&session, attachment_id) {
        Ok(record) => record,
        Err(_) => {
            return pdf_response(
                404,
                "application/json; charset=utf-8",
                json_error("PDF unavailable"),
            );
        }
    };

    match request.method().as_str() {
        "GET" | "HEAD" => {
            let json = match session.read_pdf_annotation_page(attachment_id, page_index) {
                Ok(Some(json)) => json,
                Ok(None) => r#"{"version":1,"annotations":[]}"#.to_string(),
                Err(_) => {
                    return pdf_response(
                        500,
                        "application/json; charset=utf-8",
                        json_error("Could not read PDF annotations"),
                    );
                }
            };
            pdf_response(
                200,
                "application/json; charset=utf-8",
                if request.method().as_str() == "HEAD" {
                    Cow::Borrowed(EMPTY_BYTES)
                } else {
                    Cow::Owned(json.into_bytes())
                },
            )
        }
        "POST" => {
            if record.note_trashed {
                return pdf_response(
                    403,
                    "application/json; charset=utf-8",
                    json_error("Annotations are read-only while the note is in Trash"),
                );
            }
            if request.body().len() > MAX_ANNOTATION_PAGE_BYTES {
                return pdf_response(
                    413,
                    "application/json; charset=utf-8",
                    json_error("Annotation page is too large"),
                );
            }
            let canonical = match validate_annotation_payload(request.body()) {
                Ok(canonical) => canonical,
                Err(message) => {
                    return pdf_response(
                        400,
                        "application/json; charset=utf-8",
                        json_error(message),
                    );
                }
            };
            let annotations_empty = serde_json::from_str::<Value>(&canonical)
                .ok()
                .and_then(|value| value.get("annotations").and_then(Value::as_array).cloned())
                .is_some_and(|annotations| annotations.is_empty());
            if session
                .replace_pdf_annotation_page(
                    attachment_id,
                    page_index,
                    &canonical,
                    annotations_empty,
                )
                .is_err()
            {
                return pdf_response(
                    500,
                    "application/json; charset=utf-8",
                    json_error("Could not save PDF annotations"),
                );
            }
            pdf_response(
                204,
                "application/json; charset=utf-8",
                Cow::Borrowed(EMPTY_BYTES),
            )
        }
        _ => pdf_response(
            405,
            "application/json; charset=utf-8",
            json_error("Method not allowed"),
        ),
    }
}

fn pdf_asset_route_path(path: &str) -> Option<&str> {
    let routed = path.strip_prefix("/rustnotepdf")?;
    if routed.is_empty() {
        return Some("/");
    }
    routed.starts_with('/').then_some(routed)
}

fn parse_annotation_path(path: &str) -> Option<(&str, u32)> {
    let suffix = path.strip_prefix("/annotations/")?;
    let (attachment_id, page_text) = suffix.split_once('/')?;
    if page_text.contains('/') || !valid_attachment_id(attachment_id) {
        return None;
    }
    let page_index = page_text.parse::<u32>().ok()?;
    (page_index <= MAX_PDF_PAGE_INDEX).then_some((attachment_id, page_index))
}

fn validate_annotation_payload(bytes: &[u8]) -> Result<String, &'static str> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "Annotations must be valid JSON")?;
    let object = value
        .as_object()
        .ok_or("Annotation page must be an object")?;
    if object.len() != 2 || object.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported annotation page format");
    }
    let annotations = object
        .get("annotations")
        .and_then(Value::as_array)
        .ok_or("annotations must be an array")?;
    if annotations.len() > MAX_ANNOTATIONS_PER_PAGE {
        return Err("Too many annotations on one page");
    }
    for annotation in annotations {
        validate_annotation(annotation)?;
    }
    serde_json::to_string(&value).map_err(|_| "Could not serialize annotations")
}

fn validate_annotation(annotation: &Value) -> Result<(), &'static str> {
    let object = annotation
        .as_object()
        .ok_or("Each annotation must be an object")?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or("Annotation id is required")?;
    if !valid_annotation_id(id) {
        return Err("Invalid annotation id");
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Annotation kind is required")?;
    validate_timestamp(object.get("createdAt"))?;
    validate_timestamp(object.get("updatedAt"))?;
    match kind {
        "highlight" | "underline" | "strike" => validate_text_mark(object),
        "note" => validate_note(object),
        "ink" => validate_ink(object),
        "shape" => validate_shape(object),
        "text" => validate_text_box(object),
        _ => Err("Unsupported annotation kind"),
    }
}

fn validate_text_mark(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    validate_keys(
        object,
        &[
            "id",
            "kind",
            "createdAt",
            "updatedAt",
            "rects",
            "text",
            "color",
        ],
    )?;
    validate_color(object.get("color"))?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if text.len() > MAX_TEXT_MARK_TEXT_BYTES {
        return Err("Marked text is too large");
    }
    let rects = object
        .get("rects")
        .and_then(Value::as_array)
        .ok_or("Text mark rectangles are required")?;
    if rects.is_empty() || rects.len() > MAX_TEXT_MARK_RECTS {
        return Err("Invalid text mark rectangle count");
    }
    for rect in rects {
        validate_rect(rect)?;
    }
    Ok(())
}

fn validate_note(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    validate_keys(
        object,
        &[
            "id",
            "kind",
            "createdAt",
            "updatedAt",
            "x",
            "y",
            "width",
            "height",
            "text",
            "color",
        ],
    )?;
    validate_color(object.get("color"))?;
    validate_box_geometry(object)?;
    validate_text_field(
        object,
        MAX_TEXT_ANNOTATION_BYTES,
        "Note text is required",
        "Note text is too large",
    )
}

fn validate_ink(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    validate_keys(
        object,
        &[
            "id",
            "kind",
            "createdAt",
            "updatedAt",
            "points",
            "width",
            "color",
        ],
    )?;
    validate_color(object.get("color"))?;
    let width = object
        .get("width")
        .and_then(Value::as_f64)
        .ok_or("Ink width is required")?;
    if !width.is_finite() || !(0.0005..=0.05).contains(&width) {
        return Err("Invalid ink width");
    }
    let points = object
        .get("points")
        .and_then(Value::as_array)
        .ok_or("Ink points are required")?;
    if points.len() < 2 || points.len() > MAX_INK_POINTS {
        return Err("Invalid ink point count");
    }
    for point in points {
        let pair = point
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or("Invalid ink point")?;
        for value in pair {
            validate_unit_number(value)?;
        }
    }
    Ok(())
}

fn validate_shape(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    validate_keys(
        object,
        &[
            "id",
            "kind",
            "createdAt",
            "updatedAt",
            "shape",
            "points",
            "width",
            "color",
        ],
    )?;
    validate_color(object.get("color"))?;
    match object.get("shape").and_then(Value::as_str) {
        Some("rectangle" | "ellipse" | "line") => {}
        _ => return Err("Unsupported shape type"),
    }
    let width = object
        .get("width")
        .and_then(Value::as_f64)
        .ok_or("Shape width is required")?;
    if !width.is_finite() || !(0.0005..=0.05).contains(&width) {
        return Err("Invalid shape width");
    }
    let points = object
        .get("points")
        .and_then(Value::as_array)
        .ok_or("Shape points are required")?;
    if points.len() != MAX_SHAPE_POINTS {
        return Err("Invalid shape point count");
    }
    for point in points {
        let pair = point
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or("Invalid shape point")?;
        for value in pair {
            validate_unit_number(value)?;
        }
    }
    let first = points[0].as_array().ok_or("Invalid shape point")?;
    let second = points[1].as_array().ok_or("Invalid shape point")?;
    let dx = (second[0].as_f64().unwrap_or(0.0) - first[0].as_f64().unwrap_or(0.0)).abs();
    let dy = (second[1].as_f64().unwrap_or(0.0) - first[1].as_f64().unwrap_or(0.0)).abs();
    let valid_geometry = if object.get("shape").and_then(Value::as_str) == Some("line") {
        dx.hypot(dy) >= 0.002
    } else {
        dx >= 0.002 && dy >= 0.002
    };
    if !valid_geometry {
        return Err("Shape is too small");
    }
    Ok(())
}

fn validate_text_box(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    validate_keys(
        object,
        &[
            "id",
            "kind",
            "createdAt",
            "updatedAt",
            "x",
            "y",
            "width",
            "height",
            "text",
            "color",
            "size",
        ],
    )?;
    validate_color(object.get("color"))?;
    validate_box_geometry(object)?;
    validate_text_field(
        object,
        MAX_TEXT_ANNOTATION_BYTES,
        "Text box text is required",
        "Text box text is too large",
    )?;
    match object.get("size").and_then(Value::as_str) {
        Some("small" | "medium" | "large") => Ok(()),
        _ => Err("Unsupported text size"),
    }
}

fn validate_box_geometry(object: &serde_json::Map<String, Value>) -> Result<(), &'static str> {
    for key in ["x", "y", "width", "height"] {
        let value = object
            .get(key)
            .and_then(Value::as_f64)
            .ok_or("Invalid annotation box geometry")?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err("Invalid annotation box geometry");
        }
    }
    let x = object.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = object.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    let width = object.get("width").and_then(Value::as_f64).unwrap_or(0.0);
    let height = object.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    if width <= 0.0 || height <= 0.0 || x + width > 1.0 || y + height > 1.0 {
        return Err("Invalid annotation box geometry");
    }
    Ok(())
}

fn validate_text_field(
    object: &serde_json::Map<String, Value>,
    max_bytes: usize,
    missing_error: &'static str,
    oversized_error: &'static str,
) -> Result<(), &'static str> {
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or(missing_error)?;
    if text.len() > max_bytes {
        return Err(oversized_error);
    }
    Ok(())
}

fn validate_rect(value: &Value) -> Result<(), &'static str> {
    let rect = value
        .as_array()
        .filter(|rect| rect.len() == 4)
        .ok_or("Invalid text mark rectangle")?;
    for value in rect {
        validate_unit_number(value)?;
    }
    let x = rect[0].as_f64().unwrap_or(0.0);
    let y = rect[1].as_f64().unwrap_or(0.0);
    let width = rect[2].as_f64().unwrap_or(0.0);
    let height = rect[3].as_f64().unwrap_or(0.0);
    if width <= 0.0 || height <= 0.0 || x + width > 1.0 || y + height > 1.0 {
        return Err("Invalid text mark rectangle");
    }
    Ok(())
}

fn validate_unit_number(value: &Value) -> Result<(), &'static str> {
    let number = value.as_f64().ok_or("Expected a number")?;
    if number.is_finite() && (0.0..=1.0).contains(&number) {
        Ok(())
    } else {
        Err("Normalized coordinates must be between zero and one")
    }
}

fn validate_color(value: Option<&Value>) -> Result<(), &'static str> {
    match value.and_then(Value::as_str).unwrap_or("yellow") {
        "yellow" | "green" | "blue" | "pink" | "red" | "black" => Ok(()),
        _ => Err("Unsupported annotation color"),
    }
}

fn validate_timestamp(value: Option<&Value>) -> Result<(), &'static str> {
    value
        .and_then(Value::as_u64)
        .map(|_| ())
        .ok_or("Invalid annotation timestamp")
}

fn validate_keys(
    object: &serde_json::Map<String, Value>,
    allowed: &[&str],
) -> Result<(), &'static str> {
    if object.keys().all(|key| allowed.contains(&key.as_str())) {
        Ok(())
    } else {
        Err("Annotation contains unsupported fields")
    }
}

fn valid_annotation_id(value: &str) -> bool {
    value.starts_with("ann_")
        && (5..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn json_error(message: &str) -> Cow<'static, [u8]> {
    let body = serde_json::json!({ "error": message })
        .to_string()
        .into_bytes();
    Cow::Owned(body)
}

fn pdf_attachment(
    library: &LibraryContext,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    let transport = library.transport_store();
    let session = transport
        .read_session()
        .map_err(|_| ProtocolError::new(404, "Attachment database unavailable"))?;
    pdf_attachment_with_session(&session, attachment_id)
}

fn pdf_attachment_content(
    library: &LibraryContext,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    let transport = library.transport_store();
    let session = transport
        .read_session()
        .map_err(|_| ProtocolError::new(404, "Attachment database unavailable"))?;
    pdf_attachment_content_with_session(&session, attachment_id)
}

fn pdf_attachment_content_with_session(
    session: &AttachmentTransportSession,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    let record = managed_attachment_with_session(session, attachment_id)?;
    if attachment_media::kind(&record.file_name, record.mime_type.as_deref())
        != AttachmentMediaKind::Pdf
    {
        return Err(ProtocolError::new(415, "Attachment is not a PDF"));
    }
    Ok(record)
}

fn pdf_attachment_with_session(
    session: &AttachmentTransportSession,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    let record = pdf_attachment_content_with_session(session, attachment_id)?;
    if !looks_like_pdf(&record.path) {
        return Err(ProtocolError::new(
            415,
            "Attachment does not contain a PDF header",
        ));
    }
    Ok(record)
}

fn looks_like_pdf(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut prefix = [0_u8; 1024];
    let Ok(read) = file.read(&mut prefix) else {
        return false;
    };
    prefix[..read].windows(5).any(|window| window == b"%PDF-")
}

fn managed_attachment(
    library: &LibraryContext,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    let transport = library.transport_store();
    let session = transport
        .read_session()
        .map_err(|_| ProtocolError::new(404, "Attachment database unavailable"))?;
    managed_attachment_with_session(&session, attachment_id)
}

fn managed_attachment_with_session(
    session: &AttachmentTransportSession,
    attachment_id: &str,
) -> Result<ManagedAttachment, ProtocolError> {
    if !valid_attachment_id(attachment_id) {
        return Err(ProtocolError::new(400, "Invalid attachment identifier"));
    }

    let resource = match session.attachment_resource(attachment_id) {
        Ok(Some(resource)) => resource,
        Ok(None) => return Err(ProtocolError::new(404, "Attachment not found")),
        Err(StorageError::Integrity(_)) => {
            return Err(ProtocolError::new(400, "Invalid attachment identity"));
        }
        Err(_) => {
            return Err(ProtocolError::new(
                500,
                "Could not read attachment metadata",
            ));
        }
    };

    Ok(managed_attachment_from_resource(resource))
}

fn managed_attachment_from_resource(resource: AttachmentTransportResource) -> ManagedAttachment {
    ManagedAttachment {
        path: resource.path.clone(),
        sha256: resource.content_hash.clone(),
        file_name: resource.file_name.clone(),
        mime_type: resource.mime_type.clone(),
        image_pixel_width: resource.image_pixel_width,
        image_pixel_height: resource.image_pixel_height,
        image_format: resource.image_format.clone(),
        image_animated: resource.image_animated,
        note_trashed: resource.note_trashed,
        protected: resource.protected,
        // Retain the complete transport resource so its opaque vault lease lives
        // for the entire protocol request. PDF file handles are request-scoped.
        _resource_guard: resource,
    }
}

fn pdf_protocol_error(status: u16, message: &'static str) -> Response<Cow<'static, [u8]>> {
    pdf_response(
        status,
        "text/plain; charset=utf-8",
        Cow::Borrowed(message.as_bytes()),
    )
}

fn pdf_uses_read_session(file_len: u64) -> bool {
    file_len > PDF_WHOLE_FILE_MAX_BYTES
}

fn pdf_metadata_response(attachment_id: &str, path: &Path) -> Response<Cow<'static, [u8]>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return pdf_protocol_error(404, "PDF metadata unavailable"),
    };
    let file_len = metadata.len();

    // Whole-file PDFs never need a persistent read session: the next request
    // transfers the file once and PDF.js owns the resulting Uint8Array.
    // Large PDFs keep the session warm here so their first block request avoids
    // another attachment lookup/open on the range-transport hot path.
    if !pdf_uses_read_session(file_len) {
        release_pdf_read_session(attachment_id);
    } else if pdf_read_session(attachment_id, path).is_err() {
        return pdf_protocol_error(404, "PDF metadata unavailable");
    }

    let body = serde_json::json!({
        "length": file_len,
        "whole_file_max_bytes": PDF_WHOLE_FILE_MAX_BYTES,
        "block_bytes": PDF_TRANSPORT_BLOCK_BYTES,
        "max_block_bytes": PDF_MAX_BLOCK_BYTES,
        "read_ahead_blocks": PDF_TRANSPORT_READ_AHEAD_BLOCKS,
    })
    .to_string()
    .into_bytes();
    pdf_response(200, "application/json; charset=utf-8", Cow::Owned(body))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdfDocumentRead {
    Whole,
    Range { offset: u64, length: u64 },
}

fn parse_pdf_document_read(query: Option<&str>, file_len: u64) -> Option<PdfDocumentRead> {
    let query = query?;
    if query == "full=1" {
        return (file_len <= PDF_WHOLE_FILE_MAX_BYTES).then_some(PdfDocumentRead::Whole);
    }

    let mut offset = None;
    let mut length = None;
    for field in query.split('&') {
        let (name, value) = field.split_once('=')?;
        match name {
            "offset" if offset.is_none() => offset = value.parse::<u64>().ok(),
            "length" if length.is_none() => length = value.parse::<u64>().ok(),
            _ => return None,
        }
    }
    let offset = offset?;
    let length = length?;
    if length == 0 || length > PDF_MAX_BLOCK_BYTES || offset >= file_len {
        return None;
    }
    let end = offset.checked_add(length)?;
    (end <= file_len).then_some(PdfDocumentRead::Range { offset, length })
}

fn serve_pdf_document(
    request: &Request<Vec<u8>>,
    attachment_id: &str,
    library: &LibraryContext,
) -> Response<Cow<'static, [u8]>> {
    if request.headers().contains_key("range") {
        return pdf_protocol_error(
            400,
            "PDF local transport does not accept HTTP Range headers",
        );
    }

    if request.uri().query() == Some("full=1") {
        // Small PDFs are intentionally one-shot reads. Do not create or retain
        // a PdfReadSession for data that is immediately transferred in full to
        // the PDF.js worker; the OS handle remains request-scoped and leased.
        release_pdf_read_session(attachment_id);
        let record = match pdf_attachment(library, attachment_id) {
            Ok(record) => record,
            Err(error) => return pdf_protocol_error(error.status, error.message),
        };
        let file = match PdfReadHandle::open(&record) {
            Ok(file) => file,
            Err(_) => return pdf_protocol_error(404, "PDF file missing"),
        };
        let file_len = match file.file.metadata() {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            _ => return pdf_protocol_error(404, "PDF file missing"),
        };
        if parse_pdf_document_read(request.uri().query(), file_len) != Some(PdfDocumentRead::Whole)
        {
            return pdf_protocol_error(400, "Invalid PDF local read request");
        }
        let Ok(capacity) = usize::try_from(file_len) else {
            return pdf_protocol_error(413, "PDF local read is too large");
        };
        let mut bytes = Vec::with_capacity(capacity);
        let mut bounded = (&file.file).take(file_len.saturating_add(1));
        if bounded.read_to_end(&mut bytes).is_err() || bytes.len() != capacity {
            return pdf_protocol_error(500, "Could not read PDF attachment");
        }
        return pdf_document_response(Cow::Owned(bytes));
    }

    // Re-authorize every read against the current attachment/vault state before
    // consulting cached session metadata. File handles themselves are always
    // request-scoped, so no stale descriptor can survive Lock or Encrypt.
    let record = match pdf_attachment(library, attachment_id) {
        Ok(record) => record,
        Err(error) => {
            release_pdf_read_session(attachment_id);
            return pdf_protocol_error(error.status, error.message);
        }
    };
    let file_len = match pdf_read_session(attachment_id, &record.path) {
        Ok(file_len) => file_len,
        Err(_) => return pdf_protocol_error(404, "PDF file missing"),
    };
    let Some(PdfDocumentRead::Range { offset, length }) =
        parse_pdf_document_read(request.uri().query(), file_len)
    else {
        return pdf_protocol_error(400, "Invalid PDF local read request");
    };
    let Ok(length) = usize::try_from(length) else {
        return pdf_protocol_error(413, "PDF local read is too large");
    };
    let file = match PdfReadHandle::open(&record) {
        Ok(file) => file,
        Err(_) => return pdf_protocol_error(404, "PDF file missing"),
    };
    let mut bytes = vec![0_u8; length];
    if read_pdf_exact_at(&file, offset, &mut bytes).is_err() {
        return pdf_protocol_error(500, "Could not read PDF attachment");
    }
    pdf_document_response(Cow::Owned(bytes))
}

fn pdf_document_response(body: Cow<'static, [u8]>) -> Response<Cow<'static, [u8]>> {
    let content_length = body.len();
    Response::builder()
        .status(200)
        .header("Content-Type", "application/pdf")
        .header("Content-Length", content_length.to_string())
        .header("Cache-Control", "private, no-store")
        .header("Content-Security-Policy", PDF_RESOURCE_CSP)
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap_or_else(|_| pdf_protocol_error(500, "Could not serve PDF data"))
}

fn serve_file(
    request: &Request<Vec<u8>>,
    path: &Path,
    content_type: &str,
    cache_control: &str,
    policy: FileServePolicy,
) -> Response<Cow<'static, [u8]>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return protocol_error(404, "Attachment file missing"),
    };
    let file_len = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => return protocol_error(404, "Attachment metadata unavailable"),
    };
    if request.method().as_str() == "HEAD" {
        return file_response(
            200,
            content_type,
            cache_control,
            file_len,
            None,
            Cow::Borrowed(EMPTY_BYTES),
        );
    }

    if let Some(range_header) = request
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok())
    {
        let Some(requested) = parse_single_byte_range(range_header, file_len) else {
            return range_not_satisfiable(file_len, cache_control);
        };
        let requested_length = requested
            .end
            .saturating_sub(requested.start)
            .saturating_add(1);
        let (start, end) = if requested_length > policy.max_range_bytes {
            if !policy.clamp_oversized_ranges {
                return range_not_satisfiable(file_len, cache_control);
            }
            if requested.clamp_from_end {
                (
                    requested
                        .end
                        .saturating_add(1)
                        .saturating_sub(policy.max_range_bytes),
                    requested.end,
                )
            } else {
                (
                    requested.start,
                    requested
                        .start
                        .saturating_add(policy.max_range_bytes.saturating_sub(1))
                        .min(requested.end),
                )
            }
        } else {
            (requested.start, requested.end)
        };
        let length_u64 = end.saturating_sub(start).saturating_add(1);
        let Ok(length) = usize::try_from(length_u64) else {
            return protocol_error(413, "Requested attachment range is too large");
        };
        if file.seek(SeekFrom::Start(start)).is_err() {
            return protocol_error(500, "Could not seek attachment");
        }
        let mut bytes = vec![0_u8; length];
        if file.read_exact(&mut bytes).is_err() {
            return protocol_error(500, "Could not read attachment range");
        }
        return file_response(
            206,
            content_type,
            cache_control,
            file_len,
            Some((start, end)),
            Cow::Owned(bytes),
        );
    }

    if policy.require_range {
        return protocol_error(400, "Attachment preview requires a bounded byte range");
    }
    if policy
        .max_full_bytes
        .is_some_and(|maximum| file_len > maximum)
    {
        return protocol_error(413, "Attachment is too large for automatic inline preview");
    }
    let Ok(capacity) = usize::try_from(file_len) else {
        return protocol_error(413, "Attachment is too large for automatic inline preview");
    };
    let mut bytes = Vec::with_capacity(capacity);
    let mut bounded = file.take(file_len.saturating_add(1));
    if bounded.read_to_end(&mut bytes).is_err() || bytes.len() != capacity {
        return protocol_error(500, "Could not read attachment");
    }
    file_response(
        200,
        content_type,
        cache_control,
        file_len,
        None,
        Cow::Owned(bytes),
    )
}

fn range_not_satisfiable(file_len: u64, cache_control: &str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(416)
        .header("Content-Range", format!("bytes */{file_len}"))
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", cache_control)
        .header("Content-Security-Policy", ATTACHMENT_PREVIEW_CSP)
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Borrowed(EMPTY_BYTES))
        .unwrap_or_else(|_| protocol_error(500, "Could not serve attachment"))
}

fn file_response(
    status: u16,
    content_type: &str,
    cache_control: &str,
    file_len: u64,
    range: Option<(u64, u64)>,
    body: Cow<'static, [u8]>,
) -> Response<Cow<'static, [u8]>> {
    let content_length = range
        .map(|(start, end)| end.saturating_sub(start).saturating_add(1))
        .unwrap_or(file_len);
    let mut builder = Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Content-Length", content_length.to_string())
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", cache_control)
        .header("Content-Security-Policy", ATTACHMENT_PREVIEW_CSP)
        .header("X-Content-Type-Options", "nosniff");
    if let Some((start, end)) = range {
        builder = builder.header("Content-Range", format!("bytes {start}-{end}/{file_len}"));
    }
    builder
        .body(body)
        .unwrap_or_else(|_| protocol_error(500, "Could not serve attachment"))
}

fn pdf_module_response(method: &str, module: &'static [u8]) -> Response<Cow<'static, [u8]>> {
    pdf_response(
        200,
        "text/javascript; charset=utf-8",
        if method == "HEAD" {
            Cow::Borrowed(EMPTY_BYTES)
        } else {
            Cow::Borrowed(module)
        },
    )
}

fn pdf_response(
    status: u16,
    content_type: &str,
    body: Cow<'static, [u8]>,
) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Cache-Control", "no-store")
        .header("Content-Security-Policy", PDF_RESOURCE_CSP)
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap_or_else(|_| protocol_error(500, "Could not serve PDF viewer"))
}

fn parse_single_byte_range(header: &str, file_len: u64) -> Option<ParsedByteRange> {
    if file_len == 0 {
        return None;
    }
    let value = header.strip_prefix("bytes=")?;
    if value.contains(',') {
        return None;
    }
    let (start_text, end_text) = value.split_once('-')?;
    match (start_text.trim(), end_text.trim()) {
        ("", "") => None,
        ("", suffix_text) => {
            let suffix = suffix_text.parse::<u64>().ok()?;
            if suffix == 0 {
                return None;
            }
            let length = suffix.min(file_len);
            Some(ParsedByteRange {
                start: file_len - length,
                end: file_len - 1,
                clamp_from_end: true,
            })
        }
        (start_text, "") => {
            let start = start_text.parse::<u64>().ok()?;
            (start < file_len).then_some(ParsedByteRange {
                start,
                end: file_len - 1,
                clamp_from_end: false,
            })
        }
        (start_text, end_text) => {
            let start = start_text.parse::<u64>().ok()?;
            let requested_end = end_text.parse::<u64>().ok()?;
            if start > requested_end || start >= file_len {
                return None;
            }
            Some(ParsedByteRange {
                start,
                end: requested_end.min(file_len - 1),
                clamp_from_end: false,
            })
        }
    }
}

fn attachment_asset_route_path(path: &str) -> &str {
    path.strip_prefix("/rustnoteattachment").unwrap_or(path)
}

fn attachment_preview_route(path: &str) -> Option<(&str, AttachmentPreviewVariant)> {
    let routed_path = attachment_asset_route_path(path);
    let mut parts = routed_path.strip_prefix('/')?.split('/');
    let attachment_id = parts.next()?;
    if !valid_attachment_id(attachment_id) {
        return None;
    }
    let resource = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if resource == "compatible" {
        return Some((attachment_id, AttachmentPreviewVariant::CompatibleImage));
    }
    if resource == "screen" {
        return Some((attachment_id, AttachmentPreviewVariant::ScreenImage));
    }
    (resource == "preview" || resource.starts_with("preview."))
        .then_some((attachment_id, AttachmentPreviewVariant::Original))
}

fn valid_attachment_id(value: &str) -> bool {
    value.starts_with("att_")
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn protocol_error(status: u16, message: &'static str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Borrowed(message.as_bytes()))
        .expect("static custom-protocol response must be valid")
}

#[cfg(test)]
mod pdf_transport_tests {
    use super::*;

    #[test]
    fn pdf_read_sessions_are_reserved_for_large_files() {
        assert!(!pdf_uses_read_session(PDF_WHOLE_FILE_MAX_BYTES));
        assert!(pdf_uses_read_session(PDF_WHOLE_FILE_MAX_BYTES + 1));
    }

    #[test]
    fn pdf_session_cache_never_retains_an_open_file_handle() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustnote-pdf-session-metadata-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("document.pdf");
        let bytes = b"%PDF-1.4\nrequest scoped handle\n%%EOF\n";
        std::fs::write(&path, bytes).expect("write pdf");

        let file_len =
            create_pdf_read_session("att_pdf_session_metadata", &path).expect("cache PDF metadata");
        assert_eq!(file_len, bytes.len() as u64);
        std::fs::remove_file(&path).expect("unlink pdf");
        assert_eq!(
            cached_pdf_read_session("att_pdf_session_metadata", Some(&path)),
            Some(file_len),
            "session cache may retain metadata but never an OS file handle"
        );
        assert!(
            File::open(&path).is_err(),
            "a new PDF request must open a fresh handle and therefore observe the unlink"
        );

        release_pdf_read_session("att_pdf_session_metadata");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "internal-test-support")]
    #[test]
    fn protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops() {
        use std::{sync::mpsc, thread};

        let (storage, transport, resource, note_id, _attachment_id, root) =
            rustnote::storage::protected_transport_test_fixture(
                "protected-pdf-annotation-response-lease",
                "source.pdf",
                Some("application/pdf"),
                b"%PDF-1.4\nprotected annotation response lifetime\n%%EOF\n",
            );
        assert!(resource.protected);
        let record = managed_attachment_from_resource(resource);

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let storage_for_lock = storage.clone();
        let note_for_lock = note_id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            let result =
                rustnote::storage::lock_protected_note_for_test(&storage_for_lock, &note_for_lock);
            done_tx.send(result).expect("signal lock finish");
        });

        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !rustnote::storage::protected_note_accepts_vault_reads_for_test(&storage, &note_id)
                .expect("inspect protected-note Vault read state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish PDF revocation while the response guard is alive"
        );
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must wait while a protected annotation response retains its vault lease"
        );

        drop(record);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("lock should finish after annotation response guard release")
            .expect("lock result");
        lock_thread.join().expect("join lock thread");

        drop(transport);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "internal-test-support")]
    #[test]
    fn protected_pdf_file_handle_keeps_lock_waiting_until_descriptor_drops() {
        use std::{sync::mpsc, thread};

        let (storage, transport, resource, note_id, _attachment_id, root) =
            rustnote::storage::protected_transport_test_fixture(
                "protected-pdf-handle-lease",
                "source.pdf",
                Some("application/pdf"),
                b"%PDF-1.4\nprotected descriptor lifetime\n%%EOF\n",
            );
        assert!(resource.protected);
        let record = managed_attachment_from_resource(resource);
        let file = PdfReadHandle::open(&record).expect("open protected PDF handle");
        drop(record);

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let storage_for_lock = storage.clone();
        let note_for_lock = note_id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            let result =
                rustnote::storage::lock_protected_note_for_test(&storage_for_lock, &note_for_lock);
            done_tx.send(result).expect("signal lock finish");
        });

        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !rustnote::storage::protected_note_accepts_vault_reads_for_test(&storage, &note_id)
                .expect("inspect protected-note Vault read state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish PDF revocation promptly"
        );
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must wait while the PDF OS handle retains its vault lease"
        );

        drop(file);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("lock should finish after PDF handle release")
            .expect("lock result");
        lock_thread.join().expect("join lock thread");

        drop(transport);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pdf_runtime_transport_contract_matches_viewer_plan() {
        let large_file = PDF_WHOLE_FILE_MAX_BYTES + PDF_TRANSPORT_BLOCK_BYTES + 123;

        let initial_query = format!("offset=0&length={PDF_TRANSPORT_BLOCK_BYTES}");
        assert_eq!(
            parse_pdf_document_read(Some(&initial_query), large_file),
            Some(PdfDocumentRead::Range {
                offset: 0,
                length: PDF_TRANSPORT_BLOCK_BYTES,
            })
        );

        let tail_offset = large_file - 123;
        let tail_query = format!("offset={tail_offset}&length=123");
        assert_eq!(
            parse_pdf_document_read(Some(&tail_query), large_file),
            Some(PdfDocumentRead::Range {
                offset: tail_offset,
                length: 123,
            })
        );
    }

    #[test]
    fn pdf_document_read_contract_is_bounded() {
        let small_file = PDF_WHOLE_FILE_MAX_BYTES;
        assert_eq!(
            parse_pdf_document_read(Some("full=1"), small_file),
            Some(PdfDocumentRead::Whole)
        );
        assert_eq!(
            parse_pdf_document_read(Some("full=1"), small_file + 1),
            None
        );

        let large_file = 128 * 1024 * 1024;
        assert_eq!(
            parse_pdf_document_read(Some("offset=1048576&length=1048576"), large_file),
            Some(PdfDocumentRead::Range {
                offset: 1024 * 1024,
                length: 1024 * 1024,
            })
        );
        assert_eq!(
            parse_pdf_document_read(Some("offset=0&length=2097153"), large_file,),
            None
        );
        assert_eq!(
            parse_pdf_document_read(Some("offset=0&length=0"), large_file),
            None
        );
        assert_eq!(
            parse_pdf_document_read(Some("offset=134217728&length=1"), large_file),
            None
        );
        assert_eq!(
            parse_pdf_document_read(Some("offset=0&length=1&extra=1"), large_file),
            None
        );
    }
}
