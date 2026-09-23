#include "AppLibrary.h"

#include "DocumentConverter.h"
#include "LibraryBackup.h"
#include "LibraryPaths.h"
#include "NoteExchange.h"
#include "RichDocument.h"

#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonObject>
#include <QMimeDatabase>
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

QString safeFileName(const QString &name)
{
    QString out = QFileInfo(name).fileName();
    out.replace(u'/', u'_');
    if (out.isEmpty() || out == u"." || out == u"..")
        out = u"attachment"_s;
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
    case ThumbnailRole: return n.thumbnail;
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
        {ThumbnailRole, "thumbnail"},
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
    case SectionRole: return r.section;
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
        {SectionRole, "section"},
    };
}

void FoldersModel::reset(const QList<FolderInfo> &folders, int noteCount, int trashCount, int attachmentCount,
                         const QList<SmartFolder> &smartFolders, const QList<TagInfo> &tags)
{
    QHash<QString, QList<FolderInfo>> children;
    int filed = 0;
    for (const FolderInfo &f : folders) {
        children[f.parentId].append(f);
        filed += f.noteCount;
    }

    QList<Row> rows;
    rows.append({u"all"_s, tr("All Notes"), u"all"_s, 0, noteCount, {}, {}, {}});
    rows.append({u"notes"_s, tr("Notes"), u"notes"_s, 0, noteCount - filed, {}, {}, {}});
    // Depth-first, keeping the store's alphabetical order among siblings.
    std::function<void(const QString &, int)> walk = [&](const QString &parent, int depth) {
        for (const FolderInfo &f : children.value(parent)) {
            rows.append({f.id, f.name, u"folder"_s, depth, f.noteCount, f.id, f.parentId, {}});
            walk(f.id, depth + 1);
        }
    };
    walk({}, 0);
    rows.append({u"attachments"_s, tr("Attachments"), u"attachments"_s, 0, attachmentCount, {}, {}, {}});
    rows.append({u"trash"_s, tr("Recently Deleted"), u"trash"_s, 0, trashCount, {}, {}, {}});
    for (const SmartFolder &f : smartFolders)
        rows.append({u"smart:"_s + f.id, f.name, u"smart"_s, 0, f.noteCount, f.id, {}, tr("Smart Folders")});
    for (const TagInfo &t : tags)
        rows.append({u"tag:"_s + t.name, u"#"_s + t.name, u"tag"_s, 0, t.noteCount, {}, {}, tr("Tags")});

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

// ---------------------------------------------------------------- AttachmentsModel

int AttachmentsModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_visible.size());
}

QVariant AttachmentsModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_visible.size())
        return {};
    const AttachmentItem &item = m_visible.at(index.row());
    switch (role) {
    case IsImageRole: return item.isImage;
    case BlobHashRole: return item.blobHash;
    case AttachmentIdRole: return item.attachmentId;
    case FileNameRole: return item.isImage ? tr("Image") : item.fileName;
    case DetailRole: {
        const QString size = QLocale().formattedDataSize(item.size);
        if (item.isImage)
            return size;
        const QString kind = QMimeDatabase().mimeTypeForName(item.mimeType).comment();
        return kind.isEmpty() ? size : kind + u" · "_s + size;
    }
    case ExtensionRole: return QFileInfo(item.fileName).suffix().toUpper().left(4);
    case NoteIdRole: return item.noteId;
    case NoteTitleRole: return item.noteTitle.isEmpty() ? tr("New Note") : item.noteTitle;
    }
    return {};
}

QHash<int, QByteArray> AttachmentsModel::roleNames() const
{
    return {
        {IsImageRole, "isImage"},
        {BlobHashRole, "blobHash"},
        {AttachmentIdRole, "attachmentId"},
        {FileNameRole, "fileName"},
        {DetailRole, "detail"},
        {ExtensionRole, "extension"},
        {NoteIdRole, "noteId"},
        {NoteTitleRole, "noteTitle"},
    };
}

void AttachmentsModel::setFilter(int filter)
{
    if (filter == m_filter)
        return;
    m_filter = filter;
    emit filterChanged();
    applyFilter();
}

void AttachmentsModel::reset(const QList<AttachmentItem> &items)
{
    m_items = items;
    applyFilter();
}

