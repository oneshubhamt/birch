use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    attachment_paths::canonical_blob_relative_path,
    domain::{Attachment, Citation, Note, NoteId, NoteRevision, Notebook, RevisionAttachment, RevisionSnapshot, Source, SourceId},
    editor_document::{analyze_editor_state, validate_editor_state},
    editor_projection::derive_editor_projections,
    portability::{ImportInspection, ImportSummary, ImportUndoToken, PortabilityScope, remap_canonical_editor_state},
};

use super::{
    Storage, StorageError, StorageResult, new_citation_id, new_id, new_note_id, new_source_id,
    attachments::{attachment_from_row, digest_to_hex, sha256_bytes, sha256_file, validate_attachment_blob_identity},
    atomic_output::AtomicFileOutput,
    backup_support::{MAX_ARCHIVE_BYTES, MAX_EXPANDED_BYTES, MAX_MANIFEST_BYTES, read_zip_entry_bounded, validate_zip_structure},
    history::revision_from_row,
    notebooks::notebook_from_row,
    notes::note_from_row,
};

const PORTABLE_BUNDLE_VERSION: u32 = 1;
const MANIFEST_PATH: &str = "manifest.json";
const BLOB_PREFIX: &str = "blobs/";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundleManifest {
    format_version: u32,
    notebooks: Vec<Notebook>,
    notes: Vec<Note>,
    attachments: Vec<Attachment>,
    note_parents: Vec<BundleNoteParent>,
    sources: Vec<Source>,
    note_sources: Vec<BundleNoteSource>,
    citations: Vec<Citation>,
    revisions: Vec<NoteRevision>,
    revision_attachment_refs: Vec<BundleRevisionAttachmentRef>,
    pdf_annotations: Vec<BundlePdfAnnotation>,
    blobs: Vec<BundleBlob>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundleNoteParent {
    child_note_id: NoteId,
    parent_note_id: NoteId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundleNoteSource {
    note_id: NoteId,
    source_id: SourceId,
    created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundleRevisionAttachmentRef {
    revision_id: String,
    attachment_id: String,
    sha256: String,
    byte_size: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundlePdfAnnotation {
    attachment_id: String,
    page_index: u32,
    annotations_json: String,
    updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BundleBlob {
    sha256: String,
    byte_size: u64,
}

#[derive(Debug)]
struct MaterializedBundleBlob {
    owned_plaintext: crate::security::OwnedPlaintext,
}

impl Storage {
    pub(super) fn export_portable_bundle_locked(
        &self,
        scope: &PortabilityScope,
        destination: &Path,
    ) -> StorageResult<()> {
        self.ensure_export_destination_is_external(destination)?;
        let connection = self.read_only_connection()?;
        super::portability::ensure_portability_scope_unprotected(&connection, scope)?;
        let manifest = self.build_bundle_manifest(&connection, scope)?;
        validate_bundle_manifest(&manifest)?;

        let manifest_bytes = serde_json::to_vec(&manifest)?;
        if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(StorageError::Validation(
                "Rustnote bundle manifest is too large".to_string(),
            ));
        }

        let mut output = AtomicFileOutput::create(destination)?;
        {
            let writer = output.writer()?;
            let mut archive = ZipWriter::new(writer);
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            archive.start_file(MANIFEST_PATH, options)?;
            archive.write_all(&manifest_bytes)?;
            for blob in &manifest.blobs {
                let relative = canonical_blob_relative_path(&blob.sha256).ok_or_else(|| {
                    StorageError::Integrity(format!("invalid bundle blob hash {}", blob.sha256))
                })?;
                let source = self.attachments_directory.join(relative);
                let (actual_hash, actual_size) = sha256_file(&source)?;
                if actual_hash != blob.sha256 || actual_size != blob.byte_size {
                    return Err(StorageError::Integrity(format!(
                        "bundle blob {} failed source verification",
                        blob.sha256
                    )));
                }
                archive.start_file(format!("{BLOB_PREFIX}{}", blob.sha256), options)?;
                let mut file = File::open(source)?;
                std::io::copy(&mut file, &mut archive)?;
            }
            archive.finish()?;
        }
        output.commit(|path| {
            self.inspect_portable_bundle(path)?;
            Ok(())
        })?;
        Ok(())
    }

    pub(super) fn inspect_portable_bundle(&self, source: &Path) -> StorageResult<ImportInspection> {
        self.inspect_portable_bundle_plan(source).map(|(inspection, _)| inspection)
    }

    pub(super) fn inspect_portable_bundle_plan(
        &self,
        source: &Path,
    ) -> StorageResult<(ImportInspection, String)> {
        let manifest = load_verified_bundle(source)?;
        let semantic_sha256 = bundle_manifest_fingerprint(&manifest)?;
        Ok((bundle_inspection(source, &manifest)?, semantic_sha256))
    }

    pub(super) fn import_portable_bundle_matching_staged(
        &self,
        source: &Path,
        expected_semantic_sha256: &str,
    ) -> StorageResult<ImportSummary> {
        // The caller has already established a library-scoped durable plaintext
        // owner for the staged archive. That owner is the long-lived barrier; no
        // global operation mutex is required while a potentially large bundle is
        // verified and imported.
        let manifest = load_verified_bundle(source)?;
        if bundle_manifest_fingerprint(&manifest)? != expected_semantic_sha256 {
            return Err(StorageError::Validation(
                "Rustnote bundle changed after it was inspected; inspect it again before importing".to_string(),
            ));
        }
        self.import_loaded_portable_bundle(source, &manifest)
    }

    fn import_loaded_portable_bundle(
        &self,
        source: &Path,
        manifest: &BundleManifest,
    ) -> StorageResult<ImportSummary> {
        let mut newly_materialized = self.materialize_bundle_blobs(source, manifest)?;
        match self.import_verified_bundle(manifest, &mut newly_materialized) {
            Ok(summary) => Ok(summary),
            Err(primary) => {
                let mut cleanup_failures = Vec::new();
                for blob in &mut newly_materialized {
                    if let Err(error) = self.discard_owned_plaintext(&mut blob.owned_plaintext) {
                        cleanup_failures.push(error.to_string());
                    }
                }
                if cleanup_failures.is_empty() {
                    Err(primary)
                } else {
                    Err(StorageError::Integrity(format!(
                        "{primary}; imported plaintext blob cleanup remains durably owned: {}",
                        cleanup_failures.join("; ")
                    )))
                }
            }
        }
    }

    fn build_bundle_manifest(
        &self,
        connection: &Connection,
        scope: &PortabilityScope,
    ) -> StorageResult<BundleManifest> {
        let notebooks = query_notebooks(connection)?;
        let selected_note_ids = selected_note_ids(connection, scope, &notebooks)?;
        if selected_note_ids.is_empty() {
            return Err(StorageError::Validation(
                "the selected Rustnote bundle scope contains no notes".to_string(),
            ));
        }
        let note_id_set = selected_note_ids.iter().cloned().collect::<HashSet<_>>();
        let notes = query_notes(connection, &note_id_set)?;
        let attachments = query_attachments(connection, &note_id_set)?;
        let note_parents = query_note_parents(connection, &note_id_set)?;
        let (sources, note_sources, citations) = query_sources(connection, &note_id_set)?;
        let revisions = query_revisions(connection, &note_id_set)?;

        let mut notebook_ids = notebook_ids_for_bundle(&notes, &notebooks, scope)?.into_iter().collect::<HashSet<_>>();
        let notebooks_by_id = notebooks.iter().map(|item| (item.id.as_str(), item)).collect::<HashMap<_, _>>();
        for revision in &revisions {
            let snapshot: RevisionSnapshot = serde_json::from_str(&revision.snapshot_json)?;
            let mut current = Some(snapshot.note.notebook_id.as_str());
            let mut visited = HashSet::new();
            while let Some(id) = current {
                if !visited.insert(id.to_string()) {
                    return Err(StorageError::Integrity("historical notebook hierarchy contains a cycle".to_string()));
                }
                let Some(notebook) = notebooks_by_id.get(id) else {
                    return Err(StorageError::Integrity(format!("revision {} references missing notebook {id}", revision.id)));
                };
                notebook_ids.insert(id.to_string());
                current = notebook.parent_id.as_deref();
            }
        }
        let notebook_set = notebook_ids;
        let mut selected_notebooks = notebooks
            .into_iter()
            .filter(|notebook| notebook_set.contains(&notebook.id))
            .collect::<Vec<_>>();
        for notebook in &mut selected_notebooks {
            if notebook.parent_id.as_ref().is_some_and(|parent| !notebook_set.contains(parent)) {
                notebook.parent_id = None;
            }
        }
        let revision_ids = revisions.iter().map(|revision| revision.id.clone()).collect::<HashSet<_>>();
        let revision_attachment_refs = query_revision_attachment_refs(connection, &revision_ids)?;
        let attachment_ids = attachments.iter().map(|attachment| attachment.id.clone()).collect::<HashSet<_>>();
        let pdf_annotations = query_pdf_annotations(connection, &attachment_ids)?;

        for note in &notes {
            validate_editor_state(note.editor_state()).map_err(|error| {
                StorageError::Validation(format!("note {} contains invalid editor state: {error}", note.id))
            })?;
        }
        for revision in &revisions {
            self.revision_snapshot(revision)?;
        }

        let mut blobs_by_hash = HashMap::<String, u64>::new();
        for attachment in &attachments {
            let (_, _, byte_size) = validate_attachment_blob_identity(
                &format!("attachment {}", attachment.id),
                &attachment.sha256,
                attachment.byte_size,
            )?;
            blobs_by_hash.entry(attachment.sha256.clone()).or_insert(byte_size);
        }
        for reference in &revision_attachment_refs {
            let (_, _, byte_size) = validate_attachment_blob_identity(
                &format!("revision attachment {}", reference.attachment_id),
                &reference.sha256,
                reference.byte_size,
            )?;
            match blobs_by_hash.get(&reference.sha256) {
                Some(existing) if *existing != byte_size => {
                    return Err(StorageError::Integrity(format!(
                        "blob {} has inconsistent sizes in version history",
                        reference.sha256
                    )));
                }
                _ => {
                    blobs_by_hash.insert(reference.sha256.clone(), byte_size);
                }
            }
        }
        let mut blobs = blobs_by_hash
            .into_iter()
            .map(|(sha256, byte_size)| BundleBlob { sha256, byte_size })
            .collect::<Vec<_>>();
        blobs.sort_by(|left, right| left.sha256.cmp(&right.sha256));

        Ok(BundleManifest {
            format_version: PORTABLE_BUNDLE_VERSION,
            notebooks: selected_notebooks,
            notes,
            attachments,
            note_parents,
            sources,
            note_sources,
            citations,
            revisions,
            revision_attachment_refs,
            pdf_annotations,
            blobs,
        })
    }

    fn materialize_bundle_blobs(
        &self,
        source: &Path,
        manifest: &BundleManifest,
    ) -> StorageResult<Vec<MaterializedBundleBlob>> {
        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;
        let mut created = Vec::new();

        let materialize = (|| -> StorageResult<()> {
            for blob in &manifest.blobs {
                let relative = canonical_blob_relative_path(&blob.sha256).ok_or_else(|| {
                    StorageError::InvalidBackup(format!(
                        "invalid bundle blob hash {}",
                        blob.sha256
                    ))
                })?;
                let destination = self.attachments_directory.join(&relative);

                // A previous failed import can leave a valid file plus its
                // durable generic owner. Adopt that owner into this retry so a
                // successful import transfers it atomically instead of leaving
                // startup with a stale instruction to delete a canonical blob.
                if let Some(stage_id) = self.portability_blob_plaintext_stage_id(&relative)? {
                    if crate::security::managed_path_is_present(&destination)? {
                        let (hash, size) = sha256_file(&destination)?;
                        if hash != blob.sha256 || size != blob.byte_size {
                            return Err(StorageError::Integrity(format!(
                                "owned attachment blob {} is corrupt",
                                blob.sha256
                            )));
                        }
                        let owned_plaintext = self
                            .resume_portability_blob_owned_plaintext(&stage_id, &relative)?;
                        created.push(MaterializedBundleBlob { owned_plaintext });
                        continue;
                    }
                    // The prior file is already gone but its owner-row deletion
                    // was interrupted. Retire that empty obligation before
                    // reserving a fresh create-new lifetime.
                    let mut owned_plaintext = self
                        .resume_portability_blob_owned_plaintext(&stage_id, &relative)?;
                    self.release_uncreated_owned_plaintext(&mut owned_plaintext)?;
                }

                if crate::security::managed_path_is_present(&destination)? {
                    let (hash, size) = sha256_file(&destination)?;
                    if hash != blob.sha256 || size != blob.byte_size {
                        return Err(StorageError::Integrity(format!(
                            "existing attachment blob {} is corrupt",
                            blob.sha256
                        )));
                    }
                    continue;
                }
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Reserve durable library ownership before creating the first
                // plaintext byte. The row is transferred in the same transaction
                // that creates the imported attachment/revision references.
                let mut owned_plaintext = self.reserve_portability_blob_owned_plaintext(&relative)?;
                if owned_plaintext.path() != destination {
                    let primary = StorageError::Integrity(
                        "portable bundle plaintext owner resolved to the wrong path".to_string(),
                    );
                    return Err(self.cleanup_owned_plaintext_after_error(
                        &mut owned_plaintext,
                        primary,
                    ));
                }
                let archive_path = format!("{BLOB_PREFIX}{}", blob.sha256);
                let mut entry = match archive.by_name(&archive_path) {
                    Ok(entry) => entry,
                    Err(error) => {
                        self.release_uncreated_owned_plaintext(&mut owned_plaintext)?;
                        return Err(error.into());
                    }
                };
                let mut output = match owned_plaintext.create_file() {
                    Ok(file) => file,
                    Err(error) => {
                        self.release_uncreated_owned_plaintext(&mut owned_plaintext)?;
                        return Err(error.into());
                    }
                };
                created.push(MaterializedBundleBlob { owned_plaintext });

                let mut hasher = sha2::Sha256::new();
                use sha2::Digest;
                let mut written = 0_u64;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let read = entry.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    written = written.checked_add(read as u64).ok_or_else(|| {
                        StorageError::InvalidBackup("bundle blob size overflow".to_string())
                    })?;
                    if written > blob.byte_size {
                        return Err(StorageError::InvalidBackup(format!(
                            "bundle blob {} expanded beyond its declared size",
                            blob.sha256
                        )));
                    }
                    hasher.update(&buffer[..read]);
                    output.write_all(&buffer[..read])?;
                }
                output.sync_all()?;
                let digest = digest_to_hex(&hasher.finalize());
                if written != blob.byte_size || digest != blob.sha256 {
                    return Err(StorageError::Integrity(format!(
                        "bundle blob {} failed checksum verification",
                        blob.sha256
                    )));
                }
            }
            Ok(())
        })();

        if let Err(primary) = materialize {
            let mut cleanup_failures = Vec::new();
            for blob in &mut created {
                if let Err(error) = self.discard_owned_plaintext(&mut blob.owned_plaintext) {
                    cleanup_failures.push(error.to_string());
                }
            }
            return if cleanup_failures.is_empty() {
                Err(primary)
            } else {
                Err(StorageError::Integrity(format!(
                    "{primary}; bundle blob cleanup remains durably owned: {}",
                    cleanup_failures.join("; ")
                )))
            };
        }

        Ok(created)
    }

    fn import_verified_bundle(
        &self,
        manifest: &BundleManifest,
        materialized_blobs: &mut [MaterializedBundleBlob],
    ) -> StorageResult<ImportSummary> {
        let notebook_ids = manifest.notebooks.iter().map(|item| (item.id.clone(), new_id("book"))).collect::<HashMap<_, _>>();
        let note_ids = manifest.notes.iter().map(|item| (item.id.as_str().to_string(), new_note_id())).collect::<HashMap<_, _>>();
        let mut attachment_ids = manifest.attachments.iter().map(|item| (item.id.clone(), new_id("att"))).collect::<HashMap<_, _>>();
        for revision in &manifest.revisions {
            let snapshot: RevisionSnapshot = serde_json::from_str(&revision.snapshot_json)?;
            for attachment in snapshot.attachments {
                attachment_ids.entry(attachment.id).or_insert_with(|| new_id("att"));
            }
        }
        let source_ids = manifest.sources.iter().map(|item| (item.id.as_str().to_string(), new_source_id())).collect::<HashMap<_, _>>();
        let citation_ids = manifest.citations.iter().map(|item| (item.id.clone(), new_citation_id())).collect::<HashMap<_, _>>();
        let revision_ids = manifest.revisions.iter().map(|item| (item.id.clone(), new_id("rev"))).collect::<HashMap<_, _>>();

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        insert_bundle_notebooks(&transaction, &manifest.notebooks, &notebook_ids)?;
        let mut imported_notes = Vec::<Note>::new();
        let first_fts_rowid: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(fts_rowid), 0) + 1 FROM notes",
            [],
            |row| row.get(0),
        )?;
        for (offset, source_note) in manifest.notes.iter().enumerate() {
            let offset = i64::try_from(offset)
                .map_err(|_| StorageError::Integrity("bundle contains too many notes".to_string()))?;
            let next_fts_rowid = first_fts_rowid
                .checked_add(offset)
                .ok_or_else(|| StorageError::Integrity("note FTS rowid overflow".to_string()))?;
            let mut note = source_note.clone();
            note.id = note_ids.get(source_note.id.as_str()).cloned().ok_or_else(|| StorageError::Integrity("bundle note mapping is incomplete".to_string()))?;
            note.notebook_id = notebook_ids.get(&source_note.notebook_id).cloned().ok_or_else(|| StorageError::Integrity("bundle notebook mapping is incomplete".to_string()))?;
            note.title_style = std::mem::take(&mut note.title_style).normalized();
            let editor_state = remap_canonical_editor_state(source_note.editor_state(), &note_ids, &attachment_ids).map_err(|error| StorageError::Validation(error.to_string()))?;
            let projections = derive_editor_projections(&editor_state)?;
            note.replace_document(projections.markdown, editor_state, projections.plain_text);
            note.attachment_names.clear();
            transaction.execute(
                "INSERT INTO notes(id, fts_rowid, notebook_id, title, title_style, body, editor_state, plain_text, pinned, archived, trashed, purge_after, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![&note.id, next_fts_rowid, &note.notebook_id, &note.title, serde_json::to_string(&note.title_style)?, note.body(), note.editor_state(), note.plain_text(), note.pinned as i64, note.archived as i64, note.trashed as i64, note.purge_after, note.created_at, note.updated_at],
            )?;
            self.replace_note_tags(&transaction, &note.id, &note.tags)?;
            imported_notes.push(note);
        }

        insert_bundle_attachments(&transaction, &manifest.attachments, &note_ids, &attachment_ids)?;
        insert_bundle_hierarchy(&transaction, &manifest.note_parents, &note_ids)?;
        insert_bundle_sources(&transaction, manifest, &note_ids, &source_ids, &citation_ids)?;
        insert_bundle_annotations(&transaction, &manifest.pdf_annotations, &attachment_ids)?;
        insert_bundle_revisions(&transaction, manifest, &note_ids, &notebook_ids, &attachment_ids, &revision_ids)?;

        for note in &mut imported_notes {
            note.attachment_names = embedded_attachment_names(&transaction, &note.id)?;
            let analysis = analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
            self.reconcile_document_note_links(&transaction, &note.id, &analysis.note_links)?;
            self.refresh_search_index(&transaction, note)?;
            self.enqueue_sync_upsert(&transaction, "note", note.id.as_str(), note)?;
        }
        for source_notebook in &manifest.notebooks {
            let mut notebook = source_notebook.clone();
            notebook.id = notebook_ids[&source_notebook.id].clone();
            notebook.parent_id = source_notebook.parent_id.as_ref().and_then(|parent| notebook_ids.get(parent).cloned());
            self.enqueue_sync_upsert(&transaction, "notebook", &notebook.id, &notebook)?;
        }
        for attachment in &manifest.attachments {
            let imported = imported_attachment(attachment, &note_ids, &attachment_ids)?;
            self.enqueue_sync_upsert(&transaction, "attachment", &imported.id, &imported)?;
        }

        for blob in materialized_blobs.iter_mut() {
            self.transfer_owned_plaintext_stage(&transaction, &mut blob.owned_plaintext)?;
        }

        transaction.commit()?;
        for blob in materialized_blobs.iter_mut() {
            self.complete_owned_plaintext_transfer(&mut blob.owned_plaintext)?;
        }
        Ok(ImportSummary {
            notes: manifest.notes.len(),
            notebooks: manifest.notebooks.len(),
            attachments: manifest.attachments.len(),
            warnings: Vec::new(),
            undo: Some(ImportUndoToken {
                note_ids: note_ids.values().cloned().collect(),
                notebook_ids: notebook_ids.values().cloned().collect(),
                blob_hashes: manifest.blobs.iter().map(|blob| blob.sha256.clone()).collect(),
            }),
        })
    }
}

fn query_notebooks(connection: &Connection) -> StorageResult<Vec<Notebook>> {
    let mut statement = connection.prepare("SELECT id, name, parent_id, sort_order, created_at, updated_at FROM notebooks ORDER BY parent_id IS NOT NULL, sort_order, lower(name), id")?;
    Ok(statement.query_map([], notebook_from_row)?.collect::<Result<Vec<_>, _>>()?)
}

fn selected_note_ids(connection: &Connection, scope: &PortabilityScope, notebooks: &[Notebook]) -> StorageResult<Vec<NoteId>> {
    let allowed_notebooks = match scope {
        PortabilityScope::Notebook(root) => Some(bundle_notebook_descendants(notebooks, root)?),
        _ => None,
    };
    let mut statement = connection.prepare("SELECT id, notebook_id FROM notes ORDER BY created_at, id")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, NoteId>(0)?, row.get::<_, String>(1)?)))?;
    let mut result = Vec::new();
    for row in rows {
        let (note_id, notebook_id) = row?;
        let include = match scope {
            PortabilityScope::Library => true,
            PortabilityScope::Note(selected) => selected == &note_id,
            PortabilityScope::Notebook(_) => allowed_notebooks.as_ref().is_some_and(|allowed| allowed.contains(&notebook_id)),
        };
        if include { result.push(note_id); }
    }
    Ok(result)
}

