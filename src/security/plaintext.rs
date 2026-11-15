//! Typed Rustnote-managed plaintext ownership.
//!
//! `OwnedPlaintext` is proof that cleanup authority exists before a managed
//! plaintext path is created. Managed backends may be durable (SQLite ledger),
//! tied to a Vault read session, or process-private. Explicit user-selected
//! destinations are outside Rustnote cleanup ownership and therefore outside
//! `OwnedPlaintext`. Filesystem mutation of managed plaintext is centralized here.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::domain::NoteId;
use sha2::{Digest, Sha256};

use super::{VaultPlaintextPath, VaultReadPermit};

static OWNED_PLAINTEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaintextScope {
    Note(NoteId),
    Library,
    ProcessPrivate,
    ExternalOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaintextLifecycle {
    Reserved,
    Materialized,
    Transferred,
    Retiring,
    Retired,
}

/// Durable namespace ownership for recoverable plaintext families whose
/// lifecycle is naturally directory-scoped (for example emergency recovery
/// records and disposable media derivatives). The marker lives outside the
/// managed root, so partial tree deletion can never delete the authority before
/// the remaining plaintext.
#[derive(Clone, Debug)]
pub(crate) struct DurablePlaintextNamespace {
    identity: String,
    root: PathBuf,
    marker: PathBuf,
}

#[derive(Debug)]
struct DurableNamespaceOwnership {
    _identity: String,
    root: PathBuf,
    _marker: PathBuf,
}

#[derive(Debug)]
pub(crate) struct DurablePlaintextOwnership {
    pub(crate) stage_id: String,
    pub(crate) kind: String,
    pub(crate) note_id: Option<NoteId>,
    pub(crate) relative_path: String,
}

#[derive(Debug)]
struct DurableVaultOperationOwnership {
    _operation_id: String,
    _note_id: NoteId,
}

#[derive(Debug)]
struct DurableAttachmentOperationOwnership {
    _operation_id: String,
    _note_id: NoteId,
}

#[derive(Debug)]
struct DurableAttachmentIngestOwnership {
    _ingest_id: String,
    _note_id: NoteId,
}

#[derive(Debug)]
struct DurableExternalOutputOwnership {
    _operation_id: String,
}

#[derive(Debug)]
enum DurableManagedAuthority {
    Staging(DurablePlaintextOwnership),
    VaultOperation {
        _ownership: DurableVaultOperationOwnership,
    },
    AttachmentOperation {
        _ownership: DurableAttachmentOperationOwnership,
    },
    AttachmentIngest {
        _ownership: DurableAttachmentIngestOwnership,
    },
}

#[derive(Debug)]
enum PlaintextCleanupAuthority {
    DurableManaged(DurableManagedAuthority),
    DurableExternalOutput {
        _ownership: DurableExternalOutputOwnership,
    },
    DurableNamespace(DurableNamespaceOwnership),
    VaultSession(VaultPlaintextPath),
    ProcessPrivate(crate::native::temp_owner::ProcessPrivateOwner),
}

/// A path plus stable ownership identity, scope, cleanup authority and lifecycle.
/// Constructors are intentionally backend-specific and crate-private: callers
/// cannot manufacture a managed plaintext proof from a raw `PathBuf` alone.
pub(crate) struct OwnedPlaintext {
    identity: String,
    path: PathBuf,
    scope: PlaintextScope,
    lifecycle: PlaintextLifecycle,
    authority: PlaintextCleanupAuthority,
}

impl std::fmt::Debug for OwnedPlaintext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnedPlaintext")
            .field("identity", &self.identity)
            .field("scope", &self.scope)
            .field("lifecycle", &self.lifecycle)
            .field("path", &"[REDACTED]")
            .finish()
    }
}

