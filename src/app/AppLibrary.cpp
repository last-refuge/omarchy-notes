#include "AppLibrary.h"

#include "LibraryBackup.h"
#include "RichDocument.h"

#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QLocale>
#include <QQmlEngine>
#include <QStandardPaths>

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

// Search snippets mark matches with \x02..\x03; render them as bold in
// Qt's StyledText, escaping everything else.
QString styledSnippet(const QString &snippet)
{
    QString out = snippet.toHtmlEscaped();
    out.replace(QChar(2), u"<b>"_s);
    out.replace(QChar(3), u"</b>"_s);
    return out;
}

QVariantMap result(bool ok, const QString &error = {})
{
    return {{u"ok"_s, ok}, {u"error"_s, error}};
}

} // namespace

// ---------------------------------------------------------------- NotesModel

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
    case SnippetRole:
        if (m_search && !n.snippet.isEmpty())
            return styledSnippet(n.snippet);
        return n.snippet.isEmpty() ? tr("No additional text") : n.snippet.toHtmlEscaped();
    case DateRole: return friendlyDate(m_trash ? n.deletedAt : n.updatedAt);
    case PinnedRole: return n.pinned;
    case HasAttachmentsRole: return n.hasAttachments;
    case HasChecklistRole: return n.hasChecklist;
    case FolderNameRole: return n.folderId.isEmpty() ? QString() : m_folderNames.value(n.folderId);
    case SectionRole:
        if (m_search || m_trash || !hasPinned())
            return QString();
        return n.pinned ? tr("Pinned") : tr("Notes");
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
        {FolderNameRole, "folderName"},
        {SectionRole, "section"},
    };
}

bool NotesModel::hasPinned() const
{
    return std::any_of(m_notes.cbegin(), m_notes.cend(), [](const NoteSummary &n) { return n.pinned; });
}

void NotesModel::reset(const QList<NoteSummary> &notes, bool searchResults, bool trash,
                       const QHash<QString, QString> &folderNames)
{
    beginResetModel();
    m_notes = notes;
    m_search = searchResults;
    m_trash = trash;
    m_folderNames = folderNames;
    endResetModel();
    emit countChanged();
}

void NotesModel::applySaved(const QString &id, const RichDocument &body, qint64 updatedAt, bool moveToTop)
{
    const int row = indexOf(id);
    if (row < 0)
        return;
    NoteSummary &n = m_notes[row];
    n.title = body.title();
    if (!m_search)
        n.snippet = body.snippet();
    n.updatedAt = updatedAt;
    n.hasAttachments = !body.referencedAttachments().isEmpty() || !body.referencedBlobs().isEmpty();
    emit dataChanged(index(row), index(row));
    if (!moveToTop)
        return;

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

bool NotesModel::isPinned(const QString &id) const
{
    const int row = indexOf(id);
    return row >= 0 && m_notes[row].pinned;
}

// ---------------------------------------------------------------- FoldersModel

int FoldersModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_rows.size());
}

QVariant FoldersModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_rows.size())
        return {};
    const Row &r = m_rows.at(index.row());
    switch (role) {
    case KeyRole: return r.key;
    case NameRole: return r.name;
    case KindRole: return r.kind;
    case DepthRole: return r.depth;
    case CountRole: return r.count;
    case FolderIdRole: return r.folderId;
    case ParentIdRole: return r.parentId;
    }
    return {};
}

QHash<int, QByteArray> FoldersModel::roleNames() const
{
    return {
        {KeyRole, "key"},
        {NameRole, "name"},
        {KindRole, "kind"},
        {DepthRole, "depth"},
        {CountRole, "count"},
        {FolderIdRole, "folderId"},
        {ParentIdRole, "parentId"},
    };
}

