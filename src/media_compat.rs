//! App-facing Rust-native media compatibility for containers the host player cannot use directly.
//!
//! Native playback remains first. Compatibility generation is a background job owned by this
//! module; the loopback HTTP server only serves already-completed derived renditions. Derived media
//! is disposable, bounded by a soft cache budget, and never replaces the canonical attachment.

use std::{
    collections::HashMap,
    fs::{self, File, FileTimes, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::storage::{AttachmentTransportResource, AttachmentTransportStore};

use rustnote_media_compat::{CompatibilityCandidate, CompatibilityProbe, CompatibilityProfile};

const CACHE_VERSION: &str = "planner-v2";
const CACHE_SOFT_BUDGET_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_PENDING_JOBS: usize = 8;
const MAX_MP4_TOP_LEVEL_BOXES: usize = 4096;
const CACHE_TOUCH_INTERVAL: Duration = Duration::from_secs(60);
const INVALID_MARKER_SUFFIX: &str = ".invalid";
const CACHE_OWNER_MARKER: &str = ".media-compat.plaintext-owner";
const CACHE_OWNER_IDENTITY: &str = "media-compat-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompatibilityStatus {
    Unsupported,
    Absent,
    Queued,
    Running,
    Ready,
    Failed,
}

impl CompatibilityStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Absent => "absent",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedCompatibility {
    pub(crate) path: PathBuf,
    pub(crate) content_type: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) enum MediaCompatibilityError {
    Io(Arc<io::Error>),
    QueueFull,
    WorkerUnavailable,
    WorkerStopped,
}

impl std::fmt::Display for MediaCompatibilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, formatter),
            Self::QueueFull => formatter.write_str("media compatibility queue is full"),
            Self::WorkerUnavailable => {
                formatter.write_str("media compatibility worker is unavailable")
            }
            Self::WorkerStopped => {
                formatter.write_str("media compatibility worker stopped before returning a result")
            }
        }
    }
}

impl std::error::Error for MediaCompatibilityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error.as_ref()),
            Self::QueueFull | Self::WorkerUnavailable | Self::WorkerStopped => None,
        }
    }
}

impl From<io::Error> for MediaCompatibilityError {
    fn from(error: io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

type MediaCompatibilityResult<T> = Result<T, MediaCompatibilityError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobPhase {
    Queued,
    Running,
}

type CompatibilityResult = MediaCompatibilityResult<PreparedCompatibility>;
type CompatibilityWaiter = mpsc::Sender<CompatibilityResult>;

struct JobEntry {
    phase: JobPhase,
    waiters: Vec<CompatibilityWaiter>,
}

struct JobRequest {
    key: String,
    cache_directory: PathBuf,
    resource: AttachmentTransportResource,
    profile: CompatibilityProfile,
    // `resource` owns the compound plaintext/Vault authorization for the
    // complete background lifetime, even if every waiting caller disconnects.
}

struct CompatibilityJobManager {
    jobs: Mutex<HashMap<String, JobEntry>>,
    failures: Mutex<HashMap<String, MediaCompatibilityError>>,
    sender: SyncSender<JobRequest>,
}

static JOB_MANAGER: OnceLock<Arc<CompatibilityJobManager>> = OnceLock::new();
static ACTIVE_CACHE_STREAMS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
static CACHE_TOUCHES: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();

pub(crate) struct CacheUseGuard {
    path: PathBuf,
}

impl Drop for CacheUseGuard {
    fn drop(&mut self) {
        let registry = ACTIVE_CACHE_STREAMS.get_or_init(|| Mutex::new(HashMap::new()));
        let became_unused = {
            let Ok(mut active) = registry.lock() else {
                return;
            };
            if let Some(count) = active.get_mut(&self.path) {
                if *count <= 1 {
                    active.remove(&self.path);
                    true
                } else {
                    *count -= 1;
                    false
                }
            } else {
                false
            }
        };
        if became_unused && invalid_marker_path(&self.path).is_file() {
            let _ = remove_invalidated_cache_file(&self.path);
        }
    }
}

pub(crate) fn is_compatibility_candidate(file_name: &str, mime_type: Option<&str>) -> bool {
    let mime = mime_type.unwrap_or_default().trim();
    if matches_ignore_ascii_case(
        mime,
        &[
            "video/matroska",
            "video/x-matroska",
            "audio/matroska",
            "audio/x-matroska",
        ],
    ) {
        return true;
    }

    Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("mkv") || extension.eq_ignore_ascii_case("mka")
        })
}