fn query_notes(connection: &Connection, selected: &HashSet<NoteId>) -> StorageResult<Vec<Note>> {
    let mut statement = connection.prepare(
        "SELECT n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text, n.pinned, n.archived, n.trashed, n.purge_after, n.created_at, n.updated_at,
         COALESCE((SELECT group_concat(t.name, ', ') FROM note_tags nt JOIN tags t ON t.id = nt.tag_id WHERE nt.note_id = n.id), '') AS tag_names,
         COALESCE((SELECT group_concat(a.file_name, char(31)) FROM attachments a WHERE a.note_id = n.id AND a.embedded = 1), '') AS attachment_names
         FROM notes n ORDER BY n.created_at, n.id"
    )?;
    let rows = statement.query_map([], note_from_row)?;
    let mut result = Vec::new();
    for row in rows { let note = row?; if selected.contains(&note.id) { result.push(note); } }
    Ok(result)
}

fn notebook_ids_for_bundle(notes: &[Note], notebooks: &[Notebook], scope: &PortabilityScope) -> StorageResult<Vec<String>> {
    let by_id = notebooks.iter().map(|item| (item.id.as_str(), item)).collect::<HashMap<_, _>>();
    let mut ids = HashSet::new();
    match scope {
        PortabilityScope::Library => ids.extend(notebooks.iter().map(|item| item.id.clone())),
        PortabilityScope::Notebook(root) => ids.extend(bundle_notebook_descendants(notebooks, root)?),
        PortabilityScope::Note(_) => {
            for note in notes {
                let mut current = Some(note.notebook_id.as_str());
                let mut visited = HashSet::new();
                while let Some(id) = current {
                    if !visited.insert(id.to_string()) { return Err(StorageError::Integrity("notebook hierarchy contains a cycle".to_string())); }
                    ids.insert(id.to_string());
                    current = by_id.get(id).and_then(|item| item.parent_id.as_deref());
                }
            }
        }
    }
    let mut result = ids.into_iter().collect::<Vec<_>>();
    result.sort();
    Ok(result)
}

