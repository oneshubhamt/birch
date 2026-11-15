//! Narrow synchronous persistence surface for attachment/PDF transport.
//!
//! Custom protocol and loopback-media code need fast, synchronous reads without
//! owning Rustnote's SQLite schema. This module keeps that schema knowledge in
//! storage while exposing only transport-shaped operations.

use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::attachment_paths::canonical_blob_relative_path;

use super::{Storage, StorageError, StorageResult, unix_timestamp};

#[derive(Clone, Debug)]
pub struct AttachmentTransportStore {
    storage: Storage,
}

pub struct AttachmentTransportSession {
    connection: Connection,
    storage: Storage,
}

#[derive(Clone, Debug)]
pub struct AttachmentTransportResource {
    pub path: PathBuf,
    pub content_hash: String,
    pub file_name: String,
    pub mime_type: Option<String>,
    pub image_pixel_width: Option<i64>,
    pub image_pixel_height: Option<i64>,
    pub image_format: Option<String>,
    pub image_animated: Option<bool>,
    pub note_trashed: bool,
    pub protected: bool,
    pub(crate) authorization: crate::security::PlaintextOperationPermit,
}

/// Opaque lifetime guard for note-scoped plaintext attachment ingress.
///
/// The binary transport layer may hold this value but cannot inspect or
/// reconstruct the underlying privacy-coordinator permit. Dropping the guard
/// releases the compound plaintext/Vault authorization.
pub struct AttachmentPlaintextIngressGuard {
    _authorization: crate::security::PlaintextOperationPermit,
}

impl std::fmt::Debug for AttachmentPlaintextIngressGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AttachmentPlaintextIngressGuard([REDACTED])")
    }
}

/// Opaque cleanup guard for one protected transport preview namespace.
/// Security ownership and private-temp filesystem authority stay inside the
/// library crate; the binary protocol can only request cleanup.
pub struct AttachmentEphemeralPlaintextGuard {
    owned: crate::security::OwnedPlaintext,
}

impl std::fmt::Debug for AttachmentEphemeralPlaintextGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AttachmentEphemeralPlaintextGuard([REDACTED])")
    }
}

impl AttachmentEphemeralPlaintextGuard {
    pub fn remove(&mut self) -> StorageResult<()> {
        self.owned.remove_file()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfAnnotationRow {
    pub page_index: u32,
    pub annotations_json: String,
}

impl Storage {
    pub(crate) fn transport_store(&self) -> AttachmentTransportStore {
        AttachmentTransportStore {
            storage: self.clone(),
        }
    }
}

impl AttachmentTransportResource {
    /// Reserve ownership and create one private preview directory beneath the
    /// active protected-attachment session. Ownership exists before the
    /// directory is materialized; ordinary attachments return `None`.
    pub fn prepare_ephemeral_plaintext_directory(
        &self,
        path: PathBuf,
    ) -> StorageResult<Option<AttachmentEphemeralPlaintextGuard>> {
        let Some(permit) = self.authorization.vault_permit() else {
            return Ok(None);
        };
        let mut owned = crate::security::OwnedPlaintext::reserve_vault_session(permit, path)
            .map_err(|message| StorageError::Validation(message.to_string()))?;
        owned.create_private_directory()?;
        Ok(Some(AttachmentEphemeralPlaintextGuard { owned }))
    }
}

impl AttachmentTransportStore {
    pub fn read_session(&self) -> StorageResult<AttachmentTransportSession> {
        self.open_session(true)
    }

    pub fn write_session(&self) -> StorageResult<AttachmentTransportSession> {
        self.open_session(false)
    }

    pub fn attachment_resource(
        &self,
        attachment_id: &str,
    ) -> StorageResult<Option<AttachmentTransportResource>> {
        self.read_session()?.attachment_resource(attachment_id)
    }

    /// Authorize a note-scoped plaintext ingress operation with one compound
    /// coordinator permit. Protected-note access, when available, is captured
    /// atomically with the derivative lifetime rather than reacquired later.
    pub fn plaintext_attachment_ingress_permit(
        &self,
        note_id: &crate::domain::NoteId,
    ) -> StorageResult<AttachmentPlaintextIngressGuard> {
        let authorization = self.storage.acquire_plaintext_operation_for_note(note_id)?;
        self.storage
            .validate_note_accepts_attachment_with_authorization(note_id, &authorization)?;
        Ok(AttachmentPlaintextIngressGuard {
            _authorization: authorization,
        })
    }

    fn open_session(&self, read_only: bool) -> StorageResult<AttachmentTransportSession> {
        let flags = if read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
        };
        let connection = Connection::open_with_flags(self.storage.database_path(), flags)?;
        Storage::configure_connection(&connection)?;
        Ok(AttachmentTransportSession {
            connection,
            storage: self.storage.clone(),
        })
    }
}

impl AttachmentTransportSession {
    pub fn attachment_resource(
        &self,
        attachment_id: &str,
    ) -> StorageResult<Option<AttachmentTransportResource>> {
        let note_id = self
            .connection
            .query_row(
                "SELECT note_id FROM attachments WHERE id = ?1",
                [attachment_id],
                |row| row.get::<_, crate::domain::NoteId>(0),
            )
            .optional()?;
        let Some(note_id) = note_id else {
            return Ok(None);
        };
        let authorization = self
            .storage
            .acquire_plaintext_operation_for_note(&note_id)?;
        self.attachment_resource_with_authorization(attachment_id, authorization)
    }