pub(crate) fn probe_path(path: &Path) -> io::Result<CompatibilityProbe> {
    rustnote_media_compat::probe_matroska(path)
}

pub(crate) fn parse_profile(value: &str) -> Option<CompatibilityProfile> {
    CompatibilityProfile::parse(value)
}

pub(crate) fn profile_extension(profile: CompatibilityProfile) -> &'static str {
    profile.extension()
}

pub(crate) fn profile_mime_type(profile: CompatibilityProfile) -> &'static str {
    profile.base_mime_type()
}

pub(crate) fn candidate_for_profile(
    probe: &CompatibilityProbe,
    profile: CompatibilityProfile,
) -> Option<&CompatibilityCandidate> {
    probe
        .candidates
        .iter()
        .find(|candidate| candidate.profile == profile)
}

pub(crate) fn probe_attachment(
    transport: &AttachmentTransportStore,
    cache_directory: &Path,
    attachment_id: &str,
    profile: CompatibilityProfile,
) -> MediaCompatibilityResult<CompatibilityStatus> {
    let Some(resource) = active_resource(transport, attachment_id)? else {
        return Ok(CompatibilityStatus::Unsupported);
    };
    Ok(probe_resource(cache_directory, &resource, profile)?)
}

pub(crate) fn prepare_attachment(
    transport: &AttachmentTransportStore,
    cache_directory: &Path,
    attachment_id: &str,
    profile: CompatibilityProfile,
) -> MediaCompatibilityResult<PreparedCompatibility> {
    let Some(resource) = active_resource(transport, attachment_id)? else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "media attachment not found").into());
    };
    prepare_resource(cache_directory, resource, profile)
}

pub(crate) fn invalidate_attachment(
    transport: &AttachmentTransportStore,
    cache_directory: &Path,
    attachment_id: &str,
    profile: CompatibilityProfile,
) -> MediaCompatibilityResult<bool> {
    let Some(resource) = active_resource(transport, attachment_id)? else {
        return Ok(false);
    };
    Ok(invalidate_resource(cache_directory, &resource, profile)?)
}

