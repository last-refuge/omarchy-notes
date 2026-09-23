#include "AppLibrary.h"

#include <QDateTime>
#include <QLocale>
#include <QQmlEngine>

using namespace Qt::StringLiterals;
using namespace onotes;

namespace {

AppLibrary *s_instance = nullptr;

QString friendlyDate(qint64 ms)
{
    const QDateTime when = QDateTime::fromMSecsSinceEpoch(ms);
    const QDate today = QDate::currentDate();
    const QLocale locale;
    if (when.date() == today)
        return locale.toString(when.time(), QLocale::ShortFormat);
    if (when.date() == today.addDays(-1))
        return QObject::tr("Yesterday");
    if (when.date().daysTo(today) < 7)
        return locale.dayName(when.date().dayOfWeek());
    return locale.toString(when.date(), QLocale::ShortFormat);
}

} // namespace

int NotesModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_notes.size());
}

QVariant NotesModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_notes.size())
        return {};
    const NoteSummary &n = m_notes.at(index.row());
    switch (role) {
    case IdRole: return n.id;
    case TitleRole: return n.title.isEmpty() ? tr("New Note") : n.title;
    case SnippetRole: return n.snippet.isEmpty() ? tr("No additional text") : n.snippet;
    case DateRole: return friendlyDate(n.updatedAt);
    case PinnedRole: return n.pinned;
    case HasAttachmentsRole: return n.hasAttachments;
    case HasChecklistRole: return n.hasChecklist;
    }
    return {};
}

QHash<int, QByteArray> NotesModel::roleNames() const
{
    return {
        {IdRole, "noteId"},
        {TitleRole, "title"},
        {SnippetRole, "snippet"},
        {DateRole, "date"},
        {PinnedRole, "pinned"},
        {HasAttachmentsRole, "hasAttachments"},
        {HasChecklistRole, "hasChecklist"},
    };
}

void NotesModel::reset(const QList<NoteSummary> &notes)
{
    beginResetModel();
    m_notes = notes;
    endResetModel();
}

void NotesModel::applySaved(const QString &id, const RichDocument &body, qint64 updatedAt)
{
    const int row = indexOf(id);
    if (row < 0)
        return;
    NoteSummary &n = m_notes[row];
    n.title = body.title();
    n.snippet = body.snippet();
    n.updatedAt = updatedAt;
    n.hasAttachments = !body.referencedAttachments().isEmpty() || !body.referencedBlobs().isEmpty();
    emit dataChanged(index(row), index(row));

    // Most recently edited first, below pinned notes.
    int target = 0;
    while (target < m_notes.size() && m_notes[target].pinned && !n.pinned)
        ++target;
    if (target != row && beginMoveRows({}, row, row, {}, target > row ? target + 1 : target)) {
        m_notes.move(row, target);
        endMoveRows();
    }
}

int NotesModel::indexOf(const QString &id) const
{
    for (int i = 0; i < m_notes.size(); ++i) {
        if (m_notes[i].id == id)
            return i;
    }
    return -1;
}

QString NotesModel::idAt(int row) const
{
    return row >= 0 && row < m_notes.size() ? m_notes[row].id : QString();
}

QString NotesModel::titleOf(const QString &id) const
{
    const int row = indexOf(id);
    return row < 0 ? QString() : data(index(row), TitleRole).toString();
}

AppLibrary::AppLibrary(LibraryService *service, QObject *parent)
    : QObject(parent), m_service(service)
{
    connect(m_service, &LibraryService::notesChanged, this, &AppLibrary::refresh);
    refresh();
}

AppLibrary *AppLibrary::create(QQmlEngine *, QJSEngine *)
{
    Q_ASSERT(s_instance);
    QQmlEngine::setObjectOwnership(s_instance, QQmlEngine::CppOwnership);
    return s_instance;
}

void AppLibrary::setInstance(AppLibrary *instance)
{
    s_instance = instance;
}

void AppLibrary::setSimulateSaveFailure(bool fail)
{
    if (fail == simulateSaveFailure())
        return;
    m_service->setSimulatedSaveFailure(fail);
    emit simulateSaveFailureChanged();
}

void AppLibrary::refresh()
{
    m_notes.reset(m_service->listNotes());
}

QString AppLibrary::createNote()
{
    QString error;
    const auto note = m_service->createNote(RichDocument{{Block::paragraph()}}, &error);
    if (!note) {
        emit errorOccurred(tr("Couldn't create a note: %1").arg(error));
        return {};
    }
    return note->id;
}

void AppLibrary::setPinned(const QString &id, bool pinned)
{
    if (!m_service->setPinned(id, pinned))
        emit errorOccurred(tr("Couldn't update the note."));
}
