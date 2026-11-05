use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::{
    attachment_media::{kind as attachment_kind, mime_type_from_path},
    attachment_paths::canonical_blob_relative_path,
    domain::{DEFAULT_NOTEBOOK_ID, Note, NoteId, Notebook},
    editor_projection::derive_editor_projections,
    portability::{
        ImportInspection, ImportPlan, ImportPlanPayload, ImportSummary, ImportUndoToken, PlannedImportFile,
        PlannedImportResource, PortabilityFormat, PortabilityScope, ResourceResolution, canonical_editor_state,
        decode_editable, encode_editable, is_asset_namespace_name, resolve_import_placeholders,
        rewrite_note_link_placeholders, safe_relative_resource_path,
    },
};

use super::{
    Storage, StorageError, StorageResult,
    attachments::{copy_file_atomically, sha256_bytes, sha256_file, validate_attachment_blob_identity},
    atomic_output::{AtomicDirectoryOutput, AtomicFileOutput},
    backup_support::sanitize_component,
    notes::note_from_row,
    notebooks::{notebook_export_paths, notebook_from_row},
};

const MAX_EDITABLE_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMPORT_BATCH_BYTES: u64 = 256 * 1024 * 1024;
const MAX_IMPORT_BATCH_FILES: usize = 10_000;
const MAX_IMPORT_RESOURCE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_IMPORT_RESOURCE_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_IMPORT_RESOURCES: usize = 100_000;

#[derive(Clone)]
struct ExportNote {
    note: Note,
    relative_path: PathBuf,
}

#[derive(Clone)]
struct ImportFile {
    path: PathBuf,
    relative_path: PathBuf,
    format: PortabilityFormat,
    byte_size: u64,
}

impl Storage {
    pub(super) fn prepare_portability_import(&self, source: &Path) -> StorageResult<ImportPlan> {
        if PortabilityFormat::from_path(source) == Some(PortabilityFormat::RustnoteBundle) {
            let (inspection, semantic_sha256) = self.inspect_portable_bundle_plan(source)?;
            return Ok(ImportPlan {
                source: source.to_path_buf(),
                inspection,
                payload: ImportPlanPayload::RustnoteBundle { semantic_sha256 },
            });
        }

        let (root, files) = discover_import_files(source)?;
        let resolver = ImportResourceResolver::new(&root)?;
        let mut inspection = ImportInspection::default();
        let mut resource_bytes = 0_u64;
        let mut resource_fingerprints = HashMap::<PathBuf, (String, u64)>::new();
        let mut planned_files = Vec::with_capacity(files.len());

        for file in files {
            inspection.files += 1;
            inspection.total_bytes = inspection.total_bytes.saturating_add(file.byte_size);
            inspection.notes += 1;
            match file.format {
                PortabilityFormat::PlainText => inspection.plain_text += 1,
                PortabilityFormat::Markdown => inspection.markdown += 1,
                PortabilityFormat::Html => inspection.html += 1,
                PortabilityFormat::RustnoteBundle => inspection.rustnote_bundles += 1,
            }

            let bytes = read_import_file_bytes(&file.path, file.byte_size)?;
            let file_sha256 = sha256_bytes(&bytes);
            let decoded = decode_editable(file.format, &bytes)
                .map_err(|error| StorageError::Validation(error.to_string()))?;
            inspection.warnings.extend(decoded.warnings);

            let mut resources = Vec::new();
            let mut seen_resources = HashSet::new();
            for resource in &decoded.resources {
                let source_path = resolver.resolve(&file.relative_path, &resource.path)?;
                if !seen_resources.insert(source_path.clone()) {
                    continue;
                }
                if inspection.attachments >= MAX_IMPORT_RESOURCES {
                    return Err(StorageError::Validation("import references too many attachment resources".to_string()));
                }
                let (sha256, size) = if let Some((sha256, size)) = resource_fingerprints.get(&source_path) {
                    (sha256.clone(), *size)
                } else {
                    let fingerprint = sha256_file(&source_path)?;
                    resource_fingerprints.insert(source_path.clone(), fingerprint.clone());
                    fingerprint
                };
                resource_bytes = resource_bytes.checked_add(size).ok_or_else(|| {
                    StorageError::Validation("import resource byte count overflowed".to_string())
                })?;
                if resource_bytes > MAX_IMPORT_RESOURCE_TOTAL_BYTES {
                    return Err(StorageError::Validation(
                        "import attachment resources exceed the supported 64 GiB total".to_string(),
                    ));
                }
                inspection.attachments += 1;
                resources.push(PlannedImportResource { source_path, byte_size: size, sha256 });
            }

            planned_files.push(PlannedImportFile {
                path: file.path,
                relative_path: file.relative_path,
                format: file.format,
                byte_size: file.byte_size,
                sha256: file_sha256,
                resources,
            });
        }

        inspection.total_bytes = inspection.total_bytes.saturating_add(resource_bytes);
        inspection.warnings.sort();
        inspection.warnings.dedup();
        Ok(ImportPlan {
            source: source.to_path_buf(),
            inspection,
            payload: ImportPlanPayload::Editable { root, files: planned_files },
        })
    }

    pub(super) fn import_portability_plan(&self, plan: &ImportPlan) -> StorageResult<ImportSummary> {
        match &plan.payload {
            ImportPlanPayload::Editable { root, files } => self.import_editable_files(root, files),
            ImportPlanPayload::RustnoteBundle { semantic_sha256 } => {
                self.import_planned_bundle(&plan.source, semantic_sha256)
            }
        }
    }