fn cached_resource(
    cache_directory: &Path,
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<Option<PreparedCompatibility>> {
    if resource.protected {
        return Ok(None);
    }
    if !is_compatibility_candidate(&resource.file_name, resource.mime_type.as_deref()) {
        return Ok(None);
    }
    let _owner = media_plaintext_namespace(cache_directory)?;
    let extension = profile_extension(profile);
    let destination = cache_directory.join(cache_file_name(&resource.content_hash, profile));
    if invalid_marker_path(&destination).is_file()
        || !valid_cached_preview(&destination, extension)?
    {
        return Ok(None);
    }
    Ok(Some(PreparedCompatibility {
        path: destination,
        content_type: profile_mime_type(profile),
    }))
}

pub(crate) fn acquire_cached_resource(
    cache_directory: &Path,
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<Option<(PreparedCompatibility, CacheUseGuard)>> {
    if resource.protected {
        return Ok(None);
    }
    if !is_compatibility_candidate(&resource.file_name, resource.mime_type.as_deref()) {
        return Ok(None);
    }
    let _owner = media_plaintext_namespace(cache_directory)?;
    let extension = profile_extension(profile);
    let destination = cache_directory.join(cache_file_name(&resource.content_hash, profile));
    let guard = begin_cache_use(&destination);
    if invalid_marker_path(&destination).is_file()
        || !valid_cached_preview(&destination, extension)?
    {
        drop(guard);
        return Ok(None);
    }
    Ok(Some((
        PreparedCompatibility {
            path: destination,
            content_type: profile_mime_type(profile),
        },
        guard,
    )))
}

fn begin_cache_use(path: &Path) -> CacheUseGuard {
    let path = path.to_path_buf();
    let registry = ACTIVE_CACHE_STREAMS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut active) = registry.lock() {
        *active.entry(path.clone()).or_insert(0) += 1;
    }
    let _ = touch_cache_path(&path);
    CacheUseGuard { path }
}

pub fn cleanup_stale_temporary_files(cache_directory: &Path) -> io::Result<()> {
    let _owner = media_plaintext_namespace(cache_directory)?;
    let entries = match fs::read_dir(cache_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.ends_with(INVALID_MARKER_SUFFIX) {
            let marker = entry.path();
            let removable =
                cache_path_from_invalid_marker(&marker).is_none_or(
                    |target| match remove_cache_file(&target) {
                        Ok(_) => true,
                        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                        Err(_) => false,
                    },
                );
            if removable {
                let _ = remove_cache_marker(&marker);
            }
            continue;
        }
        if !name.starts_with('.') || !name.ends_with(".tmp") {
            continue;
        }
        let stale = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_TEMP_AGE);
        if stale {
            let _ = remove_cache_file(&entry.path());
        }
    }
    Ok(())
}

pub fn prune_cache(cache_directory: &Path) -> io::Result<()> {
    prune_cache_preserving(cache_directory, None)
}

/// Remove every Rustnote-managed media derivative for one plaintext content hash.
///
/// Vault finalization uses this strict variant so a failed deletion cannot be
/// silently converted into a successful protection transition. The note-level
/// plaintext-derivative barrier guarantees that no producer for the transitioning
/// note can be publishing a new cache entry while this scrub runs.
pub(crate) fn discard_cache_strict(cache_directory: &Path, content_hash: &str) -> io::Result<()> {
    let _owner = media_plaintext_namespace(cache_directory)?;
    if !valid_content_hash(content_hash) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid media compatibility cache identity",
        ));
    }
    if let Some(manager) = JOB_MANAGER.get()
        && let Ok(mut failures) = manager.failures.lock()
    {
        let prefix = format!("{content_hash}:");
        failures.retain(|key, _| !key.starts_with(&prefix));
    }

    let entries = match fs::read_dir(cache_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let published_prefix = format!("{content_hash}.");
    let temporary_prefix = format!(".{content_hash}.");
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_published = name.starts_with(&published_prefix);
        let is_temporary = name.starts_with(&temporary_prefix) && name.ends_with(".tmp");
        let is_invalid_marker = name.starts_with('.')
            && name.ends_with(INVALID_MARKER_SUFFIX)
            && cache_path_from_invalid_marker(&path).is_some_and(|target| {
                target
                    .file_name()
                    .is_some_and(|value| value.to_string_lossy().starts_with(&published_prefix))
            });
        if !(is_published || is_temporary || is_invalid_marker) {
            continue;
        }
        if is_published {
            let _ = invalidate_cache_file(&path)?;
        } else {
            match remove_cache_file(&path) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }

    // `invalidate_cache_file` may decline to unlink a file that is still in use.
    // Vault privacy cleanup is fail-closed: any matching artifact that survives
    // the pass keeps the protection journal pending for startup recovery.
    if let Ok(entries) = fs::read_dir(cache_directory) {
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let matches = name.starts_with(&published_prefix)
                || (name.starts_with(&temporary_prefix) && name.ends_with(".tmp"))
                || (name.starts_with('.')
                    && name.ends_with(INVALID_MARKER_SUFFIX)
                    && cache_path_from_invalid_marker(&path).is_some_and(|target| {
                        target.file_name().is_some_and(|value| {
                            value.to_string_lossy().starts_with(&published_prefix)
                        })
                    }));
            if matches {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "media compatibility plaintext cache could not be fully removed",
                ));
            }
        }
    }
    Ok(())
}

