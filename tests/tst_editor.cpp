// Editor feasibility gate: drives the real QML editor offscreen and checks
// behaviour end to end against storage. Screenshots land in SCREENSHOT_DIR
// for visual review of rendering (tables, checkboxes, images, themes).

#include "AppLibrary.h"
#include "DocumentConverter.h"
#include "EditorController.h"
#include "LibraryService.h"
#include "SampleLibrary.h"
#include "ThemeController.h"

#include <QAccessible>
#include <QClipboard>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QGuiApplication>
#include <QInputMethodEvent>
#include <QMimeData>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QTextBlock>
#include <QTextDocument>
#include <QtQml/QQmlExtensionPlugin>

Q_IMPORT_QML_PLUGIN(OmarchyNotesPlugin)

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

const QByteArray kDarkTheme = R"(mode = "dark"
accent = "#dcd7ba"
selection = "#363646"
background = "#1f1f28"
dark_background = "#17171e"
foreground = "#dcd7ba"
dark_foreground = "#727169"
red = "#c34043"
yellow = "#c0a36e"
blue = "#7e9cd8"
)";

const QByteArray kLightTheme = R"(mode = "light"
accent = "#286983"
selection = "#dfdad9"
background = "#faf4ed"
dark_background = "#f2e9e1"
foreground = "#575279"
dark_foreground = "#797593"
red = "#b4637a"
yellow = "#ea9d34"
blue = "#286983"
)";

void writeFile(const QString &path, const QByteArray &data)
{
    QFile f(path);
    QVERIFY(f.open(QIODevice::WriteOnly | QIODevice::Truncate));
    f.write(data);
}

RichDocument paragraphs(const QStringList &lines)
{
    RichDocument doc;
    for (const QString &line : lines)
        doc.blocks << Block::paragraph({Span::plain(line)});
    doc.assignMissingIds();
    return doc;
}

} // namespace

class TestEditor : public QObject
{
    Q_OBJECT

    QTemporaryDir m_dataDir;
    QTemporaryDir m_themeDir;
    std::unique_ptr<LibraryService> m_service;
    std::unique_ptr<ThemeController> m_theme;
    std::unique_ptr<AppLibrary> m_library;
    std::unique_ptr<QQmlApplicationEngine> m_engine;
    QString m_mixedId;
    QQuickWindow *m_window = nullptr;
    QQuickItem *m_text = nullptr;
    QQuickItem *m_pane = nullptr;
    EditorController *m_editor = nullptr;

    QTextDocument *doc() const { return m_editor->textDocument(); }
    RichDocument current() const { return DocumentConverter::read(doc()); }

    RichDocument persisted()
    {
        m_service->waitForIdle();
        return m_service->loadNote(m_editor->noteId())->body;
    }

    QString open(const RichDocument &body)
    {
        const auto note = m_service->createNote(body);
        m_library->refresh();
        m_editor->openNote(note->id);
        m_text->forceActiveFocus();
        return note->id;
    }

    void type(const QString &text)
    {
        for (const QChar c : text)
            QTest::keyClick(m_window, c.toLatin1());
    }

    void setCursor(int position)
    {
        m_text->setProperty("cursorPosition", position);
    }

    void select(int start, int end)
    {
        QMetaObject::invokeMethod(m_text, "select", Q_ARG(int, start), Q_ARG(int, end));
    }

    int endOfBlock(int blockNumber) const
    {
        const QTextBlock b = doc()->findBlockByNumber(blockNumber);
        return b.position() + b.length() - 1;
    }

    QRectF positionToRectangle(int position)
    {
        QRectF r;
        QMetaObject::invokeMethod(m_text, "positionToRectangle", Q_RETURN_ARG(QRectF, r), Q_ARG(int, position));
        return r;
    }

