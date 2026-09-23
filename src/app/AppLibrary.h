#pragma once

#include "LibraryService.h"

#include <QAbstractListModel>
#include <QObject>
#include <QSettings>
#include <QTimer>
#include <QUrl>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>

#include <memory>

class QQmlEngine;
class QJSEngine;

class NotesModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ANONYMOUS

    Q_PROPERTY(int count READ count NOTIFY countChanged)
    Q_PROPERTY(bool hasPinned READ hasPinned NOTIFY countChanged)

public:
    enum Role {
        IdRole = Qt::UserRole + 1,
        TitleRole,
        SnippetRole,
        DateRole,
        PinnedRole,
        HasAttachmentsRole,
        HasChecklistRole,
        FolderNameRole,
        SectionRole,
    };

    using QAbstractListModel::QAbstractListModel;

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    int count() const { return int(m_notes.size()); }
    bool hasPinned() const;

    // `searchResults` switches the snippet to highlighted matches; `trash`
    // shows the deletion date.
    void reset(const QList<onotes::NoteSummary> &notes, bool searchResults, bool trash,
               const QHash<QString, QString> &folderNames);
    // Applies a saved note's new title/excerpt without re-querying storage.
    void applySaved(const QString &id, const onotes::RichDocument &body, qint64 updatedAt, bool moveToTop);
    Q_INVOKABLE int indexOf(const QString &id) const;
    Q_INVOKABLE QString idAt(int row) const;
    Q_INVOKABLE QString titleOf(const QString &id) const;
    Q_INVOKABLE bool isPinned(const QString &id) const;

signals:
    void countChanged();

private:
    QList<onotes::NoteSummary> m_notes;
    QHash<QString, QString> m_folderNames;
    bool m_search = false;
    bool m_trash = false;
};

// Sidebar rows: All Notes, Notes, the folder tree (flattened, with depth),
// and Recently Deleted.
class FoldersModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ANONYMOUS

public:
    enum Role {
        KeyRole = Qt::UserRole + 1,
        NameRole,
        KindRole,
        DepthRole,
        CountRole,
        FolderIdRole,
        ParentIdRole,
    };

    struct Row {
        QString key; // "all", "notes", "trash", or a folder id
        QString name;
        QString kind; // "all", "notes", "folder", "trash"
        int depth = 0;
        int count = 0;
        QString folderId;
        QString parentId;
    };

    using QAbstractListModel::QAbstractListModel;

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(const QList<onotes::FolderInfo> &folders, int noteCount, int trashCount);
    const QList<Row> &rows() const { return m_rows; }
    QString nameOf(const QString &key) const;
    Q_INVOKABLE int indexOfKey(const QString &key) const;

private:
    QList<Row> m_rows;
};

// QML-facing wrapper around the LibraryService: `Library` in QML. It owns
// what the note list shows (a folder, all notes, the trash, or search
// results) and every library-level action.
class AppLibrary : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Library)
    QML_SINGLETON

    Q_PROPERTY(NotesModel *notes READ notes CONSTANT)
    Q_PROPERTY(FoldersModel *folders READ folders CONSTANT)
    Q_PROPERTY(QString currentKey READ currentKey NOTIFY viewChanged)
    Q_PROPERTY(QString viewTitle READ viewTitle NOTIFY viewChanged)
    Q_PROPERTY(bool inTrash READ inTrash NOTIFY viewChanged)
    Q_PROPERTY(QString searchText READ searchText WRITE setSearchText NOTIFY searchTextChanged)
    Q_PROPERTY(bool searching READ searching NOTIFY searchTextChanged)
    Q_PROPERTY(int sortOrder READ sortOrder WRITE setSortOrder NOTIFY sortOrderChanged)
    Q_PROPERTY(int trashCount READ trashCount NOTIFY countsChanged)
    Q_PROPERTY(QString location READ location CONSTANT)
    Q_PROPERTY(QString lastNoteId READ lastNoteId WRITE setLastNoteId NOTIFY lastNoteIdChanged)
    Q_PROPERTY(bool simulateSaveFailure READ simulateSaveFailure WRITE setSimulateSaveFailure NOTIFY simulateSaveFailureChanged)