fn query_attachments(connection: &Connection, note_ids: &HashSet<NoteId>) -> StorageResult<Vec<Attachment>> {
    let mut statement = connection.prepare("SELECT id, note_id, file_name, mime_type, byte_size, sha256, image_pixel_width, image_pixel_height, image_format, image_orientation, image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at FROM attachments ORDER BY created_at, id")?;
    let rows = statement.query_map([], attachment_from_row)?;
    let mut result = Vec::new();
    for row in rows { let item = row?; if note_ids.contains(&item.note_id) { result.push(item); } }
    Ok(result)
}

fn query_note_parents(connection: &Connection, note_ids: &HashSet<NoteId>) -> StorageResult<Vec<BundleNoteParent>> {
    let mut statement = connection.prepare("SELECT child_note_id, parent_note_id FROM note_parent ORDER BY child_note_id")?;
    let rows = statement.query_map([], |row| Ok(BundleNoteParent { child_note_id: row.get(0)?, parent_note_id: row.get(1)? }))?;
    let mut result = Vec::new();
    for row in rows { let item = row?; if note_ids.contains(&item.child_note_id) && note_ids.contains(&item.parent_note_id) { result.push(item); } }
    Ok(result)
}

fn query_sources(connection: &Connection, note_ids: &HashSet<NoteId>) -> StorageResult<(Vec<Source>, Vec<BundleNoteSource>, Vec<Citation>)> {
    let mut links_statement = connection.prepare("SELECT note_id, source_id, created_at FROM note_sources ORDER BY note_id, source_id")?;
    let links = links_statement.query_map([], |row| Ok(BundleNoteSource { note_id: row.get(0)?, source_id: row.get(1)?, created_at: row.get(2)? }))?;
    let mut note_sources = Vec::new();
    let mut source_ids = HashSet::new();
    for row in links { let item = row?; if note_ids.contains(&item.note_id) { source_ids.insert(item.source_id.clone()); note_sources.push(item); } }

    let mut sources_statement = connection.prepare("SELECT id, kind, title, reference, created_at, updated_at FROM sources ORDER BY id")?;
    let source_rows = sources_statement.query_map([], |row| {
        let kind_value: String = row.get(1)?;
        let kind = crate::domain::SourceKind::parse(&kind_value).ok_or_else(|| rusqlite::Error::InvalidQuery)?;
        Ok(Source { id: row.get(0)?, kind, title: row.get(2)?, reference: row.get(3)?, created_at: row.get(4)?, updated_at: row.get(5)? })
    })?;
    let mut sources = Vec::new();
    for row in source_rows { let item = row?; if source_ids.contains(&item.id) { sources.push(item); } }

    let mut citation_statement = connection.prepare("SELECT id, note_id, source_id, locator, created_at FROM citations ORDER BY note_id, source_id, created_at, id")?;
    let citation_rows = citation_statement.query_map([], |row| Ok(Citation { id: row.get(0)?, note_id: row.get(1)?, source_id: row.get(2)?, locator: row.get(3)?, created_at: row.get(4)? }))?;
    let mut citations = Vec::new();
    for row in citation_rows { let item = row?; if note_ids.contains(&item.note_id) { citations.push(item); } }
    Ok((sources, note_sources, citations))
}

