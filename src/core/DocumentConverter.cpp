#include "DocumentConverter.h"

#include <QFont>
#include <QHash>
#include <QSet>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextFrame>
#include <QTextList>
#include <QTextTable>
#include <QUrl>
#include <QUuid>

#include <algorithm>
#include <cmath>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

constexpr char16_t kObjectReplacement = 0xFFFC;
constexpr auto kTableIdProperty = "onotesId";

class BlockIdentity : public QTextBlockUserData
{
public:
    explicit BlockIdentity(QString id) : id(std::move(id)) {}
    QString id;
};

// ---------------------------------------------------------------- writing

class Writer
{
public:
    Writer(QTextCursor cursor, const DocumentStyle &style)
        : m_cursor(std::move(cursor)), m_style(style)
    {
    }

    void writeBlocks(const QList<Block> &blocks)
    {
        for (const Block &block : blocks) {
            if (block.type == Block::Type::Table)
                writeTable(block);
            else
                writeTextBlock(block);
        }
    }

private:
    // Positions the cursor on an empty block ready to receive content,
    // without inheriting list membership from the previous block.
    void beginBlock()
    {
        if (!m_fresh) {
            m_cursor.insertBlock();
            if (QTextList *list = m_cursor.currentList())
                list->remove(m_cursor.block());
        }
        m_fresh = false;
    }

    void writeTextBlock(const Block &block)
    {
        beginBlock();

        QTextBlockFormat bf;
        bf.setLineHeight(m_style.lineHeightPercent, QTextBlockFormat::ProportionalHeight);
        if (block.type == Block::Type::Heading) {
            bf.setHeadingLevel(block.level);
            bf.setTopMargin(block.level == 1 ? 4 : 10);
            bf.setBottomMargin(4);
        }
        if (block.type == Block::Type::ListItem && block.list == ListKind::Check) {
            bf.setMarker(block.checked ? QTextBlockFormat::MarkerType::Checked
                                       : QTextBlockFormat::MarkerType::Unchecked);
        }
        m_cursor.setBlockFormat(bf);

        const QTextCharFormat base = DocumentConverter::charFormatForBlock(block, m_style);
        m_cursor.setBlockCharFormat(withCellProperties(m_cursor.blockCharFormat(), base));

        if (block.type == Block::Type::ListItem)
            attachToList(block);
        else
            m_lists.clear();

        for (const Span &span : block.spans)
            writeSpan(span, base);

        m_cursor.setCharFormat(base);
        DocumentConverter::setBlockId(m_cursor.block(), block.id);
    }

    void attachToList(const Block &block)
    {
        // Lists deeper than this item end here, so later siblings at those
        // depths restart their numbering.
        for (auto it = m_lists.begin(); it != m_lists.end();) {
            if (it.key() > block.indent)
                it = m_lists.erase(it);
            else
                ++it;
        }
        auto existing = m_lists.find(block.indent);
        if (existing != m_lists.end() && existing->kind == block.list) {
            existing->list->add(m_cursor.block());
            return;
        }
        QTextList *list = m_cursor.createList(DocumentConverter::listFormat(block.list, block.indent));
        m_lists.insert(block.indent, {block.list, list});
    }

    void writeSpan(const Span &span, const QTextCharFormat &base)
    {
        switch (span.kind) {
        case Span::Kind::Text: {
            QTextCharFormat fmt = base;
            DocumentConverter::applyMarks(fmt, span.marks, span.href, m_style);
            QString text = span.text;
            text.replace(u'\n', QChar::LineSeparator);
            text.remove(QChar(kObjectReplacement));
            m_cursor.insertText(text, fmt);
            break;
        }
        case Span::Kind::Image: {
            QTextImageFormat fmt;
            fmt.setName(DocumentConverter::imageResourceName(span.ref));
            if (span.width > 0 && span.height > 0) {
                qreal w = span.width;
                qreal h = span.height;
                if (w > m_style.maxImageWidth) {
                    h = h * m_style.maxImageWidth / w;
                    w = m_style.maxImageWidth;
                }
                fmt.setWidth(w);
                fmt.setHeight(h);
            }
            fmt.setProperty(QTextFormat::ImageAltText, span.alt);
            // Keep the logical size from the schema so a narrower display
            // does not rewrite the stored dimensions.
            fmt.setProperty(TextProperty::ImageLogicalSize, QSize(span.width, span.height));
            m_cursor.insertImage(fmt);
            break;
        }
        case Span::Kind::Attachment: {
            QTextImageFormat fmt;
            fmt.setName(DocumentConverter::attachmentResourceName(span.ref));
            fmt.setWidth(m_style.attachmentChipSize.width());
            fmt.setHeight(m_style.attachmentChipSize.height());
            m_cursor.insertImage(fmt);
            break;
        }
        }
    }

