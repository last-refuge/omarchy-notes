#include "DocumentConverter.h"
#include "RichDocument.h"
#include "SampleLibrary.h"

#include <QElapsedTimer>
#include <QJsonDocument>
#include <QTest>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextList>

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

const QString kHash = u"a3f1c2d4e5b60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"_s;
const QString kAttachment = u"8d1c3f0e-6b8a-4f7e-9d2c-1a2b3c4d5e6f"_s;
const QString kLinked = u"2f7d4c1e-0a9b-4c8d-8e7f-6a5b4c3d2e1f"_s;

RichDocument fixture()
{
    return SampleLibrary::mixedContentNote(kHash, kAttachment, kLinked);
}

RichDocument viaEditor(const RichDocument &in, QTextDocument *doc)
{
    DocumentConverter::load(in, doc, DocumentStyle{});
    return DocumentConverter::read(doc);
}

QString describe(const RichDocument &doc)
{
    return QString::fromUtf8(QJsonDocument(doc.toJson()).toJson(QJsonDocument::Indented));
}

#define COMPARE_DOCS(actual, expected)                                                    \
    do {                                                                                  \
        const RichDocument a_ = (actual);                                                 \
        const RichDocument e_ = (expected);                                               \
        if (!(a_ == e_)) {                                                                \
            qWarning().noquote() << "actual:\n" << describe(a_) << "\nexpected:\n" << describe(e_); \
            QFAIL("documents differ");                                                    \
        }                                                                                 \
    } while (false)

Block table(int rows, int columns, QList<TableCell> cells = {})
{
    Block b;
    b.type = Block::Type::Table;
    b.rows = rows;
    b.columns = columns;
    if (cells.isEmpty()) {
        for (int r = 0; r < rows; ++r)
            for (int c = 0; c < columns; ++c)
                cells.append({r, c, 1, 1, {Block::paragraph({Span::plain(u"r%1c%2"_s.arg(r).arg(c))})}});
    }
    b.cells = cells;
    return b;
}

RichDocument withIds(RichDocument doc)
{
    doc.assignMissingIds();
    return doc;
}

} // namespace

class TestDocument : public QObject
{
    Q_OBJECT

private slots:
    void jsonRoundTrip()
    {
        const RichDocument doc = fixture();
        const auto parsed = RichDocument::fromJsonBytes(doc.toJsonBytes());
        QVERIFY(parsed);
        COMPARE_DOCS(*parsed, doc);
    }

    void editorRoundTrip()
    {
        QTextDocument doc;
        const RichDocument source = fixture();
        COMPARE_DOCS(viaEditor(source, &doc), source);
    }

    void editorRoundTripIsStable()
    {
        QTextDocument doc;
        const RichDocument first = viaEditor(fixture(), &doc);
        QTextDocument again;
        COMPARE_DOCS(viaEditor(first, &again), first);
        // Reading the same document twice must not mint new ids.
        COMPARE_DOCS(DocumentConverter::read(&doc), first);
    }

    void derivedFields()
    {
        const RichDocument doc = fixture();
        QCOMPARE(doc.title(), u"Q3 planning — product sync"_s);
        QVERIFY(doc.snippet().startsWith(u"Met with Dana"_s));
        QCOMPARE(doc.referencedBlobs(), QStringList{kHash});
        QCOMPARE(doc.referencedAttachments(), QStringList{kAttachment});
        QVERIFY(doc.plainText().contains(u"Search benchmark"_s)); // table text is searchable
    }

    void tableLayouts_data()
    {
        QTest::addColumn<RichDocument>("doc");
        const Block para = Block::paragraph({Span::plain(u"text"_s)});
        QTest::newRow("table only") << withIds({{table(2, 2)}});
        QTest::newRow("two tables") << withIds({{table(1, 2), table(2, 1)}});
        QTest::newRow("sandwich") << withIds({{para, table(2, 3), para}});
        QTest::newRow("empty cells") << withIds({{table(2, 2, {{0, 0, 1, 1, {Block::paragraph()}}, {0, 1, 1, 1, {Block::paragraph()}},
                                                                {1, 0, 1, 1, {Block::paragraph()}}, {1, 1, 1, 1, {Block::paragraph()}}})}});

        Block merged = table(3, 3, {
            {0, 0, 1, 3, {Block::paragraph({Span::plain(u"Header"_s, Bold)})}},
            {1, 0, 2, 1, {Block::paragraph({Span::plain(u"tall"_s)})}},
            {1, 1, 1, 1, {Block::listItem(ListKind::Check, 0, {Span::plain(u"in a cell"_s)}, true),
                          Block::listItem(ListKind::Check, 0, {Span::plain(u"second"_s)})}},
            {1, 2, 1, 1, {Block::paragraph()}},
            {2, 1, 1, 1, {Block::paragraph({Span::plain(u"a"_s)}), Block::paragraph({Span::plain(u"b"_s)})}},
            {2, 2, 1, 1, {Block::paragraph()}},
        });
        QTest::newRow("merged cells and lists") << withIds({{para, merged}});
    }

