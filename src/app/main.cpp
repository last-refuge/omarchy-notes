#include "AppController.h"
#include "DocumentConverter.h"
#include "HyprlandSetup.h"
#include "AppLibrary.h"
#include "InstanceChannel.h"
#include "BlobImageProvider.h"
#include "LibraryService.h"
#include "SampleLibrary.h"
#include "ThemeController.h"

#include <QCommandLineParser>
#include <QElapsedTimer>
#include <QQuickWindow>
#include <QFile>
#include <QJsonObject>
#include <QThread>
#include <QDir>
#include <QGuiApplication>
#include <QLockFile>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QtQml/QQmlExtensionPlugin>

#include <cstdio>

Q_IMPORT_QML_PLUGIN(OmarchyNotesPlugin)

using namespace Qt::StringLiterals;

namespace {

// Test and benchmark hook: report the first rendered frame, then quit.
void exitAfterFirstFrame(QQmlApplicationEngine &engine, const QElapsedTimer &sinceStart)
{
    if (!qEnvironmentVariableIsSet("ONOTES_EXIT_AFTER_FIRST_FRAME") || engine.rootObjects().isEmpty())
        return;
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
    if (!window)
        return;
    QObject::connect(window, &QQuickWindow::frameSwapped, window, [window, sinceStart] {
        std::printf("first-frame %lld ms: %s\n", static_cast<long long>(sinceStart.elapsed()),
                    qPrintable(window->title()));
        std::fflush(stdout);
        QCoreApplication::quit();
    }, Qt::SingleShotConnection);
}

// Shows why the library can't be used, in a window, since a launch from the
// app launcher has no terminal to print to.
int showStartupError(QGuiApplication &app, const QString &heading, const QString &message,
                     const QString &location, const QElapsedTimer &sinceStart)
{
    std::fprintf(stderr, "%s %s\n", qPrintable(heading), qPrintable(message));
    ThemeController theme(nullptr);
    ThemeController::setInstance(&theme);
    QQuickStyle::setStyle(u"Basic"_s);
    QQmlApplicationEngine engine;
    engine.setInitialProperties({{u"heading"_s, heading}, {u"message"_s, message}, {u"location"_s, location}});
    engine.loadFromModule("OmarchyNotes", "StartupError");
    exitAfterFirstFrame(engine, sinceStart);
    app.exec();
    return 1;
}

} // namespace

