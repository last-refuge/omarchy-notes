#include "AppController.h"

#include "AppLibrary.h"
#include "DocumentConverter.h"

#include <QProcess>
#include <QRegularExpression>
#include <QQmlEngine>
#include <QWindow>

using namespace Qt::StringLiterals;

namespace {
AppController *s_instance = nullptr;
}

AppController::AppController(QObject *parent) : QObject(parent) {}

AppController *AppController::create(QQmlEngine *, QJSEngine *)
{
    Q_ASSERT(s_instance);
    QQmlEngine::setObjectOwnership(s_instance, QQmlEngine::CppOwnership);
    return s_instance;
}

void AppController::setInstance(AppController *instance)
{
    s_instance = instance;
}

void AppController::activate(QWindow *window, const QString &token)
{
    if (!window)
        return;
    // Qt's Wayland client hands XDG_ACTIVATION_TOKEN to the compositor when
    // activating, which is what lets another launch focus this window.
    if (!token.isEmpty())
        qputenv("XDG_ACTIVATION_TOKEN", token.toUtf8());
    window->show();
    window->raise();
    window->requestActivate();
    // Without a token, Hyprland ignores activation requests; ask it directly.
    if (token.isEmpty() && qEnvironmentVariableIsSet("HYPRLAND_INSTANCE_SIGNATURE")) {
        QProcess::startDetached(u"hyprctl"_s, {u"dispatch"_s, u"focuswindow"_s,
                                               u"title:^%1$"_s.arg(QRegularExpression::escape(window->title()))});
    }
}

void AppController::handle(const QJsonObject &request)
{
    const QString action = request[u"action"].toString();
    const QString token = request[u"token"].toString();
    if (action == u"new") {
        emit newNoteRequested(token);
    } else if (action == u"quick-note") {
        emit quickNoteRequested(token);
    } else if (action == u"open") {
        emit openNoteRequested(request[u"note"].toString(), token);
    } else if (action == u"capture") {
        // Captured text lands in Notes without disturbing the open window.
        AppLibrary *library = qobject_cast<AppLibrary *>(AppLibrary::create(nullptr, nullptr));
        const QString id = library ? library->captureText(request[u"text"].toString()) : QString();
        if (!id.isEmpty())
            emit captured(id);
    } else {
        emit activateRequested(token);
    }
}
