//! Canonical 2b privacy specification for note-owned data.
//!
//! The important property of this module is not merely the DTO vocabulary. The
//! `SensitiveNoteData::classify` destructuring below is deliberately exhaustive:
//! adding a field to `Note` or `NoteDocument` forces this privacy authority to be
//! revisited at compile time instead of silently inheriting a plaintext policy.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    domain::{
        Attachment, Citation, Note, NoteDocument, NoteId, NoteTitleStyle, RevisionSnapshot, Source,
    },
    security::vault_object_spec::VaultObjectId,
};

pub(crate) const SENSITIVE_NOTE_DATA_SPEC_VERSION: u16 = 1;
pub(crate) const NOTE_PRIVATE_STATE_FORMAT_VERSION: u16 = 1;
pub(crate) const METADATA_STATE_FORMAT_VERSION: u16 = 1;
pub(crate) const ATTACHMENT_INDEX_FORMAT_VERSION: u16 = 1;
pub(crate) const REVISION_OBJECT_FORMAT_VERSION: u16 = 1;
pub(crate) const REVISION_INDEX_FORMAT_VERSION: u16 = 1;
pub(crate) const VAULT_MANIFEST_FORMAT_VERSION: u16 = 2;
pub(crate) const VAULT_NOTE_FORMAT_VERSION: u16 = 3;

/// Explicit plaintext allowlist for a protected note's ordinary SQLite stub.
///
/// Everything note-owned that is not represented here (or documented as a
/// sanitized constant/derived value below) is private by default.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedScrubStep {
    NoteSensitiveColumns,
    AttachmentSensitiveColumns,
    FullTextIndex,
    TagsAndOrphans,
    Revisions,
    OutgoingLinks,
    SourcesCitationsAndOrphans,
    PdfAnnotations,
    SyncQueue,
    PersistedFilterTags,
}

impl ProtectedScrubStep {
    #[cfg(test)]
    pub(crate) fn sensitive_relational_surfaces(self) -> &'static [&'static str] {
        match self {
            Self::NoteSensitiveColumns | Self::AttachmentSensitiveColumns => &[],
            Self::FullTextIndex => &["notes_fts"],
            Self::TagsAndOrphans => &["note_tags", "orphan protected-only tags"],
            Self::Revisions => &["note_revisions", "revision_attachment_refs"],
            Self::OutgoingLinks => &["outgoing note_links"],
            Self::SourcesCitationsAndOrphans => &[
                "note_sources",
                "note-owned citations",
                "orphan protected-only sources",
            ],
            Self::PdfAnnotations => &["note attachment pdf_annotation_pages"],
            Self::SyncQueue => &["note/attachment sync_queue payloads"],
            Self::PersistedFilterTags => &[
                "persisted note-list filter tags containing protected-only labels",
            ],
        }
    }
}

pub(crate) struct ProtectedPublicPersistenceSpec;

