#include "AppController.h"
#include "DocumentConverter.h"
#include "AppLibrary.h"
#include "InstanceChannel.h"
#include "BlobImageProvider.h"
#include "LibraryService.h"
#include "SampleLibrary.h"
#include "ThemeController.h"

#include <QCommandLineParser>
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

int main(int argc, char *argv[])
{
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
    const QCommandLineOption failSaves(u"simulate-save-failure"_s,
                                       u"Make saves fail (for testing failure handling)."_s);
    parser.addOptions({dataDir, newNote, openNote, quickNote, capture, samples, failSaves});
    parser.process(app);

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
    if (!lock.tryLock(200)) {
        // Another instance is starting up; give it a moment to listen.
        for (int attempt = 0; attempt < 20; ++attempt) {
            QThread::msleep(150);
            if (delivered())
                return 0;
        }
        std::fprintf(stderr, "Omarchy Notes is running for %s but isn't responding.\n", qPrintable(paths.root));
        return 1;
    }

    onotes::LibraryService service(paths);
    QString error;
    if (!service.start(&error)) {
        std::fprintf(stderr, "Could not open the notes library: %s\n", qPrintable(error));
        return 1;
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
    return app.exec();
}