fn query_revisions(connection: &Connection, note_ids: &HashSet<NoteId>) -> StorageResult<Vec<NoteRevision>> {
    let mut statement = connection.prepare("SELECT id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at FROM note_revisions ORDER BY note_id, revision_no")?;
    let rows = statement.query_map([], revision_from_row)?;
    let mut result = Vec::new();
    for row in rows { let item = row?; if note_ids.contains(&item.note_id) { result.push(item); } }
    Ok(result)
}

fn query_revision_attachment_refs(connection: &Connection, revision_ids: &HashSet<String>) -> StorageResult<Vec<BundleRevisionAttachmentRef>> {
    let mut statement = connection.prepare("SELECT revision_id, attachment_id, sha256, byte_size FROM revision_attachment_refs ORDER BY revision_id, attachment_id")?;
    let rows = statement.query_map([], |row| Ok(BundleRevisionAttachmentRef { revision_id: row.get(0)?, attachment_id: row.get(1)?, sha256: row.get(2)?, byte_size: row.get(3)? }))?;
    let mut result = Vec::new();
    for row in rows { let item = row?; if revision_ids.contains(&item.revision_id) { result.push(item); } }
    Ok(result)
}

fn query_pdf_annotations(connection: &Connection, attachment_ids: &HashSet<String>) -> StorageResult<Vec<BundlePdfAnnotation>> {
    let mut statement = connection.prepare("SELECT attachment_id, page_index, annotations_json, updated_at FROM pdf_annotation_pages ORDER BY attachment_id, page_index")?;
    let rows = statement.query_map([], |row| Ok(BundlePdfAnnotation { attachment_id: row.get(0)?, page_index: row.get(1)?, annotations_json: row.get(2)?, updated_at: row.get(3)? }))?;
    let mut result = Vec::new();
    for row in rows { let item = row?; if attachment_ids.contains(&item.attachment_id) { result.push(item); } }
    Ok(result)
}