impl DurablePlaintextNamespace {
    /// Reopen an already-established durable namespace without creating or
    /// repairing its authority marker. Security-sensitive steady-state writers
    /// use this path so marker loss/corruption fails closed instead of silently
    /// manufacturing a replacement owner around existing plaintext.
    pub(crate) fn open_existing(
        root: PathBuf,
        marker: PathBuf,
        identity: impl Into<String>,
    ) -> io::Result<Self> {
        let identity = identity.into();
        if identity.trim().is_empty() || root == marker || marker.starts_with(&root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid durable plaintext namespace",
            ));
        }
        let marker_parent = marker.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "namespace marker has no parent",
            )
        })?;
        let marker_parent_metadata = fs::symlink_metadata(marker_parent)?;
        if marker_parent_metadata.file_type().is_symlink() || !marker_parent_metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable plaintext namespace marker parent is not a real directory",
            ));
        }
        match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "durable plaintext namespace root is not a real directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let expected = format!("rustnote-owned-plaintext-namespace-v1\n{identity}\n");
        let marker_metadata = fs::symlink_metadata(&marker)?;
        if marker_metadata.file_type().is_symlink()
            || !marker_metadata.is_file()
            || fs::read(&marker)? != expected.as_bytes()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "durable plaintext namespace marker is missing or invalid",
            ));
        }
        Ok(Self {
            identity,
            root,
            marker,
        })
    }

    /// Establish a fresh durable namespace, or reopen an existing one.
    ///
    /// When the marker is absent this operation is intentionally fail-closed:
    /// an already-nonempty managed root cannot be adopted implicitly because
    /// that would manufacture cleanup authority *after* plaintext exists.
    pub(crate) fn establish(
        root: PathBuf,
        marker: PathBuf,
        identity: impl Into<String>,
    ) -> io::Result<Self> {
        Self::establish_inner(root, marker, identity.into())
    }

    fn establish_inner(root: PathBuf, marker: PathBuf, identity: String) -> io::Result<Self> {
        if identity.trim().is_empty() || root == marker || marker.starts_with(&root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid durable plaintext namespace",
            ));
        }
        let marker_parent = marker.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "namespace marker has no parent",
            )
        })?;
        fs::create_dir_all(marker_parent)?;
        let marker_parent_metadata = fs::symlink_metadata(marker_parent)?;
        if marker_parent_metadata.file_type().is_symlink() || !marker_parent_metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable plaintext namespace marker parent is not a real directory",
            ));
        }
        let root_nonempty = match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "durable plaintext namespace root is not a real directory",
                ));
            }
            Ok(_) => fs::read_dir(&root)?.next().transpose()?.is_some(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        let marker_bytes = format!("rustnote-owned-plaintext-namespace-v1\n{identity}\n");
        match fs::symlink_metadata(&marker) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "durable plaintext namespace marker is not a regular file",
                    ));
                }
                let existing = fs::read(&marker)?;
                if existing != marker_bytes.as_bytes() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "durable plaintext namespace marker identity mismatch",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if root_nonempty {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "durable plaintext namespace marker is missing for a nonempty managed root",
                    ));
                }
                let mut file =
                    crate::native::private_temp::create_private_file_in_existing_directory(
                        &marker,
                    )?;
                file.write_all(marker_bytes.as_bytes())?;
                file.sync_all()?;
                sync_parent_directory(&marker)?;
            }
            Err(error) => return Err(error),
        }
        Ok(Self {
            identity,
            root,
            marker,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn reserve(&self, path: PathBuf) -> io::Result<OwnedPlaintext> {
        self.owned(path, PlaintextLifecycle::Reserved)
    }

    pub(crate) fn adopt_existing(&self, path: PathBuf) -> io::Result<OwnedPlaintext> {
        self.owned(path, PlaintextLifecycle::Materialized)
    }

    fn owned(&self, path: PathBuf, lifecycle: PlaintextLifecycle) -> io::Result<OwnedPlaintext> {
        if !path.starts_with(&self.root) || path == self.root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed plaintext path escapes durable namespace",
            ));
        }
        let root_metadata = fs::symlink_metadata(&self.root)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable plaintext namespace root is not a real directory",
            ));
        }
        let expected_marker = format!("rustnote-owned-plaintext-namespace-v1\n{}\n", self.identity);
        let marker_metadata = fs::symlink_metadata(&self.marker)?;
        if marker_metadata.file_type().is_symlink()
            || !marker_metadata.is_file()
            || fs::read(&self.marker)? != expected_marker.as_bytes()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "durable plaintext namespace marker is missing or invalid",
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed plaintext path has no parent",
            )
        })?;
        // Lexical containment alone is insufficient when a cache/journal
        // subdirectory has been replaced by a symlink. Resolve the existing
        // namespace root and the target parent and require real containment
        // before any producer receives an OwnedPlaintext reservation.
        let canonical_root = fs::canonicalize(&self.root)?;
        let canonical_parent = fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed plaintext parent escapes durable namespace",
            ));
        }
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Ok(OwnedPlaintext {
            identity: format!("namespace-{}-{sequence}", self.identity),
            path,
            scope: PlaintextScope::Library,
            lifecycle,
            authority: PlaintextCleanupAuthority::DurableNamespace(DurableNamespaceOwnership {
                _identity: self.identity.clone(),
                root: self.root.clone(),
                _marker: self.marker.clone(),
            }),
        })
    }

    pub(crate) fn remove_tree_strict(&self) -> io::Result<()> {
        self.remove_subtree_strict(&self.root)
    }

    pub(crate) fn remove_subtree_strict(&self, path: &Path) -> io::Result<()> {
        if !path.starts_with(&self.root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed plaintext subtree escapes durable namespace",
            ));
        }

        // Revalidate the durable authority immediately before destructive work.
        // A namespace handle is not itself an ownership proof if its external
        // marker was deleted/corrupted after acquisition. This also prevents a
        // caller from observing an absent root, recreating it, and writing new
        // plaintext while holding only stale authority.
        Self::open_existing(
            self.root.clone(),
            self.marker.clone(),
            self.identity.clone(),
        )?;

        // Absence is already the desired cleanup state. In particular, do not
        // require the parent of a never-created nested cache subtree to exist
        // merely so it can be fsynced after no filesystem mutation occurred.
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };

        // Deletion must enforce the same real-path containment as reservation.
        // A lexically in-root path can otherwise traverse an intermediate
        // symlink and make recursive cleanup operate outside Rustnote's cache or
        // recovery namespace.
        let root_metadata = fs::symlink_metadata(&self.root)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable plaintext namespace root is not a real directory",
            ));
        }
        let canonical_root = fs::canonicalize(&self.root)?;
        if path == self.root {
            // `root_metadata` above already proved this exact path is a real
            // directory, so no parent traversal is involved.
        } else if metadata.file_type().is_symlink() || metadata.is_file() {
            let parent = path.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "managed plaintext subtree has no parent",
                )
            })?;
            let canonical_parent = fs::canonicalize(parent)?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "managed plaintext subtree parent escapes durable namespace",
                ));
            }
        } else if metadata.is_dir() {
            let canonical_path = fs::canonicalize(path)?;
            if !canonical_path.starts_with(&canonical_root) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "managed plaintext subtree escapes durable namespace",
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "managed plaintext subtree has an unsupported filesystem type",
            ));
        }

        crate::native::privacy::remove_tree(path)?;
        sync_parent_directory(path)
    }
}

impl OwnedPlaintext {
    pub(crate) fn durable_managed(
        path: PathBuf,
        stage_id: String,
        kind: String,
        note_id: Option<NoteId>,
        relative_path: String,
    ) -> Self {
        Self::durable_managed_with_lifecycle(
            path,
            stage_id,
            kind,
            note_id,
            relative_path,
            PlaintextLifecycle::Reserved,
        )
    }