impl ProtectedPublicPersistenceSpec {
    /// Canonical ordinary-persistence scrub plan. The storage scrubber iterates
    /// this list directly; adding a newly classified persistence surface without
    /// an executable scrub step therefore fails validation instead of relying on
    /// a feature author to remember a second independent scrubber list.
    pub(crate) const SCRUB_STEPS: &'static [ProtectedScrubStep] = &[
        ProtectedScrubStep::AttachmentSensitiveColumns,
        ProtectedScrubStep::NoteSensitiveColumns,
        ProtectedScrubStep::FullTextIndex,
        ProtectedScrubStep::TagsAndOrphans,
        ProtectedScrubStep::Revisions,
        ProtectedScrubStep::OutgoingLinks,
        ProtectedScrubStep::SourcesCitationsAndOrphans,
        ProtectedScrubStep::PdfAnnotations,
        ProtectedScrubStep::SyncQueue,
        ProtectedScrubStep::PersistedFilterTags,
    ];

    #[cfg(test)]
    pub(crate) const NOTE_COLUMNS: &'static [&'static str] = &[
        "id",
        "fts_rowid",
        "notebook_id",
        "pinned",
        "archived",
        "trashed",
        "purge_after",
        "created_at",
        "updated_at",
    ];

    pub(crate) const NOTE_SANITIZED_COLUMNS: &'static [&'static str] = &[
        "title",
        "title_style",
        "body",
        "editor_state",
        "plain_text",
    ];

    #[cfg(test)]
    pub(crate) const ATTACHMENT_COLUMNS: &'static [&'static str] = &[
        "id",
        "note_id",
        // These values identify the encrypted blob, never its plaintext hash.
        "sha256",
        "byte_size",
        "unreferenced_at",
        "created_at",
        "updated_at",
    ];

    pub(crate) const ATTACHMENT_SANITIZED_COLUMNS: &'static [&'static str] = &[
        "file_name",
        "mime_type",
        "image_pixel_width",
        "image_pixel_height",
        "image_format",
        "image_orientation",
        "image_has_alpha",
        "image_animated",
        "source_attachment_id",
        "image_edit_recipe",
        "embedded",
    ];

    /// Minimum plaintext routing envelope for granular encrypted objects.
    /// Object kind/version and mutation timing stay authenticated inside the
    /// encrypted object/context rather than becoming plaintext activity metadata.
    #[cfg(test)]
    pub(crate) const VAULT_OBJECT_COLUMNS: &'static [&'static str] =
        &["note_id", "object_id", "ciphertext"];

    #[cfg(test)]
    pub(crate) const VAULT_ROOT_COLUMNS: &'static [&'static str] = &[
        "note_id",
        "format_version",
        "key_envelope",
        "manifest_object_id",
    ];

    /// Current schema surfaces with a direct note identity that may remain
    /// populated while a note is protected. `vault_blob_refs` contains only
    /// ciphertext routing identities/sizes; sensitive attachment metadata and
    /// plaintext hashes live inside the encrypted `AttachmentIndex`.
    #[cfg(test)]
    pub(crate) const CURRENT_PROTECTED_NOTE_SAFE_TABLES: &'static [&'static str] = &[
        "notes",
        "vault_operations",
        "attachments",
        "note_parent",
        "vault_objects",
        "vault_note_roots",
        "vault_blob_refs",
    ];

    /// Direct note-owned tables that must contain no row owned by a protected
    /// note in ordinary persistence.
    #[cfg(test)]
    pub(crate) const CURRENT_PROTECTED_NOTE_FORBIDDEN_TABLES: &'static [&'static str] = &[
        "note_tags",
        "note_revisions",
        "note_sources",
        "citations",
        "attachment_ingest_jobs",
        "attachment_operations",
        "managed_plaintext_staging",
    ];

    /// Surfaces whose note ownership is directional or indirect and therefore
    /// cannot be classified solely by looking for a `note_id` column.
    #[cfg(test)]
    pub(crate) const CURRENT_PROTECTED_NOTE_SPECIAL_SURFACES: &'static [&'static str] = &[
        "notes_fts: protected note row forbidden",
        "note_links: outgoing rows forbidden; incoming rows public structural",
        "revision_attachment_refs: forbidden through protected revisions",
        "pdf_annotation_pages: forbidden through protected attachments",
        "sync_queue: note/attachment payload rows forbidden",
        "tags: orphan protected-only tag names must be removed",
        "sources: orphan protected-only source records must be removed",
        "settings: persisted note-list filter tags must not retain protected-only labels",
    ];

    /// Protected-note rows are forbidden in these ordinary plaintext
    /// representations. Incoming links from another note are not note-owned by
    /// the protected source note; the protected note's *outgoing* links are.
    #[cfg(test)]
    pub(crate) const SENSITIVE_RELATIONAL_SURFACES: &'static [&'static str] = &[
        "notes_fts",
        "note_tags",
        "note_revisions",
        "revision_attachment_refs",
        "outgoing note_links",
        "note_sources",
        "note-owned citations",
        "note attachment pdf_annotation_pages",
        "note/attachment sync_queue payloads",
        "orphan protected-only tags",
        "orphan protected-only sources",
        "persisted note-list filter tags containing protected-only labels",
    ];

    #[cfg(test)]
    pub(crate) const PUBLIC_STRUCTURAL_SURFACES: &'static [&'static str] = &[
        "notebook placement",
        "note_parent hierarchy",
        "pinned/archive/trash state",
        "opaque Vault object/root routing plus ciphertext/key envelope",
    ];

    #[cfg(test)]
    pub(crate) const DERIVED_SENSITIVE_FIELDS: &'static [&'static str] = &[
        "NoteDocument.body",
        "NoteDocument.plain_text",
        "Note.attachment_names",
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicProtectedNoteStub {
    pub(crate) note_id: NoteId,
    /// User-approved non-secret label intentionally retained in ordinary SQLite
    /// so locked notes remain distinguishable without exposing the private title.
    pub(crate) locked_label: String,
    pub(crate) notebook_id: String,
    pub(crate) pinned: bool,
    pub(crate) archived: bool,
    pub(crate) trashed: bool,
    pub(crate) purge_after: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotePrivateState {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    pub(crate) title: String,
    pub(crate) title_style: NoteTitleStyle,
    /// Canonical editor document. Markdown/plain-text projections are derived.
    pub(crate) editor_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SensitiveSourceLink {
    pub(crate) source: Source,
    pub(crate) linked_at: i64,
    pub(crate) citations: Vec<Citation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SensitivePdfAnnotation {
    pub(crate) attachment_id: String,
    pub(crate) page_index: i64,
    pub(crate) annotations_json: String,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetadataState {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    pub(crate) tags: Vec<String>,
    pub(crate) outgoing_note_links: Vec<NoteId>,
    pub(crate) sources: Vec<SensitiveSourceLink>,
    pub(crate) pdf_annotations: Vec<SensitivePdfAnnotation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VaultBlobReference {
    pub(crate) plaintext_sha256: String,
    pub(crate) plaintext_byte_size: i64,
    pub(crate) ciphertext_sha256: String,
    pub(crate) ciphertext_byte_size: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttachmentIndex {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    pub(crate) attachments: Vec<Attachment>,
    /// Includes current and revision-only protected blobs. Plaintext hashes live
    /// only inside this encrypted object.
    pub(crate) blobs: Vec<VaultBlobReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionObject {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    pub(crate) revision_id: String,
    pub(crate) revision_no: i64,
    /// Opaque predecessor in the immutable encrypted revision chain. The
    /// manifest carries the authoritative retained head/floor, so pruning an
    /// old prefix never requires rewriting newer revision objects.
    pub(crate) previous_revision_object: Option<VaultObjectId>,
    pub(crate) snapshot_checksum: String,
    pub(crate) snapshot_byte_size: i64,
    pub(crate) created_at: i64,
    pub(crate) snapshot: RevisionSnapshot,
}

/// Compact encrypted retained-revision authority. It deliberately contains only
/// opaque object identities plus summary metadata, never revision snapshots.
/// Ordinary note-state autosave keeps this object unchanged; revision append or
/// retention rewrites only this small index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionIndexEntry {
    pub(crate) object_id: VaultObjectId,
    pub(crate) revision_no: i64,
    pub(crate) snapshot_byte_size: i64,
    pub(crate) created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionIndex {
    pub(crate) format_version: u16,
    pub(crate) note_id: NoteId,
    /// Newest -> oldest retained revisions.
    pub(crate) entries: Vec<RevisionIndexEntry>,
}

/// Logical privacy aggregate used to prove classification completeness.
///
/// Deliberately **not** serializable: 2b must not accidentally recreate the 2a
/// giant encrypted payload. Persistence stores the component objects separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SensitiveNoteData {
    pub(crate) spec_version: u16,
    pub(crate) public_stub: PublicProtectedNoteStub,
    pub(crate) note_private_state: NotePrivateState,
    pub(crate) metadata_state: MetadataState,
    pub(crate) attachment_index: AttachmentIndex,
    pub(crate) revisions: Vec<RevisionObject>,
}

impl SensitiveNoteData {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn classify(
        note: &Note,
        locked_label: String,
        attachments: Vec<Attachment>,
        blobs: Vec<VaultBlobReference>,
        outgoing_note_links: Vec<NoteId>,
        sources: Vec<SensitiveSourceLink>,
        pdf_annotations: Vec<SensitivePdfAnnotation>,
        revisions: Vec<RevisionObject>,
    ) -> Self {
        // Exhaustive destructuring is intentional. A future Note field must be
        // classified here before the crate can compile.
        let Note {
            id,
            protection: _protection_runtime_only,
            notebook_id,
            title,
            title_style,
            document,
            tags,
            attachment_names: _derived_attachment_names,
            pinned,
            archived,
            trashed,
            purge_after,
            created_at,
            updated_at,
        } = note;
        let NoteDocument {
            body: _derived_markdown,
            editor_state,
            plain_text: _derived_plain_text,
        } = document.as_ref();

        Self {
            spec_version: SENSITIVE_NOTE_DATA_SPEC_VERSION,
            public_stub: PublicProtectedNoteStub {
                note_id: id.clone(),
                locked_label,
                notebook_id: notebook_id.clone(),
                pinned: *pinned,
                archived: *archived,
                trashed: *trashed,
                purge_after: *purge_after,
                created_at: *created_at,
                updated_at: *updated_at,
            },
            note_private_state: NotePrivateState {
                format_version: NOTE_PRIVATE_STATE_FORMAT_VERSION,
                note_id: id.clone(),
                title: title.clone(),
                title_style: title_style.clone(),
                editor_state: editor_state.as_str().to_string(),
            },
            metadata_state: MetadataState {
                format_version: METADATA_STATE_FORMAT_VERSION,
                note_id: id.clone(),
                tags: tags.clone(),
                outgoing_note_links,
                sources,
                pdf_annotations,
            },
            attachment_index: AttachmentIndex {
                format_version: ATTACHMENT_INDEX_FORMAT_VERSION,
                note_id: id.clone(),
                attachments,
                blobs,
            },
            revisions,
        }
    }

    #[cfg(test)]
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.spec_version != SENSITIVE_NOTE_DATA_SPEC_VERSION {
            return Err("unsupported sensitive-note specification version");
        }
        let note_id = &self.public_stub.note_id;
        self.note_private_state.validate_for(note_id)?;
        self.metadata_state.validate_for(note_id)?;
        self.attachment_index.validate_for(note_id)?;

        let current_attachment_ids = self
            .attachment_index
            .attachments
            .iter()
            .map(|attachment| attachment.id.as_str())
            .collect::<HashSet<_>>();
        for annotation in &self.metadata_state.pdf_annotations {
            if !current_attachment_ids.contains(annotation.attachment_id.as_str()) {
                return Err("PDF annotation references a non-current attachment");
            }
        }

        let mut revision_ids = HashSet::new();
        let mut revision_numbers = HashSet::new();
        let mut required_plaintext_blobs = self
            .attachment_index
            .attachments
            .iter()
            .map(|attachment| attachment.sha256.as_str())
            .collect::<HashSet<_>>();
        for revision in &self.revisions {
            revision.validate_for(note_id)?;
            if !revision_ids.insert(revision.revision_id.as_str())
                || !revision_numbers.insert(revision.revision_no)
            {
                return Err("duplicate revision identity or number");
            }
            required_plaintext_blobs.extend(
                revision
                    .snapshot
                    .attachments
                    .iter()
                    .map(|attachment| attachment.sha256.as_str()),
            );
        }
        let available_plaintext_blobs = self
            .attachment_index
            .blobs
            .iter()
            .map(|blob| blob.plaintext_sha256.as_str())
            .collect::<HashSet<_>>();
        if required_plaintext_blobs != available_plaintext_blobs {
            return Err("attachment blob catalog does not exactly cover current/revision state");
        }
        Ok(())
    }
}

impl NotePrivateState {
    pub(crate) fn validate_for(&self, note_id: &NoteId) -> Result<(), &'static str> {
        if self.format_version != NOTE_PRIVATE_STATE_FORMAT_VERSION || &self.note_id != note_id {
            return Err("invalid private-state version or note identity");
        }
        if self.title_style.clone().normalized() != self.title_style {
            return Err("private title style is not normalized");
        }
        crate::editor_document::analyze_editor_state(&self.editor_state)
            .map_err(|_| "private editor document is invalid")?;
        Ok(())
    }
}

impl MetadataState {
    pub(crate) fn validate_for(&self, note_id: &NoteId) -> Result<(), &'static str> {
        if self.format_version != METADATA_STATE_FORMAT_VERSION || &self.note_id != note_id {
            return Err("invalid metadata-state version or note identity");
        }
        let mut tags = HashSet::new();
        for tag in &self.tags {
            let normalized = tag.trim().to_lowercase();
            if normalized.is_empty() || !tags.insert(normalized) {
                return Err("metadata contains empty or duplicate tags");
            }
        }
        let mut outgoing = HashSet::new();
        for target in &self.outgoing_note_links {
            if !outgoing.insert(target.as_str()) {
                return Err("metadata contains duplicate outgoing note links");
            }
        }
        let mut source_ids = HashSet::new();
        let mut citation_ids = HashSet::new();
        for source_link in &self.sources {
            if !source_ids.insert(source_link.source.id.as_str()) {
                return Err("metadata contains duplicate source identities");
            }
            for citation in &source_link.citations {
                if &citation.note_id != note_id
                    || citation.source_id != source_link.source.id
                    || citation.id.is_empty()
                    || citation.locator.trim().is_empty()
                    || citation.locator.len() > 1024
                    || !citation_ids.insert(citation.id.as_str())
                {
                    return Err("metadata contains an invalid citation");
                }
            }
        }
        let mut annotation_keys = HashSet::new();
        for annotation in &self.pdf_annotations {
            if annotation.attachment_id.is_empty()
                || annotation.page_index < 0
                || serde_json::from_str::<serde_json::Value>(&annotation.annotations_json).is_err()
                || !annotation_keys.insert((annotation.attachment_id.as_str(), annotation.page_index))
            {
                return Err("metadata contains an invalid PDF annotation");
            }
        }
        Ok(())
    }
}

impl AttachmentIndex {
    pub(crate) fn validate_for(&self, note_id: &NoteId) -> Result<(), &'static str> {
        if self.format_version != ATTACHMENT_INDEX_FORMAT_VERSION || &self.note_id != note_id {
            return Err("invalid attachment-index version or note identity");
        }
        let mut attachment_ids = HashSet::new();
        let mut source_by_attachment = HashMap::new();
        for attachment in &self.attachments {
            if &attachment.note_id != note_id
                || attachment.id.is_empty()
                || attachment.byte_size < 0
                || !valid_sha256(&attachment.sha256)
                || !attachment_ids.insert(attachment.id.as_str())
            {
                return Err("attachment index contains an invalid attachment");
            }
            source_by_attachment.insert(
                attachment.id.as_str(),
                attachment.source_attachment_id.as_deref(),
            );
        }
        for attachment in &self.attachments {
            if let Some(source_id) = attachment.source_attachment_id.as_deref()
                && (source_id == attachment.id || !attachment_ids.contains(source_id))
            {
                return Err("attachment index contains an invalid edited-image source");
            }
            let mut seen = HashSet::new();
            let mut cursor = Some(attachment.id.as_str());
            while let Some(current) = cursor {
                if !seen.insert(current) {
                    return Err("attachment index contains a source cycle");
                }
                cursor = source_by_attachment.get(current).copied().flatten();
            }
        }

        let mut plaintext = HashSet::new();
        let mut ciphertext = HashSet::new();
        let mut blob_sizes = HashMap::new();
        for blob in &self.blobs {
            if blob.plaintext_byte_size < 0
                || blob.ciphertext_byte_size <= 0
                || !valid_sha256(&blob.plaintext_sha256)
                || !valid_sha256(&blob.ciphertext_sha256)
                || !plaintext.insert(blob.plaintext_sha256.as_str())
                || !ciphertext.insert(blob.ciphertext_sha256.as_str())
            {
                return Err("attachment index contains an invalid blob reference");
            }
            blob_sizes.insert(blob.plaintext_sha256.as_str(), blob.plaintext_byte_size);
        }
        for attachment in &self.attachments {
            if blob_sizes.get(attachment.sha256.as_str()).copied() != Some(attachment.byte_size) {
                return Err("attachment index blob mapping is missing or has the wrong size");
            }
        }
        Ok(())
    }
}

impl RevisionIndex {
    pub(crate) fn new(
        note_id: NoteId,
        entries: Vec<RevisionIndexEntry>,
    ) -> Result<Self, &'static str> {
        let index = Self {
            format_version: REVISION_INDEX_FORMAT_VERSION,
            note_id,
            entries,
        };
        let note_id = index.note_id.clone();
        index.validate_for(&note_id)?;
        Ok(index)
    }

    pub(crate) fn validate_for(&self, note_id: &NoteId) -> Result<(), &'static str> {
        if self.format_version != REVISION_INDEX_FORMAT_VERSION || &self.note_id != note_id {
            return Err("invalid revision-index identity/version");
        }
        let mut object_ids = HashSet::new();
        let mut revision_numbers = HashSet::new();
        let mut previous_no: Option<i64> = None;
        for entry in &self.entries {
            if entry.revision_no <= 0
                || entry.snapshot_byte_size < 0
                || !object_ids.insert(entry.object_id.as_str())
                || !revision_numbers.insert(entry.revision_no)
            {
                return Err("revision index contains invalid or duplicate entries");
            }
            if let Some(newer_no) = previous_no
                && entry.revision_no != newer_no.saturating_sub(1)
            {
                return Err("revision index is not contiguous newest-to-oldest");
            }
            previous_no = Some(entry.revision_no);
        }
        Ok(())
    }

    pub(crate) fn from_newest_first(
        note_id: NoteId,
        revisions: impl IntoIterator<Item = (VaultObjectId, i64, i64, i64)>,
    ) -> Result<Self, &'static str> {
        Self::new(
            note_id,
            revisions
                .into_iter()
                .map(|(object_id, revision_no, snapshot_byte_size, created_at)| {
                    RevisionIndexEntry {
                        object_id,
                        revision_no,
                        snapshot_byte_size,
                        created_at,
                    }
                })
                .collect(),
        )
    }
}