    fn attachment_resource_with_authorization(
        &self,
        attachment_id: &str,
        authorization: crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Option<AttachmentTransportResource>> {
        let record = self
            .connection
            .query_row(
                "SELECT a.note_id,
                        EXISTS(SELECT 1 FROM vault_note_roots root WHERE root.note_id = a.note_id),
                        a.sha256, a.file_name, a.mime_type,
                        a.image_pixel_width, a.image_pixel_height, a.image_format, a.image_animated,
                        n.trashed
                 FROM attachments a
                 JOIN notes n ON n.id = a.note_id
                 WHERE a.id = ?1",
                [attachment_id],
                |row| {
                    Ok((
                        row.get::<_, crate::domain::NoteId>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?.map(|value| value != 0),
                        row.get::<_, bool>(9)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            note_id,
            protected,
            content_hash,
            file_name,
            mime_type,
            image_pixel_width,
            image_pixel_height,
            image_format,
            image_animated,
            note_trashed,
        )) = record
        else {
            return Ok(None);
        };

        if authorization.note_id() != &note_id {
            return Err(StorageError::Integrity(
                "attachment transport authorization belongs to a different note".to_string(),
            ));
        }

        if protected {
            let access = authorization.access().ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before opening its attachments".to_string(),
                )
            })?;
            if access.permit().note_id() != &note_id {
                return Err(StorageError::Integrity(
                    "attachment transport authorization belongs to a different note".to_string(),
                ));
            }
            let resource = self
                .storage
                .protected_attachment_with_access(attachment_id, access)?
                .ok_or_else(|| {
                    StorageError::Integrity(
                        "protected attachment state changed during authorized transport read"
                            .to_string(),
                    )
                })?;
            return Ok(Some(AttachmentTransportResource {
                path: resource.path,
                content_hash: resource.attachment.sha256,
                file_name: resource.attachment.file_name,
                mime_type: resource.attachment.mime_type,
                image_pixel_width: resource.attachment.image_pixel_width,
                image_pixel_height: resource.attachment.image_pixel_height,
                image_format: resource.attachment.image_format,
                image_animated: resource.attachment.image_animated,
                note_trashed,
                protected: true,
                authorization,
            }));
        }

        let relative_path = canonical_blob_relative_path(&content_hash).ok_or_else(|| {
            StorageError::Integrity("attachment has an invalid blob identity".to_string())
        })?;
        Ok(Some(AttachmentTransportResource {
            path: self.storage.attachments_directory().join(relative_path),
            content_hash,
            file_name,
            mime_type,
            image_pixel_width,
            image_pixel_height,
            image_format,
            image_animated,
            note_trashed,
            protected: false,
            authorization,
        }))
    }

