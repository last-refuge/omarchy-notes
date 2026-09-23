#pragma once

#include "NoteStore.h"

#include <QObject>
#include <QThread>

#include <atomic>
#include <memory>

namespace onotes {

// The single owner of a library. Every database and file operation runs on a
// dedicated persistence thread; the UI thread never touches SQLite.
//
// Saves are asynchronous and acknowledged through saveFinished(). The
// remaining calls block on the persistence thread and are meant for small,
// user-initiated operations.
class LibraryService : public QObject
{
    Q_OBJECT

public:
    // How long after startup the daily housekeeping runs.
    static constexpr int HousekeepingDelayMs = 30000;

    explicit LibraryService(LibraryPaths paths, QObject *parent = nullptr);
    ~LibraryService() override;

    bool start(QString *error);
    const LibraryPaths &paths() const { return m_paths; }

    QList<NoteSummary> listNotes(const NoteQuery &query = {});
    std::optional<NoteRecord> loadNote(const QString &id, QString *error = nullptr);
    std::optional<NoteRecord> createNote(const RichDocument &body, QString *error = nullptr,
                                         const QString &folderId = {});
    bool setPinned(const QString &id, bool pinned);

    // Runs a search on the persistence thread; results arrive through
    // searchFinished with the returned ticket.
    quint64 search(const QString &text);

    bool moveNote(const QString &noteId, const QString &folderId, QString *error = nullptr);
    std::optional<NoteRecord> duplicateNote(const QString &noteId, QString *error = nullptr);
    bool trashNote(const QString &noteId, QString *error = nullptr);
    bool recoverNote(const QString &noteId, QString *error = nullptr);
    bool deleteNotePermanently(const QString &noteId, QString *error = nullptr);
    int emptyTrash(QString *error = nullptr);
    int trashCount();
    int noteCount();

    QList<TagInfo> listTags();
    QList<SmartFolder> listSmartFolders();
    std::optional<SmartFolder> createSmartFolder(const QString &name, const SmartCriteria &criteria,
                                                 QString *error = nullptr);
    bool updateSmartFolder(const QString &id, const QString &name, const SmartCriteria &criteria,
                           QString *error = nullptr);
    bool deleteSmartFolder(const QString &id, QString *error = nullptr);
    QList<AttachmentItem> listAttachmentItems();

    QList<FolderInfo> listFolders();
    std::optional<FolderInfo> createFolder(const QString &name, const QString &parentId = {},
                                           QString *error = nullptr);
    bool renameFolder(const QString &id, const QString &name, QString *error = nullptr);
    bool deleteFolder(const QString &id, QString *error = nullptr);

    std::optional<BackupManifest> backupTo(const QString &directory, QString *error = nullptr);
    std::optional<BackupManifest> inspectBackup(const QString &directory, QString *error = nullptr);
    // Replaces the library with a backup. The previous library is moved to
    // `*safetyDir`; on any failure it is put back and false is returned.
    bool restoreFrom(const QString &directory, QString *safetyDir, QString *error = nullptr);
    std::optional<AttachmentInfo> addAttachment(const QString &noteId, const QString &filePath,
                                                QString *error = nullptr);
    std::optional<AttachmentInfo> attachment(const QString &id);
    std::optional<QString> addImage(const QByteArray &bytes, QString *error = nullptr);
    std::optional<QString> addImageFile(const QString &filePath, QString *error = nullptr);
    QStringList verify();

    // Path computation only; safe from any thread.
    QString blobPath(const QString &hash) const;

    // Queues a save; the result arrives through saveFinished with the ticket.
    quint64 saveNote(const SaveRequest &request);
    // Blocks until every queued save has been committed or has failed.
    // Their saveFinished signals are still delivered through the event loop;
    // call deliverPendingResults() to receive them immediately.
    void waitForIdle();
    void deliverPendingResults();

    // Makes subsequent saves fail, for exercising failure handling.
    void setSimulatedSaveFailure(bool fail) { m_simulateFailure = fail; }
    bool simulatedSaveFailure() const { return m_simulateFailure; }

signals:
    void saveFinished(quint64 ticket, const QString &noteId, const onotes::SaveResult &result);
    void searchFinished(quint64 ticket, const QList<onotes::NoteSummary> &results);
    void notesChanged();

private:
    template<typename Fn>
    auto blocking(Fn &&fn) -> decltype(fn());

    LibraryPaths m_paths;
    BlobStore m_pathResolver;
    QThread m_thread;
    QObject *m_context = nullptr; // lives on m_thread
    std::unique_ptr<NoteStore> m_store; // used only on m_thread
    std::atomic<quint64> m_nextTicket{1};
    std::atomic<bool> m_simulateFailure{false};
};

} // namespace onotes