    void screenshot(const QString &name)
    {
        QDir().mkpath(QStringLiteral(SCREENSHOT_DIR));
        QTest::qWait(50);
        const QImage image = m_window->grabWindow();
        QVERIFY(!image.isNull());
        image.save(QStringLiteral(SCREENSHOT_DIR) + u'/' + name + u".png"_s);
    }

private slots:
    void initTestCase()
    {
        writeFile(m_themeDir.filePath(u"colors.toml"_s), kDarkTheme);
        qputenv("OMARCHY_NOTES_THEME_DIR", m_themeDir.path().toLocal8Bit());
        QQuickStyle::setStyle(u"Basic"_s);

        m_service = std::make_unique<LibraryService>(LibraryPaths::at(m_dataDir.path()));
        QString error;
        QVERIFY2(m_service->start(&error), qPrintable(error));
        m_mixedId = SampleLibrary::seed(*m_service, &error);
        QVERIFY2(!m_mixedId.isEmpty(), qPrintable(error));

        m_theme = std::make_unique<ThemeController>();
        ThemeController::setInstance(m_theme.get());
        m_library = std::make_unique<AppLibrary>(m_service.get());
        AppLibrary::setInstance(m_library.get());

        m_engine = std::make_unique<QQmlApplicationEngine>();
        m_engine->setInitialProperties({{u"initialNoteId"_s, m_mixedId}});
        m_engine->loadFromModule("OmarchyNotes", "Main");
        QVERIFY(!m_engine->rootObjects().isEmpty());
        m_window = qobject_cast<QQuickWindow *>(m_engine->rootObjects().first());
        QVERIFY(m_window);
        m_window->resize(1200, 1300);
        QVERIFY(QTest::qWaitForWindowExposed(m_window));
        m_text = m_window->findChild<QQuickItem *>(u"noteText"_s);
        m_pane = m_window->findChild<QQuickItem *>(u"editorPane"_s);
        m_editor = m_window->findChild<EditorController *>(u"noteEditor"_s);
        QVERIFY(m_text && m_pane && m_editor);
        QCOMPARE(m_editor->noteId(), m_mixedId);
    }

    void cleanupTestCase()
    {
        m_engine.reset();
        m_library.reset();
        m_theme.reset();
        m_service.reset();
    }

    void rendersMixedNote()
    {
        QCOMPARE(m_theme->source(), m_themeDir.filePath(u"colors.toml"_s));
        const RichDocument body = current();
        QCOMPARE(body.title(), u"Q3 planning — product sync"_s);
        const QString image = DocumentConverter::imageResourceName(body.referencedBlobs().value(0));
        const QString chip = DocumentConverter::attachmentResourceName(body.referencedAttachments().value(0));
        QVERIFY(!doc()->resource(QTextDocument::ImageResource, QUrl(image)).isNull());
        QVERIFY(!doc()->resource(QTextDocument::ImageResource, QUrl(chip)).isNull());
        // Loading must not mark the note edited.
        QCOMPARE(m_editor->saveState(), EditorController::Saved);
        screenshot(u"mixed-note-dark-end"_s);
        setCursor(0);
        screenshot(u"mixed-note-dark"_s);
    }

    void typingAutosaves()
    {
        open(paragraphs({u"Hello"_s}));
        setCursor(5);
        type(u" world"_s);
        QCOMPARE(current().plainText(), u"Hello world"_s);
        QVERIFY(m_editor->saveState() != EditorController::Saved);
        QTRY_COMPARE_WITH_TIMEOUT(m_editor->saveState(), EditorController::Saved, 3000);
        QCOMPARE(persisted().plainText(), u"Hello world"_s);
    }

    void continuousTypingIsSavedWithinCap()
    {
        const QString id = open(paragraphs({u"Log"_s}));
        setCursor(3);
        const qint64 startRevision = m_service->loadNote(id)->revision;
        QElapsedTimer timer;
        timer.start();
        qint64 firstSaveMs = -1;
        while (timer.elapsed() < 4500) {
            QTest::keyClick(m_window, Qt::Key_A);
            QTest::qWait(120);
            if (firstSaveMs < 0 && m_service->loadNote(id)->revision > startRevision)
                firstSaveMs = timer.elapsed();
        }
        qInfo("first save while typing continuously: %lld ms (cap %d ms)", firstSaveMs,
              EditorController::MaxUnsavedMs);
        QVERIFY(firstSaveMs > 0);
        QVERIFY(firstSaveMs <= EditorController::MaxUnsavedMs + 500);
    }