impl RevisionObject {
    pub(crate) fn new(
        note_id: NoteId,
        revision_id: String,
        revision_no: i64,
        previous_revision_object: Option<VaultObjectId>,
        created_at: i64,
        snapshot: RevisionSnapshot,
    ) -> Result<Self, &'static str> {
        let mut snapshot_json =
            serde_json::to_vec(&snapshot).map_err(|_| "revision serialization failed")?;
        let snapshot_byte_size = match i64::try_from(snapshot_json.len()) {
            Ok(size) => size,
            Err(_) => {
                snapshot_json.fill(0);
                return Err("revision snapshot is too large");
            }
        };
        let snapshot_checksum = sha256_bytes(&snapshot_json);
        snapshot_json.fill(0);
        let result = Self {
            format_version: REVISION_OBJECT_FORMAT_VERSION,
            note_id,
            revision_id,
            revision_no,
            previous_revision_object,
            snapshot_checksum,
            snapshot_byte_size,
            created_at,
            snapshot,
        };
        let note_id = result.note_id.clone();
        result.validate_for(&note_id)?;
        Ok(result)
    }

    pub(crate) fn validate_for(&self, note_id: &NoteId) -> Result<(), &'static str> {
        if self.format_version != REVISION_OBJECT_FORMAT_VERSION
            || &self.note_id != note_id
            || self.revision_id.is_empty()
            || self.revision_no <= 0
            || self.snapshot.format_version != RevisionSnapshot::FORMAT_VERSION
            || self.snapshot.note.id != *note_id
        {
            return Err("invalid revision-object identity/version");
        }
        let mut snapshot_json =
            serde_json::to_vec(&self.snapshot).map_err(|_| "revision serialization failed")?;
        let byte_size = match i64::try_from(snapshot_json.len()) {
            Ok(size) => size,
            Err(_) => {
                snapshot_json.fill(0);
                return Err("revision snapshot is too large");
            }
        };
        let checksum_matches = self.snapshot_checksum == sha256_bytes(&snapshot_json);
        snapshot_json.fill(0);
        if self.snapshot_byte_size != byte_size || !checksum_matches {
            return Err("revision-object snapshot checksum/size mismatch");
        }
        let mut attachment_ids = HashSet::new();
        for attachment in &self.snapshot.attachments {
            if attachment.id.is_empty()
                || attachment.byte_size < 0
                || !valid_sha256(&attachment.sha256)
                || !attachment_ids.insert(attachment.id.as_str())
            {
                return Err("revision object contains an invalid attachment reference");
            }
        }
        Ok(())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing into String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::{NoteDocument, NoteProtectionState, RevisionAttachment, RevisionNoteSnapshot};

    fn note() -> Note {
        Note {
            id: NoteId::parse("note-2b1").unwrap(),
            protection: NoteProtectionState::Unprotected,
            notebook_id: "notebook_notes".to_string(),
            title: "private title".to_string(),
            title_style: NoteTitleStyle::default(),
            document: Arc::new(NoteDocument::new(
                "derived markdown".to_string(),
                Arc::new(crate::editor_document::EMPTY_EDITOR_STATE.trim().to_string()),
                "derived plain text".to_string(),
            )),
            tags: vec!["private-tag".to_string()],
            attachment_names: vec!["derived-name.pdf".to_string()],
            pinned: true,
            archived: false,
            trashed: false,
            purge_after: None,
            created_at: 10,
            updated_at: 20,
        }
    }

    #[test]
    fn scrub_plan_covers_every_declared_sensitive_relational_surface() {
        let mut planned = ProtectedPublicPersistenceSpec::SCRUB_STEPS
            .iter()
            .flat_map(|step| step.sensitive_relational_surfaces().iter().copied())
            .collect::<Vec<_>>();
        let mut declared = ProtectedPublicPersistenceSpec::SENSITIVE_RELATIONAL_SURFACES.to_vec();
        planned.sort_unstable();
        planned.dedup();
        declared.sort_unstable();
        declared.dedup();
        assert_eq!(
            planned, declared,
            "the default-private persistence authority and executable scrub plan must stay mechanically coupled",
        );
    }

    #[test]
    fn classification_keeps_derived_projections_out_of_private_state() {
        let note = note();
        let data = SensitiveNoteData::classify(
            &note,
            "Locked note".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(data.public_stub.locked_label, "Locked note");
        assert_eq!(data.note_private_state.title, "private title");
        assert_eq!(data.note_private_state.editor_state, note.editor_state());
        let encoded = serde_json::to_string(&data.note_private_state).unwrap();
        assert!(!encoded.contains("derived markdown"));
        assert!(!encoded.contains("derived plain text"));
        assert!(!encoded.contains("derived-name.pdf"));
        assert_eq!(data.metadata_state.tags, vec!["private-tag"]);
        data.validate().unwrap();
    }

    #[test]
    fn revision_object_authenticates_canonical_snapshot_identity() {
        let note = note();
        let snapshot = RevisionSnapshot {
            format_version: RevisionSnapshot::FORMAT_VERSION,
            note: RevisionNoteSnapshot::from_note(&note),
            attachments: Vec::<RevisionAttachment>::new(),
        };
        let revision = RevisionObject::new(
            note.id.clone(),
            "rev-1".to_string(),
            1,
            None,
            30,
            snapshot,
        )
        .unwrap();
        revision.validate_for(&note.id).unwrap();
        let mut tampered = revision.clone();
        tampered.snapshot.note.title = "tampered".to_string();
        assert!(tampered.validate_for(&note.id).is_err());
    }

    #[test]
    fn public_allowlist_and_sensitive_surfaces_are_explicit_and_disjoint() {
        for public in ProtectedPublicPersistenceSpec::NOTE_COLUMNS {
            assert!(!ProtectedPublicPersistenceSpec::NOTE_SANITIZED_COLUMNS.contains(public));
        }
        assert!(ProtectedPublicPersistenceSpec::SENSITIVE_RELATIONAL_SURFACES.contains(&"notes_fts"));
        assert!(ProtectedPublicPersistenceSpec::DERIVED_SENSITIVE_FIELDS.contains(&"NoteDocument.plain_text"));

        // Keep every declarative privacy surface exercised by the test target.
        // These lists intentionally remain out of production code unless they
        // directly drive persistence, but they are part of the canonical
        // default-private specification audited by static/security validators.
        for surfaces in [
            ProtectedPublicPersistenceSpec::ATTACHMENT_COLUMNS,
            ProtectedPublicPersistenceSpec::VAULT_OBJECT_COLUMNS,
            ProtectedPublicPersistenceSpec::VAULT_ROOT_COLUMNS,
            ProtectedPublicPersistenceSpec::CURRENT_PROTECTED_NOTE_SAFE_TABLES,
            ProtectedPublicPersistenceSpec::CURRENT_PROTECTED_NOTE_FORBIDDEN_TABLES,
            ProtectedPublicPersistenceSpec::CURRENT_PROTECTED_NOTE_SPECIAL_SURFACES,
            ProtectedPublicPersistenceSpec::PUBLIC_STRUCTURAL_SURFACES,
        ] {
            assert!(!surfaces.is_empty());
        }
    }
}
