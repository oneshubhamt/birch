use std::collections::HashSet;

use rusqlite::{Transaction, params};

use crate::domain::{BacklinkSummary, NoteId, NoteLinkTarget, NoteLinkTargetState};

use super::{Storage, StorageResult};

impl Storage {
    pub(super) fn reconcile_document_note_links(
        &self,
        transaction: &Transaction<'_>,
        source_note_id: &NoteId,
        target_note_ids: &[NoteId],
    ) -> StorageResult<()> {
        let previous = {
            let mut statement = transaction.prepare(
                "SELECT target_note_id FROM note_links WHERE source_note_id = ?1",
            )?;
            let rows = statement.query_map([source_note_id], |row| row.get::<_, NoteId>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        let current = target_note_ids.iter().cloned().collect::<HashSet<_>>();

        for removed in previous.difference(&current) {
            transaction.execute(
                "DELETE FROM note_links WHERE source_note_id = ?1 AND target_note_id = ?2",
                params![source_note_id, removed],
            )?;
        }
        for added in current.difference(&previous) {
            transaction.execute(
                "INSERT INTO note_links(source_note_id, target_note_id) VALUES (?1, ?2)",
                params![source_note_id, added],
            )?;
        }
        Ok(())
    }

    pub fn list_backlinks(&self, target_note_id: &NoteId) -> StorageResult<Vec<BacklinkSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT nl.source_note_id, n.title, b.name, n.archived, n.trashed
             FROM note_links nl
             JOIN notes n ON n.id = nl.source_note_id
             JOIN notebooks b ON b.id = n.notebook_id
             WHERE nl.target_note_id = ?1
               AND NOT EXISTS(
                   SELECT 1 FROM vault_note_roots root WHERE root.note_id = nl.source_note_id
               )
             ORDER BY n.trashed ASC, n.archived ASC,
                      LOWER(CASE WHEN TRIM(n.title) = '' THEN 'Untitled note' ELSE TRIM(n.title) END),
                      n.id",
        )?;
        let rows = statement.query_map([target_note_id], |row| {
            Ok((
                row.get::<_, NoteId>(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, i64>(4)? != 0,
            ))
        })?;
        let mut backlinks = Vec::new();
        for row in rows {
            let (source_id, title, notebook_name, archived, trashed) = row?;
            backlinks.push(BacklinkSummary {
                source_note_id: source_id,
                title,
                notebook_name,
                archived,
                trashed,
            });
        }
        drop(statement);

        // Protected sources cannot retain a plaintext relationship shadow.
        // Discover their backlinks from encrypted MetadataState only while the
        // source remains unlocked. Locked sources contribute nothing.
        let protected_sources = {
            let mut statement = connection.prepare(
                "SELECT n.id, b.name, n.archived, n.trashed
                 FROM vault_note_roots root
                 JOIN notes n ON n.id = root.note_id
                 JOIN notebooks b ON b.id = n.notebook_id
                 ORDER BY n.id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, NoteId>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        drop(connection);
        for (source_id, notebook_name, archived, trashed) in protected_sources {
            let Some(outgoing) = self.protected_outgoing_note_links_if_unlocked(&source_id)? else {
                continue;
            };
            if !outgoing.iter().any(|candidate| candidate == target_note_id) {
                continue;
            }
            let Some(source_note) = self.get_note(&source_id)? else {
                continue;
            };
            if !source_note.protection.is_unlocked() {
                continue;
            }
            backlinks.push(BacklinkSummary {
                source_note_id: source_id,
                title: source_note.title,
                notebook_name,
                archived,
                trashed,
            });
        }
        backlinks.sort_by(|left, right| {
            left.trashed
                .cmp(&right.trashed)
                .then_with(|| left.archived.cmp(&right.archived))
                .then_with(|| {
                    left.display_title()
                        .to_lowercase()
                        .cmp(&right.display_title().to_lowercase())
                })
                .then_with(|| left.source_note_id.as_str().cmp(right.source_note_id.as_str()))
        });
        Ok(backlinks)
    }

    pub fn list_note_link_targets(&self, source_note_id: &NoteId) -> StorageResult<Vec<NoteLinkTarget>> {
        if let Some(target_ids) = self.protected_outgoing_note_links_if_unlocked(source_note_id)? {
            let mut targets = Vec::with_capacity(target_ids.len());
            for target_id in target_ids {
                let Some(target_note) = self.get_note(&target_id)? else {
                    targets.push(NoteLinkTarget {
                        note_id: target_id,
                        title: None,
                        state: NoteLinkTargetState::Missing,
                    });
                    continue;
                };
                let state = if target_note.trashed {
                    NoteLinkTargetState::Trashed
                } else if target_note.archived {
                    NoteLinkTargetState::Archived
                } else {
                    NoteLinkTargetState::Active
                };
                let title = if target_note.protection.is_locked() {
                    None
                } else {
                    Some(target_note.title)
                };
                targets.push(NoteLinkTarget {
                    note_id: target_id,
                    title,
                    state,
                });
            }
            targets.sort_by(|left, right| left.note_id.as_str().cmp(right.note_id.as_str()));
            return Ok(targets);
        }

        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT nl.target_note_id, n.id, n.title, n.archived, n.trashed,
                    CASE WHEN n.id IS NOT NULL AND EXISTS(
                        SELECT 1 FROM vault_note_roots root WHERE root.note_id = n.id
                    ) THEN 1 ELSE 0 END
             FROM note_links nl
             LEFT JOIN notes n ON n.id = nl.target_note_id
             WHERE nl.source_note_id = ?1
             ORDER BY nl.target_note_id",
        )?;
        let rows = statement.query_map([source_note_id], |row| {
            Ok((
                row.get::<_, NoteId>(0)?,
                row.get::<_, Option<NoteId>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, i64>(5)? != 0,
            ))
        })?;
        let mut targets = Vec::new();
        for row in rows {
            let (target_id, resolved_id, mut title, archived, trashed, protected_target) = row?;
            let mut state = if resolved_id.is_none() {
                NoteLinkTargetState::Missing
            } else if trashed.unwrap_or_default() != 0 {
                NoteLinkTargetState::Trashed
            } else if archived.unwrap_or_default() != 0 {
                NoteLinkTargetState::Archived
            } else {
                NoteLinkTargetState::Active
            };

            // A protected target's SQLite title is intentionally scrubbed. Resolve
            // its title from the Vault only while it is currently unlocked; a
            // locked target remains resolvable by identity/state but exposes no
            // private title. This keeps ordinary-source links behaviorally
            // symmetric with protected-source links without a plaintext shadow.
            if protected_target {
                match self.get_note(&target_id)? {
                    Some(target_note) => {
                        state = if target_note.trashed {
                            NoteLinkTargetState::Trashed
                        } else if target_note.archived {
                            NoteLinkTargetState::Archived
                        } else {
                            NoteLinkTargetState::Active
                        };
                        title = target_note
                            .protection
                            .is_unlocked()
                            .then_some(target_note.title);
                    }
                    None => {
                        state = NoteLinkTargetState::Missing;
                        title = None;
                    }
                }
            }

            targets.push(NoteLinkTarget {
                note_id: target_id,
                title,
                state,
            });
        }
        Ok(targets)
    }
}
