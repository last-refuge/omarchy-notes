#include "NoteStore.h"

#include <QDateTime>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QRegularExpression>
#include <QSet>

#include <algorithm>

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

namespace {

constexpr const char *kSummaryColumns =
    "n.id, n.folder_id, n.title, n.snippet, n.created_at, n.updated_at, n.deleted_at, "
    "n.pinned, n.has_attachments, n.has_checklist";

NoteSummary readSummary(const Statement &s)
{
    NoteSummary n;
    n.id = s.text(0);
    n.folderId = s.text(1);
    n.title = s.text(2);
    n.snippet = s.text(3);
    n.createdAt = s.int64(4);
    n.updatedAt = s.int64(5);
    n.deletedAt = s.isNull(6) ? 0 : s.int64(6);
    n.pinned = s.int64(7) != 0;
    n.hasAttachments = s.int64(8) != 0;
    n.hasChecklist = s.int64(9) != 0;
    return n;
}

// Turns what someone typed into an FTS5 query: every word must match, each
// as a prefix, with FTS syntax characters treated as plain text.
QString ftsQuery(const QString &text)
{
    QStringList terms;
    for (QString word : text.split(QRegularExpression(u"\\s+"_s), Qt::SkipEmptyParts)) {
        word.remove(u'"');
        const bool hasWordChar = std::any_of(word.cbegin(), word.cend(),
                                             [](QChar c) { return c.isLetterOrNumber(); });
        if (hasWordChar)
            terms << u'"' + word + u"\"*"_s;
    }
    return terms.join(u' ');
}

} // namespace

QList<NoteSummary> NoteStore::listNotes(const NoteQuery &query)
{
    QByteArray sql = "SELECT ";
    sql += kSummaryColumns;
    sql += " FROM notes n WHERE ";
    switch (query.scope) {
    case NoteQuery::Scope::All:
        sql += "n.deleted_at IS NULL";
        break;
    case NoteQuery::Scope::Folder:
        sql += query.folderId.isEmpty() ? "n.deleted_at IS NULL AND n.folder_id IS NULL"
                                        : "n.deleted_at IS NULL AND n.folder_id = ?1";
        break;
    case NoteQuery::Scope::Trash:
        sql += "n.deleted_at IS NOT NULL";
        break;
    }
    if (query.scope == NoteQuery::Scope::Trash) {
        sql += " ORDER BY n.deleted_at DESC";
    } else {
        sql += " ORDER BY n.pinned DESC, ";
        switch (query.sort) {
        case NoteSort::Edited: sql += "n.updated_at DESC"; break;
        case NoteSort::Created: sql += "n.created_at DESC"; break;
        case NoteSort::Title: sql += "n.title = '', n.title COLLATE NOCASE, n.updated_at DESC"; break;
        }
    }

    Statement s = m_db.prepare(sql.constData());
    if (query.scope == NoteQuery::Scope::Folder && !query.folderId.isEmpty())
        s.bind(1, query.folderId);
    QList<NoteSummary> out;
    while (s.next())
        out.append(readSummary(s));
    return out;
}

QList<NoteSummary> NoteStore::search(const QString &text, int limit)
{
    const QString match = ftsQuery(text);
    if (match.isEmpty())
        return {};
    QByteArray sql = "SELECT ";
    sql += QByteArray(kSummaryColumns).replace("n.snippet", "snippet(notes_fts, -1, char(2), char(3), '…', 12)");
    sql += " FROM notes_fts JOIN notes n ON n.rowid = notes_fts.rowid "
           "WHERE notes_fts MATCH ?1 AND n.deleted_at IS NULL "
           "ORDER BY bm25(notes_fts, 4.0, 1.0) LIMIT ?2";
    Statement s = m_db.prepare(sql.constData());
    s.bind(1, match).bind(2, qint64(limit));
    QList<NoteSummary> out;
    while (s.next())
        out.append(readSummary(s));
    return out;
}

std::optional<NoteRecord> NoteStore::loadNote(const QString &id, QString *error)
{
    Statement s = m_db.prepare(
        "SELECT folder_id, body, created_at, updated_at, revision, pinned, deleted_at "
        "FROM notes WHERE id = ?1");
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
    r.deletedAt = s.isNull(6) ? 0 : s.int64(6);
    return r;
}