public:
    enum SortOrder { SortEdited = int(onotes::NoteSort::Edited), SortCreated, SortTitle };
    Q_ENUM(SortOrder)

    // `settingsFile` stores view preferences; empty uses
    // $XDG_CONFIG_HOME/omarchy-notes/settings.ini.
    explicit AppLibrary(onotes::LibraryService *service, const QString &settingsFile = {},
                        QObject *parent = nullptr);

    static AppLibrary *create(QQmlEngine *, QJSEngine *);
    static void setInstance(AppLibrary *instance);

    onotes::LibraryService *service() const { return m_service; }
    NotesModel *notes() { return &m_notes; }
    FoldersModel *folders() { return &m_folders; }
    QString currentKey() const { return m_key; }
    QString viewTitle() const;
    bool inTrash() const { return m_key == u"trash" && m_searchText.isEmpty(); }
    QString searchText() const { return m_searchText; }
    void setSearchText(const QString &text);
    bool searching() const { return !m_searchText.trimmed().isEmpty(); }
    int sortOrder() const { return int(m_sort); }
    void setSortOrder(int order);
    int trashCount() const { return m_trashCount; }
    QString location() const { return m_service->paths().root; }
    QString lastNoteId() const;
    void setLastNoteId(const QString &id);

    bool simulateSaveFailure() const { return m_service->simulatedSaveFailure(); }
    void setSimulateSaveFailure(bool fail);

    // Called by the editor after an acknowledged save.
    void noteSaved(const QString &noteId, const onotes::RichDocument &body, qint64 updatedAt);

    // Views: "all", "notes", "trash", or a folder id.
    Q_INVOKABLE void showKey(const QString &key);
    Q_INVOKABLE void refresh();

    // Notes. createNote files the note in the folder being viewed.
    Q_INVOKABLE QString createNote();
    Q_INVOKABLE void setPinned(const QString &id, bool pinned);
    Q_INVOKABLE bool moveNote(const QString &noteId, const QString &folderId);
    Q_INVOKABLE QString duplicateNote(const QString &noteId);
    Q_INVOKABLE bool trashNote(const QString &noteId);
    Q_INVOKABLE bool recoverNote(const QString &noteId);
    Q_INVOKABLE bool deleteNotePermanently(const QString &noteId);
    Q_INVOKABLE int emptyTrash();

    // Folders. Results are {ok, id, error} maps so dialogs can show errors inline.
    Q_INVOKABLE QVariantMap createFolder(const QString &name, const QString &parentId);
    Q_INVOKABLE QVariantMap renameFolder(const QString &id, const QString &name);
    Q_INVOKABLE bool deleteFolder(const QString &id);
    Q_INVOKABLE QString folderName(const QString &id) const;
    Q_INVOKABLE int folderNoteCount(const QString &id) const;
    // Destinations for "Move to": [{id, name, depth}], "Notes" first (id "").
    Q_INVOKABLE QVariantList folderChoices() const;

    // Backup. Results are maps with ok/error plus details.
    Q_INVOKABLE QString defaultBackupName() const;
    Q_INVOKABLE QVariantMap backupTo(const QUrl &parentFolder, const QString &name);
    Q_INVOKABLE QVariantMap inspectBackup(const QUrl &folder);
    // Close the open note (after flushing it) before restoring: its id may
    // not exist in the backup. libraryReplaced() follows a successful restore.
    Q_INVOKABLE QVariantMap restoreFrom(const QUrl &folder);
    Q_INVOKABLE void openLibraryFolder() const;

signals:
    void viewChanged();
    void searchTextChanged();
    void sortOrderChanged();
    void countsChanged();
    void lastNoteIdChanged();
    void simulateSaveFailureChanged();
    // A note's pinned/deleted state changed, or it was removed.
    void noteMetaChanged(const QString &noteId);
    // The whole library was replaced (restore); reopen from scratch.
    void libraryReplaced();
    void errorOccurred(const QString &message);

private:
    void reloadNotes();
    void reloadFolders();
    void runSearch();
    void onSearchFinished(quint64 ticket, const QList<onotes::NoteSummary> &results);
    QHash<QString, QString> folderNames() const;
    bool report(bool ok, const QString &error);

    onotes::LibraryService *m_service;
    std::unique_ptr<QSettings> m_settings;
    NotesModel m_notes;
    FoldersModel m_folders;
    QString m_key = QStringLiteral("all");
    QString m_searchText;
    QTimer m_searchDebounce;
    quint64 m_searchTicket = 0;
    onotes::NoteSort m_sort = onotes::NoteSort::Edited;
    int m_trashCount = 0;
};
