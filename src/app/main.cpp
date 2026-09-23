#include "AppLibrary.h"
#include "LibraryService.h"
#include "SampleLibrary.h"
#include "ThemeController.h"

#include <QCommandLineParser>
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
    const QCommandLineOption samples(u"samples"_s, u"Add sample notes if the library is empty."_s);
    const QCommandLineOption failSaves(u"simulate-save-failure"_s,
                                       u"Make saves fail (for testing failure handling)."_s);
    parser.addOptions({dataDir, newNote, openNote, samples, failSaves});
    parser.process(app);

    const onotes::LibraryPaths paths = parser.isSet(dataDir)
        ? onotes::LibraryPaths::at(parser.value(dataDir))
        : onotes::LibraryPaths::standard();

    // One process owns a library. Activating the running instance instead
    // (D-Bus) comes with the desktop integration milestone.
    QDir().mkpath(paths.root);
    QLockFile lock(paths.root + u"/.lock"_s);
    if (!lock.tryLock(200)) {
        std::fprintf(stderr, "Omarchy Notes is already running for %s\n", qPrintable(paths.root));
        return 1;
    }

    onotes::LibraryService service(paths);
    QString error;
    if (!service.start(&error)) {
        std::fprintf(stderr, "Could not open the notes library: %s\n", qPrintable(error));
        return 1;
    }
    if (parser.isSet(samples) && service.listNotes().isEmpty()) {
        if (onotes::SampleLibrary::seed(service, &error).isEmpty())
            std::fprintf(stderr, "Could not add sample notes: %s\n", qPrintable(error));
    }
    service.setSimulatedSaveFailure(parser.isSet(failSaves));

    ThemeController theme;
    ThemeController::setInstance(&theme);
    AppLibrary library(&service);
    AppLibrary::setInstance(&library);

    QString initialNote = parser.value(openNote);
    if (parser.isSet(newNote))
        initialNote = library.createNote();

    QQuickStyle::setStyle(u"Basic"_s);
    QQmlApplicationEngine engine;
    engine.setInitialProperties({{u"initialNoteId"_s, initialNote}});
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("OmarchyNotes", "Main");
    return app.exec();
}