fn bundle_notebook_descendants(notebooks: &[Notebook], root: &str) -> StorageResult<HashSet<String>> {
    if !notebooks.iter().any(|item| item.id == root) {
        return Err(StorageError::Validation("selected notebook no longer exists".to_string()));
    }
    let mut ids = HashSet::from([root.to_string()]);
    loop {
        let mut changed = false;
        for item in notebooks {
            if item.parent_id.as_ref().is_some_and(|parent| ids.contains(parent)) && ids.insert(item.id.clone()) { changed = true; }
        }
        if !changed { break; }
    }
    Ok(ids)
}

fn validate_bundle_manifest(manifest: &BundleManifest) -> StorageResult<()> {
    if manifest.format_version != PORTABLE_BUNDLE_VERSION {
        return Err(StorageError::InvalidBackup(format!("unsupported Rustnote bundle version {}", manifest.format_version)));
    }
    unique_by(manifest.notebooks.iter().map(|item| item.id.as_str()), "notebook")?;
    unique_by(manifest.notes.iter().map(|item| item.id.as_str()), "note")?;
    unique_by(manifest.attachments.iter().map(|item| item.id.as_str()), "attachment")?;
    unique_by(manifest.sources.iter().map(|item| item.id.as_str()), "source")?;
    unique_by(manifest.citations.iter().map(|item| item.id.as_str()), "citation")?;
    unique_by(manifest.revisions.iter().map(|item| item.id.as_str()), "revision")?;
    unique_by(manifest.blobs.iter().map(|item| item.sha256.as_str()), "blob")?;

    let notebook_ids = manifest.notebooks.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let note_ids = manifest.notes.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let attachment_ids = manifest.attachments.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let source_ids = manifest.sources.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let revision_ids = manifest.revisions.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
    let blobs = manifest.blobs.iter().map(|item| (item.sha256.as_str(), item.byte_size)).collect::<HashMap<_, _>>();

    validate_notebook_hierarchy(&manifest.notebooks)?;

    for notebook in &manifest.notebooks {
        if notebook.parent_id.as_deref().is_some_and(|parent| !notebook_ids.contains(parent)) {
            return Err(StorageError::InvalidBackup(format!("notebook {} references a missing parent", notebook.id)));
        }
    }
    for note in &manifest.notes {
        if !notebook_ids.contains(note.notebook_id.as_str()) {
            return Err(StorageError::InvalidBackup(format!("note {} references a missing notebook", note.id)));
        }
        validate_editor_state(note.editor_state()).map_err(|error| StorageError::InvalidBackup(format!("note {} contains invalid editor state: {error}", note.id)))?;
        let analysis = analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
        if let Some(missing) = analysis.attachment_ids.iter().find(|id| !attachment_ids.contains(id.as_str())) {
            return Err(StorageError::InvalidBackup(format!("note {} references missing attachment {missing}", note.id)));
        }
    }
    for attachment in &manifest.attachments {
        if !note_ids.contains(attachment.note_id.as_str()) {
            return Err(StorageError::InvalidBackup(format!("attachment {} references a missing note", attachment.id)));
        }
        if attachment.source_attachment_id.as_deref().is_some_and(|source| !attachment_ids.contains(source)) {
            return Err(StorageError::InvalidBackup(format!("attachment {} references a missing source attachment", attachment.id)));
        }
        let (_, _, size) = validate_attachment_blob_identity("bundle attachment", &attachment.sha256, attachment.byte_size)?;
        if blobs.get(attachment.sha256.as_str()) != Some(&size) {
            return Err(StorageError::InvalidBackup(format!("attachment {} has no matching bundle blob", attachment.id)));
        }
    }
    validate_current_attachment_sources(&manifest.attachments)?;
    for relation in &manifest.note_parents {
        if !note_ids.contains(relation.child_note_id.as_str()) || !note_ids.contains(relation.parent_note_id.as_str()) {
            return Err(StorageError::InvalidBackup("note hierarchy references a missing note".to_string()));
        }
    }
    validate_note_hierarchy(&manifest.note_parents)?;
    for relation in &manifest.note_sources {
        if !note_ids.contains(relation.note_id.as_str()) || !source_ids.contains(relation.source_id.as_str()) {
            return Err(StorageError::InvalidBackup("note-source relationship is incomplete".to_string()));
        }
    }
    for citation in &manifest.citations {
        if !note_ids.contains(citation.note_id.as_str()) || !source_ids.contains(citation.source_id.as_str()) {
            return Err(StorageError::InvalidBackup(format!("citation {} references missing content", citation.id)));
        }
    }
    let mut snapshot_attachment_refs = HashMap::<(String, String), (String, i64)>::new();
    for revision in &manifest.revisions {
        if !note_ids.contains(revision.note_id.as_str()) {
            return Err(StorageError::InvalidBackup(format!("revision {} references a missing note", revision.id)));
        }
        if sha256_bytes(revision.snapshot_json.as_bytes()) != revision.checksum || revision.byte_size != revision.snapshot_json.len() as i64 {
            return Err(StorageError::InvalidBackup(format!("revision {} failed snapshot verification", revision.id)));
        }
        let snapshot: RevisionSnapshot = serde_json::from_str(&revision.snapshot_json)?;
        if snapshot.format_version != RevisionSnapshot::FORMAT_VERSION || snapshot.note.id != revision.note_id {
            return Err(StorageError::InvalidBackup(format!("revision {} has an invalid snapshot", revision.id)));
        }
        if !notebook_ids.contains(snapshot.note.notebook_id.as_str()) {
            return Err(StorageError::InvalidBackup(format!("revision {} references a missing notebook", revision.id)));
        }
        validate_editor_state(snapshot.note.editor_state.as_str()).map_err(|error| StorageError::InvalidBackup(format!("revision {} contains invalid editor state: {error}", revision.id)))?;
        let snapshot_attachment_ids = snapshot.attachments.iter().map(|item| item.id.as_str()).collect::<HashSet<_>>();
        validate_revision_attachment_sources(&snapshot.attachments, &revision.id)?;
        let analysis = analyze_editor_state(snapshot.note.editor_state.as_str()).map_err(StorageError::from)?;
        if let Some(missing) = analysis.attachment_ids.iter().find(|id| !snapshot_attachment_ids.contains(id.as_str())) {
            return Err(StorageError::InvalidBackup(format!("revision {} references attachment {missing} outside its snapshot", revision.id)));
        }
        for attachment in &snapshot.attachments {
            let (_, _, size) = validate_attachment_blob_identity("revision attachment", &attachment.sha256, attachment.byte_size)?;
            if blobs.get(attachment.sha256.as_str()) != Some(&size) {
                return Err(StorageError::InvalidBackup(format!("revision {} references a missing blob", revision.id)));
            }
            let key = (revision.id.clone(), attachment.id.clone());
            if snapshot_attachment_refs.insert(key, (attachment.sha256.clone(), attachment.byte_size)).is_some() {
                return Err(StorageError::InvalidBackup(format!("revision {} contains duplicate attachment {}", revision.id, attachment.id)));
            }
        }
    }
    let mut indexed_revision_refs = HashSet::<(String, String)>::new();
    for reference in &manifest.revision_attachment_refs {
        if !revision_ids.contains(reference.revision_id.as_str()) {
            return Err(StorageError::InvalidBackup("revision attachment reference has no revision".to_string()));
        }
        let (_, _, size) = validate_attachment_blob_identity("revision attachment reference", &reference.sha256, reference.byte_size)?;
        if blobs.get(reference.sha256.as_str()) != Some(&size) {
            return Err(StorageError::InvalidBackup("revision attachment reference has no matching blob".to_string()));
        }
        let key = (reference.revision_id.clone(), reference.attachment_id.clone());
        let Some((snapshot_hash, snapshot_size)) = snapshot_attachment_refs.get(&key) else {
            return Err(StorageError::InvalidBackup("revision attachment reference is absent from its revision snapshot".to_string()));
        };
        if snapshot_hash != &reference.sha256 || snapshot_size != &reference.byte_size {
            return Err(StorageError::InvalidBackup("revision attachment reference disagrees with its revision snapshot".to_string()));
        }
        if !indexed_revision_refs.insert(key) {
            return Err(StorageError::InvalidBackup("bundle contains a duplicate revision attachment reference".to_string()));
        }
    }
    if indexed_revision_refs.len() != snapshot_attachment_refs.len() {
        return Err(StorageError::InvalidBackup("one or more revision snapshot attachments are missing their blob index reference".to_string()));
    }

    // Every blob copied into the canonical attachment namespace must acquire a
    // canonical database owner in the same import transaction. Reject surplus
    // manifest blobs up front so generic plaintext ownership can never be
    // released for an unreferenced file.
    let referenced_blob_hashes = manifest
        .attachments
        .iter()
        .map(|attachment| attachment.sha256.as_str())
        .chain(
            manifest
                .revision_attachment_refs
                .iter()
                .map(|reference| reference.sha256.as_str()),
        )
        .collect::<HashSet<_>>();
    if let Some(blob) = manifest
        .blobs
        .iter()
        .find(|blob| !referenced_blob_hashes.contains(blob.sha256.as_str()))
    {
        return Err(StorageError::InvalidBackup(format!(
            "bundle blob {} is not referenced by any imported attachment or revision",
            blob.sha256
        )));
    }
    for annotation in &manifest.pdf_annotations {
        if !attachment_ids.contains(annotation.attachment_id.as_str()) {
            return Err(StorageError::InvalidBackup("PDF annotation references a missing attachment".to_string()));
        }
        let _: serde_json::Value = serde_json::from_str(&annotation.annotations_json)?;
    }
    Ok(())
}

