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
    explicit LibraryService(LibraryPaths paths, QObject *parent = nullptr);
    ~LibraryService() override;

    bool start(QString *error);
    const LibraryPaths &paths() const { return m_paths; }

    QList<NoteSummary> listNotes();
    std::optional<NoteRecord> loadNote(const QString &id, QString *error = nullptr);
    std::optional<NoteRecord> createNote(const RichDocument &body, QString *error = nullptr);
    bool setPinned(const QString &id, bool pinned);
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
    void waitForIdle();

    // Makes subsequent saves fail, for exercising failure handling.
    void setSimulatedSaveFailure(bool fail) { m_simulateFailure = fail; }
    bool simulatedSaveFailure() const { return m_simulateFailure; }

signals:
    void saveFinished(quint64 ticket, const QString &noteId, const onotes::SaveResult &result);
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