void FoldersModel::reset(const QList<FolderInfo> &folders, int noteCount, int trashCount)
{
    QHash<QString, QList<FolderInfo>> children;
    int filed = 0;
    for (const FolderInfo &f : folders) {
        children[f.parentId].append(f);
        filed += f.noteCount;
    }

    QList<Row> rows;
    rows.append({u"all"_s, tr("All Notes"), u"all"_s, 0, noteCount, {}, {}});
    rows.append({u"notes"_s, tr("Notes"), u"notes"_s, 0, noteCount - filed, {}, {}});
    // Depth-first, keeping the store's alphabetical order among siblings.
    std::function<void(const QString &, int)> walk = [&](const QString &parent, int depth) {
        for (const FolderInfo &f : children.value(parent)) {
            rows.append({f.id, f.name, u"folder"_s, depth, f.noteCount, f.id, f.parentId});
            walk(f.id, depth + 1);
        }
    };
    walk({}, 0);
    rows.append({u"trash"_s, tr("Recently Deleted"), u"trash"_s, 0, trashCount, {}, {}});

    beginResetModel();
    m_rows = rows;
    endResetModel();
}

QString FoldersModel::nameOf(const QString &key) const
{
    const int row = indexOfKey(key);
    return row < 0 ? QString() : m_rows[row].name;
}

int FoldersModel::indexOfKey(const QString &key) const
{
    for (int i = 0; i < m_rows.size(); ++i) {
        if (m_rows[i].key == key)
            return i;
    }
    return -1;
}

// ---------------------------------------------------------------- AppLibrary

AppLibrary::AppLibrary(LibraryService *service, const QString &settingsFile, QObject *parent)
    : QObject(parent), m_service(service)
{
    const QString file = settingsFile.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation)
              + u"/omarchy-notes/settings.ini"_s
        : settingsFile;
    m_settings = std::make_unique<QSettings>(file, QSettings::IniFormat);
    m_sort = static_cast<NoteSort>(std::clamp(m_settings->value(u"list/sort"_s, 0).toInt(), 0, 2));
    m_key = m_settings->value(u"list/view"_s, u"all"_s).toString();

    m_searchDebounce.setSingleShot(true);
    m_searchDebounce.setInterval(120);
    connect(&m_searchDebounce, &QTimer::timeout, this, &AppLibrary::runSearch);
    connect(m_service, &LibraryService::searchFinished, this, &AppLibrary::onSearchFinished);
    connect(m_service, &LibraryService::notesChanged, this, &AppLibrary::refresh);

    reloadFolders();
    if (m_folders.indexOfKey(m_key) < 0)
        m_key = u"all"_s;
    reloadNotes();
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

QString AppLibrary::viewTitle() const
{
    if (searching())
        return tr("Search");
    return m_folders.nameOf(m_key);
}

QString AppLibrary::lastNoteId() const
{
    return m_settings->value(u"editor/lastNote"_s).toString();
}

void AppLibrary::setLastNoteId(const QString &id)
{
    if (id == lastNoteId())
        return;
    m_settings->setValue(u"editor/lastNote"_s, id);
    emit lastNoteIdChanged();
}

void AppLibrary::setSimulateSaveFailure(bool fail)
{
    if (fail == simulateSaveFailure())
        return;
    m_service->setSimulatedSaveFailure(fail);
    emit simulateSaveFailureChanged();
}

void AppLibrary::setSearchText(const QString &text)
{
    if (text == m_searchText)
        return;
    const bool wasSearching = searching();
    m_searchText = text;
    emit searchTextChanged();
    if (searching()) {
        m_searchDebounce.start();
    } else {
        m_searchDebounce.stop();
        ++m_searchTicket; // ignore results still in flight
        reloadNotes();
    }
    if (wasSearching != searching())
        emit viewChanged();
}

void AppLibrary::setSortOrder(int order)
{
    const auto sort = static_cast<NoteSort>(std::clamp(order, 0, 2));
    if (sort == m_sort)
        return;
    m_sort = sort;
    m_settings->setValue(u"list/sort"_s, int(sort));
    emit sortOrderChanged();
    reloadNotes();
}

void AppLibrary::showKey(const QString &key)
{
    if (m_folders.indexOfKey(key) < 0)
        return;
    const bool changed = key != m_key || searching();
    m_key = key;
    m_settings->setValue(u"list/view"_s, key);
    if (searching()) {
        m_searchText.clear();
        emit searchTextChanged();
    }
    reloadNotes();
    if (changed)
        emit viewChanged();
}

