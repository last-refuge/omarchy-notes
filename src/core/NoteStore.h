#pragma once

#include "BlobStore.h"
#include "Database.h"
#include "LibraryPaths.h"
#include "RichDocument.h"

#include <QJsonObject>
#include <QList>
#include <QMetaType>
#include <QString>

#include <optional>

namespace onotes {

struct NoteSummary {
    QString id;
    QString folderId; // empty: the top-level "Notes" folder
    QString title;
    QString snippet;  // for search results: the best match, marked with \x02..\x03
    QString thumbnail; // blob hash of the first image, if any
    qint64 createdAt = 0; // ms since epoch
    qint64 updatedAt = 0;
    qint64 deletedAt = 0; // non-zero while in Recently Deleted
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
    qint64 deletedAt = 0;
    qint64 revision = 0;
    bool pinned = false;
};

struct FolderInfo {
    QString id;
    QString parentId; // empty: top level
    QString name;
    int noteCount = 0; // notes directly inside, excluding deleted ones
};

enum class NoteSort { Edited, Created, Title };

struct TagInfo {
    QString name; // lowercased, without '#'
    int noteCount = 0;
};

// What a Smart Folder matches. Every set criterion must hold.
struct SmartCriteria {
    QStringList tags;
    bool matchAllTags = false; // otherwise any of the tags
    bool hasChecklist = false;
    bool hasAttachments = false;
    bool pinnedOnly = false;
    int editedWithinDays = 0;  // 0: any time
    int createdWithinDays = 0;
    QString folderId;          // empty: any folder

    bool isEmpty() const;
    QJsonObject toJson() const;
    static SmartCriteria fromJson(const QJsonObject &json);
    bool operator==(const SmartCriteria &) const = default;
};

struct SmartFolder {
    QString id;
    QString name;
    SmartCriteria criteria;
    int noteCount = 0;
};

// Which notes a list shows.
struct NoteQuery {
    enum class Scope {
        All,      // every note not in Recently Deleted
        Folder,   // one folder; an empty folderId is the top-level "Notes"
        Trash,    // Recently Deleted
        Tag,      // notes with `tag`
        Smart,    // notes matching `criteria`
    };
    Scope scope = Scope::All;
    QString folderId;
    NoteSort sort = NoteSort::Edited;
    QString tag;
    SmartCriteria criteria;

    static NoteQuery all(NoteSort sort = NoteSort::Edited) { return {Scope::All, {}, sort, {}, {}}; }
    static NoteQuery folder(const QString &id, NoteSort sort = NoteSort::Edited) { return {Scope::Folder, id, sort, {}, {}}; }
    static NoteQuery trash() { return {Scope::Trash, {}, NoteSort::Edited, {}, {}}; }
    static NoteQuery tagged(const QString &tag, NoteSort sort = NoteSort::Edited) { return {Scope::Tag, {}, sort, tag, {}}; }
    static NoteQuery smart(const SmartCriteria &criteria, NoteSort sort = NoteSort::Edited) { return {Scope::Smart, {}, sort, {}, criteria}; }
};

// An image or file inside a note, for the attachment browser.
struct AttachmentItem {
    bool isImage = false;
    QString blobHash;
    QString attachmentId; // files only
    QString fileName;     // files only
    QString mimeType;     // files only
    qint64 size = 0;
    QString noteId;
    QString noteTitle;
    qint64 noteUpdatedAt = 0;
};

struct GarbageReport {
    int blobsRemoved = 0;
    int attachmentsRemoved = 0;
    qint64 bytesFreed = 0;
};

// Describes a backup folder (see NoteStore::backupTo).
struct BackupManifest {
    static constexpr int FormatVersion = 1;
    QString path;
    QString appVersion;
    int schemaVersion = 0;
    qint64 createdAt = 0;
    int notes = 0;
    int blobs = 0;
    qint64 bytes = 0;
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
    static constexpr int SchemaVersion = 2;
    // Keep at most one history snapshot per note per interval.
    static constexpr qint64 RevisionIntervalMs = 10 * 60 * 1000;

    explicit NoteStore(LibraryPaths paths);

    bool open(QString *error);
    void close();

    // Deleted notes stay in Recently Deleted this long before being purged.
    static constexpr qint64 TrashRetentionMs = qint64(30) * 24 * 60 * 60 * 1000;
    // Unreferenced attachment data younger than this is kept: an image may
    // have been inserted into a note whose save is still pending.
    static constexpr qint64 GarbageGraceMs = qint64(24) * 60 * 60 * 1000;