    void writeTable(const Block &block)
    {
        m_lists.clear();
        QTextTable *table = m_cursor.insertTable(block.rows, block.columns,
                                                 DocumentConverter::tableFormat(m_style));
        table->setProperty(kTableIdProperty, block.id);

        for (const TableCell &cell : block.cells) {
            if (cell.rowSpan > 1 || cell.columnSpan > 1)
                table->mergeCells(cell.row, cell.column, cell.rowSpan, cell.columnSpan);
        }
        for (const TableCell &cell : block.cells) {
            const QTextTableCell target = table->cellAt(cell.row, cell.column);
            Writer inner(target.firstCursorPosition(), m_style);
            inner.writeBlocks(cell.blocks);
        }

        // Qt always keeps a block after a table; the next block reuses it.
        m_cursor = table->lastCursorPosition();
        m_cursor.movePosition(QTextCursor::NextBlock);
        m_fresh = true;
    }

    // The first block of a table cell carries the cell's own format (spans,
    // padding) in its block char format; replacing it would unmerge cells.
    static QTextCharFormat withCellProperties(const QTextCharFormat &current, const QTextCharFormat &base)
    {
        QTextCharFormat out = base;
        const auto props = current.properties();
        for (auto it = props.cbegin(); it != props.cend(); ++it) {
            if (it.key() >= QTextFormat::TableCellRowSpan && it.key() < QTextFormat::ImageName)
                out.setProperty(it.key(), it.value());
        }
        return out;
    }

    struct ActiveList {
        ListKind kind;
        QTextList *list;
    };

    QTextCursor m_cursor;
    const DocumentStyle &m_style;
    bool m_fresh = true;
    QHash<int, ActiveList> m_lists;
};

// ---------------------------------------------------------------- reading

class Reader
{
public:
    explicit Reader(ConversionReport *report) : m_report(report) {}

    QList<Block> readFrame(QTextFrame::iterator it)
    {
        struct Item {
            QTextBlock block;
            QTextFrame *frame = nullptr;
        };
        QList<Item> items;
        for (; !it.atEnd(); ++it) {
            if (QTextFrame *frame = it.currentFrame())
                items.append(Item{QTextBlock(), frame});
            else if (it.currentBlock().isValid())
                items.append(Item{it.currentBlock(), nullptr});
        }

        QList<Block> blocks;
        for (qsizetype i = 0; i < items.size(); ++i) {
            const Item &item = items[i];
            if (item.frame) {
                if (auto *table = qobject_cast<QTextTable *>(item.frame)) {
                    blocks.append(readTable(table));
                } else {
                    warn(u"Framed content was flattened into paragraphs."_s);
                    blocks.append(readFrame(item.frame->begin()));
                }
                continue;
            }
            if (isStructural(items, i))
                continue;
            blocks.append(readBlock(item.block));
        }
        return blocks;
    }

private:
    // Qt requires a text block before and after every table in a frame. An
    // empty paragraph that only exists to satisfy that is not content.
    template<typename Items>
    static bool isStructural(const Items &items, qsizetype i)
    {
        const QTextBlock &block = items[i].block;
        if (block.length() > 1 || block.textList())
            return false;
        const bool prevTable = i > 0 && items[i - 1].frame;
        const bool nextTable = i + 1 < items.size() && items[i + 1].frame;
        const bool prevBoundary = i == 0 || prevTable;
        const bool nextBoundary = i + 1 == items.size() || nextTable;
        return prevBoundary && nextBoundary && (prevTable || nextTable);
    }

    QString claimId(QString id)
    {
        if (id.isEmpty() || m_seen.contains(id))
            id = newId();
        m_seen.insert(id);
        return id;
    }