void AppLibrary::refresh()
{
    reloadFolders();
    if (m_folders.indexOfKey(m_key) < 0) {
        m_key = u"all"_s;
        emit viewChanged();
    }
    if (searching())
        runSearch();
    else
        reloadNotes();
}

void AppLibrary::reloadFolders()
{
    m_trashCount = m_service->trashCount();
    m_folders.reset(m_service->listFolders(), m_service->noteCount(), m_trashCount);
    emit countsChanged();
}

QHash<QString, QString> AppLibrary::folderNames() const
{
    QHash<QString, QString> names;
    for (const FoldersModel::Row &row : m_folders.rows()) {
        if (row.kind == u"folder")
            names.insert(row.folderId, row.name);
    }
    return names;
}

void AppLibrary::reloadNotes()
{
    if (searching())
        return;
    NoteQuery query;
    if (m_key == u"trash")
        query = NoteQuery::trash();
    else if (m_key == u"all")
        query = NoteQuery::all(m_sort);
    else if (m_key == u"notes")
        query = NoteQuery::folder({}, m_sort);
    else
        query = NoteQuery::folder(m_key, m_sort);
    m_notes.reset(m_service->listNotes(query), false, m_key == u"trash", folderNames());
}

void AppLibrary::runSearch()
{
    if (searching())
        m_searchTicket = m_service->search(m_searchText);
}

void AppLibrary::onSearchFinished(quint64 ticket, const QList<NoteSummary> &results)
{
    if (ticket != m_searchTicket || !searching())
        return;
    m_notes.reset(results, true, false, folderNames());
}

void AppLibrary::noteSaved(const QString &noteId, const RichDocument &body, qint64 updatedAt)
{
    const bool moveToTop = m_sort == NoteSort::Edited && !searching() && m_key != u"trash";
    m_notes.applySaved(noteId, body, updatedAt, moveToTop);
}

bool AppLibrary::report(bool ok, const QString &error)
{
    if (!ok && !error.isEmpty())
        emit errorOccurred(error);
    return ok;
}

QString AppLibrary::createNote()
{
    QString folderId;
    if (m_key != u"all" && m_key != u"notes" && m_key != u"trash")
        folderId = m_key;
    QString error;
    const auto note = m_service->createNote(RichDocument{{Block::paragraph()}}, &error, folderId);
    if (!note) {
        emit errorOccurred(tr("Couldn't create a note: %1").arg(error));
        return {};
    }
    // A new note should be visible in the list it was created from.
    if (searching() || m_key == u"trash") {
        m_searchText.clear();
        emit searchTextChanged();
        m_key = u"all"_s;
        emit viewChanged();
    }
    refresh();
    return note->id;
}

void AppLibrary::setPinned(const QString &id, bool pinned)
{
    if (report(m_service->setPinned(id, pinned), tr("Couldn't update the note."))) {
        refresh();
        emit noteMetaChanged(id);
    }
}

bool AppLibrary::moveNote(const QString &noteId, const QString &folderId)
{
    QString error;
    if (!report(m_service->moveNote(noteId, folderId, &error), error))
        return false;
    refresh();
    emit noteMetaChanged(noteId);
    return true;
}

QString AppLibrary::duplicateNote(const QString &noteId)
{
    QString error;
    const auto copy = m_service->duplicateNote(noteId, &error);
    if (!report(copy.has_value(), error))
        return {};
    refresh();
    return copy->id;
}

bool AppLibrary::trashNote(const QString &noteId)
{
    QString error;
    if (!report(m_service->trashNote(noteId, &error), error))
        return false;
    refresh();
    emit noteMetaChanged(noteId);
    return true;
}

bool AppLibrary::recoverNote(const QString &noteId)
{
    QString error;
    if (!report(m_service->recoverNote(noteId, &error), error))
        return false;
    refresh();
    emit noteMetaChanged(noteId);
    return true;
}

bool AppLibrary::deleteNotePermanently(const QString &noteId)
{
    QString error;
    if (!report(m_service->deleteNotePermanently(noteId, &error), error))
        return false;
    refresh();
    emit noteMetaChanged(noteId);
    return true;
}