    fn import_planned_bundle(
        &self,
        source: &Path,
        expected_semantic_sha256: &str,
    ) -> StorageResult<ImportSummary> {
        let metadata = fs::symlink_metadata(source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(StorageError::Validation("Rustnote bundle changed since inspection".to_string()));
        }
        let staged_name = format!(".portability-import-{}.rustnote", super::new_id("plan"));
        let staged = self.data_directory.join(&staged_name);
        // A bundle can contain plaintext equivalents for many notes. Serialize
        // only the reservation with snapshot/protection checks, then release the
        // operation lock before the potentially large copy/import. While this
        // library-scoped row exists, Protect and backups reject the unresolved
        // plaintext stage; after the file is deleted, the row can be released.
        let mut owned_plaintext = {
            let _operation_guard = self.lock_operations()?;
            self.reserve_portability_owned_plaintext(&staged_name)?
        };
        if owned_plaintext.path() != staged {
            return Err(StorageError::Integrity(
                "portability staging ownership path mismatch".to_string(),
            ));
        }
        let result = (|| {
            let _ = owned_plaintext.copy_and_hash_from(source)?;
            self.import_portable_bundle_matching_staged(owned_plaintext.path(), expected_semantic_sha256)
        })();
        match (result, self.discard_owned_plaintext(&mut owned_plaintext)) {
            (Ok(summary), Ok(())) => Ok(summary),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(cleanup)) => Err(StorageError::Integrity(format!(
                "Rustnote bundle import completed but plaintext staging cleanup is incomplete: {cleanup}"
            ))),
            (Err(error), Err(cleanup)) => Err(StorageError::Integrity(format!(
                "Rustnote bundle import failed ({error}) and plaintext staging cleanup is incomplete: {cleanup}"
            ))),
        }
    }

    pub(super) fn export_portable_scope(
        &self,
        scope: &PortabilityScope,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        let _operation_guard = self.lock_operations()?;
        self.export_portable_scope_locked(scope, format, destination)
    }

    /// Export a notebook while retaining privacy authority for the exact note
    /// set stabilized under the storage-operation lock. The optimistic first
    /// read avoids the forbidden storage-lock -> privacy-admission ordering;
    /// after permits are acquired, the set is rechecked under storage
    /// serialization before any plaintext is emitted.
    pub(super) fn export_portable_notebook_scope_authorized(
        &self,
        notebook_id: &str,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        let scope = PortabilityScope::Notebook(notebook_id.to_string());
        for _attempt in 0..8 {
            let candidate_ids = {
                let connection = self.read_only_connection()?;
                portable_scope_note_ids(&connection, &scope)?
            };

            if candidate_ids.is_empty() {
                let _operation_guard = self.lock_operations()?;
                let connection = self.read_only_connection()?;
                if portable_scope_note_ids(&connection, &scope)?.is_empty() {
                    return Err(StorageError::Validation(
                        "the selected export scope contains no notes".to_string(),
                    ));
                }
                continue;
            }

            let plaintext_lifetime = self.acquire_multi_note_plaintext_operation(&candidate_ids)?;
            let _operation_guard = self.lock_operations()?;
            let connection = self.read_only_connection()?;
            let stabilized_ids = portable_scope_note_ids(&connection, &scope)?;
            if stabilized_ids != candidate_ids {
                drop(connection);
                drop(_operation_guard);
                drop(plaintext_lifetime);
                continue;
            }
            ensure_portability_scope_unprotected(&connection, &scope)?;
            drop(connection);

            let result = self.export_portable_scope_locked(&scope, format, destination);
            drop(plaintext_lifetime);
            return result;
        }

        Err(StorageError::Validation(
            "the notebook changed repeatedly while export authorization was being stabilized; retry the export"
                .to_string(),
        ))
    }

    fn export_portable_scope_locked(
        &self,
        scope: &PortabilityScope,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        if format == PortabilityFormat::RustnoteBundle {
            self.export_portable_bundle_locked(scope, destination)?;
            return Ok(destination.to_path_buf());
        }
        self.ensure_export_destination_is_external(destination)?;
        let connection = self.read_only_connection()?;
        ensure_portability_scope_unprotected(&connection, scope)?;
        let notebooks = all_notebooks(&connection)?;
        let notes = export_notes(&connection, scope, format, &notebooks)?;
        if notes.is_empty() {
            return Err(StorageError::Validation("the selected export scope contains no notes".to_string()));
        }
        let note_paths = notes
            .iter()
            .map(|record| (record.note.id.as_str().to_string(), record.relative_path.to_string_lossy().replace('\\', "/")))
            .collect::<HashMap<_, _>>();

        if matches!(scope, PortabilityScope::Note(_)) {
            let note = &notes[0];
            export_one_editable_note(self, &connection, note, format, destination, &note_paths)?;
            return Ok(destination.to_path_buf());
        }

        let final_directory = destination.to_path_buf();
        let parent = final_directory.parent().ok_or_else(|| StorageError::Validation("export destination has no parent directory".to_string()))?;
        fs::create_dir_all(parent)?;
        let output = AtomicDirectoryOutput::create(&final_directory)?;
        for note in &notes {
            let file_path = output.path().join(&note.relative_path);
            export_one_editable_note(self, &connection, note, format, &file_path, &note_paths)?;
        }
        let (path, _) = output.commit(|directory| verify_export_tree(directory, notes.len(), format))?;
        Ok(path)
    }

    fn import_editable_files(&self, root: &Path, files: &[PlannedImportFile]) -> StorageResult<ImportSummary> {
        let mut created_notes = Vec::<NoteId>::new();
        let mut created_notebooks = Vec::<String>::new();
        let result = (|| {
            let resolver = ImportResourceResolver::new(root)?;
            let mut imported_resource_count = 0_usize;
            let mut imported_resource_bytes = 0_u64;
            let notebook_map = self.create_import_notebook_tree(root, files, &mut created_notebooks)?;
            let mut note_map = HashMap::<PathBuf, NoteId>::new();
            let mut stem_map = HashMap::<String, Option<NoteId>>::new();
            for file in files {
                let notebook_id = notebook_for_import_file(file, &notebook_map)
                    .unwrap_or_else(|| DEFAULT_NOTEBOOK_ID.to_string());
                let note = self.create_note(Some(&notebook_id))?;
                created_notes.push(note.id.clone());
                note_map.insert(file.relative_path.clone(), note.id.clone());
                let stem = file.relative_path.file_stem().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
                stem_map.entry(stem).and_modify(|entry| *entry = None).or_insert_with(|| Some(note.id.clone()));
            }

            let mut summary = ImportSummary { notes: files.len(), notebooks: created_notebooks.len(), ..ImportSummary::default() };
            for file in files {
                let bytes = read_import_file_bytes(&file.path, file.byte_size)?;
                if sha256_bytes(&bytes) != file.sha256 {
                    return Err(StorageError::Validation(format!(
                        "{} changed after it was inspected; inspect it again before importing",
                        file.path.display()
                    )));
                }
                let mut decoded = decode_editable(file.format, &bytes).map_err(|error| StorageError::Validation(error.to_string()))?;
                let note_id = note_map.get(&file.relative_path).cloned().ok_or_else(|| StorageError::Integrity("import note mapping is incomplete".to_string()))?;
                let mut note = self.get_note(&note_id)?.ok_or_else(|| StorageError::Integrity("newly allocated import note disappeared".to_string()))?;
                let mut resolutions = HashMap::new();
                let planned_resources = file.resources.iter()
                    .map(|resource| (resource.source_path.clone(), resource))
                    .collect::<HashMap<_, _>>();
                let mut resource_cache = HashMap::<PathBuf, ResourceResolution>::new();
                for resource in &decoded.resources {
                    let source = resolver.resolve(&file.relative_path, &resource.path)?;
                    let planned = planned_resources.get(&source).ok_or_else(|| {
                        StorageError::Validation(format!(
                            "resource {} changed after import inspection; inspect the source again",
                            resource.path
                        ))
                    })?;
                    if let Some(existing) = resource_cache.get(&source).cloned() {
                        resolutions.insert(resource.token.clone(), existing);
                        continue;
                    }
                    imported_resource_count = imported_resource_count.saturating_add(1);
                    if imported_resource_count > MAX_IMPORT_RESOURCES {
                        return Err(StorageError::Validation("import references too many attachment resources".to_string()));
                    }
                    imported_resource_bytes = imported_resource_bytes
                        .checked_add(planned.byte_size)
                        .ok_or_else(|| StorageError::Validation("import resource byte count overflowed".to_string()))?;
                    if imported_resource_bytes > MAX_IMPORT_RESOURCE_TOTAL_BYTES {
                        return Err(StorageError::Validation("import attachment resources exceed the supported 64 GiB total".to_string()));
                    }
                    let mime = mime_type_from_path(&source);
                    let attachment = self.add_attachment_file_as(&note_id, &resource.display_name, mime, &source)?;
                    if attachment.sha256 != planned.sha256
                        || u64::try_from(attachment.byte_size).ok() != Some(planned.byte_size)
                    {
                        return Err(StorageError::Validation(format!(
                            "resource {} changed after it was inspected; inspect the source again",
                            resource.path
                        )));
                    }
                    let kind = attachment_kind(&attachment.file_name, attachment.mime_type.as_deref()).as_str().to_string();
                    let resolution = ResourceResolution {
                        attachment_id: attachment.id,
                        kind,
                        display_name: attachment.file_name.clone(),
                        media_type: attachment.mime_type.clone(),
                        byte_size: attachment.byte_size,
                    };
                    resource_cache.insert(source, resolution.clone());
                    resolutions.insert(resource.token.clone(), resolution);
                    summary.attachments += 1;
                }
                resolve_import_placeholders(&mut decoded.document, &resolutions).map_err(|error| StorageError::Validation(error.to_string()))?;
                let relative_parent = file.relative_path.parent().unwrap_or_else(|| Path::new(""));
                rewrite_note_link_placeholders(&mut decoded.document, &|target| {
                    resolve_import_note_target(relative_parent, target, &note_map, &stem_map)
                });
                let editor_state = canonical_editor_state(decoded.document).map_err(|error| StorageError::Validation(error.to_string()))?;
                let projections = derive_editor_projections(&editor_state).map_err(StorageError::from)?;
                note.title = decoded.title_hint.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| file.relative_path.file_stem().and_then(|value| value.to_str()).unwrap_or("Untitled note").to_string());
                note.replace_document(projections.markdown, editor_state, projections.plain_text);
                self.save_note_owned(note)?;
                summary.warnings.extend(decoded.warnings);
            }
            summary.undo = Some(ImportUndoToken {
                note_ids: created_notes.clone(),
                notebook_ids: created_notebooks.clone(),
                blob_hashes: Vec::new(),
            });
            Ok(summary)
        })();

        match result {
            Ok(summary) => Ok(summary),
            Err(primary) => match self.rollback_portability_import(
                &created_notes,
                &created_notebooks,
                &[],
            ) {
                Ok(()) => Err(primary),
                Err(rollback) => Err(StorageError::Integrity(format!(
                    "{primary}; failed import rollback left cleanup work pending: {rollback}"
                ))),
            },
        }
    }

    fn create_import_notebook_tree(
        &self,
        root: &Path,
        files: &[PlannedImportFile],
        created: &mut Vec<String>,
    ) -> StorageResult<HashMap<PathBuf, String>> {
        if root.is_file() {
            return Ok(HashMap::new());
        }
        let mut directories = files
            .iter()
            .filter_map(|file| file.relative_path.parent().map(Path::to_path_buf))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        directories.sort_by_key(|path| (path.components().count(), path.clone()));
        let root_name = root.file_name().and_then(|value| value.to_str()).filter(|value| !value.trim().is_empty()).unwrap_or("Imported notes");
        let root_notebook = self.create_notebook(root_name, None)?;
        created.push(root_notebook.id.clone());
        let mut map = HashMap::from([(PathBuf::new(), root_notebook.id.clone())]);
        for directory in directories {
            if directory.as_os_str().is_empty() { continue; }
            let parent = directory.parent().and_then(|path| map.get(path)).cloned().unwrap_or_else(|| root_notebook.id.clone());
            let name = directory.file_name().and_then(|value| value.to_str()).unwrap_or("Imported notes");
            let notebook = self.create_notebook(name, Some(&parent))?;
            created.push(notebook.id.clone());
            map.insert(directory, notebook.id);
        }
        Ok(map)
    }

    pub(super) fn undo_portability_import(&self, token: &ImportUndoToken) -> StorageResult<()> {
        if token.is_empty() { return Ok(()); }
        self.rollback_portability_import(&token.note_ids, &token.notebook_ids, &token.blob_hashes)
    }

    fn rollback_portability_import(
        &self,
        notes: &[NoteId],
        notebooks: &[String],
        extra_hashes: &[String],
    ) -> StorageResult<()> {
        // Undo is a deletion boundary just like Trash purge. Stop every
        // note-scoped plaintext producer before taking the storage operation
        // lock, otherwise a producer holding a derivative permit could deadlock
        // while waiting for that lock or create staging after cleanup.
        let mut transition_note_ids = notes.to_vec();
        transition_note_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        transition_note_ids.dedup_by(|left, right| left.as_str() == right.as_str());
        let _privacy_guard = if transition_note_ids.is_empty() {
            None
        } else {
            Some(self.begin_multi_note_privacy_transition(
                &transition_note_ids,
                crate::security::NotePrivacyTransition::Delete,
            )?)
        };

        // Retire note-scoped staging while its specialized/generic ownership
        // rows still exist. A failure leaves those rows intact and aborts undo.
        for note_id in &transition_note_ids {
            self.retire_note_plaintext_before_delete_unlocked(note_id)?;
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut hashes = extra_hashes.iter().cloned().collect::<HashSet<_>>();
        for note_id in notes {
            let fts_rowid = transaction
                .query_row(
                    "SELECT fts_rowid FROM notes WHERE id = ?1",
                    [note_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let mut statement =
                transaction.prepare("SELECT id, sha256 FROM attachments WHERE note_id = ?1")?;
            let attachment_rows = statement
                .query_map([note_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for (attachment_id, hash) in attachment_rows {
                hashes.insert(hash);
                transaction.execute(
                    "DELETE FROM sync_queue WHERE entity_type = 'attachment' AND entity_id = ?1",
                    [&attachment_id],
                )?;
            }
            let mut revision_blob_statement = transaction.prepare(
                "SELECT DISTINCT refs.sha256
                 FROM revision_attachment_refs refs
                 JOIN note_revisions revision ON revision.id = refs.revision_id
                 WHERE revision.note_id = ?1",
            )?;
            let revision_blob_rows = revision_blob_statement
                .query_map([note_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(revision_blob_statement);
            hashes.extend(revision_blob_rows);
            if let Some(rowid) = fts_rowid {
                transaction.execute("DELETE FROM notes_fts WHERE rowid = ?1", [rowid])?;
            }
            transaction.execute(
                "DELETE FROM sync_queue WHERE entity_type = 'note' AND entity_id = ?1",
                [note_id],
            )?;
            transaction.execute("DELETE FROM notes WHERE id = ?1", [note_id])?;
        }
        let mut remaining_notebooks = notebooks
            .iter()
            .filter(|id| id.as_str() != DEFAULT_NOTEBOOK_ID)
            .cloned()
            .collect::<HashSet<_>>();
        loop {
            let mut removed_any = false;
            for notebook_id in remaining_notebooks.clone() {
                let removed = transaction.execute(
                    "DELETE FROM notebooks WHERE id = ?1 AND NOT EXISTS(SELECT 1 FROM notes WHERE notebook_id = ?1) AND NOT EXISTS(SELECT 1 FROM notebooks WHERE parent_id = ?1)",
                    [&notebook_id],
                )?;
                if removed > 0 {
                    transaction.execute(
                        "DELETE FROM sync_queue WHERE entity_type = 'notebook' AND entity_id = ?1",
                        [&notebook_id],
                    )?;
                    remaining_notebooks.remove(&notebook_id);
                    removed_any = true;
                }
            }
            if !removed_any {
                break;
            }
        }

        // Transfer any blob that becomes unreferenced from canonical database
        // ownership to the generic plaintext ledger *inside this same
        // transaction*. If this transaction rolls back, canonical references
        // remain; if it commits, the cleanup owner already exists.
        let mut disposable_blob_stages = Vec::new();
        for hash in hashes {
            let referenced: i64 = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM attachments WHERE sha256 = ?1) OR EXISTS(SELECT 1 FROM revision_attachment_refs WHERE sha256 = ?1) OR EXISTS(SELECT 1 FROM vault_blob_refs WHERE sha256 = ?1)",
                [&hash],
                |row| row.get(0),
            )?;
            if referenced == 0
                && let Some(relative) = canonical_blob_relative_path(&hash)
            {
                let stage_id = self
                    .ensure_portability_blob_plaintext_stage_in_transaction(
                        &transaction,
                        &relative,
                    )?;
                disposable_blob_stages.push(stage_id);
            }
        }
        disposable_blob_stages.sort();
        disposable_blob_stages.dedup();
        transaction.commit()?;

        let mut cleanup_failures = Vec::new();
        for stage_id in &disposable_blob_stages {
            let result = self
                .resume_owned_plaintext(stage_id)
                .and_then(|mut owned| self.discard_owned_plaintext(&mut owned));
            if let Err(error) = result {
                cleanup_failures.push(error.to_string());
            }
        }
        if !cleanup_failures.is_empty() {
            return Err(StorageError::Integrity(format!(
                "portability undo removed database references but plaintext blob cleanup remains durably owned: {}",
                cleanup_failures.join("; ")
            )));
        }

        drop(_privacy_guard);
        Ok(())
    }

}

fn discover_import_files(source: &Path) -> StorageResult<(PathBuf, Vec<ImportFile>)> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(StorageError::Validation("import source cannot be a symbolic link".to_string()));
    }
    let root = source.to_path_buf();
    let mut files = Vec::new();
    if metadata.is_file() {
        let format = PortabilityFormat::from_path(source).filter(|format| *format != PortabilityFormat::RustnoteBundle).ok_or_else(|| StorageError::Validation("unsupported import file type".to_string()))?;
        validate_import_file(source, format, PathBuf::from(source.file_name().unwrap_or_default()), &mut files)?;
    } else if metadata.is_dir() {
        discover_import_directory(source, source, &mut files)?;
    } else {
        return Err(StorageError::Validation("import source must be a regular file or directory".to_string()));
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if files.is_empty() {
        return Err(StorageError::Validation("no supported TXT, Markdown, or HTML files were found".to_string()));
    }
    if files.len() > MAX_IMPORT_BATCH_FILES {
        return Err(StorageError::Validation("import contains too many files".to_string()));
    }
    let total = files.iter().try_fold(0_u64, |total, file| total.checked_add(file.byte_size).ok_or_else(|| StorageError::Validation("import byte count overflowed".to_string())))?;
    if total > MAX_IMPORT_BATCH_BYTES {
        return Err(StorageError::Validation("import batch is larger than 256 MiB".to_string()));
    }
    Ok((root, files))
}

fn discover_import_directory(root: &Path, directory: &Path, files: &mut Vec<ImportFile>) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() { continue; }
        let path = entry.path();
        if metadata.is_dir() {
            if is_asset_namespace_name(&entry.file_name()) {
                continue;
            }
            discover_import_directory(root, &path, files)?;
        } else if metadata.is_file()
            && let Some(format) = PortabilityFormat::from_path(&path)
            && format != PortabilityFormat::RustnoteBundle
        {
            let relative = path.strip_prefix(root).map_err(|_| StorageError::Integrity("import path escaped its selected root".to_string()))?.to_path_buf();
            validate_import_file(&path, format, relative, files)?;
        }
    }
    Ok(())
}

fn validate_import_file(path: &Path, format: PortabilityFormat, relative_path: PathBuf, files: &mut Vec<ImportFile>) -> StorageResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() { return Ok(()); }
    if metadata.len() > MAX_EDITABLE_FILE_BYTES { return Err(StorageError::Validation(format!("import file is larger than 32 MiB: {}", path.display()))); }
    files.push(ImportFile { path: path.to_path_buf(), relative_path, format, byte_size: metadata.len() });
    Ok(())
}