    void tableLayouts()
    {
        QFETCH(RichDocument, doc);
        QTextDocument text;
        COMPARE_DOCS(viaEditor(doc, &text), doc);
    }

    void listNumberingRestartsAfterShallowerItem()
    {
        const RichDocument doc = withIds({{
            Block::listItem(ListKind::Ordered, 0, {Span::plain(u"one"_s)}),
            Block::listItem(ListKind::Ordered, 1, {Span::plain(u"a"_s)}),
            Block::listItem(ListKind::Ordered, 0, {Span::plain(u"two"_s)}),
            Block::listItem(ListKind::Ordered, 1, {Span::plain(u"a again"_s)}),
        }});
        QTextDocument text;
        viaEditor(doc, &text);
        const QTextBlock second = text.findBlockByNumber(1);
        const QTextBlock fourth = text.findBlockByNumber(3);
        QVERIFY(second.textList() && fourth.textList());
        QCOMPARE(fourth.textList()->itemNumber(fourth), 0);
        QCOMPARE(text.findBlockByNumber(2).textList()->itemNumber(text.findBlockByNumber(2)), 1);
    }

    void splitBlockKeepsUniqueIds()
    {
        QTextDocument text;
        const RichDocument before = viaEditor(fixture(), &text);
        QTextCursor cursor(&text);
        cursor.setPosition(text.findBlockByNumber(1).position() + 5);
        cursor.insertBlock(); // Enter in the middle of the first paragraph
        const RichDocument after = DocumentConverter::read(&text);

        QSet<QString> ids;
        std::function<void(const QList<Block> &)> collect = [&](const QList<Block> &blocks) {
            for (const Block &b : blocks) {
                QVERIFY2(!ids.contains(b.id), qPrintable(b.id));
                ids.insert(b.id);
                for (const TableCell &c : b.cells)
                    collect(c.blocks);
            }
        };
        collect(after.blocks);
        QCOMPARE(after.blocks[1].id, before.blocks[1].id); // first half keeps identity
        QVERIFY(after.blocks[2].id != before.blocks[1].id);
    }

    void pasteGetsFreshIds()
    {
        QTextDocument text;
        const RichDocument original = viaEditor(fixture(), &text);
        QTextCursor select(&text);
        select.setPosition(text.findBlockByNumber(3).position());
        select.setPosition(text.findBlockByNumber(6).position(), QTextCursor::KeepAnchor);
        const QTextDocumentFragment copied = select.selection();
        QTextCursor end(&text);
        end.movePosition(QTextCursor::End);
        end.insertBlock();
        end.insertFragment(copied);
        const RichDocument after = DocumentConverter::read(&text);
        QVERIFY(after.blocks.size() > original.blocks.size());
        QSet<QString> ids;
        for (const Block &b : after.blocks) {
            QVERIFY(!ids.contains(b.id));
            ids.insert(b.id);
        }
    }

    void selectionExport()
    {
        QTextDocument text;
        viaEditor(fixture(), &text);
        QTextCursor cursor(&text);
        cursor.setPosition(text.findBlockByNumber(8).position()); // checklist
        cursor.setPosition(text.findBlockByNumber(10).position() + 3, QTextCursor::KeepAnchor);
        const RichDocument part = DocumentConverter::readSelection(cursor);
        if (part.blocks.size() != 3)
            qWarning().noquote() << describe(part);
        QCOMPARE(part.blocks.size(), 3);
        QCOMPARE(part.blocks[0].list, ListKind::Check);
        QVERIFY(part.blocks[0].checked);
        QCOMPARE(part.blocks[2].indent, 1);
        QCOMPARE(part.blocks[2].plainText(), u"Use"_s);
    }

    void htmlPasteIsNormalized()
    {
        ConversionReport report;
        const RichDocument doc = DocumentConverter::fromHtml(
            u"<h1 style='color:red;font-family:Comic Sans'>Title</h1>"
            "<p><b>bold</b> <span style='font-size:30px'>big</span> "
            "<a href='https://example.com'>link</a> <a href='javascript:alert(1)'>bad</a>"
            "<img src='https://example.com/x.png'></p>"
            "<ul><li>one<ul><li>nested</li></ul></li></ul>"
            "<ol><li>first</li></ol>"_s, &report);

        QCOMPARE(doc.blocks[0].type, Block::Type::Heading);
        QCOMPARE(doc.blocks[0].plainText(), u"Title"_s);
        const Block &para = doc.blocks[1];
        QCOMPARE(para.spans[0].marks, quint32(Bold));
        QCOMPARE(para.spans[0].text, u"bold"_s);
        bool sawLink = false;
        for (const Span &s : para.spans) {
            if (s.href == u"https://example.com")
                sawLink = true;
            QVERIFY(!s.href.startsWith(u"javascript"));
        }
        QVERIFY(sawLink);
        QCOMPARE(doc.blocks[2].list, ListKind::Bullet);
        QCOMPARE(doc.blocks[3].indent, 1);
        QCOMPARE(doc.blocks[4].list, ListKind::Ordered);
        QCOMPARE(report.warnings.size(), 2); // unsupported link, web image
    }