fn active_resource(
    transport: &AttachmentTransportStore,
    attachment_id: &str,
) -> io::Result<Option<AttachmentTransportResource>> {
    transport
        .attachment_resource(attachment_id)
        .map_err(|error| io::Error::other(error.to_string()))
}

fn probe_resource(
    cache_directory: &Path,
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<CompatibilityStatus> {
    if resource.protected {
        return Ok(CompatibilityStatus::Unsupported);
    }
    if !is_compatibility_candidate(&resource.file_name, resource.mime_type.as_deref()) {
        return Ok(CompatibilityStatus::Unsupported);
    }
    let compatibility_probe = probe_path(&resource.path)?;
    if candidate_for_profile(&compatibility_probe, profile).is_none() {
        return Ok(CompatibilityStatus::Unsupported);
    }
    if cached_resource(cache_directory, resource, profile)?.is_some() {
        return Ok(CompatibilityStatus::Ready);
    }
    let key = job_key(resource, profile)?;
    let Some(manager) = JOB_MANAGER.get() else {
        return Ok(CompatibilityStatus::Absent);
    };
    if manager
        .failures
        .lock()
        .map_err(|_| io::Error::other("media compatibility failure registry is poisoned"))?
        .contains_key(&key)
    {
        return Ok(CompatibilityStatus::Failed);
    }
    let jobs = manager
        .jobs
        .lock()
        .map_err(|_| io::Error::other("media compatibility job registry is poisoned"))?;
    Ok(match jobs.get(&key).map(|entry| entry.phase) {
        Some(JobPhase::Queued) => CompatibilityStatus::Queued,
        Some(JobPhase::Running) => CompatibilityStatus::Running,
        None => CompatibilityStatus::Absent,
    })
}

fn prepare_resource(
    cache_directory: &Path,
    resource: AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> MediaCompatibilityResult<PreparedCompatibility> {
    if resource.protected {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "persistent media compatibility caching is disabled for encrypted notes",
        )
        .into());
    }
    if let Some(ready) = cached_resource(cache_directory, &resource, profile)? {
        let _ = touch_cache_path(&ready.path);
        return Ok(ready);
    }
    if !is_compatibility_candidate(&resource.file_name, resource.mime_type.as_deref()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no lossless compatibility profile is available for this media",
        )
        .into());
    }
    ensure_profile_supported(&resource, profile)?;

    let manager = job_manager();
    let key = job_key(&resource, profile)?;
    if let Some(error) = manager
        .failures
        .lock()
        .map_err(|_| io::Error::other("media compatibility failure registry is poisoned"))?
        .get(&key)
        .cloned()
    {
        return Err(error);
    }
    let (result_sender, result_receiver) = mpsc::channel();
    let should_enqueue = {
        let mut jobs = manager
            .jobs
            .lock()
            .map_err(|_| io::Error::other("media compatibility job registry is poisoned"))?;
        if let Some(entry) = jobs.get_mut(&key) {
            entry.waiters.push(result_sender);
            false
        } else {
            jobs.insert(
                key.clone(),
                JobEntry {
                    phase: JobPhase::Queued,
                    waiters: vec![result_sender],
                },
            );
            true
        }
    };

    if should_enqueue {
        match manager.sender.try_send(JobRequest {
            key: key.clone(),
            cache_directory: cache_directory.to_path_buf(),
            resource,
            profile,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                fail_queued_job(&manager, &key, MediaCompatibilityError::QueueFull);
            }
            Err(TrySendError::Disconnected(_)) => {
                fail_queued_job(&manager, &key, MediaCompatibilityError::WorkerUnavailable);
            }
        }
    }

    match result_receiver.recv() {
        Ok(result) => result,
        Err(_) => Err(MediaCompatibilityError::WorkerStopped),
    }
}

