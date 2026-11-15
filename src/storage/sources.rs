use std::collections::HashSet;

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::domain::{Citation, NoteId, NoteSourceSummary, Source, SourceId, SourceKind};

use crate::security::sensitive_data::SensitiveSourceLink;

use super::{Storage, StorageError, StorageResult, new_citation_id, new_source_id, unix_timestamp};

const MAX_SOURCE_TITLE_LEN: usize = 512;
const MAX_SOURCE_REFERENCE_LEN: usize = 4096;
const MAX_CITATION_LOCATOR_LEN: usize = 1024;
const MAX_SOURCE_CANDIDATES: usize = 20;

impl Storage {
    pub fn list_note_sources(&self, note_id: &NoteId) -> StorageResult<Vec<NoteSourceSummary>> {
        if let Some(summaries) = self.protected_note_sources_if_unlocked(note_id)? {
            return Ok(summaries);
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT
                s.id, s.kind, s.title, s.reference, s.created_at, s.updated_at,
                c.id, c.locator, c.created_at
             FROM note_sources ns
             JOIN sources s ON s.id = ns.source_id
             LEFT JOIN citations c
               ON c.note_id = ns.note_id AND c.source_id = ns.source_id
             WHERE ns.note_id = ?1
             ORDER BY LOWER(CASE WHEN TRIM(s.title) = '' THEN 'Untitled source' ELSE TRIM(s.title) END),
                      s.id, c.created_at, c.id",
        )?;
        let mut rows = statement.query([note_id])?;
        let mut summaries = Vec::<NoteSourceSummary>::new();
        while let Some(row) = rows.next()? {
            let source_id = row.get::<_, SourceId>(0)?;
            let kind_value = row.get::<_, String>(1)?;
            let kind = SourceKind::parse(&kind_value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid source kind {kind_value}"),
                    )),
                )
            })?;
            if summaries
                .last()
                .is_none_or(|summary| summary.source.id != source_id)
            {
                summaries.push(NoteSourceSummary {
                    source: Source {
                        id: source_id.clone(),
                        kind,
                        title: row.get(2)?,
                        reference: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    },
                    citations: Vec::new(),
                });
            }
            if let Some(citation_id) = row.get::<_, Option<String>>(6)? {
                let locator = row.get::<_, Option<String>>(7)?.unwrap_or_default();
                let created_at = row.get::<_, Option<i64>>(8)?.unwrap_or_default();
                if let Some(summary) = summaries.last_mut() {
                    summary.citations.push(Citation {
                        id: citation_id,
                        note_id: note_id.clone(),
                        source_id,
                        locator,
                        created_at,
                    });
                }
            }
        }
        Ok(summaries)
    }

    pub fn search_source_candidates(
        &self,
        note_id: &NoteId,
        query: &str,
    ) -> StorageResult<Vec<Source>> {
        let protected_attached =
            self.protected_note_sources_if_unlocked(note_id)?
                .map(|summaries| {
                    summaries
                        .into_iter()
                        .map(|summary| summary.source.id)
                        .collect::<HashSet<_>>()
                });
        let connection = self.connection()?;
        let query = query.trim();
        let pattern = format!("%{}%", escape_like_pattern(query));
        let mut statement = connection.prepare(
            "SELECT s.id, s.kind, s.title, s.reference, s.created_at, s.updated_at
             FROM sources s
             WHERE NOT EXISTS(
                 SELECT 1 FROM note_sources ns
                 WHERE ns.note_id = ?1 AND ns.source_id = s.id
             )
               AND (?2 = '' OR s.title LIKE ?3 ESCAPE '\\' COLLATE NOCASE
                    OR COALESCE(s.reference, '') LIKE ?3 ESCAPE '\\' COLLATE NOCASE)
             ORDER BY CASE WHEN ?2 = '' THEN s.updated_at END DESC,
                      LOWER(CASE WHEN TRIM(s.title) = '' THEN 'Untitled source' ELSE TRIM(s.title) END),
                      s.id
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![note_id, query, pattern, MAX_SOURCE_CANDIDATES as i64],
            source_from_row,
        )?;
        let mut candidates = rows.collect::<Result<Vec<_>, _>>()?;
        if let Some(attached) = protected_attached {
            candidates.retain(|source| !attached.contains(&source.id));
        }
        Ok(candidates)
    }

    pub fn create_source_and_attach(
        &self,
        note_id: &NoteId,
        kind: SourceKind,
        title: &str,
        reference: Option<&str>,
    ) -> StorageResult<Source> {
        let title = validate_source_title(title)?;
        let reference = normalize_source_reference(reference)?;
        let now = unix_timestamp();
        let source = Source {
            id: new_source_id(),
            kind,
            title,
            reference,
            created_at: now,
            updated_at: now,
        };
        if self
            .mutate_protected_metadata_if_unlocked(note_id, |metadata| {
                metadata.sources.push(SensitiveSourceLink {
                    source: source.clone(),
                    linked_at: now,
                    citations: Vec::new(),
                });
                Ok(())
            })?
            .is_some()
        {
            return Ok(source);
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_note_exists_and_editable(&transaction, note_id)?;
        transaction.execute(
            "INSERT INTO sources(id, kind, title, reference, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                &source.id,
                source.kind.value(),
                &source.title,
                &source.reference,
                now
            ],
        )?;
        attach_source_record(&transaction, note_id, &source.id, now)?;
        transaction.commit()?;
        Ok(source)
    }

    pub fn attach_source(&self, note_id: &NoteId, source_id: &SourceId) -> StorageResult<()> {
        let source = self
            .connection()?
            .query_row(
                "SELECT id, kind, title, reference, created_at, updated_at FROM sources WHERE id = ?1",
                [source_id],
                source_from_row,
            )
            .optional()?;
        if let Some(source) = source {
            let now = unix_timestamp();
            if self
                .mutate_protected_metadata_if_unlocked(note_id, |metadata| {
                    if !metadata
                        .sources
                        .iter()
                        .any(|link| link.source.id == source.id)
                    {
                        metadata.sources.push(SensitiveSourceLink {
                            source: source.clone(),
                            linked_at: now,
                            citations: Vec::new(),
                        });
                    }
                    Ok(())
                })?
                .is_some()
            {
                return Ok(());
            }
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_note_exists_and_editable(&transaction, note_id)?;
        ensure_source_exists(&transaction, source_id)?;
        attach_source_record(&transaction, note_id, source_id, unix_timestamp())?;
        transaction.commit()?;
        Ok(())
    }

    pub fn detach_source(&self, note_id: &NoteId, source_id: &SourceId) -> StorageResult<()> {
        if self
            .mutate_protected_metadata_if_unlocked(note_id, |metadata| {
                metadata.sources.retain(|link| &link.source.id != source_id);
                Ok(())
            })?
            .is_some()
        {
            return Ok(());
        }
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_note_exists_and_editable(&transaction, note_id)?;
        transaction.execute(
            "DELETE FROM note_sources WHERE note_id = ?1 AND source_id = ?2",
            params![note_id, source_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn add_citation(
        &self,
        note_id: &NoteId,
        source_id: &SourceId,
        locator: &str,
    ) -> StorageResult<Citation> {
        let locator = validate_citation_locator(locator)?;
        let citation = Citation {
            id: new_citation_id(),
            note_id: note_id.clone(),
            source_id: source_id.clone(),
            locator,
            created_at: unix_timestamp(),
        };
        if self
            .mutate_protected_metadata_if_unlocked(note_id, |metadata| {
                let link = metadata
                    .sources
                    .iter_mut()
                    .find(|link| &link.source.id == source_id)
                    .ok_or_else(|| {
                        StorageError::Validation(
                            "attach the source to this note before adding a citation".to_string(),
                        )
                    })?;
                link.citations.push(citation.clone());
                Ok(())
            })?
            .is_some()
        {
            return Ok(citation);
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_note_exists_and_editable(&transaction, note_id)?;
        let attached = transaction
            .query_row(
                "SELECT 1 FROM note_sources WHERE note_id = ?1 AND source_id = ?2",
                params![note_id, source_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !attached {
            return Err(StorageError::Validation(
                "attach the source to this note before adding a citation".to_string(),
            ));
        }
        transaction.execute(
            "INSERT INTO citations(id, note_id, source_id, locator, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &citation.id,
                &citation.note_id,
                &citation.source_id,
                &citation.locator,
                citation.created_at
            ],
        )?;
        transaction.commit()?;
        Ok(citation)
    }

    pub fn remove_citation(&self, note_id: &NoteId, citation_id: &str) -> StorageResult<()> {
        if self
            .mutate_protected_metadata_if_unlocked(note_id, |metadata| {
                for link in &mut metadata.sources {
                    link.citations.retain(|citation| citation.id != citation_id);
                }
                Ok(())
            })?
            .is_some()
        {
            return Ok(());
        }
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_note_exists_and_editable(&transaction, note_id)?;
        transaction.execute(
            "DELETE FROM citations WHERE id = ?1 AND note_id = ?2",
            params![citation_id, note_id],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    let kind_value = row.get::<_, String>(1)?;
    let kind = SourceKind::parse(&kind_value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid source kind {kind_value}"),
            )),
        )
    })?;
    Ok(Source {
        id: row.get(0)?,
        kind,
        title: row.get(2)?,
        reference: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn validate_source_title(title: &str) -> StorageResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(StorageError::Validation(
            "source title cannot be empty".to_string(),
        ));
    }
    if title.len() > MAX_SOURCE_TITLE_LEN || title.chars().any(char::is_control) {
        return Err(StorageError::Validation(
            "source title is invalid".to_string(),
        ));
    }
    Ok(title.to_string())
}

fn normalize_source_reference(reference: Option<&str>) -> StorageResult<Option<String>> {
    let Some(reference) = reference.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if reference.len() > MAX_SOURCE_REFERENCE_LEN || reference.chars().any(char::is_control) {
        return Err(StorageError::Validation(
            "source reference is invalid".to_string(),
        ));
    }
    Ok(Some(reference.to_string()))
}

fn validate_citation_locator(locator: &str) -> StorageResult<String> {
    let locator = locator.trim();
    if locator.is_empty()
        || locator.len() > MAX_CITATION_LOCATOR_LEN
        || locator.chars().any(char::is_control)
    {
        return Err(StorageError::Validation(
            "citation locator is invalid".to_string(),
        ));
    }
    Ok(locator.to_string())
}

fn attach_source_record(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
    source_id: &SourceId,
    created_at: i64,
) -> StorageResult<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO note_sources(note_id, source_id, created_at)
         VALUES (?1, ?2, ?3)",
        params![note_id, source_id, created_at],
    )?;
    Ok(())
}

fn ensure_note_exists_and_editable(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
) -> StorageResult<()> {
    let trashed = transaction
        .query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [note_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    match trashed {
        None => Err(StorageError::Validation(
            "the note no longer exists".to_string(),
        )),
        Some(value) if value != 0 => Err(StorageError::Validation(
            "sources are read-only while the note is in Trash".to_string(),
        )),
        Some(_) if super::protection::protected_note_exists(transaction, note_id)? => {
            Err(StorageError::Integrity(
                "protected source mutation bypassed the encrypted metadata authority".to_string(),
            ))
        }
        Some(_) => Ok(()),
    }
}

fn ensure_source_exists(transaction: &Transaction<'_>, source_id: &SourceId) -> StorageResult<()> {
    let exists = transaction
        .query_row("SELECT 1 FROM sources WHERE id = ?1", [source_id], |_| {
            Ok(())
        })
        .optional()?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "the selected source no longer exists".to_string(),
        ))
    }
}

fn escape_like_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