    /// Reconstruct a durable ownership proof for plaintext that a persisted
    /// staging row already owns and that recovery has confirmed exists. This is
    /// deliberately separate from `durable_managed`: callers cannot silently
    /// treat an unmaterialized reservation as existing plaintext.
    pub(crate) fn durable_managed_existing(
        path: PathBuf,
        stage_id: String,
        kind: String,
        note_id: Option<NoteId>,
        relative_path: String,
    ) -> Self {
        Self::durable_managed_with_lifecycle(
            path,
            stage_id,
            kind,
            note_id,
            relative_path,
            PlaintextLifecycle::Materialized,
        )
    }

    fn durable_managed_with_lifecycle(
        path: PathBuf,
        stage_id: String,
        kind: String,
        note_id: Option<NoteId>,
        relative_path: String,
        lifecycle: PlaintextLifecycle,
    ) -> Self {
        let scope = note_id
            .clone()
            .map(PlaintextScope::Note)
            .unwrap_or(PlaintextScope::Library);
        Self {
            identity: stage_id.clone(),
            path,
            scope,
            lifecycle,
            authority: PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(
                DurablePlaintextOwnership {
                    stage_id,
                    kind,
                    note_id,
                    relative_path,
                },
            )),
        }
    }

    /// Adopt a path whose durable cleanup authority is an already-committed
    /// `vault_operations` journal row. The caller must create that row before
    /// constructing this proof; this constructor never manufactures durability.
    pub(crate) fn durable_vault_operation_reserved(
        path: PathBuf,
        operation_id: String,
        note_id: NoteId,
    ) -> Self {
        Self::durable_vault_operation(path, operation_id, note_id, PlaintextLifecycle::Reserved)
    }

    /// Adopt pre-existing canonical/staged plaintext after a Vault operation
    /// journal is durable. This is used only to transfer/retire bytes whose
    /// predecessor already has ordinary database ownership.
    pub(crate) fn durable_vault_operation_existing(
        path: PathBuf,
        operation_id: String,
        note_id: NoteId,
    ) -> Self {
        Self::durable_vault_operation(
            path,
            operation_id,
            note_id,
            PlaintextLifecycle::Materialized,
        )
    }

    fn durable_vault_operation(
        path: PathBuf,
        operation_id: String,
        note_id: NoteId,
        lifecycle: PlaintextLifecycle,
    ) -> Self {
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            identity: format!("vault-operation-{operation_id}-{sequence}"),
            path,
            scope: PlaintextScope::Note(note_id.clone()),
            lifecycle,
            authority: PlaintextCleanupAuthority::DurableManaged(
                DurableManagedAuthority::VaultOperation {
                    _ownership: DurableVaultOperationOwnership {
                        _operation_id: operation_id,
                        _note_id: note_id,
                    },
                },
            ),
        }
    }

    /// Adopt plaintext whose durable cleanup authority is an already-committed
    /// `attachment_operations` row. The row owns both its `.incoming`
    /// predecessor and canonical successor until attachment persistence commits.
    pub(crate) fn durable_attachment_operation_existing(
        path: PathBuf,
        operation_id: String,
        note_id: NoteId,
    ) -> Self {
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            identity: format!("attachment-operation-{operation_id}-{sequence}"),
            path,
            scope: PlaintextScope::Note(note_id.clone()),
            lifecycle: PlaintextLifecycle::Materialized,
            authority: PlaintextCleanupAuthority::DurableManaged(
                DurableManagedAuthority::AttachmentOperation {
                    _ownership: DurableAttachmentOperationOwnership {
                        _operation_id: operation_id,
                        _note_id: note_id,
                    },
                },
            ),
        }
    }

    /// Adopt plaintext whose durable cleanup authority is an already-committed
    /// `attachment_ingest_jobs` row. The ingest row is created in the same
    /// transaction that retires the generic staging owner, so cleanup remains
    /// authoritative across process death until the ingest row itself retires.
    pub(crate) fn durable_attachment_ingest_existing(
        path: PathBuf,
        ingest_id: String,
        note_id: NoteId,
    ) -> Self {
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            identity: format!("attachment-ingest-{ingest_id}-{sequence}"),
            path,
            scope: PlaintextScope::Note(note_id.clone()),
            lifecycle: PlaintextLifecycle::Materialized,
            authority: PlaintextCleanupAuthority::DurableManaged(
                DurableManagedAuthority::AttachmentIngest {
                    _ownership: DurableAttachmentIngestOwnership {
                        _ingest_id: ingest_id,
                        _note_id: note_id,
                    },
                },
            ),
        }
    }

    /// Reserve a Rustnote-created sibling output path after an immutable,
    /// crash-durable external-output transaction record has been persisted.
    /// The final user-selected destination remains external/user-owned; only
    /// Rustnote's hidden preparation/replacement paths use this authority.
    pub(crate) fn durable_external_output_reserved(path: PathBuf, operation_id: String) -> Self {
        Self::durable_external_output(path, operation_id, PlaintextLifecycle::Reserved)
    }

    /// Reconstruct ownership for a hidden predecessor that an already-durable
    /// external-output transaction may have created before process death.
    pub(crate) fn durable_external_output_existing(path: PathBuf, operation_id: String) -> Self {
        Self::durable_external_output(path, operation_id, PlaintextLifecycle::Materialized)
    }

    fn durable_external_output(
        path: PathBuf,
        operation_id: String,
        lifecycle: PlaintextLifecycle,
    ) -> Self {
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            identity: format!("external-output-{operation_id}-{sequence}"),
            path,
            scope: PlaintextScope::ExternalOutput,
            lifecycle,
            authority: PlaintextCleanupAuthority::DurableExternalOutput {
                _ownership: DurableExternalOutputOwnership {
                    _operation_id: operation_id,
                },
            },
        }
    }

    pub(crate) fn reserve_vault_session(
        permit: &VaultReadPermit,
        path: PathBuf,
    ) -> Result<Self, &'static str> {
        let tracked = permit.track_plaintext_path(path.clone())?;
        let sequence = OWNED_PLAINTEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            identity: format!("vault-plaintext-{sequence}"),
            path,
            scope: PlaintextScope::Note(permit.note_id().clone()),
            lifecycle: PlaintextLifecycle::Reserved,
            authority: PlaintextCleanupAuthority::VaultSession(tracked),
        })
    }

    pub(crate) fn process_private(
        owner: crate::native::temp_owner::ProcessPrivateOwner,
        identity: String,
        path: PathBuf,
    ) -> Result<Self, &'static str> {
        if !path.starts_with(owner.root()) || path == owner.root() {
            return Err("process-private plaintext path escapes its cleanup owner");
        }
        Ok(Self {
            identity,
            path,
            scope: PlaintextScope::ProcessPrivate,
            lifecycle: PlaintextLifecycle::Reserved,
            authority: PlaintextCleanupAuthority::ProcessPrivate(owner),
        })
    }

    #[cfg(test)]
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub(crate) fn lifecycle(&self) -> PlaintextLifecycle {
        self.lifecycle
    }

    pub(crate) fn durable_ownership(&self) -> Option<&DurablePlaintextOwnership> {
        match &self.authority {
            PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(owner)) => {
                Some(owner)
            }
            _ => None,
        }
    }

    /// Materialize by CoW clone where available, falling back to copy. The
    /// cleanup owner already exists before either filesystem operation runs.
    pub(crate) fn copy_from(&mut self, source: &Path) -> io::Result<bool> {
        self.require_reserved()?;
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "owned plaintext destination already exists",
            ));
        }
        if crate::platform::try_clone_file(source, &self.path)? {
            if matches!(
                &self.authority,
                PlaintextCleanupAuthority::VaultSession(_)
                    | PlaintextCleanupAuthority::ProcessPrivate(_)
                    | PlaintextCleanupAuthority::DurableExternalOutput { .. }
                    | PlaintextCleanupAuthority::DurableNamespace(_)
            ) {
                crate::native::private_temp::harden_existing_file(&self.path)?;
            }
            self.lifecycle = PlaintextLifecycle::Materialized;
            return Ok(true);
        }
        // A failed best-effort clone is allowed to fall back only if it left no
        // destination behind. Never overwrite an ambiguous partial plaintext file.
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "failed plaintext clone left the owned destination materialized",
            ));
        }
        let mut reader = File::open(source)?;
        let mut writer = self.create_materialization_file()?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        io::copy(&mut reader, &mut writer)?;
        writer.sync_all()?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(false)
    }

    /// Copy and hash a managed plaintext file without a second full read on the
    /// ordinary copy path. The ownership proof exists before clone/create.
    pub(crate) fn copy_and_hash_from(&mut self, source: &Path) -> io::Result<(String, u64)> {
        self.require_reserved()?;
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "owned plaintext destination already exists",
            ));
        }
        if crate::platform::try_clone_file(source, &self.path)? {
            if matches!(
                &self.authority,
                PlaintextCleanupAuthority::VaultSession(_)
                    | PlaintextCleanupAuthority::ProcessPrivate(_)
                    | PlaintextCleanupAuthority::DurableExternalOutput { .. }
                    | PlaintextCleanupAuthority::DurableNamespace(_)
            ) {
                crate::native::private_temp::harden_existing_file(&self.path)?;
            }
            self.lifecycle = PlaintextLifecycle::Materialized;
            let mut file = File::open(&self.path)?;
            let mut hasher = Sha256::new();
            let mut size = 0_u64;
            let mut buffer = vec![0_u8; 1024 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                size = size.saturating_add(read as u64);
            }
            File::open(&self.path)?.sync_all()?;
            return Ok((digest_hex(&hasher.finalize()), size));
        }
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "failed plaintext clone left the owned destination materialized",
            ));
        }

        let mut reader = File::open(source)?;
        let mut writer = self.create_materialization_file()?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            writer.write_all(&buffer[..read])?;
            size = size.saturating_add(read as u64);
        }
        writer.sync_all()?;
        Ok((digest_hex(&hasher.finalize()), size))
    }

    /// Create a managed plaintext file only after ownership reservation exists.
    /// Vault/process-private files receive private-temp permissions; durable
    /// library staging uses create-new semantics and remains ledger-owned.
    pub(crate) fn create_file(&mut self) -> io::Result<File> {
        self.require_reserved()?;
        let file = self.create_materialization_file()?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(file)
    }

    /// Create one managed private directory only after plaintext ownership has
    /// already been reserved. This is used for Vault/process-private ephemeral
    /// namespaces whose parent is already a trusted Rustnote-owned directory.
    pub(crate) fn create_private_directory(&mut self) -> io::Result<()> {
        self.require_reserved()?;
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::VaultSession(_)
                | PlaintextCleanupAuthority::ProcessPrivate(_)
                | PlaintextCleanupAuthority::DurableNamespace(_)
        ) {
            return Err(io::Error::other(
                "private directory creation requires ephemeral plaintext ownership",
            ));
        }
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "owned private directory already exists",
            ));
        }
        crate::native::private_temp::create_private_directory_in_existing_directory(&self.path)?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(())
    }

    /// Create a Rustnote-owned external-output staging directory only after the
    /// durable output transaction has reserved this resource.
    pub(crate) fn create_external_output_directory(&mut self) -> io::Result<()> {
        self.require_reserved()?;
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableExternalOutput { .. }
        ) {
            return Err(io::Error::other(
                "external output directory creation requires durable output ownership",
            ));
        }
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "owned external output directory already exists",
            ));
        }
        crate::native::private_temp::create_private_directory_in_existing_directory(&self.path)?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(())
    }

    fn create_materialization_file(&self) -> io::Result<File> {
        match &self.authority {
            PlaintextCleanupAuthority::DurableExternalOutput { .. } => {
                crate::native::private_temp::create_private_file_in_existing_directory(&self.path)
            }
            PlaintextCleanupAuthority::VaultSession(_)
            | PlaintextCleanupAuthority::ProcessPrivate(_)
            | PlaintextCleanupAuthority::DurableNamespace(_) => {
                crate::native::private_temp::create_private_file(&self.path)
            }
            PlaintextCleanupAuthority::DurableManaged(_) => OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.path),
        }
    }

    /// Rename inside one process-private owner. The owner covers both paths
    /// before rename; the authoritative path changes only after rename succeeds.
    pub(crate) fn rename_process_private(&mut self, destination: PathBuf) -> io::Result<()> {
        let PlaintextCleanupAuthority::ProcessPrivate(owner) = &self.authority else {
            return Err(io::Error::other(
                "rename_process_private requires process-private ownership",
            ));
        };
        if !destination.starts_with(owner.root()) || destination == owner.root() {
            return Err(io::Error::other(
                "process-private rename destination escapes cleanup owner",
            ));
        }
        if !matches!(
            self.lifecycle,
            PlaintextLifecycle::Reserved | PlaintextLifecycle::Materialized
        ) {
            return Err(io::Error::other(
                "owned plaintext cannot be renamed in this state",
            ));
        }
        fs::rename(&self.path, &destination)?;
        self.path = destination;
        Ok(())
    }

    /// Adopt bytes created by a platform converter/remuxer after the durable
    /// namespace owner was reserved first. This never manufactures ownership
    /// after the fact: the OwnedPlaintext must already be in Reserved state.
    pub(crate) fn verify_materialized(&mut self) -> io::Result<()> {
        self.require_reserved()?;
        let metadata = fs::symlink_metadata(&self.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "owned plaintext materialization is not a regular file",
            ));
        }
        if matches!(
            &self.authority,
            PlaintextCleanupAuthority::VaultSession(_)
                | PlaintextCleanupAuthority::ProcessPrivate(_)
                | PlaintextCleanupAuthority::DurableExternalOutput { .. }
                | PlaintextCleanupAuthority::DurableNamespace(_)
        ) {
            crate::native::private_temp::harden_existing_file(&self.path)?;
        }
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(())
    }

    /// Rename inside one durable namespace. The marker authoritatively owns
    /// both paths before the filesystem transition, so process death cannot
    /// create an ownerless predecessor or successor.
    pub(crate) fn rename_durable_namespace(&mut self, destination: PathBuf) -> io::Result<()> {
        let PlaintextCleanupAuthority::DurableNamespace(owner) = &self.authority else {
            return Err(io::Error::other(
                "durable namespace rename requires namespace ownership",
            ));
        };
        if !destination.starts_with(&owner.root) || destination == owner.root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable namespace rename destination escapes owner",
            ));
        }
        if self.lifecycle != PlaintextLifecycle::Materialized {
            return Err(io::Error::other(
                "durable namespace rename requires materialized plaintext",
            ));
        }
        fs::rename(&self.path, &destination)?;
        self.path = destination;
        Ok(())
    }

    /// Move an existing externally-owned destination into a Rustnote-hidden
    /// predecessor path. The durable output transaction reserved this owner
    /// before the move, so the user file never enters an ownerless hidden state.
    pub(crate) fn capture_external_predecessor(&mut self, source: &Path) -> io::Result<()> {
        self.require_reserved()?;
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableExternalOutput { .. }
        ) {
            return Err(io::Error::other(
                "external predecessor capture requires durable output ownership",
            ));
        }
        if self.path.parent() != source.parent() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "external predecessor must remain in the destination directory",
            ));
        }
        if path_entry_exists(&self.path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "external predecessor staging path already exists",
            ));
        }
        let metadata = fs::symlink_metadata(source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "external predecessor is not a regular file",
            ));
        }
        fs::rename(source, &self.path)?;
        self.lifecycle = PlaintextLifecycle::Materialized;
        Ok(())
    }

    /// Publish an externally prepared resource into the explicit user-selected
    /// destination. The durable output journal continues to own any hidden
    /// predecessor while this transfer completes, but the final destination
    /// leaves Rustnote cleanup ownership immediately after rename succeeds.
    pub(crate) fn publish_external_output(&mut self, destination: &Path) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableExternalOutput { .. }
        ) {
            return Err(io::Error::other(
                "external output publication requires durable output ownership",
            ));
        }
        if self.lifecycle != PlaintextLifecycle::Materialized {
            return Err(io::Error::other(
                "external output publication requires materialized plaintext",
            ));
        }
        fs::rename(&self.path, destination)?;
        self.path = destination.to_path_buf();
        self.lifecycle = PlaintextLifecycle::Transferred;
        Ok(())
    }

    /// Rename within one durable Vault-operation owner. The journal row already
    /// covers both the pre-commit staging/retiring namespace and the recovery
    /// destination, so ownership remains continuous and the authoritative path
    /// changes only after the filesystem rename succeeds.
    pub(crate) fn rename_durable_vault_operation(
        &mut self,
        destination: PathBuf,
    ) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(
                DurableManagedAuthority::VaultOperation { .. }
            )
        ) {
            return Err(io::Error::other(
                "Vault-operation rename requires durable operation ownership",
            ));
        }
        if self.lifecycle != PlaintextLifecycle::Materialized {
            return Err(io::Error::other(
                "Vault-operation rename requires materialized plaintext",
            ));
        }
        fs::rename(&self.path, &destination)?;
        self.path = destination;
        Ok(())
    }

    /// Rename plaintext while an `attachment_operations` row owns both crash
    /// states. The authoritative path changes only after the filesystem rename
    /// succeeds; the durable journal remains the recovery authority until commit.
    pub(crate) fn rename_durable_attachment_operation(
        &mut self,
        destination: PathBuf,
    ) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(
                DurableManagedAuthority::AttachmentOperation { .. }
            )
        ) {
            return Err(io::Error::other(
                "attachment-operation rename requires durable operation ownership",
            ));
        }
        if self.lifecycle != PlaintextLifecycle::Materialized {
            return Err(io::Error::other(
                "attachment-operation rename requires materialized plaintext",
            ));
        }
        fs::rename(&self.path, &destination)?;
        self.path = destination;
        Ok(())
    }

    /// Rename a durable predecessor into a destination whose successor owner
    /// was durably established by PlaintextAuthority before this call. The
    /// predecessor keeps its original path identity until its ledger row is
    /// retired, so crash recovery can reconcile either filesystem outcome.
    pub(crate) fn rename_durable_to_preowned(&mut self, destination: &Path) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(_))
        ) {
            return Err(io::Error::other(
                "preowned durable transfer requires staging-ledger ownership",
            ));
        }
        if self.lifecycle != PlaintextLifecycle::Materialized {
            return Err(io::Error::other(
                "durable rename requires materialized predecessor plaintext",
            ));
        }
        fs::rename(&self.path, destination)?;
        // Keep `path` as the predecessor identity. The durable row records the
        // successor destination separately and remains authoritative until it
        // is retired after sync.
        self.lifecycle = PlaintextLifecycle::Transferred;
        Ok(())
    }

    pub(crate) fn retire_transferred_durable_owner(&mut self) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(_))
        ) {
            return Err(io::Error::other("plaintext is not durably managed"));
        }
        if self.lifecycle != PlaintextLifecycle::Transferred {
            return Err(io::Error::other(
                "transferred durable ownership can retire only after transfer",
            ));
        }
        require_path_absent(&self.path)?;
        self.lifecycle = PlaintextLifecycle::Retired;
        Ok(())
    }

    /// Finalize durable ownership retirement after Storage has removed the
    /// durable ledger row. A normal cleanup reaches `Retiring`; a completed
    /// predecessor→successor rename reaches `Transferred`. No other state may
    /// forget durable cleanup authority.
    pub(crate) fn retire_durable_owner_after_cleanup(&mut self) -> io::Result<()> {
        match self.lifecycle {
            PlaintextLifecycle::Retiring => self.mark_durable_owner_retired(),
            PlaintextLifecycle::Transferred => self.retire_transferred_durable_owner(),
            _ => Err(io::Error::other(
                "durable plaintext ownership cannot retire in this lifecycle state",
            )),
        }
    }

    /// Remove a directory-scoped external output staging resource while its
    /// durable output transaction still owns the complete subtree.
    pub(crate) fn remove_external_output_tree(&mut self) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableExternalOutput { .. }
        ) {
            return Err(io::Error::other(
                "external output tree cleanup requires durable output ownership",
            ));
        }
        if matches!(
            self.lifecycle,
            PlaintextLifecycle::Transferred | PlaintextLifecycle::Retired
        ) {
            return Ok(());
        }
        self.lifecycle = PlaintextLifecycle::Retiring;
        crate::native::privacy::remove_tree(&self.path)?;
        self.lifecycle = PlaintextLifecycle::Retired;
        Ok(())
    }

    /// Remove managed plaintext first. Backend ownership retirement is separate
    /// for durable SQLite owners; Vault/process-private ownership can retire here.
    pub(crate) fn remove_file(&mut self) -> io::Result<()> {
        if matches!(
            self.lifecycle,
            PlaintextLifecycle::Transferred | PlaintextLifecycle::Retired
        ) {
            return Ok(());
        }
        self.lifecycle = PlaintextLifecycle::Retiring;
        match &mut self.authority {
            PlaintextCleanupAuthority::VaultSession(tracked) => {
                tracked.remove_now()?;
                self.lifecycle = PlaintextLifecycle::Retired;
                Ok(())
            }
            PlaintextCleanupAuthority::ProcessPrivate(_)
            | PlaintextCleanupAuthority::DurableExternalOutput { .. }
            | PlaintextCleanupAuthority::DurableNamespace(_) => {
                remove_managed_file(&self.path)?;
                self.lifecycle = PlaintextLifecycle::Retired;
                Ok(())
            }
            PlaintextCleanupAuthority::DurableManaged(_) => {
                remove_managed_file(&self.path)?;
                // The SQLite ownership row must still be retired by Storage.
                Ok(())
            }
        }
    }

    pub(crate) fn promote_vault_materialized_attachment(
        &mut self,
        note_id: &NoteId,
        attachment_id: String,
    ) -> Result<PathBuf, &'static str> {
        let PlaintextCleanupAuthority::VaultSession(tracked) = &mut self.authority else {
            return Err("plaintext is not owned by a Vault session");
        };
        let path = tracked.promote_materialized_attachment(note_id, attachment_id)?;
        self.path = path.clone();
        self.lifecycle = PlaintextLifecycle::Transferred;
        Ok(path)
    }

    pub(crate) fn mark_durable_owner_retired(&mut self) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(_))
        ) {
            return Err(io::Error::other("plaintext is not durably managed"));
        }
        if self.lifecycle != PlaintextLifecycle::Retiring {
            return Err(io::Error::other(
                "durable plaintext file must be removed before owner retirement",
            ));
        }
        self.lifecycle = PlaintextLifecycle::Retired;
        Ok(())
    }

    pub(crate) fn mark_uncreated_durable_owner_retired(&mut self) -> io::Result<()> {
        if !matches!(
            &self.authority,
            PlaintextCleanupAuthority::DurableManaged(DurableManagedAuthority::Staging(_))
        ) {
            return Err(io::Error::other("plaintext is not durably managed"));
        }
        if self.lifecycle != PlaintextLifecycle::Reserved {
            return Err(io::Error::other(
                "uncreated durable ownership can retire only while reserved",
            ));
        }
        require_path_absent(&self.path)?;
        self.lifecycle = PlaintextLifecycle::Retired;
        Ok(())
    }

    /// Prove that this managed plaintext path is absent. Metadata failures are
    /// never collapsed into absence: ownership retirement must fail closed.
    pub(crate) fn ensure_path_absent(&self) -> io::Result<()> {
        require_path_absent(&self.path)
    }

    pub(crate) fn path_entry_exists(&self) -> io::Result<bool> {
        path_entry_exists(&self.path)
    }

    pub(crate) fn mark_transferred(&mut self) -> Result<(), &'static str> {
        if !matches!(
            self.lifecycle,
            PlaintextLifecycle::Reserved | PlaintextLifecycle::Materialized
        ) {
            return Err("plaintext ownership cannot transfer in this state");
        }
        self.lifecycle = PlaintextLifecycle::Transferred;
        Ok(())
    }

    pub(crate) fn into_transferred_path(mut self) -> Result<PathBuf, &'static str> {
        self.mark_transferred()?;
        Ok(self.path.clone())
    }

    fn require_reserved(&self) -> io::Result<()> {
        if self.lifecycle != PlaintextLifecycle::Reserved {
            return Err(io::Error::other(
                "plaintext materialization requires a reserved owner",
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed plaintext path has no parent",
        )
    })?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Observe a managed-sensitive path without collapsing filesystem errors into
/// absence. Only `NotFound` proves that an ownership obligation may treat the
/// path as gone; permission, loop, I/O and every other metadata error fail
/// closed by propagating to the caller.
pub(crate) fn managed_path_is_present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn path_entry_exists(path: &Path) -> io::Result<bool> {
    managed_path_is_present(path)
}

fn require_path_absent(path: &Path) -> io::Result<()> {
    if path_entry_exists(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("managed plaintext path still exists: {}", path.display()),
        ));
    }
    Ok(())
}

