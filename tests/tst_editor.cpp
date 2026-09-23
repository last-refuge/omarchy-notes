// Editor feasibility gate: drives the real QML editor offscreen and checks
// behaviour end to end against storage. Screenshots land in SCREENSHOT_DIR
// for visual review of rendering (tables, checkboxes, images, themes).

#include "AppController.h"
#include "AppLibrary.h"
#include "BlobImageProvider.h"
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
#include <QUrl>
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
    std::unique_ptr<AppController> m_app;
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

    void resetView()
    {
        m_library->setSearchText({});
        m_library->showKey(u"all"_s);
    }

    void call(const char *function, const QVariant &arg = {})
    {
        if (arg.isValid())
            QMetaObject::invokeMethod(m_window, function, Q_ARG(QVariant, arg));
        else
            QMetaObject::invokeMethod(m_window, function);
    }

    QQuickItem *item(const char *name) const
    {
        return m_window->findChild<QQuickItem *>(QString::fromLatin1(name));
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

        m_theme = std::make_unique<ThemeController>(nullptr);
        ThemeController::setInstance(m_theme.get());
        m_library = std::make_unique<AppLibrary>(m_service.get(), m_dataDir.filePath(u"settings.ini"_s));
        AppLibrary::setInstance(m_library.get());
        m_app = std::make_unique<AppController>(nullptr);
        AppController::setInstance(m_app.get());

        m_engine = std::make_unique<QQmlApplicationEngine>();
        m_engine->addImageProvider(u"blob"_s, new BlobImageProvider(m_service.get()));
        m_engine->setInitialProperties({{u"initialNoteId"_s, m_mixedId}});
        m_engine->loadFromModule("OmarchyNotes", "Main");
        QVERIFY(!m_engine->rootObjects().isEmpty());
        m_window = qobject_cast<QQuickWindow *>(m_engine->rootObjects().first());
        QVERIFY(m_window);
        m_window->resize(1200, 1300);
        QVERIFY(QTest::qWaitForWindowExposed(m_window));
        // The Quick Note window has its own editor; use the main window's.
        m_pane = m_window->findChild<QQuickItem *>(u"editorPane"_s);
        QVERIFY(m_pane);
        m_text = m_pane->findChild<QQuickItem *>(u"noteText"_s);
        m_editor = m_pane->findChild<EditorController *>(u"noteEditor"_s);
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

    void foldersAndNewNotes()
    {
        resetView();
        const QVariantMap made = m_library->createFolder(u"Work"_s, {});
        QVERIFY2(made.value(u"ok"_s).toBool(), qPrintable(made.value(u"error"_s).toString()));
        const QString folder = made.value(u"id"_s).toString();
        QVERIFY(!m_library->createFolder(u"work"_s, {}).value(u"ok"_s).toBool());

        call("showView", folder);
        QCOMPARE(m_library->currentKey(), folder);
        QCOMPARE(m_library->viewTitle(), u"Work"_s);
        QCOMPARE(m_library->notes()->count(), 0);
        QVERIFY(!m_editor->hasNote()); // an empty folder opens nothing
        screenshot(u"empty-folder"_s);

        call("newNote");
        QVERIFY(m_editor->hasNote());
        QCOMPARE(m_service->loadNote(m_editor->noteId())->folderId, folder);
        type(u"Standup"_s);
        QVERIFY(m_editor->flush());
        QCOMPARE(m_library->notes()->titleOf(m_editor->noteId()), u"Standup"_s);

        QVERIFY(m_library->moveNote(m_editor->noteId(), {}));
        QCOMPARE(m_library->notes()->count(), 0);
        QCOMPARE(m_service->loadNote(m_editor->noteId())->folderId, QString());
    }

    void deleteAndRecoverFromTheList()
    {
        resetView();
        const QString older = open(paragraphs({u"Alpha note"_s}));
        const QString newer = open(paragraphs({u"Beta note"_s}));
        QCOMPARE(m_library->notes()->idAt(0), newer);

        item("noteListView")->forceActiveFocus();
        QTest::keyClick(m_window, Qt::Key_Delete);
        QVERIFY(m_service->loadNote(newer)->deletedAt > 0);
        QCOMPARE(m_editor->noteId(), older); // the neighbour opens
        QCOMPARE(m_library->notes()->indexOf(newer), -1);

        call("showView", u"trash"_s);
        QVERIFY(m_library->inTrash());
        call("openNote", newer);
        QVERIFY(m_editor->readOnly());
        QVERIFY(m_text->property("readOnly").toBool());
        setCursor(4);
        type(u"xyz"_s);
        QCOMPARE(current().plainText(), u"Beta note"_s); // read-only
        screenshot(u"recently-deleted"_s);

        call("recoverNote", newer);
        QCOMPARE(m_service->loadNote(newer)->deletedAt, 0);
        QCOMPARE(m_library->notes()->indexOf(newer), -1); // left the trash list
    }

    void deletePermanentlyAsksFirst()
    {
        resetView();
        const QString doomed = open(paragraphs({u"Scratch"_s}));
        QVERIFY(m_library->trashNote(doomed));
        call("showView", u"trash"_s);
        call("deleteForever", doomed);
        QQuickItem *dialog = item("confirmDialog");
        Q_UNUSED(dialog);
        QObject *confirm = m_window->findChild<QObject *>(u"confirmDialog"_s);
        QTRY_VERIFY(confirm->property("opened").toBool());
        QVERIFY(m_service->loadNote(doomed)); // nothing happens until confirmed
        QTest::keyClick(m_window, Qt::Key_Return);
        QTRY_VERIFY(!m_service->loadNote(doomed));
    }

    void searchFromTheSearchField()
    {
        resetView();
        const QString target = open(paragraphs({u"Quarterly benchmark review"_s, u"Numbers look good."_s}));
        QVERIFY(m_editor->flush());
        item("searchField")->forceActiveFocus();
        type(u"quarterl bench"_s);
        QVERIFY(m_library->searching());
        QCOMPARE(m_library->viewTitle(), u"Search"_s);
        // Results arrive asynchronously, with the matching words in bold.
        auto snippetOfTarget = [&] {
            const int row = m_library->notes()->indexOf(target);
            return row < 0 ? QString() : m_library->notes()->index(row).data(NotesModel::SnippetRole).toString();
        };
        QTRY_VERIFY(snippetOfTarget().contains(u"<b>"_s));
        screenshot(u"search"_s);

        QTest::keyClick(m_window, Qt::Key_Return); // opens the best match
        QCOMPARE(m_editor->noteId(), m_library->notes()->idAt(0));
        item("searchField")->forceActiveFocus();
        QTest::keyClick(m_window, Qt::Key_Escape);
        QVERIFY(!m_library->searching());
        QCOMPARE(m_library->viewTitle(), u"All Notes"_s);
    }

    void pinningPutsANoteFirst()
    {
        resetView();
        open(paragraphs({u"Recent"_s}));
        const QString oldest = m_library->notes()->idAt(m_library->notes()->count() - 1);
        m_editor->openNote(oldest);
        m_editor->setPinned(true);
        QVERIFY(m_editor->pinned());
        QCOMPARE(m_library->notes()->idAt(0), oldest);
        QVERIFY(m_library->notes()->hasPinned());
        screenshot(u"pinned"_s);
        m_editor->setPinned(false);
        QVERIFY(!m_library->notes()->hasPinned());
    }

    void deletingAFolderMovesItsNotesToTheTrash()
    {
        resetView();
        const QString folder = m_library->createFolder(u"Temporary"_s, {}).value(u"id"_s).toString();
        call("showView", folder);
        call("newNote");
        const QString inside = m_editor->noteId();
        type(u"Inside"_s);
        QVERIFY(m_editor->flush());
        QVERIFY(m_library->deleteFolder(folder));
        QCOMPARE(m_library->currentKey(), u"all"_s); // the view moved off the gone folder
        QVERIFY(m_service->loadNote(inside)->deletedAt > 0);
        QVERIFY(m_editor->noteId() != inside || m_editor->readOnly());
    }

    void tagsAreHighlightedAndListed()
    {
        resetView();
        open(paragraphs({u"Planning"_s}));
        setCursor(8);
        type(u" #roadmap"_s);
        QVERIFY(m_editor->flush());
        QTRY_VERIFY(m_library->folders()->indexOfKey(u"tag:roadmap"_s) >= 0); // after typing pauses
        QTRY_VERIFY(!doc()->findBlockByNumber(0).layout()->formats().isEmpty()); // coloured

        // Reopening a tagged note must not count the colouring as an edit.
        const QString id = m_editor->noteId();
        open(paragraphs({u"Elsewhere"_s}));
        m_editor->openNote(id);
        QTest::qWait(50);
        QCOMPARE(m_editor->saveState(), EditorController::Saved);

        call("showView", u"tag:roadmap"_s);
        QCOMPARE(m_library->viewTitle(), u"#roadmap"_s);
        QCOMPARE(m_library->notes()->count(), 1);
        call("newNote"); // a note started from a tag carries it
        QVERIFY(m_editor->flush());
        QVERIFY(m_service->loadNote(m_editor->noteId())->body.tags().contains(u"roadmap"_s));
        screenshot(u"tags"_s);
    }

    void findInNote()
    {
        resetView();
        open(paragraphs({u"alpha beta ALPHA gamma alpha"_s}));
        setCursor(0);
        m_editor->setFindText(u"alpha"_s);
        QCOMPARE(m_editor->findCount(), 3);
        QCOMPARE(m_editor->findIndex(), 1);
        QCOMPARE(m_text->property("selectedText").toString(), u"alpha"_s);
        m_editor->findNext();
        QCOMPARE(m_editor->findIndex(), 2);
        QCOMPARE(m_text->property("selectionStart").toInt(), 11);
        m_editor->findPrevious();
        m_editor->findPrevious();
        QCOMPARE(m_editor->findIndex(), 3); // wraps around
        QCOMPARE(m_editor->saveState(), EditorController::Saved); // finding isn't editing

        setCursor(doc()->characterCount() - 1);
        type(u" alpha"_s);
        QTRY_COMPARE(m_editor->findCount(), 4); // follows edits
        m_editor->setFindText({});
        QCOMPARE(m_editor->findCount(), 0);
        QVERIFY(m_editor->flush());
    }

    void smartFolderShowsMatchingNotes()
    {
        resetView();
        RichDocument trip;
        trip.blocks = {Block::heading(1, {Span::plain(u"Trip"_s)}),
                       Block::listItem(ListKind::Check, 0, {Span::plain(u"Passport #travel"_s)})};
        trip.assignMissingIds();
        const QString tripId = open(trip);
        open(paragraphs({u"Hotel ideas #travel"_s}));
        QVariantMap criteria{{u"tags"_s, QStringList{u"travel"_s}}, {u"hasChecklist"_s, true}};
        const QVariantMap made = m_library->createSmartFolder(u"Travel checklists"_s, criteria);
        QVERIFY2(made.value(u"ok"_s).toBool(), qPrintable(made.value(u"error"_s).toString()));
        call("showView", made.value(u"id"_s));
        QCOMPARE(m_library->viewTitle(), u"Travel checklists"_s);
        QCOMPARE(m_library->notes()->count(), 1);
        QCOMPARE(m_library->notes()->idAt(0), tripId);
        QVERIFY(!m_library->createSmartFolder(u"Empty"_s, {}).value(u"ok"_s).toBool());
        screenshot(u"smart-folder"_s);
    }

    void galleryView()
    {
        resetView();
        m_library->setGalleryMode(true);
        QQuickItem *grid = item("noteGrid");
        QTRY_VERIFY(grid->isVisible());
        QVERIFY(!m_pane->isVisible()); // the gallery takes the editor's space
        QTest::qWait(300); // let thumbnails decode
        screenshot(u"gallery"_s);

        call("openNote", m_mixedId);
        QVERIFY(m_pane->isVisible());
        QVERIFY(!grid->isVisible());
        m_library->setGalleryMode(false);
        QVERIFY(m_pane->isVisible());
        QVERIFY(item("noteListView")->isVisible());
    }

    void attachmentBrowser()
    {
        resetView();
        call("showView", u"attachments"_s);
        QVERIFY(m_library->inAttachments());
        QTRY_VERIFY(item("attachmentsView")->isVisible());
        QVERIFY(m_library->attachments()->count() >= 2);
        m_library->attachments()->setFilter(AttachmentsModel::Files);
        const int files = m_library->attachments()->count();
        m_library->attachments()->setFilter(AttachmentsModel::Images);
        QVERIFY(files >= 1 && m_library->attachments()->count() >= 1);
        m_library->attachments()->setFilter(AttachmentsModel::All);
        QTest::qWait(300);
        screenshot(u"attachments"_s);

        const QString noteId = m_library->attachments()->index(0).data(AttachmentsModel::NoteIdRole).toString();
        call("openNote", noteId);
        QCOMPARE(m_editor->noteId(), noteId);
        QVERIFY(m_pane->isVisible());
        resetView();
    }

    void importAndExportFromTheApp()
    {
        resetView();
        QTemporaryDir files;
        QFile md(files.filePath(u"From Obsidian.md"_s));
        QVERIFY(md.open(QIODevice::WriteOnly));
        md.write("# From Obsidian\n\n- [ ] Try the importer #imported\n");
        md.close();
        const QVariantMap imported = m_library->importFiles({QUrl::fromLocalFile(md.fileName())});
        QCOMPARE(imported.value(u"count"_s).toInt(), 1);
        const QString id = imported.value(u"first"_s).toString();
        QVERIFY(m_library->notes()->indexOf(id) >= 0);
        QCOMPARE(m_library->exportFileName(id, u"pdf"_s), u"From Obsidian.pdf"_s);
        const QVariantMap exported = m_library->exportNote(id, u"md"_s, QUrl::fromLocalFile(files.filePath(u"out.md"_s)));
        QVERIFY2(exported.value(u"ok"_s).toBool(), qPrintable(exported.value(u"error"_s).toString()));
        QFile out(files.filePath(u"out.md"_s));
        QVERIFY(out.open(QIODevice::ReadOnly));
        QVERIFY(out.readAll().contains("- [ ] Try the importer #imported"));
    }

    void quickNote()
    {
        resetView();
        QQuickWindow *quick = m_window->findChild<QQuickWindow *>(u"quickNote"_s);
        QVERIFY(quick);
        const int before = m_service->noteCount();
        // QML must talk to the same controller the process uses.
        QCOMPARE(m_engine->singletonInstance<AppController *>("OmarchyNotes", "App"), m_app.get());
        QCOMPARE(m_engine->singletonInstance<ThemeController *>("OmarchyNotes", "Theme"), m_theme.get());
        m_app->handle({{u"action"_s, u"quick-note"_s}});
        QTRY_VERIFY(quick->isVisible());
        QVERIFY(QTest::qWaitForWindowExposed(quick));
        auto *pane = quick->findChild<QQuickItem *>(u"quickNotePane"_s);
        auto *quickEditor = pane->findChild<EditorController *>(u"noteEditor"_s);
        QVERIFY(quickEditor->hasNote());
        QCOMPARE(m_service->noteCount(), before + 1);
        QDir().mkpath(QStringLiteral(SCREENSHOT_DIR));
        quick->grabWindow().save(QStringLiteral(SCREENSHOT_DIR) + u"/quick-note.png"_s);

        // Closing an untouched Quick Note leaves nothing behind.
        quick->close();
        QTRY_VERIFY(!quick->isVisible());
        QCOMPARE(m_service->noteCount(), before);

        // One that was written in is kept, in Notes.
        m_app->handle({{u"action"_s, u"quick-note"_s}});
        QTRY_VERIFY(quick->isVisible());
        auto *quickText = pane->findChild<QQuickItem *>(u"noteText"_s);
        quickText->forceActiveFocus();
        QTest::keyClick(quick, 'h');
        QTest::keyClick(quick, 'i');
        const QString id = quickEditor->noteId();
        quick->close();
        QTRY_VERIFY(!quick->isVisible());
        QCOMPARE(m_service->loadNote(id)->body.plainText(), u"hi"_s);
        QCOMPARE(m_service->loadNote(id)->folderId, QString());
    }

    void captureFromAnotherLaunch()
    {
        resetView();
        const int before = m_service->noteCount();
        m_app->handle({{u"action"_s, u"capture"_s}, {u"text"_s, u"Call the plumber\nTuesday"_s}});
        QCOMPARE(m_service->noteCount(), before + 1);
        QCOMPARE(m_library->notes()->titleOf(m_library->notes()->idAt(0)), u"Call the plumber"_s);
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
    // Last: replaces the whole library.
    void backupAndRestoreReopenTheLibrary()
    {
        resetView();
        QTemporaryDir backups;
        const QString kept = open(paragraphs({u"Before the backup"_s}));
        QVERIFY(m_editor->flush());
        const int notesBefore = m_service->noteCount();
        const QVariantMap backup = m_library->backupTo(QUrl::fromLocalFile(backups.path()), u"nightly"_s);
        QVERIFY2(backup.value(u"ok"_s).toBool(), qPrintable(backup.value(u"error"_s).toString()));

        const QString later = open(paragraphs({u"After the backup"_s}));
        QVERIFY(m_editor->flush());
        const QUrl source = QUrl::fromLocalFile(backups.filePath(u"nightly"_s));
        QVERIFY(m_library->inspectBackup(source).value(u"ok"_s).toBool());

        m_editor->closeNote();
        const QVariantMap restored = m_library->restoreFrom(source);
        QVERIFY2(restored.value(u"ok"_s).toBool(), qPrintable(restored.value(u"error"_s).toString()));
        QCOMPARE(m_service->noteCount(), notesBefore);
        QVERIFY(!m_service->loadNote(later));
        QTRY_VERIFY(m_editor->hasNote()); // the window reopened a note

        // The restored library is fully usable.
        m_editor->openNote(kept);
        m_text->forceActiveFocus();
        setCursor(doc()->characterCount() - 1);
        type(u"!"_s);
        QVERIFY(m_editor->flush());
        QCOMPARE(persisted().plainText(), u"Before the backup!"_s);
        QDir(restored.value(u"previous"_s).toString()).removeRecursively();
    }
};

QTEST_MAIN(TestEditor)
#include "tst_editor.moc"