    void findsTags_data()
    {
        QTest::addColumn<QString>("text");
        QTest::addColumn<QStringList>("tags");
        QTest::newRow("simple") << u"Plan the #launch and #Q3-review today"_s << QStringList{u"launch"_s, u"q3-review"_s};
        QTest::newRow("start and punctuation") << u"#todo: call (#Dana) “#quoted”"_s
                                               << QStringList{u"todo"_s, u"dana"_s, u"quoted"_s};
        QTest::newRow("not tags") << u"issue #12, url.com/#anchor, a#b, ##, # space"_s << QStringList{};
        QTest::newRow("nested and unicode") << u"#work/clients #café #2026-plan"_s
                                            << QStringList{u"work/clients"_s, u"café"_s, u"2026-plan"_s};
        QTest::newRow("trailing separators") << u"#draft- and #ideas/"_s << QStringList{u"draft"_s, u"ideas"_s};
    }

    void findsTags()
    {
        QFETCH(QString, text);
        QFETCH(QStringList, tags);
        QStringList found;
        for (const TagMatch &m : findTags(text)) {
            found << m.tag;
            QCOMPARE(text.mid(m.start, m.length).toLower(), u'#' + m.tag);
        }
        QCOMPARE(found, tags);
    }

    void documentTagsSkipCodeAndLinks()
    {
        RichDocument doc;
        doc.blocks << Block::paragraph({Span::plain(u"Ship #v2 "_s), Span::plain(u"#notatag"_s, Code),
                                        Span::plain(u" see "_s), Span::plain(u"#alsonot"_s, 0, u"https://x.com/#alsonot"_s)});
        doc.blocks << Block::listItem(ListKind::Check, 0, {Span::plain(u"#V2 again, #errands"_s)});
        QCOMPARE(doc.tags(), (QStringList{u"v2"_s, u"errands"_s}));
    }

    void rejectsInvalidDocuments_data()
    {
        QTest::addColumn<QByteArray>("json");
        QTest::newRow("future schema") << QByteArray(R"({"schema":2,"blocks":[]})");
        QTest::newRow("unknown block") << QByteArray(R"({"schema":1,"blocks":[{"type":"video"}]})");
        QTest::newRow("path traversal") << QByteArray(
            R"({"schema":1,"blocks":[{"type":"paragraph","spans":[{"image":"../../etc/passwd"}]}]})");
        QTest::newRow("bad attachment") << QByteArray(
            R"({"schema":1,"blocks":[{"type":"paragraph","spans":[{"attachment":"x"}]}]})");
        QTest::newRow("unknown mark") << QByteArray(
            R"({"schema":1,"blocks":[{"type":"paragraph","spans":[{"text":"a","marks":["blink"]}]}]})");
        QTest::newRow("cell outside table") << QByteArray(
            R"({"schema":1,"blocks":[{"type":"table","rows":1,"columns":1,"cells":[{"row":1,"col":0,"blocks":[]}]}]})");
    }

    void rejectsInvalidDocuments()
    {
        QFETCH(QByteArray, json);
        QString error;
        QVERIFY(!RichDocument::fromJsonBytes(json, &error));
        QVERIFY(!error.isEmpty());
    }

    void largeDocumentTiming()
    {
        RichDocument doc;
        for (int i = 0; i < 5000; ++i) {
            doc.blocks << Block::paragraph({Span::plain(u"Paragraph %1 with "_s.arg(i)),
                                            Span::plain(u"some bold"_s, Bold),
                                            Span::plain(u" and plain text to make it realistic."_s)});
            if (i % 50 == 0)
                doc.blocks << Block::listItem(ListKind::Check, 0, {Span::plain(u"task"_s)});
        }
        doc.assignMissingIds();
        QTextDocument text;
        QElapsedTimer timer;
        timer.start();
        DocumentConverter::load(doc, &text, DocumentStyle{});
        const qint64 loadMs = timer.restart();
        const RichDocument back = DocumentConverter::read(&text);
        const qint64 readMs = timer.restart();
        const QByteArray json = back.toJsonBytes();
        const qint64 jsonMs = timer.elapsed();
        qInfo("5100 blocks: load %lld ms, read %lld ms, serialize %lld ms (%lld KB)",
              loadMs, readMs, jsonMs, qint64(json.size() / 1024));
        COMPARE_DOCS(back, doc);
    }
};

QTEST_MAIN(TestDocument)
#include "tst_document.moc"