fn job_manager() -> Arc<CompatibilityJobManager> {
    JOB_MANAGER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel(MAX_PENDING_JOBS);
            let manager = Arc::new(CompatibilityJobManager {
                jobs: Mutex::new(HashMap::new()),
                failures: Mutex::new(HashMap::new()),
                sender,
            });
            let worker_manager = Arc::clone(&manager);
            let _ = thread::Builder::new()
                .name("rustnote-media-compat".to_string())
                .spawn(move || compatibility_worker(worker_manager, receiver));
            manager
        })
        .clone()
}

fn compatibility_worker(manager: Arc<CompatibilityJobManager>, receiver: Receiver<JobRequest>) {
    while let Ok(job) = receiver.recv() {
        if let Ok(mut jobs) = manager.jobs.lock()
            && let Some(entry) = jobs.get_mut(&job.key)
        {
            entry.phase = JobPhase::Running;
        }

        let result = generate_compatibility(&job.cache_directory, &job.resource, job.profile)
            .map_err(MediaCompatibilityError::from);
        if let Ok(mut failures) = manager.failures.lock() {
            match &result {
                Ok(_) => {
                    failures.remove(&job.key);
                }
                Err(error) => {
                    failures.insert(job.key.clone(), error.clone());
                }
            }
        }
        let waiters = manager
            .jobs
            .lock()
            .ok()
            .and_then(|mut jobs| jobs.remove(&job.key))
            .map(|entry| entry.waiters)
            .unwrap_or_default();
        for waiter in waiters {
            let _ = waiter.send(result.clone());
        }
    }
}

fn fail_queued_job(manager: &CompatibilityJobManager, key: &str, error: MediaCompatibilityError) {
    let waiters = manager
        .jobs
        .lock()
        .ok()
        .and_then(|mut jobs| jobs.remove(key))
        .map(|entry| entry.waiters)
        .unwrap_or_default();
    for waiter in waiters {
        let _ = waiter.send(Err(error.clone()));
    }
}

fn ensure_profile_supported(
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<()> {
    let probe = probe_path(&resource.path)?;
    if candidate_for_profile(&probe, profile).is_some() {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "requested lossless compatibility profile is not available for this media",
    ))
}

fn generate_compatibility(
    cache_directory: &Path,
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<PreparedCompatibility> {
    if let Some(ready) = cached_resource(cache_directory, resource, profile)? {
        return Ok(ready);
    }
    ensure_profile_supported(resource, profile)?;
    let content_type = profile_mime_type(profile);
    let extension = profile_extension(profile);

    let owner = media_plaintext_namespace(cache_directory)?;
    fs::create_dir_all(cache_directory)?;
    let destination = cache_directory.join(cache_file_name(&resource.content_hash, profile));
    let invalid_marker = invalid_marker_path(&destination);
    if invalid_marker.is_file()
        || (destination.is_file() && !valid_cached_preview(&destination, extension)?)
    {
        if cache_path_in_use(&destination) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "invalid compatibility rendition is still in use",
            ));
        }
        let _ = remove_cache_file(&destination);
        let _ = remove_cache_marker(&invalid_marker);
    }
    let source_bytes = fs::metadata(&resource.path)?.len();
    prune_cache_for_incoming(cache_directory, Some(&destination), source_bytes)?;
    let temporary = cache_directory.join(format!(
        ".{}.{}.{}.{}.tmp",
        resource.content_hash,
        CACHE_VERSION,
        profile.as_str(),
        rand::random::<u64>()
    ));
    if fs::symlink_metadata(&temporary).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "media compatibility staging collision",
        ));
    }
    let mut temporary_owner = owner.reserve(temporary.clone())?;

    let started = Instant::now();
    if let Err(error) = rustnote_media_compat::remux_matroska(&resource.path, &temporary, profile) {
        let _ = temporary_owner.remove_file();
        return Err(error);
    }
    if let Err(error) = temporary_owner.verify_materialized() {
        let _ = temporary_owner.remove_file();
        return Err(error);
    }
    if !valid_cached_preview(&temporary, extension)? {
        let _ = temporary_owner.remove_file();
        return Err(invalid_data(
            "compatibility remux produced an invalid derived media file",
        ));
    }

    match temporary_owner.rename_durable_namespace(destination.clone()) {
        Ok(()) => {}
        Err(error) if destination.is_file() => {
            let _ = temporary_owner.remove_file();
            if !valid_cached_preview(&destination, extension)? {
                return Err(error);
            }
        }
        Err(error) => {
            let _ = temporary_owner.remove_file();
            return Err(error);
        }
    }
    let _ = remove_cache_marker(&invalid_marker);
    let _ = touch_cache_path(&destination);
    prune_cache_preserving(cache_directory, Some(&destination))?;

    if source_bytes >= 256 * 1024 * 1024 {
        eprintln!(
            "[rustnote][media-compat] {:.1} MiB profile={} lossless-remux={:.2}s",
            source_bytes as f64 / (1024.0 * 1024.0),
            profile.as_str(),
            started.elapsed().as_secs_f64(),
        );
    }

    Ok(PreparedCompatibility {
        path: destination,
        content_type,
    })
}

