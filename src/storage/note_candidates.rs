use rusqlite::{Connection, params};

use crate::{
    domain::{NoteCandidate, NoteId},
    search::ParsedSearch,
};

use super::{Storage, StorageResult};

const NOTE_CANDIDATE_SEARCH_LIMIT: i64 = 20;

impl Storage {
    /// Search a bounded set of non-trashed notes for note-selection workflows.
    ///
    /// This is shared by internal-link insertion and hierarchy editing so neither
    /// surface needs to preload the library into the editor runtime.
    pub fn search_note_candidates(&self, query: &str) -> StorageResult<Vec<NoteCandidate>> {
        let connection = self.connection()?;
        search_note_candidates_with_connection(&connection, query)
    }
}

fn search_note_candidates_with_connection(
    connection: &Connection,
    query: &str,
) -> StorageResult<Vec<NoteCandidate>> {
    let query = query.trim();
    if query.is_empty() {
        let mut statement = connection.prepare(
            "SELECT n.id, n.title, b.name, n.archived
             FROM notes n
             JOIN notebooks b ON b.id = n.notebook_id
             WHERE n.trashed = 0
             ORDER BY n.updated_at DESC, n.id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([NOTE_CANDIDATE_SEARCH_LIMIT], candidate_row)?;
        let mut candidates = Vec::new();
        for row in rows {
            let (id, title, notebook_name, archived) = row?;
            candidates.push(NoteCandidate {
                id,
                title,
                notebook_name,
                archived,
            });
        }
        return Ok(candidates);
    }

    let title_query = query
        .split_whitespace()
        .map(|part| {
            part.chars()
                .filter(|character| !matches!(character, '"' | ':'))
                .collect::<String>()
        })
        .filter(|part| !part.is_empty())
        .map(|part| format!("title:{part}"))
        .collect::<Vec<_>>()
        .join(" ");
    let parsed_search = ParsedSearch::parse(&title_query);
    let fts_expression = super::search_query::preview_fts_expression(&parsed_search);
    if fts_expression.is_empty() {
        return Ok(Vec::new());
    }

    let mut statement = connection.prepare(
        "SELECT n.id, n.title, b.name, n.archived
         FROM notes n
         JOIN notebooks b ON b.id = n.notebook_id
         WHERE n.trashed = 0
           AND n.id IN (
               SELECT note_id FROM notes_fts WHERE notes_fts MATCH ?1
           )
         ORDER BY
           CASE
             WHEN LOWER(TRIM(n.title)) = LOWER(?2) THEN 0
             WHEN LOWER(TRIM(n.title)) LIKE LOWER(?2) || '%' THEN 1
             ELSE 2
           END,
           n.updated_at DESC,
           n.id DESC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![fts_expression, query, NOTE_CANDIDATE_SEARCH_LIMIT],
        candidate_row,
    )?;
    let mut candidates = Vec::new();
    for row in rows {
        let (id, title, notebook_name, archived) = row?;
        candidates.push(NoteCandidate {
            id,
            title,
            notebook_name,
            archived,
        });
    }
    Ok(candidates)
}

fn candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(NoteId, String, String, bool)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get::<_, i64>(3)? != 0,
    ))
}
