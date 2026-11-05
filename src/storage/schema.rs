//! Database schema definition and schema-version invariant.

pub(super) fn canonicalize_schema_sql(sql: &str) -> String {
    let mut canonical = String::with_capacity(sql.len());
    let mut characters = sql.chars().peekable();
    let mut quote: Option<char> = None;
    let mut pending_space = false;

    while let Some(character) = characters.next() {
        if let Some(active_quote) = quote {
            canonical.push(character);
            let closes_quote = match active_quote {
                '[' => character == ']',
                other => character == other,
            };
            if closes_quote {
                if active_quote != '[' && characters.peek() == Some(&active_quote) {
                    canonical.push(characters.next().expect("peeked quote must exist"));
                } else {
                    quote = None;
                }
            }
            continue;
        }

        if character.is_whitespace() {
            pending_space = !canonical.is_empty();
            continue;
        }
        if pending_space {
            if !matches!(character, ',' | ')' | ';')
                && !matches!(canonical.chars().last(), Some('(' | ','))
            {
                canonical.push(' ');
            }
            pending_space = false;
        }
        canonical.push(character);
        if matches!(character, '\'' | '"' | '`' | '[') {
            quote = Some(character);
        }
    }

    canonical.trim_end_matches(';').trim().to_string()
}

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;