    void typingFormatAtInsertionPoint()
    {
        open(paragraphs({u"Plain "_s}));
        setCursor(6);
        QMetaObject::invokeMethod(m_pane, "toggleMark", Q_ARG(QVariant, int(EditorController::Bold)));
        QVERIFY(m_editor->bold()); // toolbar reflects the pending format
        type(u"bold"_s);
        const RichDocument body = current();
        const QList<Span> spans = body.blocks.first().spans;
        QCOMPARE(spans.size(), 2);
        QCOMPARE(spans.last().text, u"bold"_s);
        QCOMPARE(spans.last().marks, quint32(Bold));
        // One undo removes the typed text together with its formatting.
        QTest::keyClick(m_window, Qt::Key_Z, Qt::ControlModifier);
        QCOMPARE(current().plainText(), u"Plain "_s);
    }

    void formattingSelectionAndUndo()
    {
        open(paragraphs({u"alpha beta"_s}));
        select(6, 10);
        QVERIFY(m_editor->toggleMark(EditorController::Bold));
        QCOMPARE(current().blocks.first().spans.last().marks, quint32(Bold));
        QTest::keyClick(m_window, Qt::Key_Z, Qt::ControlModifier);
        QCOMPARE(current().blocks.first().spans.size(), 1);
        QTest::keyClick(m_window, Qt::Key_Z, Qt::ControlModifier | Qt::ShiftModifier);
        QCOMPARE(current().blocks.first().spans.last().marks, quint32(Bold));

        setCursor(10);
        type(u" gamma"_s);
        QTest::keyClick(m_window, Qt::Key_Z, Qt::ControlModifier);
        QVERIFY(!current().plainText().contains(u"gamma"_s));
    }

    void headingThenReturnGivesBody()
    {
        open(paragraphs({u"Meeting"_s}));
        setCursor(0);
        m_editor->setBlockStyle(1);
        QCOMPARE(current().blocks.first().type, Block::Type::Heading);
        setCursor(7);
        QTest::keyClick(m_window, Qt::Key_Return);
        type(u"notes"_s);
        const RichDocument body = current();
        QCOMPARE(body.blocks.size(), 2);
        QCOMPARE(body.blocks[1].type, Block::Type::Paragraph);
        QCOMPARE(body.blocks[1].plainText(), u"notes"_s);
    }

    void checklistKeyboardFlow()
    {
        RichDocument start;
        start.blocks << Block::listItem(ListKind::Check, 0, {Span::plain(u"Buy milk"_s)}, true);
        start.assignMissingIds();
        open(start);
        setCursor(endOfBlock(0));

        QTest::keyClick(m_window, Qt::Key_Return);
        type(u"Eggs"_s);
        RichDocument body = current();
        QCOMPARE(body.blocks.size(), 2);
        QCOMPARE(body.blocks[1].list, ListKind::Check);
        QVERIFY(!body.blocks[1].checked); // new items start unchecked

        QTest::keyClick(m_window, Qt::Key_Tab);
        QCOMPARE(current().blocks[1].indent, 1);
        QTest::keyClick(m_window, Qt::Key_Backtab);
        QCOMPARE(current().blocks[1].indent, 0);

        // Return on an empty item leaves the list.
        QTest::keyClick(m_window, Qt::Key_Return);
        QTest::keyClick(m_window, Qt::Key_Return);
        type(u"After"_s);
        body = current();
        QCOMPARE(body.blocks.last().type, Block::Type::Paragraph);
        QCOMPARE(body.blocks.last().plainText(), u"After"_s);
    }

    void checkboxClickToggles()
    {
        RichDocument start;
        start.blocks << Block::heading(1, {Span::plain(u"Groceries"_s)});
        start.blocks << Block::listItem(ListKind::Check, 0, {Span::plain(u"Coffee"_s)});
        start.assignMissingIds();
        open(start);
        QTest::qWait(50);
        screenshot(u"checklist"_s);
        const int itemStart = doc()->findBlockByNumber(1).position();
        const QRectF r = positionToRectangle(itemStart);
        const QPointF marker = m_text->mapToScene(QPointF(r.x() - 12, r.center().y()));
        QTest::mouseClick(m_window, Qt::LeftButton, Qt::NoModifier, marker.toPoint());
        QTRY_VERIFY(current().blocks[1].checked);
        QTest::qWait(400); // keep the next click from being a double-click
        QTest::mouseClick(m_window, Qt::LeftButton, Qt::NoModifier, marker.toPoint());
        QTRY_VERIFY(!current().blocks[1].checked);
        // Keyboard path.
        setCursor(itemStart + 2);
        QTest::keyClick(m_window, Qt::Key_U, Qt::ControlModifier | Qt::ShiftModifier);
        QVERIFY(current().blocks[1].checked);
        QTest::keyClick(m_window, Qt::Key_Z, Qt::ControlModifier);
        QVERIFY(!current().blocks[1].checked);
        // Clicking the text itself must not toggle.
        const bool before = current().blocks[1].checked;
        QTest::qWait(400);
        const QPointF onText = m_text->mapToScene(QPointF(r.x() + 20, r.center().y()));
        QTest::mouseClick(m_window, Qt::LeftButton, Qt::NoModifier, onText.toPoint());
        QCOMPARE(current().blocks[1].checked, before);
    }

