use rusqlite::OptionalExtension;

use crate::domain::{HierarchyNoteSummary, NoteId, NotePropertiesContext};

use super::{Storage, StorageResult};

impl Storage {
    /// Reads contextual note facts from their canonical stores without persisting a
    /// duplicate property record.
    pub fn note_properties_context(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<NotePropertiesContext> {
        let connection = self.connection()?;
        let context = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM attachments WHERE note_id = ?1 AND embedded = 1),
                    (SELECT COUNT(*) FROM note_links WHERE source_note_id = ?1),
                    (SELECT COUNT(*) FROM note_links WHERE target_note_id = ?1),
                    (SELECT COUNT(*) FROM note_parent WHERE parent_note_id = ?1),
                    p.id, p.title, b.name, p.archived, p.trashed
                 FROM notes n
                 LEFT JOIN note_parent h ON h.child_note_id = n.id
                 LEFT JOIN notes p ON p.id = h.parent_note_id
                 LEFT JOIN notebooks b ON b.id = p.notebook_id
                 WHERE n.id = ?1",
                [note_id],
                |row| {
                    let parent_id = row.get::<_, Option<NoteId>>(4)?;
                    let parent_title = row.get::<_, Option<String>>(5)?.unwrap_or_default();
                    let parent_notebook = row.get::<_, Option<String>>(6)?.unwrap_or_default();
                    let parent_archived = row.get::<_, Option<i64>>(7)?.unwrap_or_default() != 0;
                    let parent_trashed = row.get::<_, Option<i64>>(8)?.unwrap_or_default() != 0;
                    Ok(NotePropertiesContext {
                        attachment_count: row.get::<_, i64>(0)?.max(0) as usize,
                        outgoing_link_count: row.get::<_, i64>(1)?.max(0) as usize,
                        backlink_count: row.get::<_, i64>(2)?.max(0) as usize,
                        child_count: row.get::<_, i64>(3)?.max(0) as usize,
                        parent: parent_id.map(|id| HierarchyNoteSummary {
                            id,
                            title: parent_title,
                            notebook_name: parent_notebook,
                            archived: parent_archived,
                            trashed: parent_trashed,
                        }),
                    })
                },
            )
            .optional()?;
        Ok(context.unwrap_or_default())
    }
}