std::optional<NoteRecord> NoteStore::createNote(const RichDocument &body, QString *error,
                                                const QString &folderId)
{
    if (!folderId.isEmpty() && !folderExists(folderId)) {
        setError(error, u"That folder no longer exists."_s);
        return std::nullopt;
    }
    NoteRecord r;
    r.id = newId();
    r.folderId = folderId;
    r.body = body;
    r.body.assignMissingIds();
    r.createdAt = r.updatedAt = now();
    r.revision = 1;

    const bool media = !r.body.referencedAttachments().isEmpty() || !r.body.referencedBlobs().isEmpty();
    Statement s = m_db.prepare(
        "INSERT INTO notes (id, folder_id, title, snippet, plain_text, body, body_schema, "
        "has_attachments, has_checklist, created_at, updated_at, revision) "
        "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, 1)");
    s.bind(1, r.id);
    if (folderId.isEmpty())
        s.bindNull(2);
    else
        s.bind(2, folderId);
    s.bind(3, r.body.title())
        .bind(4, r.body.snippet())
        .bind(5, r.body.plainText())
        .bind(6, QString::fromUtf8(r.body.toJsonBytes()))
        .bind(7, qint64(RichDocument::SchemaVersion))
        .bind(8, qint64(media))
        .bind(9, qint64(hasChecklist(r.body.blocks)))
        .bind(10, r.createdAt);
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

// ---------------------------------------------------------------- notes

bool NoteStore::moveNote(const QString &noteId, const QString &folderId, QString *error)
{
    if (!folderId.isEmpty() && !folderExists(folderId))
        return setError(error, u"That folder no longer exists."_s);
    Statement s = m_db.prepare("UPDATE notes SET folder_id = ?2 WHERE id = ?1 AND deleted_at IS NULL");
    s.bind(1, noteId);
    if (folderId.isEmpty())
        s.bindNull(2);
    else
        s.bind(2, folderId);
    if (!s.exec())
        return setError(error, m_db.lastError());
    return m_db.changes() == 1 || setError(error, u"The note no longer exists."_s);
}

std::optional<NoteRecord> NoteStore::duplicateNote(const QString &noteId, QString *error)
{
    auto source = loadNote(noteId, error);
    if (!source)
        return std::nullopt;

    Transaction tx(m_db);
    if (!tx.isActive()) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    // Attachments belong to one note, so the copy gets its own records
    // pointing at the same stored bytes.
    QHash<QString, QString> renamed;
    for (const QString &id : source->body.referencedAttachments()) {
        if (attachment(id))
            renamed.insert(id, newId());
    }

    RichDocument body = source->body;
    std::function<void(QList<Block> &)> rewrite = [&](QList<Block> &blocks) {
        for (Block &b : blocks) {
            for (Span &span : b.spans) {
                if (span.kind == Span::Kind::Attachment && renamed.contains(span.ref))
                    span.ref = renamed.value(span.ref);
            }
            for (TableCell &c : b.cells)
                rewrite(c.blocks);
        }
    };
    rewrite(body.blocks);

    auto copy = createNote(body, error, source->folderId);
    if (!copy)
        return std::nullopt;
    for (auto it = renamed.cbegin(); it != renamed.cend(); ++it) {
        Statement s = m_db.prepare(
            "INSERT INTO attachments (id, note_id, blob_hash, file_name, mime_type, size, created_at) "
            "SELECT ?1, ?2, blob_hash, file_name, mime_type, size, ?3 FROM attachments WHERE id = ?4");
        s.bind(1, it.value()).bind(2, copy->id).bind(3, now()).bind(4, it.key());
        if (!s.exec()) {
            setError(error, m_db.lastError());
            return std::nullopt;
        }
    }
    Statement refs = m_db.prepare(
        "INSERT OR IGNORE INTO note_blob_refs (note_id, blob_hash) "
        "SELECT ?2, blob_hash FROM note_blob_refs WHERE note_id = ?1");
    refs.bind(1, noteId).bind(2, copy->id);
    if (!refs.exec() || !tx.commit()) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    return copy;
}

bool NoteStore::trashNote(const QString &noteId, QString *error)
{
    Statement s = m_db.prepare("UPDATE notes SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL");
    s.bind(1, noteId).bind(2, now());
    if (!s.exec())
        return setError(error, m_db.lastError());
    return m_db.changes() == 1 || setError(error, u"The note is already in Recently Deleted."_s);
}

bool NoteStore::recoverNote(const QString &noteId, QString *error)
{
    Statement s = m_db.prepare("UPDATE notes SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL");
    s.bind(1, noteId);
    if (!s.exec())
        return setError(error, m_db.lastError());
    return m_db.changes() == 1 || setError(error, u"The note isn't in Recently Deleted."_s);
}

bool NoteStore::deleteNotePermanently(const QString &noteId, QString *error)
{
    Statement s = m_db.prepare("DELETE FROM notes WHERE id = ?1 AND deleted_at IS NOT NULL");
    s.bind(1, noteId);
    if (!s.exec())
        return setError(error, m_db.lastError());
    return m_db.changes() == 1 || setError(error, u"Only notes in Recently Deleted can be deleted permanently."_s);
}

int NoteStore::emptyTrash(QString *error)
{
    Statement s = m_db.prepare("DELETE FROM notes WHERE deleted_at IS NOT NULL");
    if (!s.exec()) {
        setError(error, m_db.lastError());
        return -1;
    }
    return m_db.changes();
}

int NoteStore::purgeExpiredTrash(qint64 at)
{
    Statement s = m_db.prepare("DELETE FROM notes WHERE deleted_at IS NOT NULL AND deleted_at < ?1");
    s.bind(1, (at ? at : now()) - TrashRetentionMs);
    return s.exec() ? m_db.changes() : -1;
}

int NoteStore::trashCount()
{
    Statement s = m_db.prepare("SELECT COUNT(*) FROM notes WHERE deleted_at IS NOT NULL");
    return s.next() ? int(s.int64(0)) : 0;
}

int NoteStore::noteCount()
{
    Statement s = m_db.prepare("SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL");
    return s.next() ? int(s.int64(0)) : 0;
}

// ---------------------------------------------------------------- folders

bool NoteStore::folderExists(const QString &id)
{
    Statement s = m_db.prepare("SELECT 1 FROM folders WHERE id = ?1");
    s.bind(1, id);
    return s.next();
}

QStringList NoteStore::folderAndDescendants(const QString &id)
{
    Statement s = m_db.prepare(
        "WITH RECURSIVE sub(id) AS ("
        "  SELECT id FROM folders WHERE id = ?1"
        "  UNION ALL SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id"
        ") SELECT id FROM sub");
    s.bind(1, id);
    QStringList ids;
    while (s.next())
        ids << s.text(0);
    return ids;
}

QList<FolderInfo> NoteStore::listFolders()
{
    Statement s = m_db.prepare(
        "SELECT f.id, f.parent_id, f.name, "
        "  (SELECT COUNT(*) FROM notes n WHERE n.folder_id = f.id AND n.deleted_at IS NULL) "
        "FROM folders f ORDER BY f.name COLLATE NOCASE, f.created_at");
    QList<FolderInfo> out;
    while (s.next())
        out.append({s.text(0), s.text(1), s.text(2), int(s.int64(3))});
    return out;
}

namespace {

// Validates a folder name and checks it is unique among its siblings.
bool checkFolderName(Database &db, const QString &name, const QString &parentId, const QString &selfId,
                     QString *error)
{
    if (name.isEmpty())
        return setError(error, u"Give the folder a name."_s);
    if (name.size() > 200)
        return setError(error, u"That name is too long."_s);
    Statement s = db.prepare(
        "SELECT 1 FROM folders WHERE name = ?1 COLLATE NOCASE AND id != ?3 AND "
        "((?2 IS NULL AND parent_id IS NULL) OR parent_id = ?2)");
    s.bind(1, name);
    if (parentId.isEmpty())
        s.bindNull(2);
    else
        s.bind(2, parentId);
    s.bind(3, selfId);
    if (s.next())
        return setError(error, u"There's already a folder called “%1” here."_s.arg(name));
    return true;
}

} // namespace

std::optional<FolderInfo> NoteStore::createFolder(const QString &rawName, const QString &parentId,
                                                  QString *error)
{
    const QString name = rawName.simplified();
    if (!parentId.isEmpty() && !folderExists(parentId)) {
        setError(error, u"That folder no longer exists."_s);
        return std::nullopt;
    }
    if (!checkFolderName(m_db, name, parentId, {}, error))
        return std::nullopt;
    FolderInfo folder{newId(), parentId, name, 0};
    Statement s = m_db.prepare(
        "INSERT INTO folders (id, parent_id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)");
    s.bind(1, folder.id);
    if (parentId.isEmpty())
        s.bindNull(2);
    else
        s.bind(2, parentId);
    s.bind(3, name).bind(4, now());
    if (!s.exec()) {
        setError(error, m_db.lastError());
        return std::nullopt;
    }
    return folder;
}

bool NoteStore::renameFolder(const QString &id, const QString &rawName, QString *error)
{
    const QString name = rawName.simplified();
    Statement parent = m_db.prepare("SELECT parent_id FROM folders WHERE id = ?1");
    parent.bind(1, id);
    if (!parent.next())
        return setError(error, u"That folder no longer exists."_s);
    if (!checkFolderName(m_db, name, parent.text(0), id, error))
        return false;
    Statement s = m_db.prepare("UPDATE folders SET name = ?2, updated_at = ?3 WHERE id = ?1");
    s.bind(1, id).bind(2, name).bind(3, now());
    return s.exec() || setError(error, m_db.lastError());
}

bool NoteStore::deleteFolder(const QString &id, QString *error)
{
    const QStringList ids = folderAndDescendants(id);
    if (ids.isEmpty())
        return setError(error, u"That folder no longer exists."_s);
    Transaction tx(m_db);
    if (!tx.isActive())
        return setError(error, m_db.lastError());
    const qint64 timestamp = now();
    for (const QString &folder : ids) {
        // Deleted notes can't return to a folder that's gone; they recover
        // into Notes instead.
        Statement notes = m_db.prepare(
            "UPDATE notes SET deleted_at = COALESCE(deleted_at, ?2), folder_id = NULL WHERE folder_id = ?1");
        notes.bind(1, folder).bind(2, timestamp);
        if (!notes.exec())
            return setError(error, m_db.lastError());
    }
    // Children first, so no folder is removed while another points at it.
    for (auto it = ids.crbegin(); it != ids.crend(); ++it) {
        Statement del = m_db.prepare("DELETE FROM folders WHERE id = ?1");
        del.bind(1, *it);
        if (!del.exec())
            return setError(error, m_db.lastError());
    }
    return tx.commit() || setError(error, m_db.lastError());
}

// ---------------------------------------------------------------- garbage

GarbageReport NoteStore::collectGarbage(qint64 at)
{
    GarbageReport report;
    const qint64 cutoff = (at ? at : now()) - GarbageGraceMs;

    // Everything any note or saved revision still refers to.
    QSet<QString> blobs;
    QSet<QString> attachments;
    Statement bodies = m_db.prepare("SELECT body FROM notes UNION ALL SELECT body FROM note_revisions");
    while (bodies.next()) {
        const auto doc = RichDocument::fromJsonBytes(bodies.text(0).toUtf8());
        if (!doc)
            continue;
        for (const QString &hash : doc->referencedBlobs())
            blobs.insert(hash);
        for (const QString &id : doc->referencedAttachments())
            attachments.insert(id);
    }

    QStringList removedHashes;
    {
        Transaction tx(m_db);
        if (!tx.isActive())
            return report;
        Statement rows = m_db.prepare("SELECT id FROM attachments WHERE created_at < ?1");
        rows.bind(1, cutoff);
        QStringList staleAttachments;
        while (rows.next()) {
            if (!attachments.contains(rows.text(0)))
                staleAttachments << rows.text(0);
        }
        for (const QString &id : staleAttachments) {
            Statement del = m_db.prepare("DELETE FROM attachments WHERE id = ?1");
            del.bind(1, id);
            if (!del.exec())
                return report;
        }

        Statement candidates = m_db.prepare(
            "SELECT hash, size FROM blobs WHERE created_at < ?1 "
            "AND hash NOT IN (SELECT blob_hash FROM attachments) "
            "AND hash NOT IN (SELECT blob_hash FROM note_blob_refs)");
        candidates.bind(1, cutoff);
        QList<QPair<QString, qint64>> stale;
        while (candidates.next()) {
            if (!blobs.contains(candidates.text(0)))
                stale.append({candidates.text(0), candidates.int64(1)});
        }
        for (const auto &[hash, size] : stale) {
            Statement del = m_db.prepare("DELETE FROM blobs WHERE hash = ?1");
            del.bind(1, hash);
            if (!del.exec())
                return report;
            removedHashes << hash;
            report.bytesFreed += size;
        }
        if (!tx.commit())
            return GarbageReport{};
        report.attachmentsRemoved = int(staleAttachments.size());
    }
    // Files go only after the database no longer refers to them.
    for (const QString &hash : removedHashes)
        QFile::remove(m_blobs.path(hash));
    report.blobsRemoved = int(removedHashes.size());

    // Files left by an install that was interrupted before it was recorded.
    QSet<QString> known;
    Statement all = m_db.prepare("SELECT hash FROM blobs");
    while (all.next())
        known.insert(all.text(0));
    const QDateTime cutoffTime = QDateTime::fromMSecsSinceEpoch(cutoff);
    QDirIterator it(m_paths.blobs(), QDir::Files, QDirIterator::Subdirectories);
    while (it.hasNext()) {
        const QFileInfo file(it.next());
        if (!known.contains(file.fileName()) && file.lastModified() < cutoffTime) {
            report.bytesFreed += file.size();
            QFile::remove(file.filePath());
            ++report.blobsRemoved;
        }
    }
    return report;
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
