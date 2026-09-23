#pragma once

#include <QJsonObject>
#include <QObject>
#include <QWindow>
#include <QtQml/qqmlregistration.h>

class QQmlEngine;
class QJSEngine;

// Process-level requests for QML (`App`): what another launch asked for,
// and bringing windows forward on Wayland.
class AppController : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(App)
    QML_SINGLETON

    Q_PROPERTY(bool startHidden READ startHidden CONSTANT)
    Q_PROPERTY(bool startWithQuickNote READ startWithQuickNote CONSTANT)

public:
    // Not default-constructible on purpose: QML must use create(), which
    // returns the one shared instance, rather than making its own.
    explicit AppController(QObject *parent);

    static AppController *create(QQmlEngine *, QJSEngine *);
    static void setInstance(AppController *instance);

    bool startHidden() const { return m_startWithQuickNote; }
    bool startWithQuickNote() const { return m_startWithQuickNote; }
    void setStartWithQuickNote(bool quickNote) { m_startWithQuickNote = quickNote; }

    // Shows and focuses `window`, using the launcher's activation token
    // when there is one.
    Q_INVOKABLE void activate(QWindow *window, const QString &token = {});

    // Routes a request from InstanceChannel.
    void handle(const QJsonObject &request);

signals:
    void activateRequested(const QString &token);
    void newNoteRequested(const QString &token);
    void quickNoteRequested(const QString &token);
    void openNoteRequested(const QString &noteId, const QString &token);
    void captured(const QString &noteId);

private:
    bool m_startWithQuickNote = false;
};
