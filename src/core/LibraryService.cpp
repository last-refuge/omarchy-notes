#include "LibraryService.h"

#include "LibraryBackup.h"

#include <QCoreApplication>
#include <QMetaObject>

using namespace Qt::StringLiterals;

namespace onotes {

LibraryService::LibraryService(LibraryPaths paths, QObject *parent)
    : QObject(parent), m_paths(paths), m_pathResolver(paths)
{
    qRegisterMetaType<onotes::SaveResult>();
    qRegisterMetaType<QList<onotes::NoteSummary>>();
    m_thread.setObjectName(u"persistence"_s);
}

LibraryService::~LibraryService()
{
    if (m_context) {
        waitForIdle();
        blocking([this] {
            m_store->close();
            return true;
        });
        m_context->deleteLater();
        m_thread.quit();
        m_thread.wait();
    }
}

template<typename Fn>
auto LibraryService::blocking(Fn &&fn) -> decltype(fn())
{
    decltype(fn()) result{};
    QMetaObject::invokeMethod(m_context, [&] { result = fn(); }, Qt::BlockingQueuedConnection);
    return result;
}

bool LibraryService::start(QString *error)
{
    m_store = std::make_unique<NoteStore>(m_paths);
    m_context = new QObject;
    m_context->moveToThread(&m_thread);
    m_thread.start();

    QString openError;
    const bool ok = blocking([&] { return m_store->open(&openError); });
    if (!ok) {
        if (error)
            *error = openError;
        return false;
    }
    // Housekeeping runs after startup so it never delays the first window.
    QMetaObject::invokeMethod(m_context, [this] {
        const int purged = m_store->purgeExpiredTrash();
        m_store->collectGarbage();
        if (purged > 0)
            QMetaObject::invokeMethod(this, &LibraryService::notesChanged, Qt::QueuedConnection);
    }, Qt::QueuedConnection);
    return true;
}

QList<NoteSummary> LibraryService::listNotes(const NoteQuery &query)
{
    return blocking([&] { return m_store->listNotes(query); });
}

quint64 LibraryService::search(const QString &text)
{
    const quint64 ticket = m_nextTicket++;
    QMetaObject::invokeMethod(m_context, [this, ticket, text] {
        const QList<NoteSummary> results = m_store->search(text);
        QMetaObject::invokeMethod(this, [this, ticket, results] {
            emit searchFinished(ticket, results);
        }, Qt::QueuedConnection);
    }, Qt::QueuedConnection);
    return ticket;
}

bool LibraryService::moveNote(const QString &noteId, const QString &folderId, QString *error)
{
    return blocking([&] { return m_store->moveNote(noteId, folderId, error); });
}

std::optional<NoteRecord> LibraryService::duplicateNote(const QString &noteId, QString *error)
{
    return blocking([&] { return m_store->duplicateNote(noteId, error); });
}

bool LibraryService::trashNote(const QString &noteId, QString *error)
{
    return blocking([&] { return m_store->trashNote(noteId, error); });
}

bool LibraryService::recoverNote(const QString &noteId, QString *error)
{
    return blocking([&] { return m_store->recoverNote(noteId, error); });
}

bool LibraryService::deleteNotePermanently(const QString &noteId, QString *error)
{
    return blocking([&] { return m_store->deleteNotePermanently(noteId, error); });
}

int LibraryService::emptyTrash(QString *error)
{
    return blocking([&] { return m_store->emptyTrash(error); });
}

int LibraryService::trashCount()
{
    return blocking([this] { return m_store->trashCount(); });
}

int LibraryService::noteCount()
{
    return blocking([this] { return m_store->noteCount(); });
}

QList<FolderInfo> LibraryService::listFolders()
{
    return blocking([this] { return m_store->listFolders(); });
}

std::optional<FolderInfo> LibraryService::createFolder(const QString &name, const QString &parentId,
                                                       QString *error)
{
    return blocking([&] { return m_store->createFolder(name, parentId, error); });
}

bool LibraryService::renameFolder(const QString &id, const QString &name, QString *error)
{
    return blocking([&] { return m_store->renameFolder(id, name, error); });
}

bool LibraryService::deleteFolder(const QString &id, QString *error)
{
    return blocking([&] { return m_store->deleteFolder(id, error); });
}

std::optional<BackupManifest> LibraryService::backupTo(const QString &directory, QString *error)
{
    return blocking([&] { return m_store->backupTo(directory, error); });
}

std::optional<BackupManifest> LibraryService::inspectBackup(const QString &directory, QString *error)
{
    return blocking([&] { return NoteStore::inspectBackup(directory, error); });
}

bool LibraryService::restoreFrom(const QString &directory, QString *safetyDir, QString *error)
{
    return blocking([&] {
        if (!NoteStore::inspectBackup(directory, error))
            return false;
        m_store->close();
        QString safety;
        if (!replaceLibraryFiles(m_paths, directory, &safety, error)) {
            m_store->open(nullptr);
            return false;
        }
        auto restored = std::make_unique<NoteStore>(m_paths);
        QString openError;
        QStringList problems;
        const bool opened = restored->open(&openError);
        if (opened)
            problems = restored->verify();
        if (!opened || !problems.isEmpty()) {
            restored.reset();
            rollbackLibraryFiles(m_paths, safety, nullptr);
            m_store->open(nullptr);
            if (error)
                *error = u"The backup couldn't be opened, so your notes were left as they were. %1"_s
                             .arg(opened ? problems.value(0) : openError);
            return false;
        }
        m_store = std::move(restored);
        if (safetyDir)
            *safetyDir = safety;
        return true;
    });
}

std::optional<NoteRecord> LibraryService::loadNote(const QString &id, QString *error)
{
    return blocking([&] { return m_store->loadNote(id, error); });
}

std::optional<NoteRecord> LibraryService::createNote(const RichDocument &body, QString *error,
                                                    const QString &folderId)
{
    auto result = blocking([&] { return m_store->createNote(body, error, folderId); });
    if (result)
        emit notesChanged();
    return result;
}

bool LibraryService::setPinned(const QString &id, bool pinned)
{
    const bool ok = blocking([&] { return m_store->setPinned(id, pinned); });
    if (ok)
        emit notesChanged();
    return ok;
}

std::optional<AttachmentInfo> LibraryService::addAttachment(const QString &noteId, const QString &filePath,
                                                            QString *error)
{
    return blocking([&] { return m_store->addAttachment(noteId, filePath, error); });
}

std::optional<AttachmentInfo> LibraryService::attachment(const QString &id)
{
    return blocking([&] { return m_store->attachment(id); });
}

std::optional<QString> LibraryService::addImage(const QByteArray &bytes, QString *error)
{
    return blocking([&] { return m_store->addImage(bytes, error); });
}

std::optional<QString> LibraryService::addImageFile(const QString &filePath, QString *error)
{
    return blocking([&] { return m_store->addImageFile(filePath, error); });
}

QStringList LibraryService::verify()
{
    return blocking([this] { return m_store->verify(); });
}

QString LibraryService::blobPath(const QString &hash) const
{
    return m_pathResolver.path(hash);
}

quint64 LibraryService::saveNote(const SaveRequest &request)
{
    const quint64 ticket = m_nextTicket++;
    QMetaObject::invokeMethod(m_context, [this, ticket, request] {
        SaveResult result;
        if (m_simulateFailure)
            result.error = u"Simulated storage failure."_s;
        else
            result = m_store->saveNote(request);
        QMetaObject::invokeMethod(this, [this, ticket, id = request.noteId, result] {
            emit saveFinished(ticket, id, result);
        }, Qt::QueuedConnection);
    }, Qt::QueuedConnection);
    return ticket;
}

void LibraryService::waitForIdle()
{
    // The persistence thread processes calls in order, so a blocking no-op
    // returns only after every earlier save.
    blocking([] { return true; });
}

void LibraryService::deliverPendingResults()
{
    waitForIdle();
    QCoreApplication::sendPostedEvents(this, QEvent::MetaCall);
}

} // namespace onotes
