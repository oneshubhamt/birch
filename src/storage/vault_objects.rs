//! Canonical granular encrypted-object store for protected notes.
//!
//! This module is the sole steady-state protected-note persistence
//! representation. Current state is split by mutation locality; revisions are
//! immutable encrypted objects; root replacement is atomic inside SQLite.

use std::{collections::HashSet, fmt};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    domain::NoteId,
    security::{
        VaultContext, VaultDomain, VaultKey, VaultService,
        sensitive_data::{
            ATTACHMENT_INDEX_FORMAT_VERSION, AttachmentIndex, METADATA_STATE_FORMAT_VERSION,
            MetadataState, NOTE_PRIVATE_STATE_FORMAT_VERSION, NotePrivateState,
            REVISION_INDEX_FORMAT_VERSION, REVISION_OBJECT_FORMAT_VERSION, RevisionIndex,
            RevisionObject, VAULT_MANIFEST_FORMAT_VERSION, VAULT_NOTE_FORMAT_VERSION,
        },
        vault_object_spec::{VAULT_OBJECT_ID_BYTES, VAULT_OBJECT_ID_HEX_LEN, VaultObjectId},
    },
};

use super::{Storage, StorageError, StorageResult};

const VAULT_OBJECT_CONTEXT_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VaultObjectKind {
    Manifest,
    NotePrivateState,
    MetadataState,
    AttachmentIndex,
    RevisionIndex,
    Revision,
}