    pub fn read_pdf_annotation_page(
        &self,
        attachment_id: &str,
        page_index: u32,
    ) -> StorageResult<Option<String>> {
        if let Some(annotation) = self
            .storage
            .protected_pdf_annotation_if_unlocked(attachment_id, i64::from(page_index))?
        {
            return Ok(annotation.map(|(annotations_json, _)| annotations_json));
        }
        self.connection
            .query_row(
                "SELECT annotations_json FROM pdf_annotation_pages WHERE attachment_id = ?1 AND page_index = ?2",
                params![attachment_id, i64::from(page_index)],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replace_pdf_annotation_page(
        &self,
        attachment_id: &str,
        page_index: u32,
        canonical_json: &str,
        annotations_empty: bool,
    ) -> StorageResult<()> {
        if self
            .storage
            .replace_protected_pdf_annotation_page_if_unlocked(
                attachment_id,
                i64::from(page_index),
                canonical_json,
                annotations_empty,
            )?
            .is_some()
        {
            return Ok(());
        }
        if annotations_empty {
            self.connection.execute(
                "DELETE FROM pdf_annotation_pages WHERE attachment_id = ?1 AND page_index = ?2",
                params![attachment_id, i64::from(page_index)],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO pdf_annotation_pages(attachment_id, page_index, annotations_json, updated_at)\n                 VALUES (?1, ?2, ?3, ?4)\n                 ON CONFLICT(attachment_id, page_index) DO UPDATE SET\n                   annotations_json = excluded.annotations_json,\n                   updated_at = excluded.updated_at",
                params![
                    attachment_id,
                    i64::from(page_index),
                    canonical_json,
                    unix_timestamp()
                ],
            )?;
        }
        Ok(())
    }

    pub fn list_pdf_annotation_pages(
        &self,
        attachment_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<u32>> {
        if let Some(pages) = self
            .storage
            .protected_pdf_annotation_pages_if_unlocked(attachment_id, limit)?
        {
            return Ok(pages);
        }
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.connection.prepare(
            "SELECT page_index FROM pdf_annotation_pages WHERE attachment_id = ?1 ORDER BY page_index LIMIT ?2",
        )?;
        let rows =
            statement.query_map(params![attachment_id, limit], |row| row.get::<_, u32>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn pdf_annotation_rows(
        &self,
        attachment_id: &str,
        source_byte_limit: usize,
    ) -> StorageResult<Vec<PdfAnnotationRow>> {
        if let Some(rows) = self
            .storage
            .protected_pdf_annotation_rows_if_unlocked(attachment_id, source_byte_limit)?
        {
            return Ok(rows
                .into_iter()
                .map(|(page_index, annotations_json)| PdfAnnotationRow {
                    page_index,
                    annotations_json,
                })
                .collect());
        }
        let mut statement = self.connection.prepare(
            "SELECT page_index, annotations_json FROM pdf_annotation_pages \
             WHERE attachment_id = ?1 ORDER BY page_index ASC",
        )?;
        let mut rows = statement.query(params![attachment_id])?;
        let mut result = Vec::new();
        let mut source_bytes = 0_usize;
        while let Some(row) = rows.next()? {
            let annotation = PdfAnnotationRow {
                page_index: row.get(0)?,
                annotations_json: row.get(1)?,
            };
            source_bytes = source_bytes.saturating_add(annotation.annotations_json.len());
            result.push(annotation);
            if source_bytes > source_byte_limit {
                break;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::*;
    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustnote-a4-{label}-{}-{unique}",
            std::process::id()
        ));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (storage, root)
    }

    #[test]
    fn transport_store_remains_bound_to_its_originating_library() {
        let (storage, root) = test_storage("transport-resource");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("source.txt");
        fs::write(&source, b"transport").expect("write source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "source.txt", Some("text/plain"), &source)
            .expect("add attachment");
        let attachments_directory = storage.attachments_directory().to_path_buf();
        let transport = storage.transport_store();
        drop(storage);

        let resource = transport
            .attachment_resource(&attachment.id)
            .expect("read resource")
            .expect("resource exists");
        assert_eq!(resource.content_hash, attachment.sha256);
        assert_eq!(resource.file_name, attachment.file_name);
        assert!(resource.path.starts_with(attachments_directory));
        assert!(resource.path.is_file());

        drop(transport);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it() {
        let (storage, root) = test_storage("protected-transport-revocation");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("source.pdf");
        fs::write(&source, b"%PDF-1.4\ntransport lease\n%%EOF\n").expect("write pdf");
        let attachment = storage
            .add_attachment_file_as(&note.id, "source.pdf", Some("application/pdf"), &source)
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let transport = storage.transport_store();
        let resource = transport
            .attachment_resource(&attachment.id)
            .expect("protected transport read")
            .expect("protected resource");
        assert!(resource.protected);
        assert!(resource.path.is_file());

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let storage_for_lock = storage.clone();
        let note_for_lock = note.id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            done_tx.send(result).expect("signal lock finish");
        });

        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !crate::storage::protected_note_accepts_vault_reads_for_test(&storage, &note.id)
                .expect("inspect protected-note Vault read state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish revocation promptly"
        );
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must not complete while a protected transport resource is alive"
        );

        drop(resource);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("lock should finish after resource release")
            .expect("lock result");
        lock_thread.join().expect("join lock thread");
        assert!(transport.attachment_resource(&attachment.id).is_err());

        drop(transport);
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn annotation_page_round_trip_stays_inside_transport_store() {
        let (storage, root) = test_storage("transport-annotations");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("source.pdf");
        fs::write(&source, b"%PDF-1.4\n%%EOF\n").expect("write pdf");
        let attachment = storage
            .add_attachment_file_as(&note.id, "source.pdf", Some("application/pdf"), &source)
            .expect("add attachment");
        let store = storage.transport_store();
        let writer = store.write_session().expect("open writer");
        writer
            .replace_pdf_annotation_page(
                &attachment.id,
                3,
                r#"{"version":1,"annotations":[{"id":"ann_1"}]}"#,
                false,
            )
            .expect("write annotations");
        drop(writer);

        let reader = store.read_session().expect("open reader");
        assert_eq!(
            reader
                .read_pdf_annotation_page(&attachment.id, 3)
                .expect("read annotations")
                .as_deref(),
            Some(r#"{"version":1,"annotations":[{"id":"ann_1"}]}"#)
        );
        assert_eq!(
            reader
                .list_pdf_annotation_pages(&attachment.id, 16)
                .expect("list pages"),
            vec![3]
        );

        drop(reader);
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }
}
