#include "SampleLibrary.h"

#include "LibraryService.h"

#include <QBuffer>
#include <QImage>
#include <QPainter>
#include <QPdfWriter>
#include <QTemporaryDir>

using namespace Qt::StringLiterals;

namespace onotes::SampleLibrary {

namespace {

Span t(const QString &text, quint32 marks = 0)
{
    return Span::plain(text, marks);
}

Block p(const QList<Span> &spans)
{
    return Block::paragraph(spans);
}

Block li(ListKind kind, int indent, const QList<Span> &spans, bool checked = false)
{
    return Block::listItem(kind, indent, spans, checked);
}

TableCell cell(int row, int column, const QList<Span> &spans)
{
    TableCell c;
    c.row = row;
    c.column = column;
    c.blocks = {Block::paragraph(spans)};
    return c;
}

} // namespace

RichDocument mixedContentNote(const QString &imageHash, const QString &attachmentId,
                              const QString &linkedNoteId)
{
    RichDocument doc;
    auto &b = doc.blocks;
    b << Block::heading(1, {t(u"Q3 planning — product sync"_s)});
    b << p({t(u"Met with Dana, Luis and Priya on 22 September. "_s),
            t(u"Decisions"_s, Bold),
            t(u" are below; open questions are at the end."_s)});

    b << Block::heading(2, {t(u"Decisions"_s)});
    b << li(ListKind::Bullet, 0, {t(u"Ship the offline importer in the October release"_s)});
    b << li(ListKind::Bullet, 1, {t(u"Markdown and plain text first; ENEX later"_s)});
    b << li(ListKind::Bullet, 1, {t(u"Report anything we can't convert instead of dropping it"_s, Italic)});
    b << li(ListKind::Bullet, 0, {t(u"Collapse the folder sidebar on narrow tiles"_s)});

    b << Block::heading(2, {t(u"Action items"_s)});
    b << li(ListKind::Check, 0, {t(u"Draft the migration guide"_s)}, true);
    b << li(ListKind::Check, 0, {t(u"Benchmark search on the 10,000-note library"_s)});
    b << li(ListKind::Check, 1, {t(u"Use the reference ThinkPad, not a dev box"_s)}, true);
    b << li(ListKind::Check, 1, {t(u"Record p95 without typing debounce"_s)});
    b << li(ListKind::Check, 0, {t(u"Review the "_s),
                                 Span::plain(u"vendor research"_s, 0, u"note://"_s + linkedNoteId),
                                 t(u" before Friday"_s)});

    b << Block::heading(2, {t(u"Owners"_s)});
    Block table;
    table.type = Block::Type::Table;
    table.rows = 4;
    table.columns = 3;
    table.cells = {
        cell(0, 0, {t(u"Owner"_s, Bold)}), cell(0, 1, {t(u"Task"_s, Bold)}), cell(0, 2, {t(u"Due"_s, Bold)}),
        cell(1, 0, {t(u"Dana"_s)}), cell(1, 1, {t(u"Importer error report"_s)}), cell(1, 2, {t(u"3 Oct"_s)}),
        cell(2, 0, {t(u"Luis"_s)}), cell(2, 1, {t(u"Search benchmark"_s)}), cell(2, 2, {t(u"8 Oct"_s)}),
        cell(3, 0, {t(u"Priya"_s)}), cell(3, 1, {t(u"Narrow-tile layout 🚀"_s)}), cell(3, 2, {t(u"10 Oct"_s)}),
    };
    b << table;

    b << Block::heading(2, {t(u"Attachments"_s)});
    b << p({t(u"Search latency from last sprint:"_s)});
    b << p({Span::image(imageHash, 800, 360, u"Bar chart of search latency by week"_s)});
    b << p({t(u"Roadmap draft: "_s), Span::attachment(attachmentId)});

    b << Block::heading(2, {t(u"Next week"_s)});
    b << li(ListKind::Ordered, 0, {t(u"Walk through the importer demo"_s)});
    b << li(ListKind::Ordered, 0, {t(u"Agree the release checklist"_s)});
    b << li(ListKind::Ordered, 1, {t(u"Accessibility pass"_s)});
    b << li(ListKind::Ordered, 1, {t(u"Recovery drills"_s)});
    b << li(ListKind::Ordered, 0, {t(u"Retro"_s)});

    b << Block::heading(3, {t(u"Open questions"_s)});
    b << p({t(u"Do we need "_s), t(u"locked notes"_s, Highlight), t(u" in v1? "_s),
            t(u"Probably not"_s, Strikethrough), t(u" — revisit after the beta."_s)});
    b << p({t(u"Try it: "_s), t(u"omarchy-notes --new \"Groceries\""_s, Code)});
    b << p({t(u"Background reading: "_s),
            Span::plain(u"SQLite FTS5"_s, 0, u"https://sqlite.org/fts5.html"_s),
            t(u" and the Qt rich text docs."_s, Underline)});
    b << p({t(u"Localized greeting for the demo:\nمرحبا بالعالم — שלום עולם — 你好"_s)});
    doc.assignMissingIds();
    return doc;
}

RichDocument vendorResearchNote()
{
    RichDocument doc;
    doc.blocks << Block::heading(1, {t(u"Vendor research"_s)});
    doc.blocks << p({t(u"Shortlist for the sync server, compared on self-hosting, "
                       "encryption at rest and export."_s)});
    doc.blocks << li(ListKind::Bullet, 0, {t(u"Option A — "_s), t(u"self-hosted"_s, Bold),
                                           t(u", Postgres, MIT"_s)});
    doc.blocks << li(ListKind::Bullet, 0, {t(u"Option B — managed, per-seat pricing"_s)});
    doc.blocks << li(ListKind::Bullet, 0, {t(u"Option C — "_s),
                                           Span::plain(u"CRDT docs"_s, 0, u"https://crdt.tech"_s)});
    doc.assignMissingIds();
    return doc;
}

RichDocument shoppingNote()
{
    RichDocument doc;
    doc.blocks << Block::heading(1, {t(u"Groceries"_s)});
    doc.blocks << li(ListKind::Check, 0, {t(u"Oat milk"_s)}, true);
    doc.blocks << li(ListKind::Check, 0, {t(u"Coffee beans"_s)});
    doc.blocks << li(ListKind::Check, 0, {t(u"Lemons (6)"_s)});
    doc.blocks << li(ListKind::Check, 0, {t(u"Basil"_s)}, true);
    doc.assignMissingIds();
    return doc;
}

QByteArray chartPng()
{
    QImage image(800, 360, QImage::Format_ARGB32_Premultiplied);
    image.fill(QColor(0xf6, 0xf6, 0xf4));
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    const int values[] = {182, 164, 151, 139, 128, 121, 117, 112};
    const int baseline = 320;
    painter.setPen(QPen(QColor(0xc8, 0xc8, 0xc4), 1));
    for (int y = baseline; y > 40; y -= 50)
        painter.drawLine(40, y, 780, y);
    for (int i = 0; i < 8; ++i) {
        const int h = values[i] * 250 / 200;
        painter.fillRect(QRectF(60 + i * 90, baseline - h, 56, h), QColor(0x5a, 0x7d, 0xb0));
    }
    painter.setPen(QColor(0x40, 0x40, 0x40));
    QFont font = painter.font();
    font.setPixelSize(18);
    painter.setFont(font);
    painter.drawText(QRect(40, 8, 740, 28), Qt::AlignLeft, u"Search p95 (ms), weeks 31–38"_s);
    painter.end();

    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    image.save(&buffer, "PNG");
    return bytes;
}

bool writeSamplePdf(const QString &path)
{
    QPdfWriter writer(path);
    writer.setTitle(u"Q3 roadmap draft"_s);
    writer.setPageSize(QPageSize(QPageSize::A4));
    QPainter painter;
    if (!painter.begin(&writer))
        return false;
    QFont font = painter.font();
    font.setPointSize(20);
    painter.setFont(font);
    painter.drawText(QRect(0, 0, writer.width(), 800), Qt::AlignLeft, u"Q3 roadmap — draft"_s);
    font.setPointSize(11);
    painter.setFont(font);
    painter.drawText(QRect(0, 1000, writer.width(), 4000), Qt::AlignLeft | Qt::TextWordWrap,
                     u"1. Offline importer\n2. Search benchmark\n3. Narrow-tile layout\n"
                     "4. Release checklist"_s);
    return painter.end();
}

QString seed(LibraryService &library, QString *error)
{
    auto vendor = library.createNote(vendorResearchNote(), error);
    if (!vendor || !library.createNote(shoppingNote(), error))
        return {};

    const auto imageHash = library.addImage(chartPng(), error);
    if (!imageHash)
        return {};

    // The attachment belongs to a note, so create the note first.
    auto mixed = library.createNote(RichDocument{{Block::paragraph()}}, error);
    if (!mixed)
        return {};

    QTemporaryDir scratch;
    const QString pdfPath = scratch.filePath(u"Q3 roadmap draft.pdf"_s);
    if (!writeSamplePdf(pdfPath)) {
        if (error)
            *error = u"Could not create the sample PDF."_s;
        return {};
    }
    const auto attachment = library.addAttachment(mixed->id, pdfPath, error);
    if (!attachment)
        return {};

    SaveRequest save;
    save.noteId = mixed->id;
    save.baseRevision = mixed->revision;
    save.body = mixedContentNote(*imageHash, attachment->id, vendor->id);
    library.saveNote(save);
    library.waitForIdle();
    return mixed->id;
}

} // namespace onotes::SampleLibrary
