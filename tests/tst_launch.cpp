// Runs the real binary: headless capture, single-instance hand-off, and
// capture delivered to a running instance.

#include "InstanceChannel.h"
#include "LibraryService.h"

#include <QDir>
#include <QFile>
#include <QLocalSocket>

#include <unistd.h>

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
        if (!p.waitForFinished(15000)) {
            p.kill();
            p.waitForFinished();
            return -1;
        }
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
    void initTestCase()
    {
        // The app and this test must agree on where the socket lives.
        qputenv("XDG_RUNTIME_DIR", m_dir.path().toLocal8Bit());
    }

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

    void firstFrameAndStartupTime()
    {
        QProcess p;
        QProcessEnvironment e = env();
        e.insert(u"ONOTES_EXIT_AFTER_FIRST_FRAME"_s, u"1"_s);
        p.setProcessEnvironment(e);
        p.start(QStringLiteral(APP_PATH), {u"--data-dir"_s, m_dir.filePath(u"startup"_s)});
        QVERIFY(p.waitForFinished(20000));
        const QByteArray out = p.readAllStandardOutput();
        QVERIFY2(out.contains("first-frame") && out.contains("Omarchy Notes"), out.constData());
        qInfo("%s", out.trimmed().constData());
    }

    void unusableLibraryShowsAnErrorWindow()
    {
        if (::geteuid() == 0)
            QSKIP("root can write to read-only folders");
        const QString library = m_dir.filePath(u"locked"_s);
        QDir().mkpath(library);
        QFile::setPermissions(library, QFileDevice::ReadOwner | QFileDevice::ExeOwner);
        QProcess p;
        QProcessEnvironment e = env();
        e.insert(u"ONOTES_EXIT_AFTER_FIRST_FRAME"_s, u"1"_s);
        p.setProcessEnvironment(e);
        p.start(QStringLiteral(APP_PATH), {u"--data-dir"_s, library});
        const bool finished = p.waitForFinished(20000);
        QFile::setPermissions(library, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
        QVERIFY(finished);
        QCOMPARE(p.exitCode(), 1);
        QVERIFY(p.readAllStandardOutput().contains("first-frame")); // a window was shown
        QVERIFY(p.readAllStandardError().contains("can't open your notes"));
    }

    void hyprlandSetupIsSafe()
    {
        const QString config = m_dir.filePath(u"hyprconfig"_s);
        const QString omarchy = m_dir.filePath(u"omarchy"_s);
        QDir().mkpath(config + u"/hypr"_s);
        QDir().mkpath(omarchy + u"/default/hypr/bindings"_s);
        auto writeFile = [](const QString &path, const QByteArray &data) {
            QFile f(path);
            QVERIFY(f.open(QIODevice::WriteOnly));
            f.write(data);
        };
        writeFile(config + u"/hypr/bindings.lua"_s, R"(o.bind("SUPER + B", "Browser", { launch = "chromium" }))" "\n");
        writeFile(config + u"/hypr/hyprland.lua"_s, R"(require("bindings"))" "\n");
        writeFile(omarchy + u"/default/hypr/bindings/utilities.lua"_s,
                  R"(o.bind("SUPER + SPACE", "Omarchy menu", "omarchy-menu toggle"))" "\n");

        auto setup = [&](QByteArray *out) {
            QProcess p;
            QProcessEnvironment e = env();
            e.insert(u"XDG_CONFIG_HOME"_s, config);
            e.insert(u"OMARCHY_PATH"_s, omarchy);
            p.setProcessEnvironment(e);
            p.start(QStringLiteral(APP_PATH), {u"--setup-hyprland"_s});
            p.waitForFinished(15000);
            *out = p.readAllStandardOutput();
            return p.exitCode();
        };
        auto read = [](const QString &path) {
            QFile f(path);
            return f.open(QIODevice::ReadOnly) ? f.readAll() : QByteArray();
        };
        QByteArray out;
        QCOMPARE(setup(&out), 0);
        QVERIFY(read(config + u"/hypr/bindings.lua"_s).contains(R"(o.bind("SUPER + ALT + N", "Quick Note")"));
        QVERIFY(read(config + u"/hypr/hyprland.lua"_s).contains(R"(title = "^Quick Note$")"));

        // Running it again changes nothing.
        const QByteArray before = read(config + u"/hypr/bindings.lua"_s);
        QCOMPARE(setup(&out), 0);
        QCOMPARE(read(config + u"/hypr/bindings.lua"_s), before);
        QVERIFY(out.contains("already set up"));

        // A key that's taken (here by Omarchy's defaults) is never overwritten.
        writeFile(config + u"/hypr/bindings.lua"_s, "-- fresh\n");
        writeFile(omarchy + u"/default/hypr/bindings/utilities.lua"_s,
                  R"(o.bind("SUPER + ALT + N", "Something else", "thing"))" "\n");
        QCOMPARE(setup(&out), 1);
        QVERIFY(out.contains("already bound"));
        QCOMPARE(read(config + u"/hypr/bindings.lua"_s), QByteArray("-- fresh\n"));
    }

    void secondLaunchHandsOffToTheFirst()
    {
        QProcess app;
        app.setProcessEnvironment(env());
        app.start(QStringLiteral(APP_PATH), {u"--data-dir"_s, m_dir.filePath(u"library"_s)});
        QVERIFY(app.waitForStarted());
        // Wait until it's listening, so the next launch can't race it for
        // the library lock.
        const QString socket = InstanceChannel::socketPath(LibraryPaths::at(m_dir.filePath(u"library"_s)));
        QTRY_VERIFY_WITH_TIMEOUT([&] {
            QLocalSocket probe;
            probe.connectToServer(socket);
            return probe.waitForConnected(100);
        }(), 10000);
        QByteArray out;
        QCOMPARE(run({}, &out), 0);
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