fn validate_current_attachment_sources(attachments: &[Attachment]) -> StorageResult<()> {
    let by_id = attachments.iter().map(|attachment| (attachment.id.as_str(), attachment)).collect::<HashMap<_, _>>();
    for attachment in attachments {
        if let Some(source_id) = attachment.source_attachment_id.as_deref() {
            let source = by_id.get(source_id).ok_or_else(|| StorageError::InvalidBackup(format!("attachment {} references a missing source attachment", attachment.id)))?;
            if source.note_id != attachment.note_id {
                return Err(StorageError::InvalidBackup(format!("attachment {} uses a source from another note", attachment.id)));
            }
        }
    }
    validate_attachment_source_graph(
        attachments.iter().map(|attachment| (attachment.id.as_str(), attachment.source_attachment_id.as_deref())),
        "bundle attachment",
    )
}

fn validate_revision_attachment_sources(attachments: &[RevisionAttachment], revision_id: &str) -> StorageResult<()> {
    let ids = attachments.iter().map(|attachment| attachment.id.as_str()).collect::<HashSet<_>>();
    for attachment in attachments {
        if attachment.source_attachment_id.as_deref().is_some_and(|source| !ids.contains(source)) {
            return Err(StorageError::InvalidBackup(format!("revision {revision_id} attachment {} references a source outside its snapshot", attachment.id)));
        }
    }
    validate_attachment_source_graph(
        attachments.iter().map(|attachment| (attachment.id.as_str(), attachment.source_attachment_id.as_deref())),
        "revision attachment",
    )
}

fn validate_attachment_source_graph<'a>(
    attachments: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    label: &str,
) -> StorageResult<()> {
    let sources = attachments.into_iter().collect::<HashMap<_, _>>();
    for id in sources.keys().copied() {
        let mut current = Some(id);
        let mut visited = HashSet::new();
        while let Some(attachment_id) = current {
            if !visited.insert(attachment_id) {
                return Err(StorageError::InvalidBackup(format!("{label} source chain contains a cycle")));
            }
            current = sources.get(attachment_id).copied().flatten();
        }
    }
    Ok(())
}

fn validate_note_hierarchy(relations: &[BundleNoteParent]) -> StorageResult<()> {
    let mut parents = HashMap::<&str, &str>::new();
    for relation in relations {
        if relation.child_note_id == relation.parent_note_id {
            return Err(StorageError::InvalidBackup("Rustnote bundle note hierarchy contains a self-parent relationship".to_string()));
        }
        if parents.insert(relation.child_note_id.as_str(), relation.parent_note_id.as_str()).is_some() {
            return Err(StorageError::InvalidBackup("Rustnote bundle note hierarchy assigns more than one parent to a note".to_string()));
        }
    }
    for child in parents.keys().copied() {
        let mut current = Some(child);
        let mut visited = HashSet::new();
        while let Some(note_id) = current {
            if !visited.insert(note_id) {
                return Err(StorageError::InvalidBackup("Rustnote bundle note hierarchy contains a cycle".to_string()));
            }
            current = parents.get(note_id).copied();
        }
    }
    Ok(())
}

fn validate_notebook_hierarchy(notebooks: &[Notebook]) -> StorageResult<()> {
    let by_id = notebooks.iter().map(|notebook| (notebook.id.as_str(), notebook)).collect::<HashMap<_, _>>();
    for notebook in notebooks {
        let mut current = notebook.parent_id.as_deref();
        let mut visited = HashSet::from([notebook.id.as_str()]);
        while let Some(parent) = current {
            if !visited.insert(parent) {
                return Err(StorageError::InvalidBackup("Rustnote bundle notebook hierarchy contains a cycle".to_string()));
            }
            current = by_id.get(parent).and_then(|item| item.parent_id.as_deref());
        }
    }
    Ok(())
}

fn unique_by<'a>(items: impl Iterator<Item = &'a str>, kind: &str) -> StorageResult<()> {
    let mut seen = HashSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(StorageError::InvalidBackup(format!("bundle contains duplicate {kind} identifier")));
        }
    }
    Ok(())
}

fn bundle_inspection(source: &Path, manifest: &BundleManifest) -> StorageResult<ImportInspection> {
    let note_ids = manifest.notes.iter().map(|note| note.id.clone()).collect::<HashSet<_>>();
    let mut external_note_links = 0_usize;
    for note in &manifest.notes {
        let analysis = analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
        external_note_links += analysis.note_links.iter().filter(|target| !note_ids.contains(*target)).count();
    }
    let warnings = if external_note_links == 0 {
        Vec::new()
    } else {
        vec![format!("{external_note_links} internal note links point outside this bundle and may remain unresolved in another library")]
    };
    Ok(ImportInspection {
        files: 1,
        total_bytes: fs::metadata(source)?.len(),
        rustnote_bundles: 1,
        notes: manifest.notes.len(),
        notebooks: manifest.notebooks.len(),
        attachments: manifest.attachments.len(),
        revisions: manifest.revisions.len(),
        sources: manifest.sources.len(),
        citations: manifest.citations.len(),
        warnings,
        ..ImportInspection::default()
    })
}

