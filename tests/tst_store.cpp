#include "Database.h"
#include "LibraryService.h"
#include "NoteStore.h"
#include "SampleLibrary.h"

#include <QFile>
#include <QProcess>
#include <QRandomGenerator>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include <csignal>

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

RichDocument textDoc(const QString &text)
{
    RichDocument doc;
    doc.blocks = {Block::paragraph({Span::plain(text)})};
    doc.assignMissingIds();
    return doc;
}

} // namespace

class TestStore : public QObject
{
    Q_OBJECT

private slots:
    void createSaveLoad()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QString error;
        QVERIFY2(store.open(&error), qPrintable(error));

        auto note = store.createNote(textDoc(u"First line\nsecond"_s));
        QVERIFY(note);
        QCOMPARE(note->revision, 1);

        SaveRequest save{note->id, note->revision, textDoc(u"Updated title"_s)};
        const SaveResult result = store.saveNote(save);
        QVERIFY2(result.status == SaveResult::Status::Saved, qPrintable(result.error));
        QCOMPARE(result.revision, 2);

        const auto loaded = store.loadNote(note->id);
        QVERIFY(loaded);
        QCOMPARE(loaded->body, save.body);
        QCOMPARE(loaded->revision, 2);

        const auto list = store.listNotes();
        QCOMPARE(list.size(), 1);
        QCOMPARE(list[0].title, u"Updated title"_s);
        QVERIFY(store.verify().isEmpty());
    }

    void staleRevisionIsAConflict()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        auto note = store.createNote(textDoc(u"a"_s));
        QCOMPARE(store.saveNote({note->id, 1, textDoc(u"b"_s)}).status, SaveResult::Status::Saved);
        const SaveResult stale = store.saveNote({note->id, 1, textDoc(u"c"_s)});
        QCOMPARE(stale.status, SaveResult::Status::Conflict);
        QCOMPARE(store.loadNote(note->id)->body.plainText(), u"b"_s);
    }

    void revisionSnapshotsAreThrottled()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        auto note = store.createNote(textDoc(u"a"_s));
        qint64 rev = note->revision;
        for (int i = 0; i < 20; ++i)
            rev = store.saveNote({note->id, rev, textDoc(QString::number(i))}).revision;
        QCOMPARE(store.revisionCount(note->id), 1);
    }

    void attachmentsAndImages()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        auto note = store.createNote(textDoc(u"a"_s));

        const QString source = dir.filePath(u"report.pdf"_s);
        QVERIFY(SampleLibrary::writeSamplePdf(source));
        QString error;
        const auto attachment = store.addAttachment(note->id, source, &error);
        QVERIFY2(attachment, qPrintable(error));
        QCOMPARE(attachment->fileName, u"report.pdf"_s);
        QCOMPARE(attachment->mimeType, u"application/pdf"_s);
        QVERIFY(QFile::exists(store.blobPath(attachment->blobHash)));

        const auto image = store.addImage(SampleLibrary::chartPng());
        QVERIFY(image);
        // Same bytes, same blob.
        QCOMPARE(*store.addImage(SampleLibrary::chartPng()), *image);

        RichDocument body = textDoc(u"with media"_s);
        body.blocks[0].spans << Span::image(*image, 800, 360) << Span::attachment(attachment->id)
                             << Span::image(QString(64, u'0'), 1, 1); // unknown blob
        QCOMPARE(store.saveNote({note->id, note->revision, body}).status, SaveResult::Status::Saved);
        QVERIFY(store.verify().isEmpty());
        QVERIFY(QDir(dir.filePath(u"staging"_s)).isEmpty());
    }

    void rejectsNewerSchema()
    {
        QTemporaryDir dir;
        {
            NoteStore store(LibraryPaths::at(dir.path()));
            QVERIFY(store.open(nullptr));
        }
        {
            Database db;
            QVERIFY(db.open(dir.filePath(u"library.sqlite3"_s), nullptr));
            QVERIFY(db.exec("PRAGMA user_version=99"));
        }
        NoteStore store(LibraryPaths::at(dir.path()));
        QString error;
        QVERIFY(!store.open(&error));
        QVERIFY(error.contains(u"newer version"_s));
    }

    void backup()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        auto note = store.createNote(textDoc(u"keep me"_s));
        QTemporaryDir out;
        QString error;
        QVERIFY2(store.backupDatabase(out.filePath(u"library.sqlite3"_s), &error), qPrintable(error));
        QDir().mkpath(out.filePath(u"blobs"_s));
        NoteStore restored(LibraryPaths::at(out.path()));
        QVERIFY(restored.open(&error));
        QCOMPARE(restored.loadNote(note->id)->body.plainText(), u"keep me"_s);
    }

    void serviceSavesOffThread()
    {
        QTemporaryDir dir;
        LibraryService library(LibraryPaths::at(dir.path()));
        QString error;
        QVERIFY2(library.start(&error), qPrintable(error));
        const QString mixedId = SampleLibrary::seed(library, &error);
        QVERIFY2(!mixedId.isEmpty(), qPrintable(error));
        QCOMPARE(library.listNotes().size(), 3);
        const auto mixed = library.loadNote(mixedId);
        QVERIFY(mixed);
        QCOMPARE(mixed->body.title(), u"Q3 planning — product sync"_s);

        auto resultFor = [&](quint64 ticket) {
            QSignalSpy spy(&library, &LibraryService::saveFinished);
            for (;;) {
                for (const auto &args : spy) {
                    if (args.at(0).toULongLong() == ticket)
                        return args.at(2).value<SaveResult>();
                }
                if (!spy.wait(5000))
                    return SaveResult{};
            }
        };
        library.setSimulatedSaveFailure(true);
        const SaveResult failed = resultFor(library.saveNote({mixedId, mixed->revision, textDoc(u"x"_s)}));
        QCOMPARE(failed.status, SaveResult::Status::Failed);
        QVERIFY(!failed.error.isEmpty());
        library.setSimulatedSaveFailure(false);
        const SaveResult saved = resultFor(library.saveNote({mixedId, mixed->revision, textDoc(u"x"_s)}));
        QCOMPARE(saved.status, SaveResult::Status::Saved);
        QVERIFY(library.verify().isEmpty());
    }

    // Kill the writer with SIGKILL at random points; every save it
    // acknowledged must be present afterwards and the library consistent.
    void acknowledgedSavesSurviveKill()
    {
        QTemporaryDir dir;
        QString noteId;
        {
            NoteStore store(LibraryPaths::at(dir.path()));
            QVERIFY(store.open(nullptr));
            noteId = store.createNote(textDoc(u"0"_s))->id;
        }

        for (int round = 0; round < 8; ++round) {
            QProcess child;
            child.setProcessChannelMode(QProcess::SeparateChannels);
            child.start(QStringLiteral(CRASH_HELPER_PATH), {dir.path(), noteId});
            QVERIFY(child.waitForStarted());

            qint64 lastAck = 0;
            const int acksBeforeKill = 5 + int(QRandomGenerator::global()->bounded(40));
            int acks = 0;
            QByteArray pending;
            while (acks < acksBeforeKill) {
                QVERIFY2(child.waitForReadyRead(5000), child.readAllStandardError().constData());
                pending += child.readAllStandardOutput();
                qsizetype nl;
                while ((nl = pending.indexOf('\n')) >= 0) {
                    const QByteArray line = pending.left(nl);
                    pending.remove(0, nl + 1);
                    if (line.startsWith("ACK ")) {
                        lastAck = line.mid(4).toLongLong();
                        ++acks;
                    }
                }
            }
            // Let it get partway into the next transaction.
            QThread::usleep(QRandomGenerator::global()->bounded(3000));
            ::kill(pid_t(child.processId()), SIGKILL);
            child.waitForFinished();
            // Anything acknowledged after our last read also counts.
            pending += child.readAllStandardOutput();
            for (const QByteArray &line : pending.split('\n')) {
                if (line.startsWith("ACK "))
                    lastAck = line.mid(4).toLongLong();
            }

            NoteStore store(LibraryPaths::at(dir.path()));
            QString error;
            QVERIFY2(store.open(&error), qPrintable(error));
            const auto note = store.loadNote(noteId, &error);
            QVERIFY2(note, qPrintable(error));
            const qint64 stored = note->body.blocks.value(1).plainText().toLongLong();
            QVERIFY2(stored >= lastAck,
                     qPrintable(u"round %1: stored %2 < acknowledged %3"_s.arg(round).arg(stored).arg(lastAck)));
            const QStringList problems = store.verify();
            QVERIFY2(problems.isEmpty(), qPrintable(problems.join(u'\n')));
        }
    }
};

QTEST_MAIN(TestStore)
#include "tst_store.moc"
