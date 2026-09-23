#include "LibraryService.h"
#include "NoteExchange.h"
#include "SampleLibrary.h"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTemporaryDir>
#include <QTest>

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

void write(const QString &path, const QByteArray &data)
{
    QFile f(path);
    QVERIFY(f.open(QIODevice::WriteOnly));
    f.write(data);
}

QString read(const QString &path)
{
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()) : QString();
}

int countBlocks(const QList<Block> &blocks, Block::Type type)
{
    int n = 0;
    for (const Block &b : blocks) {
        n += b.type == type;
        for (const TableCell &c : b.cells)
            n += countBlocks(c.blocks, type);
    }
    return n;
}

} // namespace

class TestExchange : public QObject
{
    Q_OBJECT

    QTemporaryDir m_dir;
    std::unique_ptr<LibraryService> m_library;
    QString m_mixed;

private slots:
    void initTestCase()
    {
        m_library = std::make_unique<LibraryService>(LibraryPaths::at(m_dir.filePath(u"library"_s)));
        QString error;
        QVERIFY2(m_library->start(&error), qPrintable(error));
        m_mixed = SampleLibrary::seed(*m_library, &error);
        QVERIFY(!m_mixed.isEmpty());
    }

    void importsMarkdown()
    {
        const QString dir = m_dir.filePath(u"import"_s);
        QDir().mkpath(dir + u"/img"_s);
        write(dir + u"/img/chart one.png"_s, SampleLibrary::chartPng());
        write(dir + u"/Trip plan.md"_s, R"(---
tags: [travel]
created: 2026-01-01
---
# Lisbon trip

Pack light, see the **tiles**. #travel

- [x] Book flights
- [ ] Book hotel
  - [ ] Near Alfama

| Day | Plan |
|-----|------|
| Fri | Belém |

![chart](img/chart%20one.png)
![remote](https://example.com/photo.jpg)
)");
        const ImportReport report = importFiles(*m_library, {dir + u"/Trip plan.md"_s}, {});
        QCOMPARE(report.createdIds.size(), 1);
        QCOMPARE(report.warnings.size(), 1); // the web image, named
        QVERIFY(report.warnings[0].contains(u"Trip plan.md"_s));

        const RichDocument body = m_library->loadNote(report.createdIds[0])->body;
        QCOMPARE(body.title(), u"Lisbon trip"_s); // front matter stripped
        QCOMPARE(body.blocks[0].type, Block::Type::Heading);
        QVERIFY(body.tags().contains(u"travel"_s));
        int checks = 0, checked = 0, nested = 0;
        for (const Block &b : body.blocks) {
            if (b.list == ListKind::Check) {
                ++checks;
                checked += b.checked;
                nested += b.indent == 1;
            }
        }
        QCOMPARE(checks, 3);
        QCOMPARE(checked, 1);
        QCOMPARE(nested, 1);
        QCOMPARE(countBlocks(body.blocks, Block::Type::Table), 1);
        QCOMPARE(body.referencedBlobs().size(), 1); // stored in the library
        QVERIFY(QFile::exists(m_library->blobPath(body.referencedBlobs()[0])));
        QVERIFY(m_library->verify().isEmpty());
    }

    void importsPlainTextAndNamesUntitledFiles()
    {
        const QString path = m_dir.filePath(u"Shopping list.txt"_s);
        write(path, "\n\nmilk\neggs\n");
        const ImportReport report = importFiles(*m_library, {path, m_dir.filePath(u"missing.md"_s)}, {});
        QCOMPARE(report.createdIds.size(), 1);
        QCOMPARE(report.warnings.size(), 1);
        QCOMPARE(m_library->loadNote(report.createdIds[0])->body.title(), u"Shopping list"_s);
    }

    void exportsMarkdownWithFiles()
    {
        const QString out = m_dir.filePath(u"export/Q3 planning.md"_s);
        QDir().mkpath(m_dir.filePath(u"export"_s));
        QString error;
        QVERIFY2(exportNote(*m_library, m_mixed, ExportFormat::Markdown, out, &error), qPrintable(error));
        const QString md = read(out);
        QVERIFY(md.contains(u"# Q3 planning"_s));
        QVERIFY(md.contains(u"- [x] Draft the migration guide"_s));
        QVERIFY(md.contains(u"|Owner|"_s));
        QVERIFY(md.contains(u"~~Probably not~~"_s));
        QVERIFY(!md.contains(u"_ and the Qt rich"_s)); // underline isn't turned into italics
        QVERIFY(md.contains(u"](Q3%20planning%20files/image-1.png)"_s));
        QVERIFY(md.contains(u"(Q3%20planning%20files/Q3%20roadmap%20draft.pdf)"_s));
        QVERIFY(!md.contains(u"blob:"_s) && !md.contains(u"note://"_s));
        QVERIFY(QFile::exists(m_dir.filePath(u"export/Q3 planning files/image-1.png"_s)));
        QVERIFY(QFile::exists(m_dir.filePath(u"export/Q3 planning files/Q3 roadmap draft.pdf"_s)));

        // And it comes back in with its structure.
        const ImportReport again = importFiles(*m_library, {out}, {});
        QCOMPARE(again.createdIds.size(), 1);
        const RichDocument body = m_library->loadNote(again.createdIds[0])->body;
        QCOMPARE(countBlocks(body.blocks, Block::Type::Table), 1);
        QCOMPARE(body.referencedBlobs().size(), 1);
    }

    void exportsHtmlAsOneFile()
    {
        const QString out = m_dir.filePath(u"note.html"_s);
        QString error;
        QVERIFY2(exportNote(*m_library, m_mixed, ExportFormat::Html, out, &error), qPrintable(error));
        const QString html = read(out);
        QVERIFY(html.contains(u"<title>Q3 planning"_s));
        QVERIFY(html.contains(u"data:image/png;base64,"_s));
        QVERIFY(html.contains(u"<table"_s));
        QVERIFY(QFile::exists(m_dir.filePath(u"note files/Q3 roadmap draft.pdf"_s)));
    }

    void exportsPdf()
    {
        const QString out = m_dir.filePath(u"note.pdf"_s);
        QString error;
        QVERIFY2(exportNote(*m_library, m_mixed, ExportFormat::Pdf, out, &error), qPrintable(error));
        QFile pdf(out);
        QVERIFY(pdf.open(QIODevice::ReadOnly));
        QVERIFY(pdf.read(5) == "%PDF-");
        QVERIFY(pdf.size() > 5000);
        if (qEnvironmentVariableIsSet("ONOTES_KEEP_EXPORTS"))
            QFile::copy(out, qEnvironmentVariable("ONOTES_KEEP_EXPORTS") + u"/note.pdf"_s);
    }

    void exportsTheWholeLibrary()
    {
        const auto folder = m_library->createFolder(u"Work/Clients"_s); // name with a slash
        QVERIFY(folder);
        RichDocument linking;
        linking.blocks = {Block::paragraph({Span::plain(u"See "_s),
                                            Span::plain(u"the plan"_s, 0, u"note://"_s + m_mixed)})};
        linking.assignMissingIds();
        QVERIFY(m_library->createNote(linking, nullptr, folder->id));

        QString error;
        const auto root = exportLibraryAsMarkdown(*m_library, m_dir.path(), &error);
        QVERIFY2(root, qPrintable(error));
        QVERIFY(QFile::exists(*root + u"/Q3 planning — product sync.md"_s));
        QVERIFY(QFile::exists(*root + u"/Vendor research.md"_s));
        const QString linked = read(*root + u"/Work Clients/See the plan.md"_s);
        // (The title is taken twice after the re-import test, so either file is right.)
        static const QRegularExpression link(u"\\(\\.\\./Q3%20planning%20%E2%80%94%20product%20sync(%202)?\\.md\\)"_s);
        QVERIFY2(link.match(linked).hasMatch(), qPrintable(linked));
    }

    void fileNames()
    {
        QCOMPARE(fileNameForTitle(u"a/b: c?"_s), u"a b c"_s);
        QCOMPARE(fileNameForTitle(u"..hidden"_s), u"hidden"_s);
        QCOMPARE(fileNameForTitle(u"   "_s), u"Untitled note"_s);
    }
};

QTEST_MAIN(TestExchange)
#include "tst_exchange.moc"