    void clipboardRoundTripKeepsStructure()
    {
        const auto mixed = m_service->loadNote(m_mixedId);
        open(mixed->body);
        // Select from "Action items" through the table.
        int from = -1;
        int to = -1;
        for (QTextBlock b = doc()->begin(); b.isValid(); b = b.next()) {
            if (b.text() == u"Action items")
                from = b.position();
            if (b.text() == u"10 Oct")
                to = b.position() + b.length();
        }
        QVERIFY(from >= 0 && to > from);
        select(from, to + 1);
        m_editor->copy();
        QVERIFY(QGuiApplication::clipboard()->mimeData()->hasFormat(u"application/x-omarchy-notes-fragment+json"_s));
        setCursor(doc()->characterCount() - 1);
        QTest::keyClick(m_window, Qt::Key_Return);
        QTest::keyClick(m_window, Qt::Key_V, Qt::ControlModifier);

        const RichDocument body = current();
        int tables = 0;
        int checks = 0;
        QSet<QString> ids;
        for (const Block &b : body.blocks) {
            tables += b.type == Block::Type::Table;
            checks += b.list == ListKind::Check;
            QVERIFY2(!ids.contains(b.id), "duplicate block id after paste");
            ids.insert(b.id);
        }
        QCOMPARE(tables, 2);
        QCOMPARE(checks, 10);
        QVERIFY(m_editor->flush());
        QCOMPARE(persisted(), current());
    }

    void htmlPasteIsNormalized()
    {
        open(paragraphs({u""_s}));
        auto *mime = new QMimeData;
        mime->setHtml(u"<h2 style='font-family:Comic Sans MS;color:red'>From the web</h2>"
                      "<ul><li>first</li><li><b>second</b></li></ul>"
                      "<p>see <a href='https://example.com'>this</a><img src='https://example.com/a.png'></p>"_s);
        QGuiApplication::clipboard()->setMimeData(mime);
        QSignalSpy notices(m_editor, &EditorController::notice);
        QTest::keyClick(m_window, Qt::Key_V, Qt::ControlModifier);
        const RichDocument body = current();
        QCOMPARE(body.blocks[0].type, Block::Type::Heading);
        QCOMPARE(body.blocks[1].list, ListKind::Bullet);
        QCOMPARE(body.blocks[2].spans.first().marks, quint32(Bold));
        QCOMPARE(body.blocks[3].spans.last().href, u"https://example.com"_s);
        QCOMPARE(notices.size(), 1); // the web image was reported, not silently dropped
    }

    void inputMethodComposition()
    {
        open(paragraphs({u"Tokyo: "_s}));
        setCursor(7);
        QInputMethodEvent preedit(u"とうきょう"_s, {});
        QCoreApplication::sendEvent(m_text, &preedit);
        QCOMPARE(current().plainText(), u"Tokyo: "_s); // preedit is not content
        QInputMethodEvent commit;
        commit.setCommitString(u"東京"_s);
        QCoreApplication::sendEvent(m_text, &commit);
        QCOMPARE(current().plainText(), u"Tokyo: 東京"_s);
        QVERIFY(m_editor->flush());
        QCOMPARE(persisted().plainText(), u"Tokyo: 東京"_s);
    }

