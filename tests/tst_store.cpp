#include "Database.h"
#include "LibraryBackup.h"
#include "LibraryService.h"
#include "NoteStore.h"
#include "SampleLibrary.h"

#include <QDateTime>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
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

QStringList titles(const QList<NoteSummary> &notes)
{
    QStringList out;
    for (const NoteSummary &n : notes)
        out << n.title;
    return out;
}

constexpr qint64 kFarFuture = qint64(4102444800000); // 2100-01-01

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

    void foldersAndMoves()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        QString error;
        const auto work = store.createFolder(u"  Work  "_s, {}, &error);
        QVERIFY2(work, qPrintable(error));
        QCOMPARE(work->name, u"Work"_s);
        const auto clients = store.createFolder(u"Clients"_s, work->id);
        QVERIFY(clients);
        QVERIFY(!store.createFolder(u"work"_s, {}, &error)); // siblings must differ
        QVERIFY(error.contains(u"already"_s));
        QVERIFY(store.createFolder(u"Clients"_s)); // same name elsewhere is fine
        QVERIFY(!store.createFolder(u"   "_s, {}, &error));

        const auto loose = store.createNote(textDoc(u"Loose"_s));
        const auto filed = store.createNote(textDoc(u"Filed"_s), nullptr, clients->id);
        QVERIFY(loose && filed);
        QCOMPARE(titles(store.listNotes(NoteQuery::folder({}))), QStringList{u"Loose"_s});
        QCOMPARE(titles(store.listNotes(NoteQuery::folder(clients->id))), QStringList{u"Filed"_s});
        QCOMPARE(store.listNotes().size(), 2);

        QVERIFY(store.moveNote(loose->id, work->id));
        QCOMPARE(store.listNotes(NoteQuery::folder({})).size(), 0);
        QVERIFY(store.renameFolder(work->id, u"Office"_s));
        QVERIFY(!store.renameFolder(clients->id, u"   "_s, &error));

        int workCount = -1;
        for (const FolderInfo &f : store.listFolders()) {
            if (f.id == work->id) {
                QCOMPARE(f.name, u"Office"_s);
                workCount = f.noteCount;
            }
            if (f.id == clients->id)
                QCOMPARE(f.parentId, work->id);
        }
        QCOMPARE(workCount, 1);
        QVERIFY(!store.moveNote(loose->id, u"8d1c3f0e-6b8a-4f7e-9d2c-1a2b3c4d5e6f"_s, &error));
    }

    void sortingKeepsPinnedFirst()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        const auto banana = store.createNote(textDoc(u"banana"_s));
        QTest::qWait(5);
        const auto apple = store.createNote(textDoc(u"Apple"_s));
        QTest::qWait(5);
        const auto cherry = store.createNote(textDoc(u"cherry"_s));
        QTest::qWait(5);
        // Editing banana makes it the most recently edited.
        QVERIFY(store.saveNote({banana->id, banana->revision, textDoc(u"banana split"_s)}).status
                == SaveResult::Status::Saved);

        QCOMPARE(titles(store.listNotes(NoteQuery::all(NoteSort::Edited))),
                 (QStringList{u"banana split"_s, u"cherry"_s, u"Apple"_s}));
        QCOMPARE(titles(store.listNotes(NoteQuery::all(NoteSort::Created))),
                 (QStringList{u"cherry"_s, u"Apple"_s, u"banana split"_s}));
        QCOMPARE(titles(store.listNotes(NoteQuery::all(NoteSort::Title))),
                 (QStringList{u"Apple"_s, u"banana split"_s, u"cherry"_s}));
        QVERIFY(store.setPinned(cherry->id, true));
        QCOMPARE(titles(store.listNotes(NoteQuery::all(NoteSort::Title))),
                 (QStringList{u"cherry"_s, u"Apple"_s, u"banana split"_s}));
        Q_UNUSED(apple);
    }

    void trashLifecycle()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        const auto folder = store.createFolder(u"Ideas"_s);
        const auto note = store.createNote(textDoc(u"Idea"_s), nullptr, folder->id);
        const auto other = store.createNote(textDoc(u"Other"_s));

        QString error;
        QVERIFY(!store.deleteNotePermanently(note->id, &error)); // must be in the trash first
        QVERIFY(store.trashNote(note->id));
        QVERIFY(!store.trashNote(note->id));
        QCOMPARE(store.noteCount(), 1);
        QCOMPARE(store.trashCount(), 1);
        const auto trashed = store.listNotes(NoteQuery::trash());
        QCOMPARE(trashed.size(), 1);
        QVERIFY(trashed[0].deletedAt > 0);
        QVERIFY(store.loadNote(note->id)->deletedAt > 0);

        QVERIFY(store.recoverNote(note->id));
        QCOMPARE(titles(store.listNotes(NoteQuery::folder(folder->id))), QStringList{u"Idea"_s});

        QVERIFY(store.trashNote(note->id));
        QVERIFY(store.deleteNotePermanently(note->id));
        QVERIFY(!store.loadNote(note->id));

        QVERIFY(store.trashNote(other->id));
        QCOMPARE(store.purgeExpiredTrash(QDateTime::currentMSecsSinceEpoch()), 0); // not old enough
        QCOMPARE(store.purgeExpiredTrash(kFarFuture), 1);
        QCOMPARE(store.trashCount(), 0);
        QVERIFY(store.verify().isEmpty());
    }

    void deletingAFolderTrashesItsNotes()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        const auto top = store.createFolder(u"Projects"_s);
        const auto sub = store.createFolder(u"Old"_s, top->id);
        const auto deeper = store.createFolder(u"Older"_s, sub->id);
        store.createNote(textDoc(u"A"_s), nullptr, top->id);
        const auto b = store.createNote(textDoc(u"B"_s), nullptr, deeper->id);
        const auto kept = store.createNote(textDoc(u"Kept"_s));

        QVERIFY(store.deleteFolder(top->id));
        QVERIFY(store.listFolders().isEmpty());
        QCOMPARE(titles(store.listNotes()), QStringList{u"Kept"_s});
        QCOMPARE(store.trashCount(), 2);
        // Their folder is gone, so recovered notes land in Notes.
        QVERIFY(store.recoverNote(b->id));
        QCOMPARE(store.loadNote(b->id)->folderId, QString());
        QVERIFY(store.verify().isEmpty());
        Q_UNUSED(kept);
    }

    void searchMatchesPrefixesAndSkipsTrash()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        RichDocument meeting;
        meeting.blocks = {Block::heading(1, {Span::plain(u"Q3 planning"_s)}),
                          Block::paragraph({Span::plain(u"Benchmark search on the reference ThinkPad."_s)})};
        meeting.assignMissingIds();
        store.createNote(meeting);
        store.createNote(textDoc(u"Groceries: lemons, basil, café au lait"_s));
        const auto old = store.createNote(textDoc(u"Benchmark results from last year"_s));
        QVERIFY(store.trashNote(old->id));

        auto hits = store.search(u"bench"_s);
        QCOMPARE(hits.size(), 1);
        QCOMPARE(hits[0].title, u"Q3 planning"_s);
        QVERIFY(hits[0].snippet.contains(QChar(2)) && hits[0].snippet.contains(QChar(3)));
        QCOMPARE(store.search(u"lem GRO"_s).size(), 1);       // every word, any order, any case
        QCOMPARE(store.search(u"cafe"_s).size(), 1);          // diacritics ignored
        QCOMPARE(store.search(u"lemons planning"_s).size(), 0);
        // FTS syntax is treated as plain text, never an error.
        for (const QString &odd : {u"\""_s, u"-"_s, u"*"_s, u"NEAR("_s, u"a OR"_s, u"\"unterminated"_s})
            QVERIFY(store.search(odd).size() >= 0);
        QVERIFY(store.search(u"   "_s).isEmpty());
    }

    void duplicateGetsItsOwnAttachments()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        const auto note = store.createNote(textDoc(u"With file"_s));
        const QString pdf = dir.filePath(u"plan.pdf"_s);
        QVERIFY(SampleLibrary::writeSamplePdf(pdf));
        const auto attachment = store.addAttachment(note->id, pdf);
        RichDocument body = textDoc(u"With file"_s);
        body.blocks[0].spans << Span::attachment(attachment->id);
        QVERIFY(store.saveNote({note->id, note->revision, body}).status == SaveResult::Status::Saved);

        const auto copy = store.duplicateNote(note->id);
        QVERIFY(copy);
        const QString copiedId = copy->body.referencedAttachments().value(0);
        QVERIFY(!copiedId.isEmpty() && copiedId != attachment->id);
        QCOMPARE(store.attachment(copiedId)->blobHash, attachment->blobHash);

        // Deleting the original must not break the copy, even after cleanup.
        QVERIFY(store.trashNote(note->id));
        QVERIFY(store.deleteNotePermanently(note->id));
        store.collectGarbage(kFarFuture);
        QVERIFY(store.attachment(copiedId));
        QVERIFY(QFile::exists(store.blobPath(attachment->blobHash)));
        QVERIFY(store.verify().isEmpty());
    }

    void garbageCollectionKeepsWhatIsReferenced()
    {
        QTemporaryDir dir;
        NoteStore store(LibraryPaths::at(dir.path()));
        QVERIFY(store.open(nullptr));
        const auto used = store.addImage(SampleLibrary::chartPng());
        const auto unused = store.addImage(QByteArray("not really an image"));
        const auto note = store.createNote(textDoc(u"chart"_s));
        RichDocument body = textDoc(u"chart"_s);
        body.blocks[0].spans << Span::image(*used, 800, 360);
        QVERIFY(store.saveNote({note->id, note->revision, body}).status == SaveResult::Status::Saved);
        // An orphan file from an interrupted install.
        QFile orphan(dir.filePath(u"blobs/ff/"_s) + QString(64, u'f'));
        QDir().mkpath(dir.filePath(u"blobs/ff"_s));
        QVERIFY(orphan.open(QIODevice::WriteOnly));
        orphan.write("x");
        orphan.close();

        // Within the grace period nothing is removed.
        QCOMPARE(store.collectGarbage().blobsRemoved, 0);
        const GarbageReport report = store.collectGarbage(kFarFuture);
        QCOMPARE(report.blobsRemoved, 2);
        QVERIFY(QFile::exists(store.blobPath(*used)));
        QVERIFY(!QFile::exists(store.blobPath(*unused)));
        QVERIFY(!orphan.exists());
        QVERIFY(store.verify().isEmpty());
    }

    void backupAndRestore()
    {
        QTemporaryDir dir;
        QTemporaryDir backups;
        LibraryService library(LibraryPaths::at(dir.filePath(u"library"_s)));
        QString error;
        QVERIFY2(library.start(&error), qPrintable(error));
        QVERIFY(!SampleLibrary::seed(library, &error).isEmpty());
        const QStringList before = titles(library.listNotes());

        const QString target = backups.filePath(defaultBackupName());
        const auto manifest = library.backupTo(target, &error);
        QVERIFY2(manifest, qPrintable(error));
        QCOMPARE(manifest->notes, 3);
        QCOMPARE(manifest->blobs, 2); // chart image + PDF
        QVERIFY(!QFileInfo::exists(target + u".partial"_s));
        QVERIFY(library.inspectBackup(target, &error));
        QVERIFY(!library.backupTo(target, &error)); // never overwrites

        // Change things, then restore.
        library.createNote(textDoc(u"Written after the backup"_s));
        library.trashNote(library.listNotes().last().id);
        QString safety;
        QVERIFY2(library.restoreFrom(target, &safety, &error), qPrintable(error));
        QCOMPARE(titles(library.listNotes()), before);
        QCOMPARE(library.trashCount(), 0);
        QVERIFY(library.verify().isEmpty());
        QVERIFY(QFileInfo::exists(safety + u"/library.sqlite3"_s)); // the replaced library is kept
        // The restored library is live: it saves normally.
        const auto note = library.createNote(textDoc(u"After restore"_s));
        QVERIFY(note);
        QVERIFY(library.inspectBackup(target)); // the backup itself is untouched
    }

    void restoreRejectsBadBackups()
    {
        QTemporaryDir dir;
        QTemporaryDir backups;
        LibraryService library(LibraryPaths::at(dir.filePath(u"library"_s)));
        QString error;
        QVERIFY(library.start(&error));
        QVERIFY(!SampleLibrary::seed(library, &error).isEmpty());
        const QString good = backups.filePath(u"good"_s);
        QVERIFY(library.backupTo(good));

        QVERIFY(!library.inspectBackup(backups.path(), &error)); // not a backup
        QVERIFY(error.contains(u"isn't"_s));

        // Remove one attachment from a copy of the backup.
        const QString broken = backups.filePath(u"broken"_s);
        QVERIFY(QDir().mkpath(broken));
        QVERIFY(QProcess::execute(u"cp"_s, {u"-a"_s, good + u"/."_s, broken}) == 0);
        QDirIterator blobs(broken + u"/blobs"_s, QDir::Files, QDirIterator::Subdirectories);
        QVERIFY(blobs.hasNext());
        QVERIFY(QFile::remove(blobs.next()));
        QVERIFY(!library.inspectBackup(broken, &error));
        QVERIFY(error.contains(u"incomplete"_s));

        const QStringList before = titles(library.listNotes());
        QString safety;
        QVERIFY(!library.restoreFrom(broken, &safety, &error));
        QCOMPARE(titles(library.listNotes()), before); // nothing changed
        QVERIFY(safety.isEmpty());
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
