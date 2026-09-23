// Saves a note in a tight loop and prints "ACK <counter>" only after each
// commit returns. The parent test kills this process with SIGKILL at random
// points and checks that every acknowledged save survived.

#include "NoteStore.h"

#include <QCoreApplication>
#include <QTextStream>

#include <cstdio>

using namespace onotes;
using namespace Qt::StringLiterals;

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    if (argc < 3)
        return 2;
    NoteStore store(LibraryPaths::at(QString::fromLocal8Bit(argv[1])));
    QString error;
    if (!store.open(&error)) {
        std::fprintf(stderr, "open failed: %s\n", qPrintable(error));
        return 1;
    }
    const QString noteId = QString::fromLocal8Bit(argv[2]);
    auto note = store.loadNote(noteId, &error);
    if (!note) {
        std::fprintf(stderr, "load failed: %s\n", qPrintable(error));
        return 1;
    }
    qint64 revision = note->revision;
    const QString filler = u"Lorem ipsum dolor sit amet, consectetur adipiscing elit. "_s.repeated(40);

    for (qint64 counter = 1;; ++counter) {
        SaveRequest request;
        request.noteId = noteId;
        request.baseRevision = revision;
        request.body.blocks = {
            Block::heading(1, {Span::plain(u"Counter"_s)}),
            Block::paragraph({Span::plain(QString::number(counter))}),
            Block::paragraph({Span::plain(filler)}),
        };
        request.body.assignMissingIds();
        const SaveResult result = store.saveNote(request);
        if (result.status != SaveResult::Status::Saved) {
            std::fprintf(stderr, "save failed: %s\n", qPrintable(result.error));
            return 1;
        }
        revision = result.revision;
        std::printf("ACK %lld\n", static_cast<long long>(counter));
        std::fflush(stdout);
    }
}