int AppLibrary::emptyTrash()
{
    const QList<NoteSummary> trashed = m_service->listNotes(NoteQuery::trash());
    QString error;
    const int removed = m_service->emptyTrash(&error);
    report(removed >= 0, error);
    refresh();
    for (const NoteSummary &n : trashed)
        emit noteMetaChanged(n.id);
    return removed;
}

QVariantMap AppLibrary::createFolder(const QString &name, const QString &parentId)
{
    QString error;
    const auto folder = m_service->createFolder(name, parentId, &error);
    if (!folder)
        return result(false, error);
    refresh();
    QVariantMap out = result(true);
    out.insert(u"id"_s, folder->id);
    return out;
}

QVariantMap AppLibrary::renameFolder(const QString &id, const QString &name)
{
    QString error;
    if (!m_service->renameFolder(id, name, &error))
        return result(false, error);
    refresh();
    emit viewChanged(); // the title may be this folder's name
    return result(true);
}

bool AppLibrary::deleteFolder(const QString &id)
{
    // Notes inside are moved to Recently Deleted; tell open editors.
    QStringList affected;
    for (const NoteSummary &n : m_service->listNotes(NoteQuery::all()))
        affected << n.id;
    QString error;
    if (!report(m_service->deleteFolder(id, &error), error))
        return false;
    refresh();
    for (const QString &noteId : affected)
        emit noteMetaChanged(noteId);
    return true;
}

QString AppLibrary::folderName(const QString &id) const
{
    return id.isEmpty() ? tr("Notes") : m_folders.nameOf(id);
}

int AppLibrary::folderNoteCount(const QString &id) const
{
    const int row = m_folders.indexOfKey(id);
    return row < 0 ? 0 : m_folders.rows()[row].count;
}

QVariantList AppLibrary::folderChoices() const
{
    QVariantList out;
    out.append(QVariantMap{{u"id"_s, QString()}, {u"name"_s, tr("Notes")}, {u"depth"_s, 0}});
    for (const FoldersModel::Row &row : m_folders.rows()) {
        if (row.kind == u"folder")
            out.append(QVariantMap{{u"id"_s, row.folderId}, {u"name"_s, row.name}, {u"depth"_s, row.depth}});
    }
    return out;
}

QString AppLibrary::defaultBackupName() const
{
    return onotes::defaultBackupName();
}

QVariantMap AppLibrary::backupTo(const QUrl &parentFolder, const QString &name)
{
    const QString path = QDir(parentFolder.toLocalFile()).filePath(name.trimmed());
    QString error;
    const auto manifest = m_service->backupTo(path, &error);
    if (!manifest)
        return result(false, error);
    QVariantMap out = result(true);
    out.insert(u"path"_s, manifest->path);
    out.insert(u"notes"_s, manifest->notes);
    out.insert(u"attachments"_s, manifest->blobs);
    out.insert(u"size"_s, QLocale().formattedDataSize(manifest->bytes));
    return out;
}

QVariantMap AppLibrary::inspectBackup(const QUrl &folder)
{
    QString error;
    const auto manifest = m_service->inspectBackup(folder.toLocalFile(), &error);
    if (!manifest)
        return result(false, error);
    QVariantMap out = result(true);
    out.insert(u"path"_s, manifest->path);
    out.insert(u"notes"_s, manifest->notes);
    out.insert(u"attachments"_s, manifest->blobs);
    out.insert(u"size"_s, QLocale().formattedDataSize(manifest->bytes));
    out.insert(u"created"_s, QLocale().toString(QDateTime::fromMSecsSinceEpoch(manifest->createdAt),
                                                QLocale::LongFormat));
    return out;
}

QVariantMap AppLibrary::restoreFrom(const QUrl &folder)
{
    QString safety;
    QString error;
    if (!m_service->restoreFrom(folder.toLocalFile(), &safety, &error))
        return result(false, error);
    m_searchText.clear();
    emit searchTextChanged();
    m_key = u"all"_s;
    refresh();
    emit viewChanged();
    emit libraryReplaced();
    QVariantMap out = result(true);
    out.insert(u"previous"_s, safety);
    return out;
}

void AppLibrary::openLibraryFolder() const
{
    QDesktopServices::openUrl(QUrl::fromLocalFile(location()));
}
