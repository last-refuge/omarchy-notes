#include "NoteStore.h"

#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QMimeDatabase>

#include <sqlite3.h>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

constexpr const char *kSchemaV1 = R"sql(
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
)sql";

qint64 now()
{
    return QDateTime::currentMSecsSinceEpoch();
}

bool setError(QString *error, const QString &message)
{
    if (error)
        *error = message;
    return false;
}

bool hasChecklist(const QList<Block> &blocks)
{
    for (const Block &b : blocks) {
        if (b.type == Block::Type::ListItem && b.list == ListKind::Check)
            return true;
        for (const TableCell &c : b.cells) {
            if (hasChecklist(c.blocks))
                return true;
        }
    }
    return false;
}

} // namespace

NoteStore::NoteStore(LibraryPaths paths) : m_paths(paths), m_blobs(std::move(paths)) {}

bool NoteStore::open(QString *error)
{
    if (!QDir().mkpath(m_paths.root))
        return setError(error, u"Could not create the library folder %1."_s.arg(m_paths.root));
    if (!m_blobs.prepare(error))
        return false;
    if (!m_db.open(m_paths.database(), error))
        return false;

    // WAL keeps readers unblocked; synchronous=FULL makes every acknowledged
    // commit durable across power loss, which "Saved" promises.
    {
        Statement wal = m_db.prepare("PRAGMA journal_mode=WAL");
        if (!wal.next() || wal.text(0).compare(u"wal"_s, Qt::CaseInsensitive) != 0)
            return setError(error, u"The library database does not support write-ahead logging."_s);
    }
    if (!m_db.exec("PRAGMA synchronous=FULL") || !m_db.exec("PRAGMA foreign_keys=ON"))
        return setError(error, m_db.lastError());

    return migrate(error);
}

void NoteStore::close()
{
    m_db.close();
}

bool NoteStore::migrate(QString *error)
{
    const int version = m_db.userVersion();
    if (version > SchemaVersion) {
        return setError(error, u"This library was created by a newer version of the app "
                               "(schema %1). Update the app to open it."_s.arg(version));
    }
    if (version == SchemaVersion)
        return true;

    Transaction tx(m_db);
    if (!tx.isActive())
        return setError(error, m_db.lastError());
    if (version < 1) {
        if (!m_db.exec(kSchemaV1))
            return setError(error, u"Could not create the library: %1"_s.arg(m_db.lastError()));
    }
    // Future migrations go here, each guarded by `version < N`.
    const QByteArray setVersion = "PRAGMA user_version=" + QByteArray::number(SchemaVersion);
    if (!m_db.exec(setVersion.constData()) || !tx.commit())
        return setError(error, u"Could not upgrade the library: %1"_s.arg(m_db.lastError()));
    return true;
}

QList<NoteSummary> NoteStore::listNotes()
{
    QList<NoteSummary> out;
    Statement s = m_db.prepare(
        "SELECT id, title, snippet, updated_at, pinned, has_attachments, has_checklist FROM notes "
        "WHERE deleted_at IS NULL ORDER BY pinned DESC, updated_at DESC");
    while (s.next()) {
        NoteSummary n;
        n.id = s.text(0);
        n.title = s.text(1);
        n.snippet = s.text(2);
        n.updatedAt = s.int64(3);
        n.pinned = s.int64(4) != 0;
        n.hasAttachments = s.int64(5) != 0;
        n.hasChecklist = s.int64(6) != 0;
        out.append(n);
    }
    return out;
}

std::optional<NoteRecord> NoteStore::loadNote(const QString &id, QString *error)
{
    Statement s = m_db.prepare(
        "SELECT folder_id, body, created_at, updated_at, revision, pinned FROM notes WHERE id = ?1");
    s.bind(1, id);
    if (!s.next()) {
        setError(error, s.failed() ? m_db.lastError() : u"The note no longer exists."_s);
        return std::nullopt;
    }
    QString parseError;
    auto body = RichDocument::fromJsonBytes(s.text(1).toUtf8(), &parseError);
    if (!body) {
        setError(error, u"The note could not be read: %1"_s.arg(parseError));
        return std::nullopt;
    }
    NoteRecord r;
    r.id = id;
    r.folderId = s.text(0);
    r.body = std::move(*body);
    r.createdAt = s.int64(2);
    r.updatedAt = s.int64(3);
    r.revision = s.int64(4);
    r.pinned = s.int64(5) != 0;
    return r;
}