    void failedSaveKeepsDraftAndRecovers()
    {
        open(paragraphs({u"Draft"_s}));
        setCursor(5);
        m_library->setSimulateSaveFailure(true);
        type(u" v2"_s);
        QTRY_COMPARE_WITH_TIMEOUT(m_editor->saveState(), EditorController::Failed, 3000);
        QVERIFY(!m_editor->saveError().isEmpty());
        QCOMPARE(current().plainText(), u"Draft v2"_s); // draft kept in the editor
        QVERIFY(!m_editor->flush());
        screenshot(u"save-failed"_s);

        m_library->setSimulateSaveFailure(false);
        m_editor->retrySave();
        QTRY_COMPARE_WITH_TIMEOUT(m_editor->saveState(), EditorController::Saved, 3000);
        QCOMPARE(persisted().plainText(), u"Draft v2"_s);
    }

    void followsThemeSwitch()
    {
        m_editor->openNote(m_mixedId);
        const RichDocument before = current();
        QSignalSpy changed(m_theme.get(), &ThemeController::changed);
        writeFile(m_themeDir.filePath(u"colors.toml"_s), kLightTheme);
        QTRY_VERIFY_WITH_TIMEOUT(changed.size() > 0, 3000);
        QVERIFY(!m_theme->dark());
        QCOMPARE(current(), before); // content untouched by the restyle
        QCOMPARE(m_editor->saveState(), EditorController::Saved);
        QCOMPARE(m_text->property("color").value<QColor>(), m_theme->text());
        screenshot(u"mixed-note-light"_s);
    }

    void accessibleEditor()
    {
        QAccessibleInterface *iface = QAccessible::queryAccessibleInterface(m_text);
        QVERIFY(iface);
        QCOMPARE(iface->role(), QAccessible::EditableText);
        QVERIFY(iface->textInterface());
        const QString text = iface->textInterface()->text(0, iface->textInterface()->characterCount());
        QVERIFY(text.contains(u"Q3 planning"_s));
        // Report what assistive tech sees for embedded objects.
        qInfo("accessible text has %lld object placeholder(s)", qint64(text.count(QChar(0xFFFC))));
    }

    void narrowTile()
    {
        m_window->resize(520, 900);
        QTest::qWait(100);
        QQuickItem *list = m_window->findChild<QQuickItem *>(u"noteList"_s);
        QVERIFY(list);
        QVERIFY(!list->isVisible());
        QVERIFY(m_pane->isVisible());
        screenshot(u"narrow-tile"_s);
        m_window->resize(1200, 1300);
    }

    void largeNoteResponsiveness()
    {
        RichDocument big;
        for (int i = 0; i < 3000; ++i) {
            big.blocks << Block::paragraph({Span::plain(u"Paragraph %1 of a long research note with "_s.arg(i)),
                                            Span::plain(u"some emphasis"_s, Italic),
                                            Span::plain(u" and enough text to wrap across the column."_s)});
            if (i % 150 == 0) {
                Block t;
                t.type = Block::Type::Table;
                t.rows = 3;
                t.columns = 3;
                for (int r = 0; r < 3; ++r)
                    for (int c = 0; c < 3; ++c)
                        t.cells << TableCell{r, c, 1, 1, {Block::paragraph({Span::plain(u"cell"_s)})}};
                big.blocks << t;
            }
        }
        big.assignMissingIds();
        const auto note = m_service->createNote(big);
        m_library->refresh();

        QElapsedTimer timer;
        timer.start();
        m_editor->openNote(note->id);
        QTest::qWait(0);
        const qint64 openMs = timer.elapsed();

        setCursor(doc()->findBlockByNumber(1500).position());
        qint64 worstKeyMs = 0;
        for (int i = 0; i < 20; ++i) {
            timer.restart();
            QTest::keyClick(m_window, Qt::Key_X);
            QCoreApplication::processEvents();
            worstKeyMs = std::max(worstKeyMs, timer.elapsed());
        }
        timer.restart();
        const RichDocument body = DocumentConverter::read(doc());
        const qint64 readMs = timer.elapsed();
        qInfo("large note (%lld blocks): open %lld ms, worst keystroke %lld ms, snapshot for save %lld ms",
              qint64(body.blocks.size()), openMs, worstKeyMs, readMs);
        QVERIFY(m_editor->flush());
    }
};

QTEST_MAIN(TestEditor)
#include "tst_editor.moc"