    Block readBlock(const QTextBlock &textBlock)
    {
        Block block;
        const QTextBlockFormat bf = textBlock.blockFormat();

        QString id = DocumentConverter::blockId(textBlock);
        const QString claimed = claimId(id);
        if (claimed != id)
            DocumentConverter::setBlockId(textBlock, claimed);
        block.id = claimed;

        if (textBlock.textList()) {
            const QTextListFormat lf = textBlock.textList()->format();
            block.type = Block::Type::ListItem;
            block.list = DocumentConverter::listKindOf(textBlock);
            block.indent = std::clamp(lf.indent() - 1, 0, 16);
            block.checked = bf.marker() == QTextBlockFormat::MarkerType::Checked;
        } else if (bf.headingLevel() > 0) {
            block.type = Block::Type::Heading;
            block.level = std::min(bf.headingLevel(), 3);
        } else if (bf.marker() != QTextBlockFormat::MarkerType::NoMarker) {
            // A checkbox marker outside a list (possible from HTML/Markdown).
            block.type = Block::Type::ListItem;
            block.list = ListKind::Check;
            block.checked = bf.marker() == QTextBlockFormat::MarkerType::Checked;
        }

        const bool inHeading = block.type == Block::Type::Heading;
        for (auto it = textBlock.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            if (!fragment.isValid())
                continue;
            const QTextCharFormat cf = fragment.charFormat();
            const QString text = fragment.text();
            if (cf.isImageFormat()) {
                readObjects(cf.toImageFormat(), text.count(QChar(kObjectReplacement)), &block);
                continue;
            }
            QString clean = text;
            clean.remove(QChar(kObjectReplacement));
            clean.replace(QChar::LineSeparator, u'\n');
            clean.replace(QChar::ParagraphSeparator, u'\n');
            if (clean.isEmpty())
                continue;

            QString href;
            if (cf.isAnchor()) {
                href = cf.anchorHref();
                if (!DocumentConverter::isAllowedLink(href)) {
                    if (!href.isEmpty())
                        warn(u"A link with an unsupported address was removed."_s);
                    href.clear();
                }
            }
            appendText(&block, clean, DocumentConverter::marksOf(cf, inHeading), href);
        }
        return block;
    }

    void readObjects(const QTextImageFormat &fmt, qsizetype count, Block *block)
    {
        const QString name = fmt.name();
        for (qsizetype n = 0; n < count; ++n) {
            if (const QString hash = DocumentConverter::blobHashFromResource(name); !hash.isEmpty()) {
                QSize logical = fmt.property(TextProperty::ImageLogicalSize).toSize();
                if (!logical.isValid() || logical.isEmpty())
                    logical = QSize(qRound(fmt.width()), qRound(fmt.height()));
                block->spans.append(Span::image(hash, logical.width(), logical.height(),
                                                fmt.property(QTextFormat::ImageAltText).toString()));
            } else if (const QString id = DocumentConverter::attachmentIdFromResource(name); !id.isEmpty()) {
                block->spans.append(Span::attachment(id));
            } else {
                warn(u"An image that isn't stored in this library was not kept."_s);
            }
        }
    }

    static void appendText(Block *block, const QString &text, quint32 marks, const QString &href)
    {
        if (!block->spans.isEmpty()) {
            Span &last = block->spans.last();
            if (last.kind == Span::Kind::Text && last.marks == marks && last.href == href) {
                last.text += text;
                return;
            }
        }
        block->spans.append(Span::plain(text, marks, href));
    }

    Block readTable(QTextTable *table)
    {
        Block block;
        block.type = Block::Type::Table;
        const QString id = table->property(kTableIdProperty).toString();
        block.id = claimId(id);
        if (block.id != id)
            table->setProperty(kTableIdProperty, block.id);
        block.rows = table->rows();
        block.columns = table->columns();
        for (int r = 0; r < block.rows; ++r) {
            for (int c = 0; c < block.columns; ++c) {
                const QTextTableCell cell = table->cellAt(r, c);
                if (!cell.isValid() || cell.row() != r || cell.column() != c)
                    continue; // covered by a merged cell
                TableCell out;
                out.row = r;
                out.column = c;
                out.rowSpan = cell.rowSpan();
                out.columnSpan = cell.columnSpan();
                out.blocks = readFrame(cell.begin());
                block.cells.append(out);
            }
        }
        return block;
    }