fn bundle_manifest_fingerprint(manifest: &BundleManifest) -> StorageResult<String> {
    Ok(sha256_bytes(&serde_json::to_vec(manifest)?))
}

fn load_verified_bundle(source: &Path) -> StorageResult<BundleManifest> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StorageError::Validation("Rustnote bundle must be a regular file".to_string()));
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(StorageError::Validation("Rustnote bundle exceeds the supported archive size".to_string()));
    }
    let file = File::open(source)?;
    let mut archive = ZipArchive::new(file)?;
    let names = validate_zip_structure(&mut archive)?;
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if file.compression() != CompressionMethod::Stored {
            return Err(StorageError::InvalidBackup("Rustnote bundles must use stored ZIP entries".to_string()));
        }
        expanded = expanded.checked_add(file.size()).ok_or_else(|| StorageError::InvalidBackup("bundle size overflow".to_string()))?;
        if expanded > MAX_EXPANDED_BYTES {
            return Err(StorageError::InvalidBackup("Rustnote bundle expands beyond the supported limit".to_string()));
        }
    }
    if !names.contains(MANIFEST_PATH) {
        return Err(StorageError::InvalidBackup("Rustnote bundle is missing manifest.json".to_string()));
    }
    let manifest_bytes = read_zip_entry_bounded(&mut archive, MANIFEST_PATH, MAX_MANIFEST_BYTES)?;
    let manifest: BundleManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_bundle_manifest(&manifest)?;
    if names.len() != manifest.blobs.len() + 1 {
        return Err(StorageError::InvalidBackup("Rustnote bundle contains unexpected archive entries".to_string()));
    }
    for blob in &manifest.blobs {
        let path = format!("{BLOB_PREFIX}{}", blob.sha256);
        if !names.contains(&path) {
            return Err(StorageError::InvalidBackup(format!("Rustnote bundle is missing {path}")));
        }
        let mut entry = archive.by_name(&path)?;
        if entry.size() != blob.byte_size {
            return Err(StorageError::InvalidBackup(format!("bundle blob {} has the wrong size", blob.sha256)));
        }
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 { break; }
            hasher.update(&buffer[..read]);
        }
        if digest_to_hex(&hasher.finalize()) != blob.sha256 {
            return Err(StorageError::Integrity(format!("bundle blob {} failed checksum verification", blob.sha256)));
        }
    }
    Ok(manifest)
}

fn insert_bundle_notebooks(
    transaction: &rusqlite::Transaction<'_>,
    notebooks: &[Notebook],
    ids: &HashMap<String, String>,
) -> StorageResult<()> {
    let mut pending = notebooks.iter().collect::<Vec<_>>();
    let mut inserted = HashSet::new();
    while !pending.is_empty() {
        let Some(index) = pending.iter().position(|source| {
            source.parent_id.as_ref().is_none_or(|parent| inserted.contains(parent))
        }) else {
            return Err(StorageError::Integrity(
                "bundle notebook hierarchy could not be imported".to_string(),
            ));
        };
        let source = pending.remove(index);
        let id = ids.get(&source.id).ok_or_else(|| {
            StorageError::Integrity("bundle notebook mapping is incomplete".to_string())
        })?;
        let parent = source.parent_id.as_ref().and_then(|value| ids.get(value));
        transaction.execute(
            "INSERT INTO notebooks(id, name, parent_id, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, &source.name, parent, source.sort_order, source.created_at, source.updated_at],
        )?;
        inserted.insert(source.id.clone());
    }
    Ok(())
}

fn imported_attachment(source: &Attachment, note_ids: &HashMap<String, NoteId>, attachment_ids: &HashMap<String, String>) -> StorageResult<Attachment> {
    let mut item = source.clone();
    item.id = attachment_ids.get(&source.id).cloned().ok_or_else(|| StorageError::Integrity("attachment mapping is incomplete".to_string()))?;
    item.note_id = note_ids.get(source.note_id.as_str()).cloned().ok_or_else(|| StorageError::Integrity("attachment note mapping is incomplete".to_string()))?;
    item.source_attachment_id = source.source_attachment_id.as_ref().and_then(|value| attachment_ids.get(value).cloned());
    Ok(item)
}

fn insert_bundle_attachments(
    transaction: &rusqlite::Transaction<'_>,
    attachments: &[Attachment],
    note_ids: &HashMap<String, NoteId>,
    attachment_ids: &HashMap<String, String>,
) -> StorageResult<()> {
    let mut items = attachments.iter().collect::<Vec<_>>();
    items.sort_by_key(|item| item.source_attachment_id.is_some());
    for source in items {
        let item = imported_attachment(source, note_ids, attachment_ids)?;
        transaction.execute(
            "INSERT INTO attachments(id, note_id, file_name, mime_type, byte_size, sha256, image_pixel_width, image_pixel_height, image_format, image_orientation, image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![item.id, item.note_id, item.file_name, item.mime_type, item.byte_size, item.sha256, item.image_pixel_width, item.image_pixel_height, item.image_format, item.image_orientation, item.image_has_alpha.map(i64::from), item.image_animated.map(i64::from), item.source_attachment_id, item.image_edit_recipe, item.embedded as i64, item.unreferenced_at, item.created_at, item.updated_at],
        )?;
    }
    Ok(())
}

fn insert_bundle_hierarchy(transaction: &rusqlite::Transaction<'_>, relations: &[BundleNoteParent], note_ids: &HashMap<String, NoteId>) -> StorageResult<()> {
    for relation in relations {
        transaction.execute("INSERT INTO note_parent(child_note_id, parent_note_id) VALUES (?1, ?2)", params![&note_ids[relation.child_note_id.as_str()], &note_ids[relation.parent_note_id.as_str()]])?;
    }
    Ok(())
}