fn remove_managed_file(path: &Path) -> io::Result<()> {
    let removed = crate::native::privacy::remove_file(path)?;
    if !removed {
        require_path_absent(path).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                io::Error::other(format!(
                    "managed plaintext path is not a removable file: {}",
                    path.display()
                ))
            } else {
                error
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_process_private_rename_retains_predecessor_ownership() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-rename-{}",
            rand::random::<u64>()
        ));
        let owner =
            crate::native::temp_owner::process_session_owner(&parent, "owned plaintext test")
                .expect("owner");
        let source = owner.root().join("source.tmp");
        let mut plaintext =
            OwnedPlaintext::process_private(owner, "rename-test".to_string(), source.clone())
                .expect("owned plaintext");
        let mut file = plaintext.create_file().expect("materialize");
        use std::io::Write;
        file.write_all(b"secret").expect("write");
        drop(file);

        let invalid = parent.join("outside.tmp");
        assert!(plaintext.rename_process_private(invalid).is_err());
        assert_eq!(plaintext.path(), source);
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Materialized);
        plaintext.remove_file().expect("cleanup");
    }

    #[test]
    fn failed_vault_operation_rename_retains_predecessor_ownership() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-vault-rename-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).expect("test root");
        let source = root.join("source.tmp");
        let note_id = NoteId::parse("note-vault-operation-rename").expect("note id");
        let mut plaintext = OwnedPlaintext::durable_vault_operation_reserved(
            source.clone(),
            "operation-test".to_string(),
            note_id,
        );
        let mut file = plaintext
            .create_file()
            .expect("materialize owned plaintext");
        use std::io::Write as _;
        file.write_all(b"secret").expect("write source plaintext");
        drop(file);
        let invalid = root.join("missing-parent").join("destination.tmp");

        assert!(plaintext.rename_durable_vault_operation(invalid).is_err());
        assert_eq!(plaintext.path(), source);
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Materialized);
        assert!(
            source.exists(),
            "failed rename must preserve predecessor plaintext"
        );

        plaintext.remove_file().expect("remove test plaintext");
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_attachment_operation_rename_retains_predecessor_ownership() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-attachment-rename-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).expect("test root");
        let source = root.join("source.tmp");
        std::fs::write(&source, b"secret").expect("materialize source plaintext");
        let note_id = NoteId::parse("note-attachment-operation-rename").expect("note id");
        let mut plaintext = OwnedPlaintext::durable_attachment_operation_existing(
            source.clone(),
            "attachment-operation-test".to_string(),
            note_id,
        );
        let invalid = root.join("missing-parent").join("destination.tmp");

        assert!(
            plaintext
                .rename_durable_attachment_operation(invalid)
                .is_err()
        );
        assert_eq!(plaintext.path(), source);
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Materialized);
        assert!(
            source.exists(),
            "failed rename must preserve predecessor plaintext"
        );

        plaintext.remove_file().expect("remove test plaintext");
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn true_not_found_is_conclusive_plaintext_absence() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-not-found-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).expect("test root");
        let managed_path = root.join("missing.tmp");

        assert!(!managed_path_is_present(&managed_path).expect("observe missing path"));
        let mut plaintext = OwnedPlaintext::durable_managed(
            managed_path,
            "not-found-stage".to_string(),
            "portability_import".to_string(),
            None,
            "not-found/missing.tmp".to_string(),
        );
        plaintext
            .mark_uncreated_durable_owner_retired()
            .expect("NotFound may retire an uncreated owner");
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Retired);

        std::fs::remove_dir_all(root).expect("remove test root");
    }

    #[cfg(unix)]
    #[test]
    fn metadata_failure_is_not_treated_as_plaintext_absence() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-metadata-error-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).expect("test root");
        let loop_path = root.join("loop");
        symlink("loop", &loop_path).expect("symlink loop");
        let managed_path = loop_path.join("plaintext.tmp");

        assert!(
            path_entry_exists(&managed_path).is_err(),
            "metadata errors must not collapse into a false absence result"
        );
        let mut plaintext = OwnedPlaintext::durable_managed(
            managed_path,
            "metadata-error-stage".to_string(),
            "portability_import".to_string(),
            None,
            "metadata-error/plaintext.tmp".to_string(),
        );
        assert!(
            plaintext.mark_uncreated_durable_owner_retired().is_err(),
            "durable ownership must survive when path absence cannot be proved"
        );
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Reserved);

        std::fs::remove_file(&loop_path).expect("remove symlink loop");
        std::fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn process_private_delete_failure_keeps_retiring_state_and_owner() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-delete-{}",
            rand::random::<u64>()
        ));
        let owner =
            crate::native::temp_owner::process_session_owner(&parent, "owned plaintext test")
                .expect("owner");
        let directory = owner.root().join("not-a-file");
        std::fs::create_dir(&directory).expect("directory");
        let mut plaintext =
            OwnedPlaintext::process_private(owner, "delete-test".to_string(), directory.clone())
                .expect("owned plaintext");
        assert!(plaintext.remove_file().is_err());
        assert_eq!(plaintext.lifecycle(), PlaintextLifecycle::Retiring);
        assert!(directory.exists());
        std::fs::remove_dir(&directory).expect("repair test path");
    }

    #[cfg(unix)]
    #[test]
    fn durable_namespace_rejects_symlink_parent_escape() {
        use std::os::unix::fs::symlink;

        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-namespace-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        let outside = parent.join("outside");
        std::fs::create_dir_all(&root).expect("namespace root");
        std::fs::create_dir_all(&outside).expect("outside root");
        let marker = parent.join(".cache-owner");
        let namespace =
            DurablePlaintextNamespace::establish(root.clone(), marker, "namespace-test")
                .expect("namespace");
        symlink(&outside, root.join("escape")).expect("escape symlink");

        let error = namespace
            .reserve(root.join("escape").join("secret.tmp"))
            .expect_err("symlink escape must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);

        std::fs::remove_file(root.join("escape")).expect("remove symlink");
        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[test]
    fn durable_namespace_strict_cleanup_accepts_absent_nested_subtree() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-absent-subtree-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        std::fs::create_dir_all(&parent).expect("namespace parent");
        let marker = parent.join(".cache-owner");
        let namespace =
            DurablePlaintextNamespace::establish(root.clone(), marker, "absent-subtree-test")
                .expect("namespace");

        namespace
            .remove_subtree_strict(&root.join("never-created").join("hash"))
            .expect("already-absent subtree is clean");

        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[cfg(unix)]
    #[test]
    fn durable_namespace_strict_cleanup_rejects_symlink_parent_escape() {
        use std::os::unix::fs::symlink;

        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-delete-escape-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        let outside = parent.join("outside");
        std::fs::create_dir_all(&root).expect("namespace root");
        std::fs::create_dir_all(&outside).expect("outside root");
        let outside_file = outside.join("secret.txt");
        std::fs::write(&outside_file, b"must survive").expect("outside file");
        let marker = parent.join(".cache-owner");
        let namespace =
            DurablePlaintextNamespace::establish(root.clone(), marker, "delete-escape-test")
                .expect("namespace");
        symlink(&outside, root.join("escape")).expect("escape symlink");

        let error = namespace
            .remove_subtree_strict(&root.join("escape").join("secret.txt"))
            .expect_err("symlink-parent deletion escape must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            std::fs::read(&outside_file).expect("outside file survives"),
            b"must survive"
        );

        std::fs::remove_file(root.join("escape")).expect("remove symlink");
        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[test]
    fn durable_namespace_refuses_materialization_after_marker_loss() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-marker-loss-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("namespace root");
        let marker = parent.join(".cache-owner");
        let namespace =
            DurablePlaintextNamespace::establish(root.clone(), marker.clone(), "marker-loss-test")
                .expect("namespace");
        std::fs::remove_file(&marker).expect("remove marker");

        assert!(
            namespace.reserve(root.join("secret.tmp")).is_err(),
            "plaintext must not materialize after its durable namespace authority disappears"
        );

        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[test]
    fn durable_namespace_strict_cleanup_refuses_marker_loss() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-cleanup-marker-loss-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("namespace root");
        let marker = parent.join(".cache-owner");
        let namespace = DurablePlaintextNamespace::establish(
            root.clone(),
            marker.clone(),
            "cleanup-marker-loss-test",
        )
        .expect("namespace");
        let secret = root.join("secret.tmp");
        std::fs::write(&secret, b"must remain without authority").expect("secret");
        std::fs::remove_file(&marker).expect("remove marker");

        assert!(
            namespace.remove_tree_strict().is_err(),
            "strict cleanup must not trust a stale namespace handle after marker loss"
        );
        assert_eq!(
            std::fs::read(&secret).expect("plaintext preserved when authority is gone"),
            b"must remain without authority"
        );
        assert!(
            !marker.exists(),
            "cleanup must never recreate missing authority"
        );

        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[test]
    fn durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-no-posthoc-adoption-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("namespace root");
        std::fs::write(root.join("secret.tmp"), b"pre-existing plaintext")
            .expect("pre-existing plaintext");
        let marker = parent.join(".cache-owner");

        let error = DurablePlaintextNamespace::establish(
            root.clone(),
            marker.clone(),
            "no-posthoc-adoption-test",
        )
        .expect_err("fresh establishment must reject a nonempty unowned root");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            !marker.exists(),
            "failed establishment must not manufacture authority"
        );
        assert!(root.join("secret.tmp").exists());

        std::fs::remove_dir_all(parent).expect("remove test root");
    }

    #[test]
    fn durable_namespace_open_existing_never_recreates_missing_marker() {
        let parent = std::env::temp_dir().join(format!(
            "rustnote-owned-plaintext-open-existing-{}",
            rand::random::<u64>()
        ));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("namespace root");
        let marker = parent.join(".cache-owner");
        DurablePlaintextNamespace::establish(root.clone(), marker.clone(), "open-existing-test")
            .expect("namespace");
        std::fs::remove_file(&marker).expect("remove marker");

        assert!(
            DurablePlaintextNamespace::open_existing(root, marker.clone(), "open-existing-test",)
                .is_err(),
            "verify-only reopen must fail closed after marker loss",
        );
        assert!(
            !marker.exists(),
            "verify-only reopen must never recreate authority"
        );

        std::fs::remove_dir_all(parent).expect("remove test root");
    }
}
