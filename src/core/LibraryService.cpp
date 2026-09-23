#include "LibraryService.h"

#include <QMetaObject>

using namespace Qt::StringLiterals;

namespace onotes {

LibraryService::LibraryService(LibraryPaths paths, QObject *parent)
    : QObject(parent), m_paths(paths), m_pathResolver(paths)
{
    qRegisterMetaType<onotes::SaveResult>();
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
    if (!ok && error)
        *error = openError;
    return ok;
}

QList<NoteSummary> LibraryService::listNotes()
{
    return blocking([this] { return m_store->listNotes(); });
}

std::optional<NoteRecord> LibraryService::loadNote(const QString &id, QString *error)
{
    return blocking([&] { return m_store->loadNote(id, error); });
}

std::optional<NoteRecord> LibraryService::createNote(const RichDocument &body, QString *error)
{
    auto result = blocking([&] { return m_store->createNote(body, error); });
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

} // namespace onotes
