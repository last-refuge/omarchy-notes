#pragma once

#include "BlobStore.h"
#include "Database.h"
#include "LibraryPaths.h"
#include "RichDocument.h"

#include <QList>
#include <QMetaType>
#include <QString>

#include <optional>

namespace onotes {

struct NoteSummary {
    QString id;
    QString title;
    QString snippet;
    qint64 updatedAt = 0; // ms since epoch
    bool pinned = false;
    bool hasAttachments = false;
    bool hasChecklist = false;
};

struct NoteRecord {
    QString id;
    QString folderId;
    RichDocument body;
    qint64 createdAt = 0;
    qint64 updatedAt = 0;
    qint64 revision = 0;
    bool pinned = false;
};

struct AttachmentInfo {
    QString id;
    QString noteId;
    QString blobHash;
    QString fileName;
    QString mimeType;
    qint64 size = 0;
};

struct SaveRequest {
    QString noteId;
    qint64 baseRevision = 0; // must match the stored revision
    RichDocument body;
};

struct SaveResult {
    enum class Status { Saved, Conflict, Failed };
    Status status = Status::Failed;
    qint64 revision = 0;
    qint64 updatedAt = 0;
    QString error;
};

// Owns the SQLite library and blob directory. Not thread-safe: one instance
// is owned by the persistence thread (see LibraryService).
class NoteStore
{
public:
    static constexpr int SchemaVersion = 1;
    // Keep at most one history snapshot per note per interval.
    static constexpr qint64 RevisionIntervalMs = 10 * 60 * 1000;

    explicit NoteStore(LibraryPaths paths);

    bool open(QString *error);
    void close();

    QList<NoteSummary> listNotes();
    std::optional<NoteRecord> loadNote(const QString &id, QString *error = nullptr);
    std::optional<NoteRecord> createNote(const RichDocument &body, QString *error = nullptr);
    SaveResult saveNote(const SaveRequest &request);
    bool setPinned(const QString &id, bool pinned);
    int revisionCount(const QString &noteId);

    std::optional<AttachmentInfo> addAttachment(const QString &noteId, const QString &filePath,
                                                QString *error = nullptr);
    std::optional<AttachmentInfo> attachment(const QString &id);
    // Stores image bytes and returns the blob hash.
    std::optional<QString> addImage(const QByteArray &bytes, QString *error = nullptr);
    std::optional<QString> addImageFile(const QString &filePath, QString *error = nullptr);

    QString blobPath(const QString &hash) const { return m_blobs.path(hash); }
    const LibraryPaths &paths() const { return m_paths; }

    // PRAGMA integrity_check plus attachment reference checks.
    QStringList verify();
    // Consistent online snapshot of the database (attachments are copied separately).
    bool backupDatabase(const QString &destination, QString *error);

private:
    bool migrate(QString *error);
    bool registerBlob(const BlobStore::Installed &blob);

    LibraryPaths m_paths;
    BlobStore m_blobs;
    Database m_db;
};

} // namespace onotes

Q_DECLARE_METATYPE(onotes::SaveResult)
Q_DECLARE_METATYPE(onotes::NoteSummary)