impl VaultObjectKind {
    pub(crate) const fn domain(self) -> VaultDomain {
        match self {
            Self::Manifest => VaultDomain::Manifest,
            Self::NotePrivateState => VaultDomain::NotePrivateState,
            Self::MetadataState => VaultDomain::NoteMetadata,
            Self::AttachmentIndex => VaultDomain::AttachmentIndex,
            Self::RevisionIndex => VaultDomain::RevisionIndex,
            Self::Revision => VaultDomain::Revision,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VaultManifest {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    /// Monotonic logical generation of the protected object graph.
    pub(crate) schema_generation: u64,
    pub(crate) note_private_state: VaultObjectId,
    pub(crate) metadata_state: VaultObjectId,
    pub(crate) attachment_index: VaultObjectId,
    /// Compact encrypted membership/summary index for retained revisions.
    /// Ordinary note-state commits keep this object unchanged.
    pub(crate) revision_index: VaultObjectId,
    /// Revision history is an immutable encrypted predecessor chain. Keeping
    /// only its retained head/floor makes ordinary note-state commits O(1) in
    /// revision-history length. `revision_floor` is the oldest retained object;
    /// its predecessor may already have been pruned and is therefore not
    /// traversed.
    pub(crate) revision_head: Option<VaultObjectId>,
    pub(crate) revision_floor: Option<VaultObjectId>,
    pub(crate) revision_count: u64,
}

impl VaultManifest {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        note_id: NoteId,
        schema_generation: u64,
        note_private_state: VaultObjectId,
        metadata_state: VaultObjectId,
        attachment_index: VaultObjectId,
        revision_index: VaultObjectId,
        revision_head: Option<VaultObjectId>,
        revision_floor: Option<VaultObjectId>,
        revision_count: u64,
    ) -> StorageResult<Self> {
        let manifest = Self {
            format_version: VAULT_MANIFEST_FORMAT_VERSION,
            note_id,
            schema_generation,
            note_private_state,
            metadata_state,
            attachment_index,
            revision_index,
            revision_head,
            revision_floor,
            revision_count,
        };
        let note_id = manifest.note_id.clone();
        manifest.validate_for(&note_id)?;
        Ok(manifest)
    }

    pub(crate) fn validate_for(&self, note_id: &NoteId) -> StorageResult<()> {
        if self.format_version != VAULT_MANIFEST_FORMAT_VERSION
            || &self.note_id != note_id
            || self.schema_generation == 0
        {
            return Err(StorageError::Integrity(
                "vault manifest has an invalid version, note identity, or generation".to_string(),
            ));
        }
        let mut object_ids = HashSet::new();
        for object_id in [
            &self.note_private_state,
            &self.metadata_state,
            &self.attachment_index,
            &self.revision_index,
        ] {
            if !object_ids.insert(object_id.as_str()) {
                return Err(StorageError::Integrity(
                    "vault manifest reuses a mutable object identity".to_string(),
                ));
            }
        }

        match (
            self.revision_count,
            self.revision_head.as_ref(),
            self.revision_floor.as_ref(),
        ) {
            (0, None, None) => {}
            (1, Some(head), Some(floor)) if head == floor => {}
            (count, Some(head), Some(floor)) if count > 1 && head != floor => {}
            _ => {
                return Err(StorageError::Integrity(
                    "vault manifest has inconsistent revision-chain bounds".to_string(),
                ));
            }
        }
        for revision_id in [self.revision_head.as_ref(), self.revision_floor.as_ref()]
            .into_iter()
            .flatten()
        {
            if object_ids.contains(revision_id.as_str()) {
                return Err(StorageError::Integrity(
                    "vault manifest reuses a current-state object as a revision".to_string(),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) trait VaultObjectPayload: Serialize + DeserializeOwned {
    const KIND: VaultObjectKind;
    const FORMAT_VERSION: u16;

    fn validate_for(&self, note_id: &NoteId) -> StorageResult<()>;
}

impl VaultObjectPayload for VaultManifest {
    const KIND: VaultObjectKind = VaultObjectKind::Manifest;
    const FORMAT_VERSION: u16 = VAULT_MANIFEST_FORMAT_VERSION;

    fn validate_for(&self, note_id: &NoteId) -> StorageResult<()> {
        VaultManifest::validate_for(self, note_id)
    }
}

macro_rules! implement_sensitive_payload {
    ($type:ty, $kind:expr, $version:expr) => {
        impl VaultObjectPayload for $type {
            const KIND: VaultObjectKind = $kind;
            const FORMAT_VERSION: u16 = $version;

            fn validate_for(&self, note_id: &NoteId) -> StorageResult<()> {
                <$type>::validate_for(self, note_id)
                    .map_err(|message| StorageError::Integrity(message.to_string()))
            }
        }
    };
}

implement_sensitive_payload!(
    NotePrivateState,
    VaultObjectKind::NotePrivateState,
    NOTE_PRIVATE_STATE_FORMAT_VERSION
);
implement_sensitive_payload!(
    MetadataState,
    VaultObjectKind::MetadataState,
    METADATA_STATE_FORMAT_VERSION
);
implement_sensitive_payload!(
    AttachmentIndex,
    VaultObjectKind::AttachmentIndex,
    ATTACHMENT_INDEX_FORMAT_VERSION
);
implement_sensitive_payload!(
    RevisionIndex,
    VaultObjectKind::RevisionIndex,
    REVISION_INDEX_FORMAT_VERSION
);
implement_sensitive_payload!(
    RevisionObject,
    VaultObjectKind::Revision,
    REVISION_OBJECT_FORMAT_VERSION
);

#[derive(Clone)]
pub(crate) struct VaultNoteRootRecord {
    pub(crate) note_id: NoteId,
    pub(crate) format_version: u16,
    pub(crate) key_envelope: Vec<u8>,
    pub(crate) manifest_object_id: VaultObjectId,
}

impl fmt::Debug for VaultNoteRootRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaultNoteRootRecord")
            .field("note_id", &self.note_id)
            .field("format_version", &self.format_version)
            .field("key_envelope", &"[ENCRYPTED]")
            .field("manifest_object_id", &self.manifest_object_id)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VaultGraphHead {
    pub(crate) root: VaultNoteRootRecord,
    pub(crate) manifest: VaultManifest,
}

#[derive(Clone, Debug)]
pub(crate) struct VaultCurrentGraph {
    pub(crate) root: VaultNoteRootRecord,
    pub(crate) manifest: VaultManifest,
    pub(crate) note_private_state: NotePrivateState,
    pub(crate) metadata_state: MetadataState,
    pub(crate) attachment_index: AttachmentIndex,
}

impl Storage {
    pub(crate) fn insert_vault_object<T: VaultObjectPayload>(
        &self,
        transaction: &Transaction<'_>,
        note_key: &VaultKey,
        note_id: &NoteId,
        payload: &T,
    ) -> StorageResult<VaultObjectId> {
        payload.validate_for(note_id)?;
        let object_id = new_vault_object_id();
        let context_bytes =
            vault_object_context_bytes(note_id, T::KIND, &object_id, T::FORMAT_VERSION)?;
        let mut plaintext = serde_json::to_vec(payload)?;
        let ciphertext_result = VaultService::new().seal_object(
            note_key,
            VaultContext::new(T::KIND.domain(), &context_bytes),
            &plaintext,
        );
        plaintext.fill(0);
        let ciphertext = ciphertext_result?;
        transaction.execute(
            "INSERT INTO vault_objects(note_id, object_id, ciphertext) VALUES (?1, ?2, ?3)",
            params![note_id, object_id.as_str(), ciphertext],
        )?;
        Ok(object_id)
    }

    pub(crate) fn read_vault_object<T: VaultObjectPayload>(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
        object_id: &VaultObjectId,
    ) -> StorageResult<T> {
        let ciphertext = connection
            .query_row(
                "SELECT ciphertext FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                params![note_id, object_id.as_str()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::Integrity("vault object is missing".to_string()))?;
        let context_bytes =
            vault_object_context_bytes(note_id, T::KIND, object_id, T::FORMAT_VERSION)?;
        let mut plaintext = VaultService::new().open_object(
            note_key,
            VaultContext::new(T::KIND.domain(), &context_bytes),
            &ciphertext,
        )?;
        let payload_result = serde_json::from_slice::<T>(&plaintext).map_err(StorageError::from);
        plaintext.fill(0);
        let payload = payload_result?;
        payload.validate_for(note_id)?;
        Ok(payload)
    }

    /// Resolve one protected revision through the encrypted retained-revision
    /// index. Only the compact index and selected revision snapshot are opened;
    /// unrelated revision snapshots are never decrypted. Membership is therefore
    /// authoritative even when unreachable/orphan objects temporarily exist.
    pub(crate) fn read_vault_revision_by_handle(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
        manifest: &VaultManifest,
        revision_handle: &str,
    ) -> StorageResult<Option<RevisionObject>> {
        manifest.validate_for(note_id)?;
        let index: RevisionIndex =
            self.read_vault_object(connection, note_key, note_id, &manifest.revision_index)?;
        self.validate_revision_index_for_manifest(manifest, &index)?;
        let Some(entry) = index
            .entries
            .iter()
            .find(|entry| entry.object_id.as_str() == revision_handle)
        else {
            return Ok(None);
        };
        let revision: RevisionObject =
            self.read_vault_object(connection, note_key, note_id, &entry.object_id)?;
        if revision.revision_no != entry.revision_no
            || revision.snapshot_byte_size != entry.snapshot_byte_size
            || revision.created_at != entry.created_at
        {
            return Err(StorageError::Integrity(
                "vault revision index metadata does not match revision object".to_string(),
            ));
        }
        Ok(Some(revision))
    }

    pub(crate) fn read_vault_revision_index(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
        manifest: &VaultManifest,
    ) -> StorageResult<RevisionIndex> {
        manifest.validate_for(note_id)?;
        let index: RevisionIndex =
            self.read_vault_object(connection, note_key, note_id, &manifest.revision_index)?;
        self.validate_revision_index_for_manifest(manifest, &index)?;
        Ok(index)
    }

    fn validate_revision_index_for_manifest(
        &self,
        manifest: &VaultManifest,
        index: &RevisionIndex,
    ) -> StorageResult<()> {
        index
            .validate_for(&manifest.note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        let count = u64::try_from(index.entries.len()).map_err(|_| {
            StorageError::Integrity("vault revision index is too large".to_string())
        })?;
        if count != manifest.revision_count
            || index.entries.first().map(|entry| &entry.object_id)
                != manifest.revision_head.as_ref()
            || index.entries.last().map(|entry| &entry.object_id)
                != manifest.revision_floor.as_ref()
        {
            return Err(StorageError::Integrity(
                "vault revision index does not match manifest bounds".to_string(),
            ));
        }
        Ok(())
    }

    /// Read and validate the retained encrypted revision chain newest → oldest.
    /// Ordinary note-state commits never call this helper; their cost therefore
    /// remains independent of revision-history length.
    pub(crate) fn read_vault_revision_chain(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
        manifest: &VaultManifest,
    ) -> StorageResult<Vec<(VaultObjectId, RevisionObject)>> {
        manifest.validate_for(note_id)?;
        let (Some(mut current), Some(floor)) = (
            manifest.revision_head.clone(),
            manifest.revision_floor.as_ref(),
        ) else {
            // Even an empty history has an encrypted index object. Validate it
            // so corruption cannot be ignored merely because no revision head
            // is present.
            let _ = self.read_vault_revision_index(connection, note_key, note_id, manifest)?;
            return Ok(Vec::new());
        };
        let expected_count = usize::try_from(manifest.revision_count).map_err(|_| {
            StorageError::Integrity("vault revision count is too large".to_string())
        })?;
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut newer_revision_no = None;

        loop {
            if !seen.insert(current.clone()) {
                return Err(StorageError::Integrity(
                    "vault revision chain contains a cycle".to_string(),
                ));
            }
            let revision: RevisionObject =
                self.read_vault_object(connection, note_key, note_id, &current)?;
            if newer_revision_no.is_some_and(|newer| revision.revision_no >= newer) {
                return Err(StorageError::Integrity(
                    "vault revision chain is not strictly newest-to-oldest".to_string(),
                ));
            }
            newer_revision_no = Some(revision.revision_no);
            let reached_floor = &current == floor;
            let predecessor = revision.previous_revision_object.clone();
            chain.push((current.clone(), revision));

            if chain.len() > expected_count {
                return Err(StorageError::Integrity(
                    "vault revision chain exceeds manifest count".to_string(),
                ));
            }
            if reached_floor {
                break;
            }
            current = predecessor.ok_or_else(|| {
                StorageError::Integrity(
                    "vault revision chain ended before its retained floor".to_string(),
                )
            })?;
        }

        if chain.len() != expected_count {
            return Err(StorageError::Integrity(
                "vault revision chain count does not match manifest".to_string(),
            ));
        }
        let index = self.read_vault_revision_index(connection, note_key, note_id, manifest)?;
        for ((object_id, revision), entry) in chain.iter().zip(index.entries.iter()) {
            if object_id != &entry.object_id
                || revision.revision_no != entry.revision_no
                || revision.snapshot_byte_size != entry.snapshot_byte_size
                || revision.created_at != entry.created_at
            {
                return Err(StorageError::Integrity(
                    "vault revision chain does not match encrypted revision index".to_string(),
                ));
            }
        }
        Ok(chain)
    }

    /// Install/replace the canonical encrypted root inside the caller's SQLite
    /// transaction. Referenced encrypted objects must already exist before the
    /// root becomes visible. Their kind/version are cryptographically bound by
    /// the typed open context rather than exposed as plaintext SQLite metadata.
    pub(crate) fn install_vault_note_root(
        &self,
        transaction: &Transaction<'_>,
        note_key: &VaultKey,
        note_id: &NoteId,
        key_envelope: &[u8],
        manifest_object_id: &VaultObjectId,
    ) -> StorageResult<()> {
        if key_envelope.is_empty() {
            return Err(StorageError::Integrity(
                "vault note root has an empty key envelope".to_string(),
            ));
        }
        let manifest: VaultManifest =
            self.read_vault_object(transaction, note_key, note_id, manifest_object_id)?;
        verify_object_exists(transaction, note_id, &manifest.note_private_state)?;
        verify_object_exists(transaction, note_id, &manifest.metadata_state)?;
        verify_object_exists(transaction, note_id, &manifest.attachment_index)?;
        verify_object_exists(transaction, note_id, &manifest.revision_index)?;
        if let Some(revision_head) = manifest.revision_head.as_ref() {
            verify_object_exists(transaction, note_id, revision_head)?;
        }
        if let Some(revision_floor) = manifest.revision_floor.as_ref()
            && manifest.revision_head.as_ref() != Some(revision_floor)
        {
            verify_object_exists(transaction, note_id, revision_floor)?;
        }

        transaction.execute(
            "INSERT INTO vault_note_roots(note_id, format_version, key_envelope, manifest_object_id)\n             VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(note_id) DO UPDATE SET\n                 format_version = excluded.format_version,\n                 key_envelope = excluded.key_envelope,\n                 manifest_object_id = excluded.manifest_object_id",
            params![
                note_id,
                i64::from(VAULT_NOTE_FORMAT_VERSION),
                key_envelope,
                manifest_object_id.as_str(),
            ],
        )?;
        Ok(())
    }

    pub(crate) fn vault_note_root_record(
        &self,
        connection: &Connection,
        note_id: &NoteId,
    ) -> StorageResult<Option<VaultNoteRootRecord>> {
        let row = connection
            .query_row(
                "SELECT format_version, key_envelope, manifest_object_id\n                 FROM vault_note_roots WHERE note_id = ?1",
                [note_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(format_version, key_envelope, manifest_object_id)| {
            let format_version = u16::try_from(format_version).map_err(|_| {
                StorageError::Integrity("vault note root version is invalid".to_string())
            })?;
            if format_version != VAULT_NOTE_FORMAT_VERSION || key_envelope.is_empty() {
                return Err(StorageError::Integrity(
                    "vault note root has an unsupported version or empty key envelope".to_string(),
                ));
            }
            Ok(VaultNoteRootRecord {
                note_id: note_id.clone(),
                format_version,
                key_envelope,
                manifest_object_id: VaultObjectId::parse(manifest_object_id)
                    .map_err(|message| StorageError::Integrity(message.to_string()))?,
            })
        })
        .transpose()
    }

    pub(crate) fn read_vault_graph_head(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
    ) -> StorageResult<Option<VaultGraphHead>> {
        let Some(root) = self.vault_note_root_record(connection, note_id)? else {
            return Ok(None);
        };
        let manifest: VaultManifest =
            self.read_vault_object(connection, note_key, note_id, &root.manifest_object_id)?;
        Ok(Some(VaultGraphHead { root, manifest }))
    }

    pub(crate) fn read_vault_current_graph(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
    ) -> StorageResult<Option<VaultCurrentGraph>> {
        let Some(head) = self.read_vault_graph_head(connection, note_key, note_id)? else {
            return Ok(None);
        };
        let note_private_state: NotePrivateState = self.read_vault_object(
            connection,
            note_key,
            note_id,
            &head.manifest.note_private_state,
        )?;
        let metadata_state: MetadataState =
            self.read_vault_object(connection, note_key, note_id, &head.manifest.metadata_state)?;
        let attachment_index: AttachmentIndex = self.read_vault_object(
            connection,
            note_key,
            note_id,
            &head.manifest.attachment_index,
        )?;
        Ok(Some(VaultCurrentGraph {
            root: head.root,
            manifest: head.manifest,
            note_private_state,
            metadata_state,
            attachment_index,
        }))
    }

    /// Atomically replace the canonical manifest/root after callers have inserted
    /// any new component objects. The previous manifest and superseded mutable
    /// objects are removed in the same SQLite transaction, so a crash observes
    /// either the complete old graph or the complete new graph.
    pub(crate) fn commit_vault_manifest_root(
        &self,
        transaction: &Transaction<'_>,
        note_key: &VaultKey,
        root: &VaultNoteRootRecord,
        new_manifest: &VaultManifest,
    ) -> StorageResult<VaultObjectId> {
        // Derive retirement exclusively from the manifest authenticated by the
        // currently committed root. Callers cannot nominate either the old
        // graph or any object for deletion.
        let old_manifest: VaultManifest = self.read_vault_object(
            transaction,
            note_key,
            &root.note_id,
            &root.manifest_object_id,
        )?;
        old_manifest.validate_for(&root.note_id)?;
        new_manifest.validate_for(&root.note_id)?;
        if new_manifest.schema_generation != old_manifest.schema_generation.saturating_add(1) {
            return Err(StorageError::Integrity(
                "vault manifest generation did not advance exactly once".to_string(),
            ));
        }
        let manifest_object_id =
            self.insert_vault_object(transaction, note_key, &root.note_id, new_manifest)?;
        self.install_vault_note_root(
            transaction,
            note_key,
            &root.note_id,
            &root.key_envelope,
            &manifest_object_id,
        )?;
        // Superseded mutable objects are derived here, never supplied by the
        // caller. This keeps revision/history objects outside the mutable-object
        // deletion authority and prevents a stale or malformed caller list from
        // deleting an object still referenced by the newly committed manifest.
        for (old_object_id, new_object_id) in [
            (
                &old_manifest.note_private_state,
                &new_manifest.note_private_state,
            ),
            (&old_manifest.metadata_state, &new_manifest.metadata_state),
            (
                &old_manifest.attachment_index,
                &new_manifest.attachment_index,
            ),
            (&old_manifest.revision_index, &new_manifest.revision_index),
        ] {
            if old_object_id != new_object_id {
                transaction.execute(
                    "DELETE FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                    params![&root.note_id, old_object_id.as_str()],
                )?;
            }
        }
        if root.manifest_object_id != manifest_object_id {
            transaction.execute(
                "DELETE FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                params![&root.note_id, root.manifest_object_id.as_str()],
            )?;
        }
        Ok(manifest_object_id)
    }

    pub(crate) fn replace_vault_blob_refs(
        &self,
        transaction: &Transaction<'_>,
        note_id: &NoteId,
        attachment_index: &AttachmentIndex,
    ) -> StorageResult<()> {
        attachment_index
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        transaction.execute("DELETE FROM vault_blob_refs WHERE note_id = ?1", [note_id])?;
        for blob in &attachment_index.blobs {
            transaction.execute(
                "INSERT INTO vault_blob_refs(note_id, sha256, byte_size) VALUES (?1, ?2, ?3)",
                params![note_id, &blob.ciphertext_sha256, blob.ciphertext_byte_size],
            )?;
        }
        Ok(())
    }

    pub(crate) fn delete_unreachable_vault_objects(
        &self,
        transaction: &Transaction<'_>,
        note_key: &VaultKey,
        note_id: &NoteId,
    ) -> StorageResult<usize> {
        let Some(root) = self.vault_note_root_record(transaction, note_id)? else {
            return Ok(0);
        };
        let manifest: VaultManifest =
            self.read_vault_object(transaction, note_key, note_id, &root.manifest_object_id)?;
        let mut reachable = HashSet::from([
            root.manifest_object_id.clone(),
            manifest.note_private_state.clone(),
            manifest.metadata_state.clone(),
            manifest.attachment_index.clone(),
            manifest.revision_index.clone(),
        ]);
        for (object_id, _) in
            self.read_vault_revision_chain(transaction, note_key, note_id, &manifest)?
        {
            reachable.insert(object_id);
        }
        let object_ids = {
            let mut statement =
                transaction.prepare("SELECT object_id FROM vault_objects WHERE note_id = ?1")?;
            let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut removed = 0;
        for raw in object_ids {
            let object_id = VaultObjectId::parse(raw)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            if !reachable.contains(&object_id) {
                removed += transaction.execute(
                    "DELETE FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                    params![note_id, object_id.as_str()],
                )?;
            }
        }
        Ok(removed)
    }
}

fn verify_object_exists(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
    object_id: &VaultObjectId,
) -> StorageResult<()> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM vault_objects WHERE note_id = ?1 AND object_id = ?2)",
        params![note_id, object_id.as_str()],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(StorageError::Integrity(
            "vault root references a missing encrypted object".to_string(),
        ));
    }
    Ok(())
}

fn new_vault_object_id() -> VaultObjectId {
    let bytes = rand::random::<[u8; VAULT_OBJECT_ID_BYTES]>();
    let mut value = String::with_capacity(VAULT_OBJECT_ID_HEX_LEN);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing into String cannot fail");
    }
    VaultObjectId::parse(value).expect("fresh random Vault object id must be valid")
}

/// Stable, unambiguous AAD object identity. The encrypted-object container also
/// authenticates its own format header; this binds the *Rustnote logical object*
/// to note identity, kind, opaque object ID and logical payload version.
fn vault_object_context_bytes(
    note_id: &NoteId,
    kind: VaultObjectKind,
    object_id: &VaultObjectId,
    format_version: u16,
) -> StorageResult<Vec<u8>> {
    let note_bytes = note_id.as_str().as_bytes();
    let object_bytes = object_id.as_str().as_bytes();
    let note_len = u16::try_from(note_bytes.len())
        .map_err(|_| StorageError::Integrity("vault context note id is too long".to_string()))?;
    let object_len = u16::try_from(object_bytes.len())
        .map_err(|_| StorageError::Integrity("vault context object id is too long".to_string()))?;

    let mut context = Vec::with_capacity(32 + note_bytes.len() + object_bytes.len());
    context.extend_from_slice(b"rustnote-vault-object\0");
    context.extend_from_slice(&VAULT_OBJECT_CONTEXT_VERSION.to_le_bytes());
    context.push(match kind {
        VaultObjectKind::Manifest => 1,
        VaultObjectKind::NotePrivateState => 2,
        VaultObjectKind::MetadataState => 3,
        VaultObjectKind::AttachmentIndex => 4,
        VaultObjectKind::Revision => 5,
        VaultObjectKind::RevisionIndex => 6,
    });
    context.extend_from_slice(&format_version.to_le_bytes());
    context.extend_from_slice(&note_len.to_le_bytes());
    context.extend_from_slice(note_bytes);
    context.extend_from_slice(&object_len.to_le_bytes());
    context.extend_from_slice(object_bytes);
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{NoteId, NoteTitleStyle, RevisionSnapshot},
        security::{Argon2idParams, VaultError},
        storage::initialization::EXAMPLE_NOTE_ID,
    };

    #[test]
    fn object_ids_are_opaque_random_fixed_width_hex() {
        let first = new_vault_object_id();
        let second = new_vault_object_id();
        assert_ne!(first, second);
        assert_eq!(first.as_str().len(), VAULT_OBJECT_ID_HEX_LEN);
        VaultObjectId::parse(first.as_str()).unwrap();
        assert!(VaultObjectId::parse("not-an-object-id").is_err());
    }

    #[test]
    fn authenticated_context_changes_for_note_kind_object_and_format() {
        let note_a = NoteId::parse("note-a").unwrap();
        let note_b = NoteId::parse("note-b").unwrap();
        let object_a = VaultObjectId::parse("00".repeat(VAULT_OBJECT_ID_BYTES)).unwrap();
        let object_b = VaultObjectId::parse("11".repeat(VAULT_OBJECT_ID_BYTES)).unwrap();
        let base = vault_object_context_bytes(
            &note_a,
            VaultObjectKind::NotePrivateState,
            &object_a,
            NOTE_PRIVATE_STATE_FORMAT_VERSION,
        )
        .unwrap();
        assert_ne!(
            base,
            vault_object_context_bytes(
                &note_b,
                VaultObjectKind::NotePrivateState,
                &object_a,
                NOTE_PRIVATE_STATE_FORMAT_VERSION,
            )
            .unwrap()
        );
        assert_ne!(
            base,
            vault_object_context_bytes(
                &note_a,
                VaultObjectKind::MetadataState,
                &object_a,
                NOTE_PRIVATE_STATE_FORMAT_VERSION,
            )
            .unwrap()
        );
        assert_ne!(
            base,
            vault_object_context_bytes(
                &note_a,
                VaultObjectKind::NotePrivateState,
                &object_b,
                NOTE_PRIVATE_STATE_FORMAT_VERSION,
            )
            .unwrap()
        );
        assert_ne!(
            base,
            vault_object_context_bytes(
                &note_a,
                VaultObjectKind::NotePrivateState,
                &object_a,
                NOTE_PRIVATE_STATE_FORMAT_VERSION + 1,
            )
            .unwrap()
        );
    }

    #[test]
    fn granular_object_store_round_trips_with_authenticated_root() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-2b1-object-store-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups")).unwrap();
        let note_id = NoteId::parse(EXAMPLE_NOTE_ID).unwrap();
        let vault = VaultService::new();
        let note_key = vault.generate_key().unwrap();
        let note = storage.get_note(&note_id).unwrap().unwrap();
        let key_envelope = vault
            .wrap_key_with_password(
                &note_key,
                b"2b1-test-password",
                VaultContext::new(VaultDomain::KeyEnvelope, note_id.as_str().as_bytes()),
                Argon2idParams::V1_INTERACTIVE,
            )
            .unwrap();

        let mut connection = storage.connection().unwrap();
        let transaction = connection.transaction().unwrap();
        let private_state = NotePrivateState {
            format_version: NOTE_PRIVATE_STATE_FORMAT_VERSION,
            note_id: note_id.clone(),
            title: "2b private title".to_string(),
            title_style: NoteTitleStyle::default(),
            editor_state: crate::editor_document::EMPTY_EDITOR_STATE
                .trim()
                .to_string(),
        };
        let metadata = MetadataState {
            format_version: METADATA_STATE_FORMAT_VERSION,
            note_id: note_id.clone(),
            tags: vec!["vault".to_string()],
            outgoing_note_links: Vec::new(),
            sources: Vec::new(),
            pdf_annotations: Vec::new(),
        };
        let attachment_index = AttachmentIndex {
            format_version: ATTACHMENT_INDEX_FORMAT_VERSION,
            note_id: note_id.clone(),
            attachments: Vec::new(),
            blobs: Vec::new(),
        };
        let private_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &private_state)
            .unwrap();
        let metadata_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &metadata)
            .unwrap();
        let attachment_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &attachment_index)
            .unwrap();
        let revision_one = RevisionObject::new(
            note_id.clone(),
            "rev-2b1-1".to_string(),
            1,
            None,
            1,
            RevisionSnapshot::new(&note, Vec::new()),
        )
        .unwrap();
        let revision_one_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &revision_one)
            .unwrap();
        let revision_two = RevisionObject::new(
            note_id.clone(),
            "rev-2b1-2".to_string(),
            2,
            Some(revision_one_id.clone()),
            2,
            RevisionSnapshot::new(&note, Vec::new()),
        )
        .unwrap();
        let revision_two_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &revision_two)
            .unwrap();
        // A valid same-note revision object that is deliberately not retained by
        // the manifest/index. Direct lookup must reject it even while GC has not
        // yet removed the unreachable ciphertext row.
        let orphan_revision = RevisionObject::new(
            note_id.clone(),
            "rev-2b1-orphan".to_string(),
            3,
            Some(revision_two_id.clone()),
            3,
            RevisionSnapshot::new(&note, Vec::new()),
        )
        .unwrap();
        let orphan_revision_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &orphan_revision)
            .unwrap();
        let revision_index = RevisionIndex::from_newest_first(
            note_id.clone(),
            [
                (
                    revision_two_id.clone(),
                    revision_two.revision_no,
                    revision_two.snapshot_byte_size,
                    revision_two.created_at,
                ),
                (
                    revision_one_id.clone(),
                    revision_one.revision_no,
                    revision_one.snapshot_byte_size,
                    revision_one.created_at,
                ),
            ],
        )
        .unwrap();
        let revision_index_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &revision_index)
            .unwrap();
        let manifest = VaultManifest::new(
            note_id.clone(),
            1,
            private_id.clone(),
            metadata_id.clone(),
            attachment_id.clone(),
            revision_index_id.clone(),
            Some(revision_two_id.clone()),
            Some(revision_one_id.clone()),
            2,
        )
        .unwrap();
        let manifest_id = storage
            .insert_vault_object(&transaction, &note_key, &note_id, &manifest)
            .unwrap();
        assert!(matches!(
            storage.install_vault_note_root(
                &transaction,
                &note_key,
                &note_id,
                &key_envelope,
                &private_id,
            ),
            Err(StorageError::Vault(VaultError::AuthenticationFailed))
        ));
        storage
            .install_vault_note_root(
                &transaction,
                &note_key,
                &note_id,
                &key_envelope,
                &manifest_id,
            )
            .unwrap();
        transaction.commit().unwrap();

        let connection = storage.connection().unwrap();
        let root_record = storage
            .vault_note_root_record(&connection, &note_id)
            .unwrap()
            .unwrap();
        assert_eq!(root_record.format_version, VAULT_NOTE_FORMAT_VERSION);
        assert_eq!(root_record.manifest_object_id, manifest_id);
        let opened_manifest: VaultManifest = storage
            .read_vault_object(&connection, &note_key, &note_id, &manifest_id)
            .unwrap();
        assert_eq!(opened_manifest, manifest);
        let revisions = storage
            .read_vault_revision_chain(&connection, &note_key, &note_id, &opened_manifest)
            .unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].0, revision_two_id);
        assert_eq!(revisions[0].1.revision_no, 2);
        assert_eq!(revisions[1].0, revision_one_id);
        assert_eq!(revisions[1].1.revision_no, 1);
        let direct_revision = storage
            .read_vault_revision_by_handle(
                &connection,
                &note_key,
                &note_id,
                &opened_manifest,
                revision_one_id.as_str(),
            )
            .unwrap()
            .expect("direct revision handle");
        assert_eq!(direct_revision.revision_no, 1);
        assert_eq!(direct_revision.revision_id, "rev-2b1-1");
        assert!(
            storage
                .read_vault_revision_by_handle(
                    &connection,
                    &note_key,
                    &note_id,
                    &opened_manifest,
                    orphan_revision_id.as_str(),
                )
                .unwrap()
                .is_none(),
            "a valid same-note orphan revision must not bypass retained-index membership",
        );
        assert!(
            storage
                .read_vault_revision_by_handle(
                    &connection,
                    &note_key,
                    &note_id,
                    &opened_manifest,
                    "00000000000000000000000000000000",
                )
                .unwrap()
                .is_none(),
            "a stale or pruned opaque revision handle is an expected absence",
        );
        let opened_private: NotePrivateState = storage
            .read_vault_object(&connection, &note_key, &note_id, &private_id)
            .unwrap();
        assert_eq!(opened_private.title, "2b private title");
        assert!(matches!(
            storage.read_vault_object::<MetadataState>(
                &connection,
                &note_key,
                &note_id,
                &private_id,
            ),
            Err(StorageError::Vault(VaultError::AuthenticationFailed))
        ));

        let ciphertext: Vec<u8> = connection
            .query_row(
                "SELECT ciphertext FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                params![&note_id, private_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            !ciphertext
                .windows(b"2b private title".len())
                .any(|window| window == b"2b private title")
        );

        let mut tampered = ciphertext;
        let last = tampered
            .last_mut()
            .expect("encrypted Vault object must contain authenticated ciphertext");
        *last ^= 0x01;
        connection
            .execute(
                "UPDATE vault_objects SET ciphertext = ?1 WHERE note_id = ?2 AND object_id = ?3",
                params![tampered, &note_id, private_id.as_str()],
            )
            .unwrap();
        assert!(matches!(
            storage.read_vault_object::<NotePrivateState>(
                &connection,
                &note_key,
                &note_id,
                &private_id,
            ),
            Err(StorageError::Vault(VaultError::AuthenticationFailed))
        ));
        drop(connection);
        drop(storage);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ciphertext_cannot_open_under_a_different_logical_object_context() {
        let vault = VaultService::new();
        let key = vault.generate_key().unwrap();
        let note = NoteId::parse("note-context").unwrap();
        let object_a = VaultObjectId::parse("22".repeat(VAULT_OBJECT_ID_BYTES)).unwrap();
        let object_b = VaultObjectId::parse("33".repeat(VAULT_OBJECT_ID_BYTES)).unwrap();
        let context_a = vault_object_context_bytes(
            &note,
            VaultObjectKind::NotePrivateState,
            &object_a,
            NOTE_PRIVATE_STATE_FORMAT_VERSION,
        )
        .unwrap();
        let context_b = vault_object_context_bytes(
            &note,
            VaultObjectKind::NotePrivateState,
            &object_b,
            NOTE_PRIVATE_STATE_FORMAT_VERSION,
        )
        .unwrap();
        let ciphertext = vault
            .seal_object(
                &key,
                VaultContext::new(VaultDomain::NotePrivateState, &context_a),
                b"secret",
            )
            .unwrap();
        assert!(matches!(
            vault.open_object(
                &key,
                VaultContext::new(VaultDomain::NotePrivateState, &context_b),
                &ciphertext,
            ),
            Err(VaultError::AuthenticationFailed)
        ));
    }
}