int main(int argc, char *argv[])
{
    QElapsedTimer sinceStart;
    sinceStart.start();
    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName(u"omarchy-notes"_s);
    QGuiApplication::setApplicationDisplayName(u"Omarchy Notes"_s);
    QGuiApplication::setApplicationVersion(QStringLiteral(ONOTES_VERSION));
    QGuiApplication::setDesktopFileName(QStringLiteral(ONOTES_APP_ID));

    QCommandLineParser parser;
    parser.setApplicationDescription(u"Notes for Omarchy"_s);
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption dataDir(u"data-dir"_s, u"Use the library in <dir>."_s, u"dir"_s);
    const QCommandLineOption newNote(u"new"_s, u"Start with a new note."_s);
    const QCommandLineOption openNote(u"note"_s, u"Open the note with <id>."_s, u"id"_s);
    const QCommandLineOption quickNote(u"quick-note"_s, u"Open a Quick Note window."_s);
    const QCommandLineOption capture(u"capture"_s, u"Save <text> as a new note (\"-\" reads standard input)."_s,
                                     u"text"_s);
    const QCommandLineOption samples(u"samples"_s, u"Add sample notes if the library is empty."_s);
    const QCommandLineOption setupHyprland(u"setup-hyprland"_s,
                                           u"Add a Quick Note key (Super+Alt+N) and floating rule to Hyprland."_s);
    const QCommandLineOption failSaves(u"simulate-save-failure"_s,
                                       u"Make saves fail (for testing failure handling)."_s);
    parser.addOptions({dataDir, newNote, openNote, quickNote, capture, samples, setupHyprland, failSaves});
    parser.process(app);

    if (parser.isSet(setupHyprland)) {
        const HyprlandSetupResult setup = setUpHyprland();
        for (const QString &line : setup.messages)
            std::printf("%s\n", qPrintable(line));
        return setup.ok ? 0 : 1;
    }

    const onotes::LibraryPaths paths = parser.isSet(dataDir)
        ? onotes::LibraryPaths::at(parser.value(dataDir))
        : onotes::LibraryPaths::standard();

    // What this launch is asking for.
    QString captured;
    if (parser.isSet(capture)) {
        captured = parser.value(capture);
        if (captured == u"-") {
            QFile in;
            if (in.open(stdin, QIODevice::ReadOnly))
                captured = QString::fromUtf8(in.readAll());
        }
        if (captured.trimmed().isEmpty()) {
            std::fprintf(stderr, "Nothing to capture.\n");
            return 1;
        }
    }
    QJsonObject request{{u"token"_s, qEnvironmentVariable("XDG_ACTIVATION_TOKEN")}};
    if (parser.isSet(capture))
        request[u"action"_s] = u"capture"_s, request[u"text"_s] = captured;
    else if (parser.isSet(quickNote))
        request[u"action"_s] = u"quick-note"_s;
    else if (parser.isSet(newNote))
        request[u"action"_s] = u"new"_s;
    else if (parser.isSet(openNote))
        request[u"action"_s] = u"open"_s, request[u"note"_s] = parser.value(openNote);
    else
        request[u"action"_s] = u"activate"_s;

    // One process owns a library: hand the request to it if it's running.
    const QString socket = InstanceChannel::socketPath(paths);
    auto delivered = [&] {
        if (!InstanceChannel::send(socket, request))
            return false;
        if (parser.isSet(capture))
            std::printf("Saved to Omarchy Notes.\n");
        return true;
    };
    if (delivered())
        return 0;
    QDir().mkpath(paths.root);
    QLockFile lock(paths.root + u"/.lock"_s);
    const bool locked = lock.tryLock(200);
    if (!locked && lock.error() != QLockFile::LockFailedError) {
        // Not "someone else has it": the folder itself is unusable.
        const QString why = QObject::tr("Omarchy Notes can't write to its library at %1. Check that the folder "
                                        "and its files belong to you and aren't read-only.").arg(paths.root);
        if (parser.isSet(capture)) {
            std::fprintf(stderr, "%s\n", qPrintable(why));
            return 1;
        }
        return showStartupError(app, QObject::tr("Omarchy Notes can't open your notes."), why, paths.root,
                                sinceStart);
    }
    if (!locked) {
        // Another instance is starting up; give it a moment to listen.
        for (int attempt = 0; attempt < 20; ++attempt) {
            QThread::msleep(150);
            if (delivered())
                return 0;
        }
        if (parser.isSet(capture)) {
            std::fprintf(stderr, "Omarchy Notes is running for %s but isn't responding.\n", qPrintable(paths.root));
            return 1;
        }
        return showStartupError(app, QObject::tr("Omarchy Notes is already running but isn't responding."),
                                QObject::tr("Close the other Omarchy Notes window, or end it from a terminal "
                                            "with \u201cpkill omarchy-notes\u201d, then open it again."),
                                paths.root, sinceStart);
    }

    onotes::LibraryService service(paths);
    QString error;
    if (!service.start(&error)) {
        if (parser.isSet(capture)) {
            std::fprintf(stderr, "Could not open the notes library: %s\n", qPrintable(error));
            return 1;
        }
        return showStartupError(app, QObject::tr("Omarchy Notes can't open your notes."), error, paths.root,
                                sinceStart);
    }
    // Capturing without a running app needs no window.
    if (parser.isSet(capture)) {
        const auto note = service.createNote(onotes::DocumentConverter::fromPlainText(captured.trimmed()), &error);
        if (!note) {
            std::fprintf(stderr, "Could not save the note: %s\n", qPrintable(error));
            return 1;
        }
        std::printf("Saved to Omarchy Notes.\n");
        return 0;
    }
    if (parser.isSet(samples) && service.listNotes().isEmpty()) {
        if (onotes::SampleLibrary::seed(service, &error).isEmpty())
            std::fprintf(stderr, "Could not add sample notes: %s\n", qPrintable(error));
    }
    service.setSimulatedSaveFailure(parser.isSet(failSaves));

    ThemeController theme(nullptr);
    ThemeController::setInstance(&theme);
    AppLibrary library(&service);
    AppLibrary::setInstance(&library);
    AppController controller(nullptr);
    controller.setStartWithQuickNote(parser.isSet(quickNote));
    AppController::setInstance(&controller);
    InstanceChannel channel;
    QString listenError;
    if (!channel.listen(socket, &listenError))
        std::fprintf(stderr, "Other launches won't reach this window: %s\n", qPrintable(listenError));
    QObject::connect(&channel, &InstanceChannel::requestReceived, &controller, &AppController::handle);

    QString initialNote = parser.value(openNote);
    if (parser.isSet(newNote))
        initialNote = library.createNote();

    QQuickStyle::setStyle(u"Basic"_s);
    QQmlApplicationEngine engine;
    engine.addImageProvider(u"blob"_s, new BlobImageProvider(&service));
    engine.setInitialProperties({{u"initialNoteId"_s, initialNote}});
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("OmarchyNotes", "Main");
    exitAfterFirstFrame(engine, sinceStart);
    return app.exec();
}