pub(super) const BASE_SCHEMA: &str = r#"
CREATE TABLE notebooks (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES notebooks(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_notebooks_parent
    ON notebooks(parent_id, sort_order, name);

CREATE TRIGGER protect_default_notebook_delete
BEFORE DELETE ON notebooks
WHEN OLD.id = 'notebook_notes'
BEGIN
    SELECT RAISE(ABORT, 'the default Notes notebook cannot be deleted');
END;

CREATE TABLE notes (
    id           TEXT PRIMARY KEY,
    fts_rowid    INTEGER NOT NULL UNIQUE CHECK(fts_rowid > 0),
    notebook_id  TEXT NOT NULL REFERENCES notebooks(id) ON DELETE RESTRICT,
    title        TEXT NOT NULL DEFAULT '',
    body         TEXT NOT NULL DEFAULT '',
    editor_state TEXT NOT NULL,
    plain_text   TEXT NOT NULL,
    pinned       INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0, 1)),
    archived     INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
    trashed      INTEGER NOT NULL DEFAULT 0 CHECK(trashed IN (0, 1)),
    purge_after  INTEGER,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    title_style  TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE vault_operations (
    note_id       TEXT PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
    operation     TEXT NOT NULL CHECK(operation IN ('protect', 'unprotect')),
    phase         TEXT NOT NULL CHECK(phase IN ('prepared', 'database_committed')),
    operation_id  TEXT NOT NULL CHECK(length(operation_id) BETWEEN 1 AND 160),
    details_json  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX idx_vault_operations_phase
    ON vault_operations(phase, updated_at, note_id);

CREATE TABLE tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL COLLATE NOCASE UNIQUE,
    created_at  INTEGER NOT NULL
);

CREATE TABLE note_tags (
    note_id  TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY(note_id, tag_id)
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE VIRTUAL TABLE notes_fts USING fts5(
    note_id UNINDEXED,
    title,
    body,
    tags,
    attachment_names,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE note_revisions (
    id             TEXT PRIMARY KEY,
    note_id        TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    revision_no    INTEGER NOT NULL CHECK(revision_no > 0),
    checksum       TEXT NOT NULL,
    snapshot_json  TEXT NOT NULL,
    byte_size      INTEGER NOT NULL DEFAULT 0 CHECK(byte_size >= 0),
    created_at     INTEGER NOT NULL,
    UNIQUE(note_id, revision_no)
);

CREATE INDEX idx_note_revisions_note
    ON note_revisions(note_id, revision_no DESC);

CREATE TABLE revision_attachment_refs (
    revision_id    TEXT NOT NULL REFERENCES note_revisions(id) ON DELETE CASCADE,
    attachment_id  TEXT NOT NULL,
    sha256         TEXT NOT NULL CHECK(
        length(sha256) = 64
        AND sha256 = lower(sha256)
        AND sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    byte_size      INTEGER NOT NULL CHECK(byte_size >= 0),
    PRIMARY KEY(revision_id, attachment_id)
);

CREATE INDEX idx_revision_attachment_refs_blob
    ON revision_attachment_refs(sha256);

CREATE INDEX idx_revision_attachment_refs_attachment
    ON revision_attachment_refs(attachment_id);

CREATE TABLE sync_queue (
    id             TEXT PRIMARY KEY,
    entity_type    TEXT NOT NULL CHECK(entity_type IN ('note', 'attachment', 'notebook')),
    entity_id      TEXT NOT NULL,
    operation      TEXT NOT NULL CHECK(operation IN ('upsert', 'delete')),
    payload_json   TEXT NOT NULL,
    checksum       TEXT NOT NULL,
    state          TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending', 'in_progress', 'failed', 'completed')),
    attempt_count  INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE INDEX idx_sync_queue_state
    ON sync_queue(state, created_at);

CREATE UNIQUE INDEX idx_sync_queue_pending_entity
    ON sync_queue(entity_type, entity_id)
    WHERE state = 'pending';

CREATE TABLE attachments (
    id               TEXT PRIMARY KEY,
    note_id          TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    file_name        TEXT NOT NULL,
    mime_type        TEXT,
    byte_size        INTEGER NOT NULL CHECK(byte_size >= 0),
    sha256           TEXT NOT NULL CHECK(
        length(sha256) = 64
        AND sha256 = lower(sha256)
        AND sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    image_pixel_width  INTEGER CHECK(image_pixel_width IS NULL OR image_pixel_width > 0),
    image_pixel_height INTEGER CHECK(image_pixel_height IS NULL OR image_pixel_height > 0),
    image_format       TEXT CHECK(image_format IS NULL OR length(image_format) <= 32),
    image_orientation  INTEGER CHECK(image_orientation IS NULL OR image_orientation BETWEEN 1 AND 8),
    image_has_alpha    INTEGER CHECK(image_has_alpha IS NULL OR image_has_alpha IN (0, 1)),
    image_animated     INTEGER CHECK(image_animated IS NULL OR image_animated IN (0, 1)),
    source_attachment_id TEXT REFERENCES attachments(id) ON DELETE CASCADE,
    image_edit_recipe   TEXT CHECK(image_edit_recipe IS NULL OR length(image_edit_recipe) <= 524288),
    embedded         INTEGER NOT NULL DEFAULT 0 CHECK(embedded IN (0, 1)),
    unreferenced_at  INTEGER,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    CHECK(source_attachment_id IS NULL OR source_attachment_id <> id),
    CHECK((source_attachment_id IS NULL) = (image_edit_recipe IS NULL)),
    CHECK(
        (embedded = 1 AND unreferenced_at IS NULL) OR
        (embedded = 0 AND unreferenced_at IS NOT NULL)
    )
);

CREATE INDEX idx_attachments_note
    ON attachments(note_id, created_at, LOWER(file_name), id);

CREATE INDEX idx_attachments_blob
    ON attachments(sha256);

CREATE INDEX idx_attachments_unreferenced
    ON attachments(unreferenced_at, id)
    WHERE embedded = 0;

CREATE INDEX idx_attachments_source
    ON attachments(source_attachment_id)
    WHERE source_attachment_id IS NOT NULL;

CREATE TABLE note_links (
    source_note_id  TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    target_note_id  TEXT NOT NULL,
    PRIMARY KEY(source_note_id, target_note_id)
);

CREATE INDEX idx_note_links_target
    ON note_links(target_note_id, source_note_id);

CREATE TABLE note_parent (
    child_note_id   TEXT PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
    parent_note_id  TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    CHECK(child_note_id <> parent_note_id)
);

CREATE INDEX idx_note_parent_parent
    ON note_parent(parent_note_id, child_note_id);

CREATE TABLE sources (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK(kind IN ('web', 'book', 'journal', 'pdf', 'report', 'other')),
    title       TEXT NOT NULL,
    reference   TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_sources_title
    ON sources(title COLLATE NOCASE, id);

CREATE TABLE note_sources (
    note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    source_id   TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY(note_id, source_id)
);

CREATE INDEX idx_note_sources_source
    ON note_sources(source_id, note_id);

CREATE TABLE citations (
    id          TEXT PRIMARY KEY,
    note_id     TEXT NOT NULL,
    source_id   TEXT NOT NULL,
    locator     TEXT NOT NULL CHECK(LENGTH(TRIM(locator)) BETWEEN 1 AND 1024),
    created_at  INTEGER NOT NULL,
    FOREIGN KEY(note_id, source_id) REFERENCES note_sources(note_id, source_id) ON DELETE CASCADE
);

CREATE INDEX idx_citations_note_source
    ON citations(note_id, source_id, created_at, id);

CREATE TABLE pdf_annotation_pages (
    attachment_id    TEXT NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
    page_index       INTEGER NOT NULL CHECK(page_index >= 0),
    annotations_json TEXT NOT NULL,
    updated_at       INTEGER NOT NULL,
    PRIMARY KEY(attachment_id, page_index)
);

CREATE TABLE attachment_ingest_jobs (
    id                     TEXT PRIMARY KEY,
    attachment_id          TEXT NOT NULL UNIQUE,
    note_id                TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    file_name              TEXT NOT NULL CHECK(length(file_name) BETWEEN 1 AND 1024),
    mime_type              TEXT CHECK(mime_type IS NULL OR length(mime_type) <= 255),
    byte_size              INTEGER NOT NULL CHECK(byte_size >= 0),
    staging_relative_path  TEXT NOT NULL UNIQUE,
    request_id             TEXT CHECK(request_id IS NULL OR length(request_id) BETWEEN 1 AND 128),
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL
);

CREATE INDEX idx_attachment_ingest_jobs_note
    ON attachment_ingest_jobs(note_id, created_at, id);
CREATE INDEX idx_attachment_ingest_jobs_request
    ON attachment_ingest_jobs(note_id, request_id, created_at, id)
    WHERE request_id IS NOT NULL;

CREATE TABLE attachment_operations (
    id                     TEXT PRIMARY KEY,
    operation              TEXT NOT NULL CHECK(operation IN ('add', 'remove')),
    attachment_id          TEXT NOT NULL,
    note_id                TEXT NOT NULL,
    attachment_json        TEXT NOT NULL,
    staging_relative_path  TEXT,
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_attachment_operations_attachment
    ON attachment_operations(attachment_id);


CREATE TABLE managed_plaintext_staging (
    id                    TEXT PRIMARY KEY,
    kind                  TEXT NOT NULL CHECK(kind IN ('attachment_incoming', 'portability_import', 'portability_blob')),
    note_id               TEXT,
    relative_path         TEXT NOT NULL UNIQUE,
    created_at            INTEGER NOT NULL,
    transfer_destination  TEXT,
    transfer_state        TEXT CHECK(transfer_state IS NULL OR transfer_state = 'prepared'),
    CHECK(
        (kind = 'attachment_incoming' AND note_id IS NOT NULL)
        OR (kind IN ('portability_import', 'portability_blob') AND note_id IS NULL)
    )
);

CREATE INDEX idx_managed_plaintext_staging_note
    ON managed_plaintext_staging(note_id, created_at, id)
    WHERE note_id IS NOT NULL;

CREATE TABLE vault_objects (
    note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    object_id   TEXT NOT NULL CHECK(
        length(object_id) = 32
        AND object_id = lower(object_id)
        AND object_id NOT GLOB '*[^0-9a-f]*'
    ),
    ciphertext  BLOB NOT NULL CHECK(length(ciphertext) > 0),
    PRIMARY KEY(note_id, object_id)
);

CREATE TABLE vault_note_roots (
    note_id             TEXT PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
    format_version      INTEGER NOT NULL CHECK(format_version > 0),
    key_envelope        BLOB NOT NULL CHECK(length(key_envelope) > 0),
    manifest_object_id  TEXT NOT NULL CHECK(
        length(manifest_object_id) = 32
        AND manifest_object_id = lower(manifest_object_id)
        AND manifest_object_id NOT GLOB '*[^0-9a-f]*'
    ),
    FOREIGN KEY(note_id, manifest_object_id)
        REFERENCES vault_objects(note_id, object_id)
);

CREATE TABLE vault_blob_refs (
    note_id      TEXT NOT NULL REFERENCES vault_note_roots(note_id) ON DELETE CASCADE,
    sha256       TEXT NOT NULL CHECK(length(sha256) = 64),
    byte_size    INTEGER NOT NULL CHECK(byte_size > 0),
    PRIMARY KEY(note_id, sha256)
);
CREATE INDEX idx_vault_blob_refs_sha256 ON vault_blob_refs(sha256);

INSERT INTO settings(key, value)
VALUES ('revision.max_total_bytes', '268435456');

PRAGMA user_version = 21;
"#;