fn invalidate_resource(
    cache_directory: &Path,
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<bool> {
    if let Ok(key) = job_key(resource, profile)
        && let Some(manager) = JOB_MANAGER.get()
        && let Ok(mut failures) = manager.failures.lock()
    {
        failures.remove(&key);
    }
    let path = cache_directory.join(cache_file_name(&resource.content_hash, profile));
    invalidate_cache_file(&path)
}

fn invalidate_cache_file(path: &Path) -> io::Result<bool> {
    let marker = invalid_marker_path(path);
    ensure_cache_marker(&marker)?;
    match remove_cache_file_if_unused(path) {
        Ok(Some(removed)) => {
            let _ = remove_cache_marker(&marker);
            Ok(removed)
        }
        Ok(None) => {
            // The marker makes the rendition logically invalid immediately. CacheUseGuard removes
            // an in-flight stream's file after its final request releases the active-use guard.
            Ok(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock
            ) =>
        {
            // A platform may keep a file handle open slightly beyond our request. Preserve the
            // invalid marker so the cache cannot be reused; startup cleanup retries deletion.
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

fn remove_cache_file_if_unused(path: &Path) -> io::Result<Option<bool>> {
    let registry = ACTIVE_CACHE_STREAMS.get_or_init(|| Mutex::new(HashMap::new()));
    let active = registry
        .lock()
        .map_err(|_| io::Error::other("media compatibility active-stream registry is poisoned"))?;
    if active.get(path).copied().unwrap_or_default() > 0 {
        return Ok(None);
    }
    remove_cache_file(path).map(Some)
}

fn remove_invalidated_cache_file(path: &Path) -> io::Result<()> {
    match remove_cache_file(path) {
        Ok(_) => {
            let _ = remove_cache_marker(&invalid_marker_path(path));
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn ensure_cache_marker(marker: &Path) -> io::Result<()> {
    let cache_directory = marker.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "media cache marker has no parent",
        )
    })?;
    let owner = media_plaintext_namespace(cache_directory)?;
    fs::create_dir_all(cache_directory)?;
    match fs::symlink_metadata(marker) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "media cache invalidation marker is not a regular file",
            ));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut owned = owner.reserve(marker.to_path_buf())?;
    let mut file = owned.create_file()?;
    file.write_all(b"invalid\n")?;
    file.sync_all()
}

fn remove_cache_marker(marker: &Path) -> io::Result<bool> {
    remove_cache_file(marker)
}

fn remove_cache_file(path: &Path) -> io::Result<bool> {
    const RETRIES: usize = 4;
    let Some(cache_directory) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "media cache path has no parent",
        ));
    };
    let owner = media_plaintext_namespace(cache_directory)?;
    for attempt in 0..RETRIES {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                forget_cache_path(path);
                return Ok(false);
            }
            Err(error) => return Err(error),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "media compatibility cache path is not a regular file",
                ));
            }
            Ok(_) => {}
        }
        let mut owned = owner.adopt_existing(path.to_path_buf())?;
        match owned.remove_file() {
            Ok(()) => {
                forget_cache_path(path);
                return Ok(true);
            }
            Err(error)
                if attempt + 1 < RETRIES
                    && matches!(
                        error.kind(),
                        io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock
                    ) =>
            {
                thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

pub(crate) fn persistent_cache_owner(
    cache_directory: &Path,
) -> io::Result<crate::security::DurablePlaintextNamespace> {
    media_plaintext_namespace(cache_directory)
}

pub(crate) fn discard_all_cache_strict(cache_directory: &Path) -> io::Result<()> {
    let owner = media_plaintext_namespace(cache_directory)?;
    owner.remove_tree_strict()
}

fn media_plaintext_namespace(
    cache_directory: &Path,
) -> io::Result<crate::security::DurablePlaintextNamespace> {
    let parent = cache_directory.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "media compatibility cache has no parent",
        )
    })?;
    crate::security::DurablePlaintextNamespace::establish(
        cache_directory.to_path_buf(),
        parent.join(CACHE_OWNER_MARKER),
        CACHE_OWNER_IDENTITY,
    )
}

