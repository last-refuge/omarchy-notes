// Runs the real binary: headless capture, single-instance hand-off, and
// capture delivered to a running instance.

#include "LibraryService.h"

#include <QProcess>
#include <QTemporaryDir>
#include <QTest>

using namespace onotes;
using namespace Qt::StringLiterals;

class TestLaunch : public QObject
{
    Q_OBJECT

    QTemporaryDir m_dir;

    QProcessEnvironment env() const
    {
        QProcessEnvironment e = QProcessEnvironment::systemEnvironment();
        e.insert(u"QT_QPA_PLATFORM"_s, u"offscreen"_s);
        e.insert(u"QT_QUICK_BACKEND"_s, u"software"_s);
        e.insert(u"XDG_CONFIG_HOME"_s, m_dir.filePath(u"config"_s));
        e.insert(u"XDG_RUNTIME_DIR"_s, m_dir.path());
        e.remove(u"HYPRLAND_INSTANCE_SIGNATURE"_s);
        // Desktop theme plugins (e.g. GTK) need a real display.
        e.remove(u"QT_QPA_PLATFORMTHEME"_s);
        return e;
    }

    int run(const QStringList &args, QByteArray *out = nullptr, const QByteArray &input = {})
    {
        QProcess p;
        p.setProcessEnvironment(env());
        p.start(QStringLiteral(APP_PATH), QStringList{u"--data-dir"_s, m_dir.filePath(u"library"_s)} + args);
        if (!input.isEmpty()) {
            p.write(input);
            p.closeWriteChannel();
        }
        if (!p.waitForFinished(15000))
            return -1;
        if (out)
            *out = p.readAllStandardOutput();
        return p.exitCode();
    }

    QStringList titles()
    {
        LibraryService library(LibraryPaths::at(m_dir.filePath(u"library"_s)));
        QStringList out;
        if (library.start(nullptr)) {
            for (const NoteSummary &n : library.listNotes())
                out << n.title;
        }
        return out;
    }

private slots:
    void captureWithoutARunningApp()
    {
        QByteArray out;
        QCOMPARE(run({u"--capture"_s, u"Buy stamps"_s}, &out), 0);
        QVERIFY(out.contains("Saved"));
        QCOMPARE(run({u"--capture"_s, u"-"_s}, nullptr, "From a pipe\nsecond line\n"), 0);
        QCOMPARE(run({u"--capture"_s, u"   "_s}), 1);
        const QStringList t = titles();
        QVERIFY(t.contains(u"Buy stamps"_s));
        QVERIFY(t.contains(u"From a pipe"_s));
    }

    void secondLaunchHandsOffToTheFirst()
    {
        QProcess app;
        app.setProcessEnvironment(env());
        app.start(QStringLiteral(APP_PATH), {u"--data-dir"_s, m_dir.filePath(u"library"_s)});
        QVERIFY(app.waitForStarted());
        // Wait until it's listening.
        QByteArray out;
        QTRY_COMPARE_WITH_TIMEOUT(run({}, &out), 0, 10000);
        QCOMPARE(app.state(), QProcess::Running); // the first instance kept running

        QCOMPARE(run({u"--capture"_s, u"Delivered to the running app"_s}, &out), 0);
        QVERIFY(out.contains("Saved"));
        QCOMPARE(run({u"--new"_s}), 0);
        QCOMPARE(run({u"--quick-note"_s}), 0);
        QCOMPARE(app.state(), QProcess::Running);

        app.terminate();
        if (!app.waitForFinished(5000)) {
            app.kill();
            app.waitForFinished();
        }
        QVERIFY(titles().contains(u"Delivered to the running app"_s));
    }
};

QTEST_MAIN(TestLaunch)
#include "tst_launch.moc"