pub(super) fn ensure_portability_scope_unprotected(
    connection: &Connection,
    scope: &PortabilityScope,
) -> StorageResult<()> {
    let notebooks = all_notebooks(connection)?;
    let allowed_notebooks = match scope {
        PortabilityScope::Notebook(root) => Some(notebook_descendants(&notebooks, root)?),
        PortabilityScope::Library | PortabilityScope::Note(_) => None,
    };
    let mut statement = connection.prepare(
        "SELECT n.id, n.notebook_id FROM vault_note_roots root JOIN notes n ON n.id = root.note_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, NoteId>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (note_id, notebook_id) = row?;
        let included = match scope {
            PortabilityScope::Library => true,
            PortabilityScope::Note(selected) => selected == &note_id,
            PortabilityScope::Notebook(_) => allowed_notebooks
                .as_ref()
                .is_some_and(|ids| ids.contains(&notebook_id)),
        };
        if included {
            return Err(StorageError::Validation(
                "protected notes cannot be included in plaintext editable/portable exports; remove protection first"
                    .to_string(),
            ));
        }
    }
    Ok(())
}


fn portable_scope_note_ids(
    connection: &Connection,
    scope: &PortabilityScope,
) -> StorageResult<Vec<NoteId>> {
    let notebooks = all_notebooks(connection)?;
    let allowed_notebooks = match scope {
        PortabilityScope::Notebook(root) => Some(notebook_descendants(&notebooks, root)?),
        PortabilityScope::Library | PortabilityScope::Note(_) => None,
    };
    let mut statement = connection.prepare("SELECT id, notebook_id FROM notes ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, NoteId>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut note_ids = Vec::new();
    for row in rows {
        let (note_id, notebook_id) = row?;
        let included = match scope {
            PortabilityScope::Library => true,
            PortabilityScope::Note(selected) => selected == &note_id,
            PortabilityScope::Notebook(_) => allowed_notebooks
                .as_ref()
                .is_some_and(|ids| ids.contains(&notebook_id)),
        };
        if included {
            note_ids.push(note_id);
        }
    }
    note_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    Ok(note_ids)
}

fn all_notebooks(connection: &Connection) -> StorageResult<Vec<Notebook>> {
    let mut statement = connection.prepare("SELECT id, name, parent_id, sort_order, created_at, updated_at FROM notebooks ORDER BY parent_id IS NOT NULL, sort_order, lower(name), id")?;
    Ok(statement.query_map([], notebook_from_row)?.collect::<Result<Vec<_>, _>>()?)
}

fn export_notes(connection: &Connection, scope: &PortabilityScope, format: PortabilityFormat, notebooks: &[Notebook]) -> StorageResult<Vec<ExportNote>> {
    let allowed_notebooks = match scope {
        PortabilityScope::Library | PortabilityScope::Note(_) => None,
        PortabilityScope::Notebook(root) => Some(notebook_descendants(notebooks, root)?),
    };
    let notebook_paths = notebook_export_paths(notebooks)?;
    let mut statement = connection.prepare(
        "SELECT n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text, n.pinned, n.archived, n.trashed, n.purge_after, n.created_at, n.updated_at,
         COALESCE((SELECT group_concat(t.name, ', ') FROM note_tags nt JOIN tags t ON t.id = nt.tag_id WHERE nt.note_id = n.id), '') AS tag_names,
         COALESCE((SELECT group_concat(a.file_name, char(31)) FROM attachments a WHERE a.note_id = n.id AND a.embedded = 1), '') AS attachment_names
         FROM notes n ORDER BY n.created_at, n.id"
    )?;
    let mut rows = statement.query([])?;
    let mut notes = Vec::new();
    let mut used_paths = HashSet::<String>::new();
    while let Some(row) = rows.next()? {
        let note = note_from_row(row)?;
        let included = match scope {
            PortabilityScope::Library => true,
            PortabilityScope::Note(note_id) => &note.id == note_id,
            PortabilityScope::Notebook(_) => allowed_notebooks.as_ref().is_some_and(|ids| ids.contains(&note.notebook_id)),
        };
        if !included { continue; }
        let directory = if matches!(scope, PortabilityScope::Note(_)) {
            PathBuf::new()
        } else if note.trashed {
            PathBuf::from("_Trash")
        } else if note.archived {
            PathBuf::from("_Archive")
        } else {
            notebook_paths.get(&note.notebook_id).cloned().unwrap_or_default()
        };
        let relative_path = unique_note_export_path(&directory, note.display_title(), format, &mut used_paths);
        notes.push(ExportNote { note, relative_path });
    }
    Ok(notes)
}

fn export_one_editable_note(storage: &Storage, connection: &Connection, record: &ExportNote, format: PortabilityFormat, destination: &Path, note_paths: &HashMap<String, String>) -> StorageResult<()> {
    if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; }
    let asset_directory_name = format!("{}.assets", destination.file_stem().and_then(|value| value.to_str()).unwrap_or("note"));
    let asset_directory = destination.parent().unwrap_or_else(|| Path::new(".")).join(&asset_directory_name);
    if matches!(format, PortabilityFormat::Markdown | PortabilityFormat::Html) && asset_directory.exists() {
        return Err(StorageError::Validation(format!("portable asset destination already exists: {}", asset_directory.display())));
    }
    let mut attachment_paths = HashMap::new();
    let result = (|| -> StorageResult<()> {
    if matches!(format, PortabilityFormat::Markdown | PortabilityFormat::Html) {
        let attachments = storage.list_attachments_with_connection(connection, &record.note.id)?;
        for attachment in attachments.into_iter().filter(|attachment| attachment.embedded) {
            let (relative_blob, _, expected_size) = validate_attachment_blob_identity(&format!("attachment {}", attachment.id), &attachment.sha256, attachment.byte_size)?;
            fs::create_dir_all(&asset_directory)?;
            let short_id = attachment.id.chars().rev().take(8).collect::<String>().chars().rev().collect::<String>();
            let file_name = format!("{}-{}", sanitize_component(&short_id), sanitize_component(&attachment.file_name));
            let target = asset_directory.join(&file_name);
            copy_file_atomically(&storage.attachments_directory.join(relative_blob), &target)?;
            let (checksum, size) = sha256_file(&target)?;
            if checksum != attachment.sha256 || size != expected_size { return Err(StorageError::Integrity(format!("exported attachment {} failed verification", attachment.id))); }
            attachment_paths.insert(attachment.id, PathBuf::from(&asset_directory_name).join(file_name).to_string_lossy().replace('\\', "/"));
        }
    }
    let relative_note_paths = relative_note_paths_from(record, note_paths);
    let bytes = encode_editable(format, &record.note.title, record.note.editor_state(), &attachment_paths, &relative_note_paths).map_err(|error| StorageError::Validation(error.to_string()))?;
    let mut output = AtomicFileOutput::create(destination)?;
    output.writer()?.write_all(&bytes)?;
    output.commit(|path| { let metadata = fs::symlink_metadata(path)?; if metadata.file_type().is_symlink() || !metadata.is_file() { return Err(StorageError::Integrity("portable note was not written as a regular file".to_string())); } Ok(()) })?;
    Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&asset_directory);
    }
    result
}