void AttachmentsModel::applyFilter()
{
    beginResetModel();
    m_visible.clear();
    for (const AttachmentItem &item : m_items) {
        if (m_filter == All || (m_filter == Images) == item.isImage)
            m_visible.append(item);
    }
    endResetModel();
    emit countChanged();
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
    m_gallery = m_settings->value(u"list/gallery"_s, false).toBool();

    m_searchDebounce.setSingleShot(true);
    m_searchDebounce.setInterval(120);
    connect(&m_searchDebounce, &QTimer::timeout, this, &AppLibrary::runSearch);
    m_sidebarRefresh.setSingleShot(true);
    m_sidebarRefresh.setInterval(800);
    connect(&m_sidebarRefresh, &QTimer::timeout, this, [this] {
        reloadFolders();
        // Membership of tag and Smart Folder views can change with an edit.
        if (!searching() && (m_key.startsWith(u"tag:") || m_key.startsWith(u"smart:")))
            reloadNotes();
    });
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

void AppLibrary::setGalleryMode(bool gallery)
{
    if (gallery == m_gallery)
        return;
    m_gallery = gallery;
    m_settings->setValue(u"list/gallery"_s, gallery);
    emit galleryModeChanged();
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
    m_smartFolders = m_service->listSmartFolders();
    const QList<TagInfo> tags = m_service->listTags();
    m_tags.clear();
    for (const TagInfo &t : tags)
        m_tags << t.name;
    const QList<AttachmentItem> items = m_service->listAttachmentItems();
    m_folders.reset(m_service->listFolders(), m_service->noteCount(), m_trashCount, int(items.size()),
                    m_smartFolders, tags);
    if (m_key == u"attachments")
        m_attachments.reset(items);
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
    if (m_key == u"trash") {
        query = NoteQuery::trash();
    } else if (m_key == u"attachments") {
        m_attachments.reset(m_service->listAttachmentItems());
        m_notes.reset({}, false, false, {});
        return;
    } else if (m_key.startsWith(u"tag:")) {
        query = NoteQuery::tagged(m_key.mid(4), m_sort);
    } else if (m_key.startsWith(u"smart:")) {
        query = NoteQuery::all(m_sort);
        for (const SmartFolder &f : m_smartFolders) {
            if (f.id == m_key.mid(6))
                query = NoteQuery::smart(f.criteria, m_sort);
        }
    } else if (m_key == u"all")
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
    m_sidebarRefresh.start();
}

bool AppLibrary::report(bool ok, const QString &error)
{
    if (!ok && !error.isEmpty())
        emit errorOccurred(error);
    return ok;
}

QString AppLibrary::createNote()
{
    const bool special = m_key == u"all" || m_key == u"notes" || m_key == u"trash" || m_key == u"attachments"
        || m_key.startsWith(u"tag:") || m_key.startsWith(u"smart:");
    const QString folderId = special ? QString() : m_key;
    RichDocument body{{Block::paragraph()}};
    // A note started from a tag carries the tag, so it shows up there.
    if (m_key.startsWith(u"tag:") && !searching())
        body.blocks.append(Block::paragraph({Span::plain(u"#"_s + m_key.mid(4))}));
    QString error;
    const auto note = m_service->createNote(body, &error, folderId);
    if (!note) {
        emit errorOccurred(tr("Couldn't create a note: %1").arg(error));
        return {};
    }
    // A new note should be visible in the list it was created from.
    if (searching() || m_key == u"trash" || m_key == u"attachments" || m_key.startsWith(u"smart:")) {
        m_searchText.clear();
        emit searchTextChanged();
        m_key = u"all"_s;
        emit viewChanged();
    }
    refresh();
    return note->id;
}

QString AppLibrary::createQuickNote()
{
    QString error;
    const auto note = m_service->createNote(RichDocument{{Block::paragraph()}}, &error);
    if (!note) {
        emit errorOccurred(tr("Couldn't create a note: %1").arg(error));
        return {};
    }
    refresh();
    return note->id;
}

bool AppLibrary::discardIfEmpty(const QString &noteId)
{
    const auto note = m_service->loadNote(noteId);
    if (!note || !note->body.plainText().trimmed().isEmpty() || !note->body.referencedBlobs().isEmpty()
        || !note->body.referencedAttachments().isEmpty())
        return false;
    const bool removed = m_service->trashNote(noteId) && m_service->deleteNotePermanently(noteId);
    refresh();
    emit noteMetaChanged(noteId);
    return removed;
}

QString AppLibrary::captureText(const QString &text)
{
    if (text.trimmed().isEmpty())
        return {};
    QString error;
    const auto note = m_service->createNote(DocumentConverter::fromPlainText(text.trimmed()), &error);
    if (!note) {
        emit errorOccurred(tr("Couldn't save the captured note: %1").arg(error));
        return {};
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

QVariantMap AppLibrary::createSmartFolder(const QString &name, const QVariantMap &criteria)
{
    QString error;
    const auto folder = m_service->createSmartFolder(
        name, SmartCriteria::fromJson(QJsonObject::fromVariantMap(criteria)), &error);
    if (!folder)
        return result(false, error);
    refresh();
    QVariantMap out = result(true);
    out.insert(u"id"_s, u"smart:"_s + folder->id);
    return out;
}

QVariantMap AppLibrary::updateSmartFolder(const QString &id, const QString &name, const QVariantMap &criteria)
{
    QString error;
    if (!m_service->updateSmartFolder(id, name, SmartCriteria::fromJson(QJsonObject::fromVariantMap(criteria)), &error))
        return result(false, error);
    refresh();
    emit viewChanged();
    return result(true);
}

bool AppLibrary::deleteSmartFolder(const QString &id)
{
    QString error;
    if (!report(m_service->deleteSmartFolder(id, &error), error))
        return false;
    refresh();
    return true;
}

QVariantMap AppLibrary::smartFolder(const QString &id) const
{
    for (const SmartFolder &f : m_smartFolders) {
        if (f.id == id) {
            QVariantMap out = f.criteria.toJson().toVariantMap();
            out.insert(u"name"_s, f.name);
            return out;
        }
    }
    return {};
}

bool AppLibrary::openAttachment(const QString &attachmentId)
{
    const auto info = m_service->attachment(attachmentId);
    const QString source = info ? m_service->blobPath(info->blobHash) : QString();
    if (!info || !QFileInfo::exists(source)) {
        emit errorOccurred(tr("This attachment's data is missing from the library."));
        return false;
    }
    // External apps need a real file name; give them a read-only copy.
    const QString dir = cacheDirectory() + u"/open/"_s + attachmentId;
    QDir().mkpath(dir);
    const QString target = dir + u'/' + safeFileName(info->fileName);
    if (!QFileInfo::exists(target)) {
        if (!QFile::copy(source, target)) {
            emit errorOccurred(tr("Couldn't prepare the attachment for opening."));
            return false;
        }
        QFile::setPermissions(target, QFileDevice::ReadOwner | QFileDevice::ReadUser);
    }
    if (!QDesktopServices::openUrl(QUrl::fromLocalFile(target))) {
        emit errorOccurred(tr("No application is set up to open %1.").arg(info->fileName));
        return false;
    }
    return true;
}

bool AppLibrary::openImage(const QString &blobHash)
{
    const QString source = m_service->blobPath(blobHash);
    if (source.isEmpty() || !QFileInfo::exists(source)) {
        emit errorOccurred(tr("This image's data is missing from the library."));
        return false;
    }
    // Hand images to the desktop viewer through a named copy.
    const QString dir = cacheDirectory() + u"/open/"_s + blobHash.left(16);
    QDir().mkpath(dir);
    const QString suffix = QMimeDatabase().mimeTypeForFile(source).preferredSuffix();
    const QString target = dir + u"/image."_s + (suffix.isEmpty() ? u"png"_s : suffix);
    if (!QFileInfo::exists(target))
        QFile::copy(source, target);
    return QDesktopServices::openUrl(QUrl::fromLocalFile(target));
}

QVariantMap AppLibrary::importFiles(const QList<QUrl> &files)
{
    QStringList paths;
    for (const QUrl &url : files)
        paths << url.toLocalFile();
    const bool special = m_key == u"all" || m_key == u"notes" || m_key == u"trash" || m_key == u"attachments"
        || m_key.startsWith(u"tag:") || m_key.startsWith(u"smart:") || searching();
    const ImportReport report = onotes::importFiles(*m_service, paths, special ? QString() : m_key);
    if (special) {
        m_searchText.clear();
        emit searchTextChanged();
        m_key = u"all"_s;
        emit viewChanged();
    }
    refresh();
    QVariantMap out = result(!report.createdIds.isEmpty(), report.warnings.join(u'\n'));
    out.insert(u"count"_s, report.createdIds.size());
    out.insert(u"first"_s, report.createdIds.value(0));
    out.insert(u"warnings"_s, report.warnings);
    return out;
}

QVariantMap AppLibrary::importFolder(const QUrl &folder)
{
    QString folderId;
    const ImportReport report = onotes::importFolder(*m_service, folder.toLocalFile(), {}, &folderId);
    m_searchText.clear();
    emit searchTextChanged();
    refresh();
    if (!folderId.isEmpty())
        showKey(folderId);
    QVariantMap out = result(!report.createdIds.isEmpty(), report.warnings.join(u'\n'));
    out.insert(u"count"_s, report.createdIds.size());
    out.insert(u"first"_s, report.createdIds.value(0));
    out.insert(u"warnings"_s, report.warnings);
    return out;
}

QString AppLibrary::exportFileName(const QString &noteId, const QString &format) const
{
    const auto note = m_service->loadNote(noteId);
    const QString base = fileNameForTitle(note ? note->body.title() : QString());
    const QString suffix = format == u"html" ? u".html"_s : format == u"pdf" ? u".pdf"_s : u".md"_s;
    return base + suffix;
}

QVariantMap AppLibrary::exportNote(const QString &noteId, const QString &format, const QUrl &file)
{
    const ExportFormat f = format == u"html" ? ExportFormat::Html
        : format == u"pdf"                   ? ExportFormat::Pdf
                                             : ExportFormat::Markdown;
    QString error;
    if (!onotes::exportNote(*m_service, noteId, f, file.toLocalFile(), &error))
        return result(false, error);
    QVariantMap out = result(true);
    out.insert(u"path"_s, file.toLocalFile());
    return out;
}

QVariantMap AppLibrary::exportAll(const QUrl &folder)
{
    QString error;
    const auto root = exportLibraryAsMarkdown(*m_service, folder.toLocalFile(), &error);
    if (!root)
        return result(false, error);
    QVariantMap out = result(true);
    out.insert(u"path"_s, *root);
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