    QList<NoteSummary> listNotes(const NoteQuery &query = {});
    // Full-text search over notes not in Recently Deleted, best matches first.
    // Every word must match; words match as prefixes ("bench" finds "benchmark").
    QList<NoteSummary> search(const QString &text, int limit = 200);
    std::optional<NoteRecord> loadNote(const QString &id, QString *error = nullptr);
    std::optional<NoteRecord> createNote(const RichDocument &body, QString *error = nullptr,
                                         const QString &folderId = {});
    SaveResult saveNote(const SaveRequest &request);
    bool setPinned(const QString &id, bool pinned);
    int revisionCount(const QString &noteId);

    // Notes
    bool moveNote(const QString &noteId, const QString &folderId, QString *error = nullptr);
    std::optional<NoteRecord> duplicateNote(const QString &noteId, QString *error = nullptr);
    bool trashNote(const QString &noteId, QString *error = nullptr);
    // Brings a note back from Recently Deleted, into its folder if that still exists.
    bool recoverNote(const QString &noteId, QString *error = nullptr);
    // Only notes already in Recently Deleted can be deleted permanently.
    bool deleteNotePermanently(const QString &noteId, QString *error = nullptr);
    int emptyTrash(QString *error = nullptr);
    int purgeExpiredTrash(qint64 now = 0);
    int trashCount();
    int noteCount();

    // Tags come from #tags in note text and are kept up to date on save.
    QList<TagInfo> listTags();

    // Smart Folders
    QList<SmartFolder> listSmartFolders();
    std::optional<SmartFolder> createSmartFolder(const QString &name, const SmartCriteria &criteria,
                                                 QString *error = nullptr);
    bool updateSmartFolder(const QString &id, const QString &name, const SmartCriteria &criteria,
                           QString *error = nullptr);
    bool deleteSmartFolder(const QString &id, QString *error = nullptr);

    // Every image and file in notes that aren't deleted, newest notes first.
    QList<AttachmentItem> listAttachmentItems();

    // Folders
    QList<FolderInfo> listFolders();
    std::optional<FolderInfo> createFolder(const QString &name, const QString &parentId = {},
                                           QString *error = nullptr);
    bool renameFolder(const QString &id, const QString &name, QString *error = nullptr);
    // Moves the folder's notes (and its subfolders' notes) to Recently Deleted,
    // then removes the folders.
    bool deleteFolder(const QString &id, QString *error = nullptr);

    // Removes attachment data no note or revision refers to any more.
    GarbageReport collectGarbage(qint64 now = 0);

    // Writes a complete, self-describing backup folder at `directory`
    // (which must not exist yet).
    std::optional<BackupManifest> backupTo(const QString &directory, QString *error = nullptr);
    // Checks that a backup folder is complete and readable by this version.
    static std::optional<BackupManifest> inspectBackup(const QString &directory, QString *error = nullptr);

    std::optional<AttachmentInfo> addAttachment(const QString &noteId, const QString &filePath,
                                                QString *error = nullptr);
    std::optional<AttachmentInfo> attachment(const QString &id);
    // Stores image bytes and returns the blob hash.
    std::optional<QString> addImage(const QByteArray &bytes, QString *error = nullptr);
    std::optional<QString> addImageFile(const QString &filePath, QString *error = nullptr);

    QString blobPath(const QString &hash) const { return m_blobs.path(hash); }
    // The open connection, for maintenance tasks and failure-injection tests.
    Database &database() { return m_db; }
    const LibraryPaths &paths() const { return m_paths; }

    // PRAGMA integrity_check plus attachment reference checks.
    QStringList verify();
    // Consistent online snapshot of the database (attachments are copied separately).
    bool backupDatabase(const QString &destination, QString *error);

private:
    bool migrate(QString *error);
    bool folderExists(const QString &id);
    // Rewrites what's derived from a note's body: image and attachment
    // references, tags and the thumbnail.
    bool writeDerived(const QString &noteId, const RichDocument &body);
    int countMatching(const NoteQuery &query);
    QStringList folderAndDescendants(const QString &id);
    bool registerBlob(const BlobStore::Installed &blob);

    LibraryPaths m_paths;
    BlobStore m_blobs;
    Database m_db;
};

} // namespace onotes

Q_DECLARE_METATYPE(onotes::SaveResult)
Q_DECLARE_METATYPE(onotes::NoteSummary)
Q_DECLARE_METATYPE(QList<onotes::NoteSummary>)