fn relative_note_paths_from(record: &ExportNote, note_paths: &HashMap<String, String>) -> HashMap<String, String> {
    let current = &record.relative_path;
    let current_parent = current.parent().unwrap_or_else(|| Path::new(""));
    note_paths.iter().map(|(id, target)| {
        let relative = relative_path_between(current_parent, Path::new(target)).unwrap_or_else(|| PathBuf::from(target));
        (id.clone(), relative.to_string_lossy().replace('\\', "/"))
    }).collect()
}

fn relative_path_between(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let mut common = 0_usize;
    while common < from_components.len() && common < to_components.len() && from_components[common] == to_components[common] { common += 1; }
    let mut result = PathBuf::new();
    for _ in common..from_components.len() { result.push(".."); }
    for component in &to_components[common..] { result.push(component.as_os_str()); }
    Some(result)
}

fn verify_export_tree(directory: &Path, expected_notes: usize, format: PortabilityFormat) -> StorageResult<()> {
    let mut count = 0_usize;
    fn walk(directory: &Path, extension: &str, count: &mut usize) -> StorageResult<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.file_type()?;
            if metadata.is_symlink() {
                return Err(StorageError::Integrity("portable export contains a symbolic link".to_string()));
            }
            if metadata.is_dir() {
                if is_asset_namespace_name(&entry.file_name()) {
                    continue;
                }
                walk(&entry.path(), extension, count)?;
            } else if entry.path().extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension)) {
                *count += 1;
            }
        }
        Ok(())
    }
    walk(directory, format.extension(), &mut count)?;
    if count != expected_notes { return Err(StorageError::Integrity(format!("portable export expected {expected_notes} note files but wrote {count}"))); }
    Ok(())
}

