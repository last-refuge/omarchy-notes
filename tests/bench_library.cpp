// Performance benchmark for the release gates (not part of ctest):
//   bench_library [notes=10000] [attachment-mb=0]
// Builds a realistic library, then measures storage, search and app startup
// against the targets in the plan. Prints a Markdown table.

#include "LibraryService.h"
#include "SampleLibrary.h"

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QGuiApplication>
#include <QProcess>
#include <QRandomGenerator>
#include <QTemporaryDir>

#include <algorithm>
#include <cstdio>

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

const QStringList kWords = {
    u"meeting"_s, u"roadmap"_s, u"benchmark"_s, u"budget"_s, u"customer"_s, u"release"_s, u"design"_s,
    u"search"_s, u"notes"_s, u"travel"_s, u"groceries"_s, u"invoice"_s, u"planning"_s, u"research"_s,
    u"interview"_s, u"recipe"_s, u"workout"_s, u"project"_s, u"feedback"_s, u"quarterly"_s, u"launch"_s,
    u"migration"_s, u"sqlite"_s, u"wayland"_s, u"hyprland"_s, u"omarchy"_s, u"lisbon"_s, u"kyoto"_s,
};

QString sentence(QRandomGenerator &rng, int words)
{
    QStringList out;
    for (int i = 0; i < words; ++i)
        out << kWords[rng.bounded(int(kWords.size()))];
    out[0][0] = out[0][0].toUpper();
    return out.join(u' ') + u'.';
}

RichDocument note(QRandomGenerator &rng, int index)
{
    RichDocument doc;
    doc.blocks << Block::heading(1, {Span::plain(sentence(rng, 3).chopped(1) + u' ' + QString::number(index))});
    const int paragraphs = 3 + rng.bounded(12);
    for (int p = 0; p < paragraphs; ++p)
        doc.blocks << Block::paragraph({Span::plain(sentence(rng, 12 + rng.bounded(30)))});
    if (index % 5 == 0) {
        for (int i = 0; i < 4; ++i)
            doc.blocks << Block::listItem(ListKind::Check, 0, {Span::plain(sentence(rng, 5))}, i % 2);
    }
    if (index % 7 == 0)
        doc.blocks << Block::paragraph({Span::plain(u"#"_s + kWords[rng.bounded(int(kWords.size()))])});
    doc.assignMissingIds();
    return doc;
}

qint64 percentile(QList<qint64> values, double p)
{
    std::sort(values.begin(), values.end());
    return values.isEmpty() ? 0 : values[std::min<qsizetype>(values.size() - 1, qsizetype(p * values.size()))];
}

qint64 firstFrameMs(const QString &app, const QString &library)
{
    QProcess process;
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(u"ONOTES_EXIT_AFTER_FIRST_FRAME"_s, u"1"_s);
    process.setProcessEnvironment(env);
    QElapsedTimer timer;
    timer.start();
    process.start(app, {u"--data-dir"_s, library});
    if (!process.waitForFinished(60000))
        return -1;
    return process.readAllStandardOutput().contains("first-frame") ? timer.elapsed() : -1;
}

} // namespace

int main(int argc, char **argv)
{
    QGuiApplication app(argc, argv);
    const int noteCount = argc > 1 ? QString::fromLocal8Bit(argv[1]).toInt() : 10000;
    const int attachmentMb = argc > 2 ? QString::fromLocal8Bit(argv[2]).toInt() : 0;
    QTemporaryDir dir;
    const QString libraryPath = dir.filePath(u"library"_s);
    QRandomGenerator rng(20260923);

    QElapsedTimer timer;
    {
        LibraryService library(LibraryPaths::at(libraryPath));
        library.start(nullptr);
        timer.start();
        for (int i = 0; i < noteCount; ++i)
            library.createNote(note(rng, i));
        std::fprintf(stderr, "created %d notes in %lld ms\n", noteCount, static_cast<long long>(timer.elapsed()));
        // Attachments, if requested: 1 MB images spread across notes.
        const auto notes = library.listNotes();
        QByteArray blob(1024 * 1024, '\0');
        for (int i = 0; i < attachmentMb; ++i) {
            for (int b = 0; b < blob.size(); b += 4096)
                blob[b] = char(rng.bounded(256));
            const auto hash = library.addImage(blob);
            if (!hash)
                break;
            SaveRequest save;
            const auto record = library.loadNote(notes[i % notes.size()].id);
            save.noteId = record->id;
            save.baseRevision = record->revision;
            save.body = record->body;
            save.body.blocks << Block::paragraph({Span::image(*hash, 64, 64)});
            library.saveNote(save);
        }
        library.waitForIdle();
    }

    qint64 openMs = 0, listMs = 0, sidebarMs = 0, listed = 0;
    QList<qint64> searches;
    {
        LibraryService library(LibraryPaths::at(libraryPath));
        timer.restart();
        library.start(nullptr);
        openMs = timer.restart();
        listed = library.listNotes().size();
        listMs = timer.restart();
        library.listTags();
        library.listFolders();
        library.listSmartFolders();
        library.listAttachmentItems();
        sidebarMs = timer.restart();

        for (int i = 0; i < 200; ++i) {
            QStringList terms;
            for (int t = 0; t < 1 + rng.bounded(3); ++t)
                terms << kWords[rng.bounded(int(kWords.size()))].left(3 + rng.bounded(4));
            QElapsedTimer one;
            one.start();
            QEventLoop loop;
            quint64 ticket = 0;
            QObject::connect(&library, &LibraryService::searchFinished, &loop, [&](quint64 t) {
                if (t == ticket)
                    loop.quit();
            });
            ticket = library.search(terms.join(u' '));
            loop.exec();
            searches << one.elapsed();
        }
    }

    // Launches of the real app on this library, to its first rendered frame.
    const QString binary = QCoreApplication::applicationDirPath() + u"/../src/app/omarchy-notes"_s;
    const qint64 first = firstFrameMs(binary, libraryPath);
    QList<qint64> warm;
    for (int i = 0; i < 5; ++i)
        warm << firstFrameMs(binary, libraryPath);

    std::printf("| Measure | Target | Result |\n|---|---|---|\n");
    std::printf("| Notes in library | 10,000 | %lld |\n", static_cast<long long>(listed));
    std::printf("| Attachment data | 1 GB | %d MB |\n", attachmentMb);
    std::printf("| First launch to first frame | < 1.5 s | %lld ms |\n", static_cast<long long>(first));
    std::printf("| Warm launch to first frame (median of 5) | < 500 ms | %lld ms |\n",
                static_cast<long long>(percentile(warm, 0.5)));
    std::printf("| Open library | | %lld ms |\n", static_cast<long long>(openMs));
    std::printf("| List all notes | | %lld ms |\n", static_cast<long long>(listMs));
    std::printf("| Sidebar data (folders, tags, Smart Folders, attachments) | | %lld ms |\n",
                static_cast<long long>(sidebarMs));
    std::printf("| Search p50 (200 queries) | | %lld ms |\n", static_cast<long long>(percentile(searches, 0.5)));
    std::printf("| Search p95 (200 queries) | < 150 ms | %lld ms |\n", static_cast<long long>(percentile(searches, 0.95)));
    return 0;
}
