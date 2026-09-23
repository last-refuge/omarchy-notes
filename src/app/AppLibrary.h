#pragma once

#include "LibraryService.h"

#include <QAbstractListModel>
#include <QObject>
#include <QtQml/qqmlregistration.h>

class QQmlEngine;
class QJSEngine;

class NotesModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ANONYMOUS

public:
    enum Role {
        IdRole = Qt::UserRole + 1,
        TitleRole,
        SnippetRole,
        DateRole,
        PinnedRole,
        HasAttachmentsRole,
        HasChecklistRole,
    };

    using QAbstractListModel::QAbstractListModel;

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(const QList<onotes::NoteSummary> &notes);
    // Applies a saved note's new title/excerpt without re-querying storage.
    void applySaved(const QString &id, const onotes::RichDocument &body, qint64 updatedAt);
    Q_INVOKABLE int indexOf(const QString &id) const;
    Q_INVOKABLE QString idAt(int row) const;
    Q_INVOKABLE QString titleOf(const QString &id) const;

private:
    QList<onotes::NoteSummary> m_notes;
};

// QML-facing wrapper around the LibraryService: `Library` in QML.
class AppLibrary : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Library)
    QML_SINGLETON

    Q_PROPERTY(NotesModel *notes READ notes CONSTANT)
    Q_PROPERTY(QString location READ location CONSTANT)
    Q_PROPERTY(bool simulateSaveFailure READ simulateSaveFailure WRITE setSimulateSaveFailure NOTIFY simulateSaveFailureChanged)

public:
    explicit AppLibrary(onotes::LibraryService *service, QObject *parent = nullptr);

    static AppLibrary *create(QQmlEngine *, QJSEngine *);
    static void setInstance(AppLibrary *instance);

    onotes::LibraryService *service() const { return m_service; }
    NotesModel *notes() { return &m_notes; }
    QString location() const { return m_service->paths().root; }

    bool simulateSaveFailure() const { return m_service->simulatedSaveFailure(); }
    void setSimulateSaveFailure(bool fail);

    Q_INVOKABLE void refresh();
    // Creates an empty note and returns its id (empty on failure).
    Q_INVOKABLE QString createNote();
    Q_INVOKABLE void setPinned(const QString &id, bool pinned);

signals:
    void simulateSaveFailureChanged();
    void errorOccurred(const QString &message);

private:
    onotes::LibraryService *m_service;
    NotesModel m_notes;
};