fn notebook_descendants(notebooks: &[Notebook], root: &str) -> StorageResult<HashSet<String>> {
    if !notebooks.iter().any(|notebook| notebook.id == root) { return Err(StorageError::Validation("selected notebook no longer exists".to_string())); }
    let mut ids = HashSet::from([root.to_string()]);
    loop { let mut changed = false; for notebook in notebooks { if notebook.parent_id.as_ref().is_some_and(|parent| ids.contains(parent)) && ids.insert(notebook.id.clone()) { changed = true; } } if !changed { break; } }
    Ok(ids)
}

fn unique_note_export_path(directory: &Path, title: &str, format: PortabilityFormat, used: &mut HashSet<String>) -> PathBuf {
    let stem = sanitize_component(title);
    for index in 1..10_000_usize {
        let file_name = if index == 1 {
            format!("{stem}.{}", format.extension())
        } else {
            format!("{stem}-{index}.{}", format.extension())
        };
        let candidate = directory.join(file_name);
        let key = candidate.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
        if used.insert(key) {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}.{}", super::new_id("export"), format.extension()))
}

fn notebook_for_import_file(file: &PlannedImportFile, map: &HashMap<PathBuf, String>) -> Option<String> {
    let parent = file.relative_path.parent().unwrap_or_else(|| Path::new(""));
    map.get(parent).cloned().or_else(|| map.get(Path::new("")).cloned())
}

struct ImportResourceResolver {
    selected_root: PathBuf,
    lexical_allowed_root: PathBuf,
    allowed_root: PathBuf,
    source_is_file: bool,
}

impl ImportResourceResolver {
    fn new(root: &Path) -> StorageResult<Self> {
        let metadata = fs::symlink_metadata(root)?;
        if metadata.file_type().is_symlink() {
            return Err(StorageError::Validation("import source cannot be a symbolic link".to_string()));
        }
        let source_is_file = metadata.is_file();
        let allowed = if source_is_file {
            root.parent().unwrap_or_else(|| Path::new("."))
        } else {
            root
        };
        Ok(Self {
            selected_root: root.to_path_buf(),
            lexical_allowed_root: allowed.to_path_buf(),
            allowed_root: fs::canonicalize(allowed)?,
            source_is_file,
        })
    }

    fn resolve(&self, note_relative_path: &Path, resource: &str) -> StorageResult<PathBuf> {
        let resource = safe_relative_resource_path(resource).ok_or_else(|| {
            StorageError::Validation(format!("unsafe imported resource path: {resource}"))
        })?;
        let base = if self.source_is_file {
            self.selected_root.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        } else {
            self.selected_root.join(note_relative_path.parent().unwrap_or_else(|| Path::new("")))
        };
        let candidate = base.join(resource);
        reject_symlink_components(&self.lexical_allowed_root, &candidate)?;
        let metadata = fs::symlink_metadata(&candidate)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(StorageError::Validation(format!(
                "import resource is not a regular file: {}",
                candidate.display()
            )));
        }
        if metadata.len() > MAX_IMPORT_RESOURCE_BYTES {
            return Err(StorageError::Validation(format!(
                "import resource is larger than 16 GiB: {}",
                candidate.display()
            )));
        }
        let canonical = fs::canonicalize(&candidate)?;
        if !canonical.starts_with(&self.allowed_root) {
            return Err(StorageError::Validation(format!(
                "import resource escapes the selected root: {}",
                candidate.display()
            )));
        }
        Ok(canonical)
    }
}