std::optional<NoteRecord> NoteStore::createNote(const RichDocument &body, QString *error)
{
    NoteRecord r;
    r.id = newId();
    r.body = body;
    r.body.assignMissingIds();
    r.createdAt = r.updatedAt = now();
    r.revision = 1;

    Statement s = m_db.prepare(
        "INSERT INTO notes (id, title, snippet, plain_text, body, body_schema, has_checklist, "
        "created_at, updated_at, revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1)");
    s.bind(1, r.id)
        .bind(2, r.body.title())
        .bind(3, r.body.snippet())
        .bind(4, r.body.plainText())
        .bind(5, QString::fromUtf8(r.body.toJsonBytes()))
        .bind(6, qint64(RichDocument::SchemaVersion))
        .bind(7, qint64(hasChecklist(r.body.blocks)))
        .bind(8, r.createdAt);
    if (!s.exec()) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    return r;
}

SaveResult NoteStore::saveNote(const SaveRequest &request)
{
    SaveResult result;
    Transaction tx(m_db);
    if (!tx.isActive()) {
        result.error = m_db.lastError();
        return result;
    }

    Statement current = m_db.prepare("SELECT revision FROM notes WHERE id = ?1");
    current.bind(1, request.noteId);
    if (!current.next()) {
        result.error = current.failed() ? m_db.lastError() : u"The note no longer exists."_s;
        return result;
    }
    const qint64 storedRevision = current.int64(0);
    if (storedRevision != request.baseRevision) {
        result.status = SaveResult::Status::Conflict;
        result.revision = storedRevision;
        result.error = u"The note was changed elsewhere."_s;
        return result;
    }

    const RichDocument &body = request.body;
    const QString json = QString::fromUtf8(body.toJsonBytes());
    const qint64 timestamp = now();
    const qint64 revision = storedRevision + 1;
    const QStringList attachments = body.referencedAttachments();

    Statement update = m_db.prepare(
        "UPDATE notes SET title = ?2, snippet = ?3, plain_text = ?4, body = ?5, body_schema = ?6, "
        "has_attachments = ?7, has_checklist = ?8, updated_at = ?9, revision = ?10 WHERE id = ?1");
    update.bind(1, request.noteId)
        .bind(2, body.title())
        .bind(3, body.snippet())
        .bind(4, body.plainText())
        .bind(5, json)
        .bind(6, qint64(RichDocument::SchemaVersion))
        .bind(7, qint64(!attachments.isEmpty() || !body.referencedBlobs().isEmpty()))
        .bind(8, qint64(hasChecklist(body.blocks)))
        .bind(9, timestamp)
        .bind(10, revision);
    if (!update.exec()) {
        result.error = m_db.lastError();
        return result;
    }

    // Image references, for integrity checks and future garbage collection.
    // References to blobs this library doesn't have (e.g. pasted from
    // elsewhere) are skipped instead of failing the save.
    Statement clearRefs = m_db.prepare("DELETE FROM note_blob_refs WHERE note_id = ?1");
    clearRefs.bind(1, request.noteId);
    if (!clearRefs.exec()) {
        result.error = m_db.lastError();
        return result;
    }
    for (const QString &hash : body.referencedBlobs()) {
        Statement ref = m_db.prepare(
            "INSERT OR IGNORE INTO note_blob_refs (note_id, blob_hash) "
            "SELECT ?1, hash FROM blobs WHERE hash = ?2");
        ref.bind(1, request.noteId).bind(2, hash);
        if (!ref.exec()) {
            result.error = m_db.lastError();
            return result;
        }
    }

    Statement lastSnapshot = m_db.prepare(
        "SELECT MAX(created_at) FROM note_revisions WHERE note_id = ?1");
    lastSnapshot.bind(1, request.noteId);
    const bool needSnapshot = !lastSnapshot.next() || lastSnapshot.isNull(0)
        || timestamp - lastSnapshot.int64(0) >= RevisionIntervalMs;
    if (needSnapshot) {
        Statement snap = m_db.prepare(
            "INSERT INTO note_revisions (note_id, revision, body, body_schema, created_at) "
            "VALUES (?1, ?2, ?3, ?4, ?5)");
        snap.bind(1, request.noteId)
            .bind(2, revision)
            .bind(3, json)
            .bind(4, qint64(RichDocument::SchemaVersion))
            .bind(5, timestamp);
        if (!snap.exec()) {
            result.error = m_db.lastError();
            return result;
        }
    }

    if (!tx.commit()) {
        result.error = m_db.lastError();
        return result;
    }
    result.status = SaveResult::Status::Saved;
    result.revision = revision;
    result.updatedAt = timestamp;
    return result;
}

