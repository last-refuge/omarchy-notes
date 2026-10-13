-- Schema copied from this repository’s MIT-licensed Qt NoteStore.cpp.

ALTER TABLE notes ADD COLUMN thumbnail TEXT;

CREATE TABLE note_tags (
    note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    PRIMARY KEY (note_id, tag)
) WITHOUT ROWID;
CREATE INDEX note_tags_by_tag ON note_tags(tag);

CREATE TABLE note_attachment_refs (
    note_id       TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    attachment_id TEXT NOT NULL,
    PRIMARY KEY (note_id, attachment_id)
) WITHOUT ROWID;
CREATE INDEX note_attachment_refs_by_attachment ON note_attachment_refs(attachment_id);

CREATE TABLE smart_folders (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    criteria    TEXT NOT NULL,
    sort_key    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