    void warn(const QString &message)
    {
        if (m_report)
            m_report->warn(message);
    }

    ConversionReport *m_report;
    QSet<QString> m_seen;
};

} // namespace

qreal DocumentStyle::headingPixelSize(int level) const
{
    switch (level) {
    case 1: return std::round(bodyPixelSize * 1.75);
    case 2: return std::round(bodyPixelSize * 1.35);
    default: return std::round(bodyPixelSize * 1.15);
    }
}

void ConversionReport::warn(const QString &message)
{
    if (!warnings.contains(message))
        warnings.append(message);
}

void DocumentConverter::load(const RichDocument &source, QTextDocument *doc, const DocumentStyle &style)
{
    doc->setUndoRedoEnabled(false);
    doc->clear();
    {
        QTextCursor cursor(doc);
        cursor.beginEditBlock();
        Writer writer(cursor, style);
        writer.writeBlocks(source.blocks);
        cursor.endEditBlock();
    }
    doc->setUndoRedoEnabled(true);
    doc->setModified(false);
}

void DocumentConverter::insert(const RichDocument &source, QTextCursor &cursor, const DocumentStyle &style)
{
    QTextDocument scratch;
    scratch.setDefaultFont(cursor.document()->defaultFont());
    load(source, &scratch, style);
    cursor.insertFragment(QTextDocumentFragment(&scratch));
}

RichDocument DocumentConverter::read(QTextDocument *doc, ConversionReport *report)
{
    Reader reader(report);
    RichDocument out;
    out.blocks = reader.readFrame(doc->rootFrame()->begin());
    if (out.blocks.isEmpty())
        out.blocks.append(Block::paragraph());
    return out;
}

RichDocument DocumentConverter::readSelection(const QTextCursor &cursor, ConversionReport *report)
{
    QTextDocument scratch;
    QTextCursor(&scratch).insertFragment(cursor.selection());
    RichDocument out = read(&scratch, report);
    // A selection that starts at a block boundary is inserted after the
    // scratch document's initial empty block; that block is not content.
    const int start = cursor.selectionStart();
    const bool startsAtBlock = cursor.document()->findBlock(start).position() == start;
    if (startsAtBlock && out.blocks.size() > 1 && out.blocks.first().isEmptyParagraph())
        out.blocks.removeFirst();
    return out;
}

RichDocument DocumentConverter::fromHtml(const QString &html, ConversionReport *report)
{
    QTextDocument scratch;
    scratch.setHtml(html);
    return read(&scratch, report);
}

RichDocument DocumentConverter::fromPlainText(const QString &text)
{
    RichDocument doc;
    QString normalized = text;
    normalized.replace(u"\r\n"_s, u"\n"_s);
    for (const QString &line : normalized.split(u'\n')) {
        Block b = Block::paragraph();
        if (!line.isEmpty())
            b.spans.append(Span::plain(line));
        doc.blocks.append(b);
    }
    return doc;
}

QTextCharFormat DocumentConverter::charFormatForBlock(const Block &block, const DocumentStyle &style)
{
    QTextCharFormat fmt;
    fmt.setForeground(style.text);
    if (block.type == Block::Type::Heading) {
        fmt.setProperty(QTextFormat::FontPixelSize, int(style.headingPixelSize(block.level)));
        fmt.setFontWeight(QFont::Bold);
    }
    return fmt;
}

void DocumentConverter::applyMarks(QTextCharFormat &fmt, quint32 marks, const QString &href,
                                   const DocumentStyle &style)
{
    if (marks & Bold)
        fmt.setFontWeight(QFont::Bold);
    if (marks & Italic)
        fmt.setFontItalic(true);
    if (marks & Underline)
        fmt.setFontUnderline(true);
    if (marks & Strikethrough)
        fmt.setFontStrikeOut(true);
    if (marks & Highlight) {
        fmt.setBackground(style.highlight);
        fmt.setProperty(TextProperty::Highlight, true);
    }
    if (marks & Code) {
        fmt.setFontFamilies({style.codeFamily});
        fmt.setFontFixedPitch(true);
        fmt.setProperty(TextProperty::Code, true);
    }
    if (!href.isEmpty()) {
        fmt.setAnchor(true);
        fmt.setAnchorHref(href);
        fmt.setForeground(style.link);
        fmt.setFontUnderline(true);
    }
}