bool NoteStore::setPinned(const QString &id, bool pinned)
{
    Statement s = m_db.prepare("UPDATE notes SET pinned = ?2 WHERE id = ?1");
    s.bind(1, id).bind(2, qint64(pinned));
    return s.exec();
}

int NoteStore::revisionCount(const QString &noteId)
{
    Statement s = m_db.prepare("SELECT COUNT(*) FROM note_revisions WHERE note_id = ?1");
    s.bind(1, noteId);
    return s.next() ? int(s.int64(0)) : 0;
}

bool NoteStore::registerBlob(const BlobStore::Installed &blob)
{
    Statement s = m_db.prepare(
        "INSERT OR IGNORE INTO blobs (hash, size, created_at) VALUES (?1, ?2, ?3)");
    s.bind(1, blob.hash).bind(2, blob.size).bind(3, now());
    return s.exec();
}

std::optional<AttachmentInfo> NoteStore::addAttachment(const QString &noteId, const QString &filePath,
                                                       QString *error)
{
    const auto installed = m_blobs.installFile(filePath, error);
    if (!installed)
        return std::nullopt;

    AttachmentInfo info;
    info.id = newId();
    info.noteId = noteId;
    info.blobHash = installed->hash;
    info.fileName = QFileInfo(filePath).fileName();
    info.mimeType = QMimeDatabase().mimeTypeForFile(filePath).name();
    info.size = installed->size;

    Transaction tx(m_db);
    Statement s = m_db.prepare(
        "INSERT INTO attachments (id, note_id, blob_hash, file_name, mime_type, size, created_at) "
        "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)");
    s.bind(1, info.id)
        .bind(2, info.noteId)
        .bind(3, info.blobHash)
        .bind(4, info.fileName)
        .bind(5, info.mimeType)
        .bind(6, info.size)
        .bind(7, now());
    if (!tx.isActive() || !registerBlob(*installed) || !s.exec() || !tx.commit()) {
        setError(error, u"Could not record the attachment: %1"_s.arg(m_db.lastError()));
        return std::nullopt;
    }
    return info;
}

std::optional<AttachmentInfo> NoteStore::attachment(const QString &id)
{
    Statement s = m_db.prepare(
        "SELECT note_id, blob_hash, file_name, mime_type, size FROM attachments WHERE id = ?1");
    s.bind(1, id);
    if (!s.next())
        return std::nullopt;
    AttachmentInfo info;
    info.id = id;
    info.noteId = s.text(0);
    info.blobHash = s.text(1);
    info.fileName = s.text(2);
    info.mimeType = s.text(3);
    info.size = s.int64(4);
    return info;
}

std::optional<QString> NoteStore::addImage(const QByteArray &bytes, QString *error)
{
    const auto installed = m_blobs.installBytes(bytes, error);
    if (!installed)
        return std::nullopt;
    if (!registerBlob(*installed)) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    return installed->hash;
}

std::optional<QString> NoteStore::addImageFile(const QString &filePath, QString *error)
{
    const auto installed = m_blobs.installFile(filePath, error);
    if (!installed)
        return std::nullopt;
    if (!registerBlob(*installed)) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    return installed->hash;
}

QStringList NoteStore::verify()
{
    QStringList problems;
    Statement integrity = m_db.prepare("PRAGMA integrity_check");
    while (integrity.next()) {
        const QString row = integrity.text(0);
        if (row != u"ok")
            problems.append(u"integrity: "_s + row);
    }
    Statement fk = m_db.prepare("PRAGMA foreign_key_check");
    while (fk.next())
        problems.append(u"foreign key: %1 row %2"_s.arg(fk.text(0)).arg(fk.int64(1)));
    Statement blobs = m_db.prepare("SELECT hash FROM blobs");
    while (blobs.next()) {
        if (!m_blobs.contains(blobs.text(0)))
            problems.append(u"missing blob "_s + blobs.text(0));
    }
    Statement notes = m_db.prepare("SELECT id, body FROM notes");
    while (notes.next()) {
        QString err;
        if (!RichDocument::fromJsonBytes(notes.text(1).toUtf8(), &err))
            problems.append(u"note %1: %2"_s.arg(notes.text(0), err));
    }
    return problems;
}

bool NoteStore::backupDatabase(const QString &destination, QString *error)
{
    Database target;
    if (!target.open(destination, error))
        return false;
    sqlite3_backup *backup = sqlite3_backup_init(target.handle(), "main", m_db.handle(), "main");
    if (!backup)
        return setError(error, target.lastError());
    const int rc = sqlite3_backup_step(backup, -1);
    sqlite3_backup_finish(backup);
    if (rc != SQLITE_DONE)
        return setError(error, target.lastError());
    return true;
}

} // namespace onotes