fn forget_cache_path(path: &Path) {
    if let Some(touches) = CACHE_TOUCHES.get()
        && let Ok(mut touches) = touches.lock()
    {
        touches.remove(path);
    }
}

fn job_key(
    resource: &AttachmentTransportResource,
    profile: CompatibilityProfile,
) -> io::Result<String> {
    if !is_compatibility_candidate(&resource.file_name, resource.mime_type.as_deref()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "media is not a compatibility candidate",
        ));
    }
    Ok(format!(
        "{}:{CACHE_VERSION}:{}",
        resource.content_hash,
        profile.as_str()
    ))
}

fn cache_file_name(content_hash: &str, profile: CompatibilityProfile) -> String {
    format!(
        "{content_hash}.{CACHE_VERSION}.{}.{}",
        profile.as_str(),
        profile.extension()
    )
}

fn invalid_marker_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("media-compat");
    path.with_file_name(format!(".{file_name}{INVALID_MARKER_SUFFIX}"))
}

fn cache_path_from_invalid_marker(marker: &Path) -> Option<PathBuf> {
    let name = marker.file_name()?.to_str()?;
    let cache_name = name
        .strip_prefix('.')?
        .strip_suffix(INVALID_MARKER_SUFFIX)?;
    if cache_name.is_empty() {
        return None;
    }
    Some(marker.with_file_name(cache_name))
}

fn valid_cached_preview(path: &Path, expected_extension: &str) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() < 32 {
        return Ok(false);
    }
    match expected_extension {
        "mp4" | "m4a" => validate_iso_bmff(path, metadata.len()),
        "webm" => rustnote_media_compat::validate_webm(path),
        "flac" => validate_file_prefix(path, b"fLaC"),
        "mp3" => validate_mp3_prefix(path),
        "ac3" | "eac3" => validate_file_prefix(path, &[0x0B, 0x77]),
        _ => Ok(false),
    }
}

fn validate_file_prefix(path: &Path, expected: &[u8]) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut prefix = vec![0_u8; expected.len()];
    if file.read_exact(&mut prefix).is_err() {
        return Ok(false);
    }
    Ok(prefix == expected)
}

fn validate_mp3_prefix(path: &Path) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; 4];
    if file.read_exact(&mut prefix).is_err() {
        return Ok(false);
    }
    Ok(prefix[0] == 0xFF && prefix[1] & 0xE0 == 0xE0)
}