fn insert_bundle_sources(
    transaction: &rusqlite::Transaction<'_>,
    manifest: &BundleManifest,
    note_ids: &HashMap<String, NoteId>,
    source_ids: &HashMap<String, SourceId>,
    citation_ids: &HashMap<String, String>,
) -> StorageResult<()> {
    for source in &manifest.sources {
        let id = &source_ids[source.id.as_str()];
        transaction.execute("INSERT INTO sources(id, kind, title, reference, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![id, source.kind.value(), &source.title, &source.reference, source.created_at, source.updated_at])?;
    }
    for link in &manifest.note_sources {
        transaction.execute("INSERT INTO note_sources(note_id, source_id, created_at) VALUES (?1, ?2, ?3)", params![&note_ids[link.note_id.as_str()], &source_ids[link.source_id.as_str()], link.created_at])?;
    }
    for citation in &manifest.citations {
        transaction.execute("INSERT INTO citations(id, note_id, source_id, locator, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![&citation_ids[&citation.id], &note_ids[citation.note_id.as_str()], &source_ids[citation.source_id.as_str()], &citation.locator, citation.created_at])?;
    }
    Ok(())
}

fn insert_bundle_annotations(transaction: &rusqlite::Transaction<'_>, annotations: &[BundlePdfAnnotation], attachment_ids: &HashMap<String, String>) -> StorageResult<()> {
    for annotation in annotations {
        transaction.execute("INSERT INTO pdf_annotation_pages(attachment_id, page_index, annotations_json, updated_at) VALUES (?1, ?2, ?3, ?4)", params![&attachment_ids[&annotation.attachment_id], annotation.page_index, &annotation.annotations_json, annotation.updated_at])?;
    }
    Ok(())
}

fn insert_bundle_revisions(
    transaction: &rusqlite::Transaction<'_>,
    manifest: &BundleManifest,
    note_ids: &HashMap<String, NoteId>,
    notebook_ids: &HashMap<String, String>,
    attachment_ids: &HashMap<String, String>,
    revision_ids: &HashMap<String, String>,
) -> StorageResult<()> {
    for revision in &manifest.revisions {
        let mut snapshot: RevisionSnapshot = serde_json::from_str(&revision.snapshot_json)?;
        let old_note_id = snapshot.note.id.as_str().to_string();
        snapshot.note.id = note_ids[&old_note_id].clone();
        snapshot.note.notebook_id = notebook_ids.get(&snapshot.note.notebook_id).cloned().ok_or_else(|| StorageError::Integrity("revision notebook mapping is incomplete".to_string()))?;
        let old_editor = snapshot.note.editor_state.as_str().to_string();
        let editor = remap_canonical_editor_state(&old_editor, note_ids, attachment_ids).map_err(|error| StorageError::Validation(error.to_string()))?;
        snapshot.note.editor_state = std::sync::Arc::new(editor);
        for attachment in &mut snapshot.attachments {
            let old_id = attachment.id.clone();
            attachment.id = attachment_ids.get(&old_id).cloned().ok_or_else(|| StorageError::Integrity("revision attachment mapping is incomplete".to_string()))?;
            attachment.source_attachment_id = attachment.source_attachment_id.as_ref().and_then(|value| attachment_ids.get(value).cloned());
        }
        let snapshot_json = serde_json::to_string(&snapshot)?;
        let checksum = sha256_bytes(snapshot_json.as_bytes());
        let byte_size = snapshot_json.len() as i64;
        transaction.execute("INSERT INTO note_revisions(id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![&revision_ids[&revision.id], &note_ids[revision.note_id.as_str()], revision.revision_no, checksum, &snapshot_json, byte_size, revision.created_at])?;
    }
    for reference in &manifest.revision_attachment_refs {
        transaction.execute("INSERT INTO revision_attachment_refs(revision_id, attachment_id, sha256, byte_size) VALUES (?1, ?2, ?3, ?4)", params![&revision_ids[&reference.revision_id], &attachment_ids[&reference.attachment_id], &reference.sha256, reference.byte_size])?;
    }
    Ok(())
}

fn embedded_attachment_names(connection: &Connection, note_id: &NoteId) -> StorageResult<Vec<String>> {
    let mut statement = connection.prepare("SELECT file_name FROM attachments WHERE note_id = ?1 AND embedded = 1 ORDER BY created_at, lower(file_name), id")?;
    Ok(statement.query_map([note_id], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_source_graph_rejects_cycles() {
        assert!(validate_attachment_source_graph(
            [("a", Some("b")), ("b", Some("a"))],
            "attachment",
        ).is_err());
        assert!(validate_attachment_source_graph(
            [("a", None), ("b", Some("a"))],
            "attachment",
        ).is_ok());
    }

    #[test]
    fn note_hierarchy_rejects_cycles_and_duplicate_parents() {
        let a = NoteId::parse("a").expect("note a");
        let b = NoteId::parse("b").expect("note b");
        let c = NoteId::parse("c").expect("note c");
        assert!(validate_note_hierarchy(&[
            BundleNoteParent { child_note_id: a.clone(), parent_note_id: b.clone() },
            BundleNoteParent { child_note_id: b.clone(), parent_note_id: a.clone() },
        ]).is_err());
        assert!(validate_note_hierarchy(&[
            BundleNoteParent { child_note_id: a.clone(), parent_note_id: b },
            BundleNoteParent { child_note_id: a, parent_note_id: c },
        ]).is_err());
    }

    #[test]
    fn manifest_rejects_notebook_cycles() {
        let manifest = BundleManifest {
            format_version: PORTABLE_BUNDLE_VERSION,
            notebooks: vec![
                Notebook { id: "a".to_string(), name: "A".to_string(), parent_id: Some("b".to_string()), sort_order: 0, created_at: 1, updated_at: 1 },
                Notebook { id: "b".to_string(), name: "B".to_string(), parent_id: Some("a".to_string()), sort_order: 1, created_at: 1, updated_at: 1 },
            ],
            notes: Vec::new(), attachments: Vec::new(), note_parents: Vec::new(),
            sources: Vec::new(), note_sources: Vec::new(), citations: Vec::new(), revisions: Vec::new(),
            revision_attachment_refs: Vec::new(), pdf_annotations: Vec::new(), blobs: Vec::new(),
        };
        assert!(validate_bundle_manifest(&manifest).is_err());
    }

    #[test]
    fn manifest_rejects_duplicate_blob_hashes() {
        let manifest = BundleManifest {
            format_version: PORTABLE_BUNDLE_VERSION,
            notebooks: Vec::new(), notes: Vec::new(), attachments: Vec::new(), note_parents: Vec::new(),
            sources: Vec::new(), note_sources: Vec::new(), citations: Vec::new(), revisions: Vec::new(),
            revision_attachment_refs: Vec::new(), pdf_annotations: Vec::new(),
            blobs: vec![BundleBlob { sha256: "0".repeat(64), byte_size: 0 }, BundleBlob { sha256: "0".repeat(64), byte_size: 0 }],
        };
        assert!(validate_bundle_manifest(&manifest).is_err());
    }

    #[test]
    fn manifest_rejects_surplus_unreferenced_blob() {
        let manifest = BundleManifest {
            format_version: PORTABLE_BUNDLE_VERSION,
            notebooks: Vec::new(),
            notes: Vec::new(),
            attachments: Vec::new(),
            note_parents: Vec::new(),
            sources: Vec::new(),
            note_sources: Vec::new(),
            citations: Vec::new(),
            revisions: Vec::new(),
            revision_attachment_refs: Vec::new(),
            pdf_annotations: Vec::new(),
            blobs: vec![BundleBlob {
                sha256: "0".repeat(64),
                byte_size: 0,
            }],
        };
        let error = validate_bundle_manifest(&manifest)
            .expect_err("surplus bundle blob must not enter the canonical namespace");
        assert!(
            error
                .to_string()
                .contains("is not referenced by any imported attachment or revision")
        );
    }

    #[test]
    fn semantic_bundle_fingerprint_tracks_verified_manifest_content() {
        let manifest = BundleManifest {
            format_version: PORTABLE_BUNDLE_VERSION,
            notebooks: Vec::new(), notes: Vec::new(), attachments: Vec::new(), note_parents: Vec::new(),
            sources: Vec::new(), note_sources: Vec::new(), citations: Vec::new(), revisions: Vec::new(),
            revision_attachment_refs: Vec::new(), pdf_annotations: Vec::new(), blobs: Vec::new(),
        };
        let first = bundle_manifest_fingerprint(&manifest).expect("fingerprint");
        let mut changed = manifest.clone();
        changed.blobs.push(BundleBlob { sha256: "0".repeat(64), byte_size: 0 });
        let second = bundle_manifest_fingerprint(&changed).expect("changed fingerprint");
        assert_ne!(first, second);
    }

}