quint32 DocumentConverter::marksOf(const QTextCharFormat &cf, bool inHeading)
{
    quint32 marks = 0;
    if (!inHeading && cf.fontWeight() >= QFont::DemiBold)
        marks |= Bold;
    if (cf.fontItalic())
        marks |= Italic;
    // Links are drawn underlined; that is presentation, not a mark.
    if (cf.fontUnderline() && !cf.isAnchor())
        marks |= Underline;
    if (cf.fontStrikeOut())
        marks |= Strikethrough;
    if (cf.boolProperty(TextProperty::Highlight))
        marks |= Highlight;
    if (cf.boolProperty(TextProperty::Code) || cf.fontFixedPitch())
        marks |= Code;
    return marks;
}

ListKind DocumentConverter::listKindOf(const QTextBlock &block)
{
    const QTextList *list = block.textList();
    if (!list)
        return ListKind::None;
    if (block.blockFormat().marker() != QTextBlockFormat::MarkerType::NoMarker)
        return ListKind::Check;
    const QTextListFormat lf = list->format();
    if (lf.hasProperty(TextProperty::ListKind))
        return static_cast<ListKind>(lf.intProperty(TextProperty::ListKind));
    switch (lf.style()) {
    case QTextListFormat::ListDecimal:
    case QTextListFormat::ListLowerAlpha:
    case QTextListFormat::ListUpperAlpha:
    case QTextListFormat::ListLowerRoman:
    case QTextListFormat::ListUpperRoman:
        return ListKind::Ordered;
    default:
        return ListKind::Bullet;
    }
}

QTextListFormat DocumentConverter::listFormat(ListKind kind, int indent)
{
    static constexpr QTextListFormat::Style bullets[] = {
        QTextListFormat::ListDisc, QTextListFormat::ListCircle, QTextListFormat::ListSquare};
    static constexpr QTextListFormat::Style ordered[] = {
        QTextListFormat::ListDecimal, QTextListFormat::ListLowerAlpha, QTextListFormat::ListLowerRoman};
    QTextListFormat lf;
    lf.setIndent(indent + 1);
    lf.setStyle(kind == ListKind::Ordered ? ordered[indent % 3] : bullets[indent % 3]);
    lf.setProperty(TextProperty::ListKind, int(kind));
    return lf;
}

QTextTableFormat DocumentConverter::tableFormat(const DocumentStyle &style)
{
    QTextTableFormat tf;
    tf.setBorder(1);
    tf.setBorderBrush(style.tableBorder);
    tf.setBorderStyle(QTextFrameFormat::BorderStyle_Solid);
    tf.setBorderCollapse(true);
    tf.setCellPadding(6);
    tf.setCellSpacing(0);
    tf.setTopMargin(8);
    tf.setBottomMargin(8);
    tf.setWidth(QTextLength(QTextLength::PercentageLength, 100));
    return tf;
}

QString DocumentConverter::imageResourceName(const QString &blobHash)
{
    return u"blob:"_s + blobHash;
}

QString DocumentConverter::attachmentResourceName(const QString &attachmentId)
{
    return u"attachment:"_s + attachmentId;
}

QString DocumentConverter::blobHashFromResource(const QString &name)
{
    if (!name.startsWith(u"blob:"))
        return {};
    const QString hash = name.mid(5);
    return isValidBlobHash(hash) ? hash : QString();
}

QString DocumentConverter::attachmentIdFromResource(const QString &name)
{
    if (!name.startsWith(u"attachment:"))
        return {};
    const QString id = name.mid(11);
    return QUuid::fromString(id).isNull() ? QString() : id;
}

QString DocumentConverter::blockId(const QTextBlock &block)
{
    if (auto *identity = dynamic_cast<BlockIdentity *>(block.userData()))
        return identity->id;
    return {};
}

void DocumentConverter::setBlockId(QTextBlock block, const QString &id)
{
    if (id.isEmpty())
        return;
    block.setUserData(new BlockIdentity(id));
}

bool DocumentConverter::isAllowedLink(const QString &href)
{
    const QUrl url(href);
    if (!url.isValid())
        return false;
    static const QStringList schemes = {u"http"_s, u"https"_s, u"mailto"_s, u"note"_s};
    return schemes.contains(url.scheme().toLower());
}

} // namespace onotes