fn validate_iso_bmff(path: &Path, file_len: u64) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut offset = 0_u64;
    let mut box_count = 0_usize;
    let mut saw_ftyp = false;
    let mut saw_moov = false;
    let mut saw_mdat = false;

    while offset < file_len && box_count < MAX_MP4_TOP_LEVEL_BOXES {
        if file_len.saturating_sub(offset) < 8 {
            return Ok(false);
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut header = [0_u8; 8];
        file.read_exact(&mut header)?;
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let box_type = &header[4..8];
        let (header_len, box_len) = if size32 == 1 {
            if file_len.saturating_sub(offset) < 16 {
                return Ok(false);
            }
            let mut extended = [0_u8; 8];
            file.read_exact(&mut extended)?;
            (16_u64, u64::from_be_bytes(extended))
        } else if size32 == 0 {
            (8_u64, file_len - offset)
        } else {
            (8_u64, u64::from(size32))
        };
        if box_len < header_len || box_len > file_len.saturating_sub(offset) {
            return Ok(false);
        }

        match box_type {
            b"ftyp" => {
                if box_len < header_len + 8 {
                    return Ok(false);
                }
                let mut brand = [0_u8; 4];
                file.read_exact(&mut brand)?;
                if brand != *b"isom" {
                    return Ok(false);
                }
                saw_ftyp = true;
            }
            b"moov" => saw_moov = true,
            b"mdat" => saw_mdat = true,
            _ => {}
        }

        offset = offset.saturating_add(box_len);
        box_count += 1;
    }

    Ok(offset == file_len && saw_ftyp && saw_moov && saw_mdat)
}

fn touch_cache_path(path: &Path) -> io::Result<()> {
    let touches = CACHE_TOUCHES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut last_used) = touches.lock() {
        if last_used
            .get(path)
            .is_some_and(|last_touch| last_touch.elapsed() < CACHE_TOUCH_INTERVAL)
        {
            return Ok(());
        }
        last_used.insert(path.to_path_buf(), Instant::now());
    }
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let now = SystemTime::now();
    file.set_times(FileTimes::new().set_accessed(now).set_modified(now))
}

fn prune_cache_for_incoming(
    cache_directory: &Path,
    preserve: Option<&Path>,
    estimated_bytes: u64,
) -> io::Result<()> {
    let target_existing_bytes =
        CACHE_SOFT_BUDGET_BYTES.saturating_sub(estimated_bytes.min(CACHE_SOFT_BUDGET_BYTES));
    prune_cache_to_limit(cache_directory, preserve, target_existing_bytes)
}

fn prune_cache_preserving(cache_directory: &Path, preserve: Option<&Path>) -> io::Result<()> {
    prune_cache_to_limit(cache_directory, preserve, CACHE_SOFT_BUDGET_BYTES)
}

fn prune_cache_to_limit(
    cache_directory: &Path,
    preserve: Option<&Path>,
    limit_bytes: u64,
) -> io::Result<()> {
    let entries = match fs::read_dir(cache_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    let mut total = 0_u64;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with('.') || !is_derived_cache_file_name(&file_name) {
            continue;
        }
        let path = entry.path();
        let size = metadata.len();
        total = total.saturating_add(size);
        candidates.push((metadata.modified().unwrap_or(UNIX_EPOCH), path, size));
    }
    if total <= limit_bytes {
        return Ok(());
    }

    candidates.sort_by_key(|(modified, _, _)| *modified);
    for (_, path, size) in candidates {
        if total <= limit_bytes {
            break;
        }
        if preserve.is_some_and(|preserved| preserved == path) {
            continue;
        }
        if let Ok(Some(_)) = remove_cache_file_if_unused(&path) {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

fn cache_path_in_use(path: &Path) -> bool {
    ACTIVE_CACHE_STREAMS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|active| active.get(path).copied())
        .is_some_and(|count| count > 0)
}

fn is_derived_cache_file_name(file_name: &str) -> bool {
    let Some((content_hash, _)) = file_name.split_once('.') else {
        return false;
    };
    valid_content_hash(content_hash)
        && matches!(
            Path::new(file_name)
                .extension()
                .and_then(|value| value.to_str()),
            Some("mp4" | "m4a" | "webm" | "flac" | "mp3" | "ac3" | "eac3")
        )
}

fn valid_content_hash(content_hash: &str) -> bool {
    content_hash.len() == 64 && content_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
