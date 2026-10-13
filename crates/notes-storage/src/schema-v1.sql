-- Schema copied from this repository’s MIT-licensed Qt NoteStore.cpp.

CREATE TABLE folders (
    id          TEXT PRIMARY KEY NOT NULL,
    parent_id   TEXT REFERENCES folders(id) ON DELETE RESTRICT,
    name        TEXT NOT NULL,
    sort_key    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE notes (
    id           TEXT PRIMARY KEY NOT NULL,
    folder_id    TEXT REFERENCES folders(id) ON DELETE RESTRICT,
    title        TEXT NOT NULL DEFAULT '',
    snippet      TEXT NOT NULL DEFAULT '',
    plain_text   TEXT NOT NULL DEFAULT '',
    body         TEXT NOT NULL,
    body_schema  INTEGER NOT NULL,
    has_attachments INTEGER NOT NULL DEFAULT 0,
    has_checklist   INTEGER NOT NULL DEFAULT 0,
    pinned       INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    revision     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX notes_by_folder ON notes(folder_id, deleted_at, pinned DESC, updated_at DESC);

CREATE TABLE note_revisions (
    note_id      TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    revision     INTEGER NOT NULL,
    body         TEXT NOT NULL,
    body_schema  INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (note_id, revision)
) WITHOUT ROWID;

CREATE TABLE blobs (
    hash        TEXT PRIMARY KEY NOT NULL,
    size        INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE attachments (
    id          TEXT PRIMARY KEY NOT NULL,
    note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    blob_hash   TEXT NOT NULL REFERENCES blobs(hash),
    file_name   TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    size        INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX attachments_by_note ON attachments(note_id);

CREATE TABLE note_blob_refs (
    note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    blob_hash   TEXT NOT NULL REFERENCES blobs(hash),
    PRIMARY KEY (note_id, blob_hash)
) WITHOUT ROWID;
CREATE INDEX note_blob_refs_by_blob ON note_blob_refs(blob_hash);

CREATE VIRTUAL TABLE notes_fts USING fts5(
    title, plain_text,
    content='notes', content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, plain_text) VALUES (new.rowid, new.title, new.plain_text);
END;
CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, plain_text) VALUES ('delete', old.rowid, old.title, old.plain_text);
END;
CREATE TRIGGER notes_fts_update AFTER UPDATE OF title, plain_text ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, plain_text) VALUES ('delete', old.rowid, old.title, old.plain_text);
    INSERT INTO notes_fts(rowid, title, plain_text) VALUES (new.rowid, new.title, new.plain_text);
END;