fn reject_symlink_components(allowed_root: &Path, candidate: &Path) -> StorageResult<()> {
    let relative = candidate.strip_prefix(allowed_root).map_err(|_| {
        StorageError::Integrity("import resource candidate escaped its lexical root".to_string())
    })?;
    let mut current = allowed_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else { continue; };
        current.push(part);
        if let Ok(metadata) = fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(StorageError::Validation(format!(
                "import resource path contains a symbolic link: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

fn read_import_file_bytes(path: &Path, expected_size: u64) -> StorageResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != expected_size {
        return Err(StorageError::Validation(format!(
            "{} changed after it was inspected; inspect it again before importing",
            path.display()
        )));
    }
    if expected_size > MAX_EDITABLE_FILE_BYTES {
        return Err(StorageError::Validation(format!(
            "import file is larger than 32 MiB: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(StorageError::from)
}

fn resolve_import_note_target(relative_parent: &Path, target: &str, note_map: &HashMap<PathBuf, NoteId>, stem_map: &HashMap<String, Option<NoteId>>) -> Option<NoteId> {
    let target = target.split('#').next().unwrap_or(target).trim();
    if target.is_empty() { return None; }
    if !target.contains('/') && !target.contains('\\') && Path::new(target).extension().is_none() {
        return stem_map.get(&target.to_ascii_lowercase()).and_then(Clone::clone);
    }
    let relative = normalize_relative_path(&relative_parent.join(target))?;
    note_map.get(&relative).cloned()
}

fn normalize_relative_path(path: &Path) -> Option<PathBuf> {
    let mut stack = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => stack.push(part.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir => { stack.pop()?; }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(stack.into_iter().collect())
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::new_id;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let root = std::env::temp_dir().join(format!("rustnote-{}", new_id(label)));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups")).expect("storage");
        (storage, root)
    }

    #[test]
    fn editable_import_is_bounded_and_rejects_symlink_source() {
        let (_storage, root) = test_storage("portable-inspect");
        let path = root.join("note.md"); fs::write(&path, "# Note\n\nBody").expect("write");
        let (_, files) = discover_import_files(&path).expect("discover");
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn exported_asset_directories_are_not_discovered_as_notes() {
        let (_storage, root) = test_storage("portable-assets");
        let folder = root.join("export");
        let assets = folder.join("Topic.assets");
        fs::create_dir_all(&assets).expect("asset directory");
        fs::write(folder.join("Topic.md"), "![Attachment](Topic.assets/reference.md)\n").expect("note");
        fs::write(assets.join("reference.md"), "this is an attachment, not another note").expect("asset");

        let (_, files) = discover_import_files(&folder).expect("discover");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, PathBuf::from("Topic.md"));
    }

    #[test]
    fn undo_import_removes_nested_notebooks_independent_of_token_order() {
        let (storage, root) = test_storage("portable-undo-notebooks");
        let parent = storage.create_notebook("Imported parent", None).expect("parent notebook");
        let child = storage.create_notebook("Imported child", Some(&parent.id)).expect("child notebook");
        let token = ImportUndoToken {
            note_ids: Vec::new(),
            notebook_ids: vec![parent.id.clone(), child.id.clone()],
            blob_hashes: Vec::new(),
        };
        storage.undo_portability_import(&token).expect("undo nested notebooks");
        let notebooks = storage.list_notebooks().expect("list notebooks");
        assert!(!notebooks.iter().any(|notebook| notebook.id == parent.id || notebook.id == child.id));
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_resources_must_remain_inside_the_selected_import_root() {
        let (_storage, root) = test_storage("portable-resource-root");
        let folder = root.join("export");
        let assets = folder.join("Topic.assets");
        fs::create_dir_all(&assets).expect("asset directory");
        fs::write(folder.join("Topic.md"), "![Map](Topic.assets/map.png)\n").expect("note");
        fs::write(assets.join("map.png"), b"image-bytes").expect("asset");

        let resolver = ImportResourceResolver::new(&folder).expect("resolver");
        let source = resolver.resolve(Path::new("Topic.md"), "Topic.assets/map.png")
            .expect("safe resource");
        assert_eq!(
            source,
            fs::canonicalize(assets.join("map.png")).expect("canonical asset path")
        );
        assert!(resolver.resolve(Path::new("Topic.md"), "../outside.png").is_err());
    }
    #[test]
    fn planned_import_rejects_note_file_changes_after_preview() {
        let (storage, root) = test_storage("portable-plan-note-change");
        let path = root.join("note.md");
        fs::write(&path, "# Original\n\nBody\n").expect("write original");
        let plan = storage.prepare_portability_import(&path).expect("prepare plan");
        fs::write(&path, "# Changed\n\nBody\n").expect("mutate source");

        let error = storage.import_portability_plan(&plan).expect_err("changed source must fail");
        assert!(error.to_string().contains("changed after it was inspected"));
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn planned_import_rejects_attachment_changes_after_preview() {
        let (storage, root) = test_storage("portable-plan-resource-change");
        let folder = root.join("export");
        let assets = folder.join("Topic.assets");
        fs::create_dir_all(&assets).expect("asset directory");
        fs::write(folder.join("Topic.md"), "![Map](Topic.assets/map.png)\n").expect("note");
        fs::write(assets.join("map.png"), b"original-bytes").expect("asset");
        let plan = storage.prepare_portability_import(&folder).expect("prepare plan");
        fs::write(assets.join("map.png"), b"changed--bytes").expect("mutate asset");

        let error = storage.import_portability_plan(&plan).expect_err("changed asset must fail");
        assert!(error.to_string().contains("changed after it was inspected"));
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }


    #[test]
    fn planned_folder_import_does_not_expand_when_new_files_appear_after_preview() {
        let (storage, root) = test_storage("portable-plan-file-set");
        let folder = root.join("import");
        fs::create_dir_all(&folder).expect("import folder");
        fs::write(folder.join("A.md"), "# A\n").expect("first note");
        let plan = storage.prepare_portability_import(&folder).expect("prepare plan");
        fs::write(folder.join("B.md"), "# B\n").expect("late note");

        let summary = storage.import_portability_plan(&plan).expect("import approved plan");
        assert_eq!(summary.notes, 1);
        storage.undo_portability_import(summary.undo.as_ref().expect("undo token")).expect("undo");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }


    #[test]
    fn undo_import_keeps_blob_cleanup_owned_until_unlink_succeeds() {
        let (storage, root) = test_storage("portable-undo-blob-owner");
        let note = storage.create_note(None).expect("test note");
        let source = root.join("owned.bin");
        let bytes = b"portable undo plaintext blob";
        fs::write(&source, bytes).expect("source blob");
        let attachment = storage
            .add_attachment_file_as(&note.id, "owned.bin", None, &source)
            .expect("attachment");
        let relative = canonical_blob_relative_path(&attachment.sha256).expect("canonical path");
        let blob = storage.attachments_directory.join(&relative);
        assert!(blob.is_file());

        // Force the post-commit unlink to fail. Undo is allowed to remove the
        // database references only because the same transaction first adopts
        // the canonical blob into managed_plaintext_staging.
        fs::remove_file(&blob).expect("replace blob with cleanup obstacle");
        fs::create_dir(&blob).expect("cleanup obstacle");
        let token = ImportUndoToken {
            note_ids: vec![note.id.clone()],
            notebook_ids: Vec::new(),
            blob_hashes: vec![attachment.sha256.clone()],
        };
        let error = storage
            .undo_portability_import(&token)
            .expect_err("unlink obstacle must make undo report incomplete cleanup");
        assert!(error.to_string().contains("remains durably owned"));
        assert!(storage.get_note(&note.id).expect("note lookup").is_none());
        let owner_id: String = storage
            .connection()
            .expect("owner connection")
            .query_row(
                "SELECT id FROM managed_plaintext_staging WHERE relative_path = ?1",
                [&relative],
                |row| row.get(0),
            )
            .expect("durable blob cleanup owner");

        fs::remove_dir(&blob).expect("remove cleanup obstacle");
        fs::write(&blob, bytes).expect("restore owned plaintext for retry");
        storage
            .undo_portability_import(&token)
            .expect("retry cleanup through existing durable owner");
        assert!(!blob.exists(), "retry must remove the unreferenced plaintext blob");
        let owner_exists: i64 = storage
            .connection()
            .expect("owner retry connection")
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM managed_plaintext_staging WHERE id = ?1)",
                [&owner_id],
                |row| row.get(0),
            )
            .expect("owner retry lookup");
        assert_eq!(owner_exists, 0, "owner row may disappear only after unlink succeeds");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

}
