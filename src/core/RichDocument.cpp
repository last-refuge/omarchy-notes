#include "RichDocument.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QSet>
#include <QUuid>

#include <algorithm>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

struct MarkName {
    Mark mark;
    const char *name;
};

constexpr MarkName kMarkNames[] = {
    {Bold, "bold"},
    {Italic, "italic"},
    {Underline, "underline"},
    {Strikethrough, "strikethrough"},
    {Highlight, "highlight"},
    {Code, "code"},
};

const char *listKindName(ListKind kind)
{
    switch (kind) {
    case ListKind::Bullet: return "bullet";
    case ListKind::Ordered: return "ordered";
    case ListKind::Check: return "check";
    case ListKind::None: break;
    }
    return "bullet";
}

std::optional<ListKind> listKindFromName(const QString &name)
{
    if (name == u"bullet") return ListKind::Bullet;
    if (name == u"ordered") return ListKind::Ordered;
    if (name == u"check") return ListKind::Check;
    return std::nullopt;
}

QJsonObject spanToJson(const Span &span)
{
    QJsonObject o;
    switch (span.kind) {
    case Span::Kind::Text: {
        o[u"text"] = span.text;
        if (span.marks) {
            QJsonArray marks;
            for (const auto &m : kMarkNames) {
                if (span.marks & m.mark)
                    marks.append(QLatin1StringView(m.name));
            }
            o[u"marks"] = marks;
        }
        if (!span.href.isEmpty())
            o[u"href"] = span.href;
        break;
    }
    case Span::Kind::Image:
        o[u"image"] = span.ref;
        if (span.width > 0) o[u"width"] = span.width;
        if (span.height > 0) o[u"height"] = span.height;
        if (!span.alt.isEmpty()) o[u"alt"] = span.alt;
        break;
    case Span::Kind::Attachment:
        o[u"attachment"] = span.ref;
        break;
    }
    return o;
}

QJsonArray blocksToJson(const QList<Block> &blocks);

QJsonObject blockToJson(const Block &block)
{
    QJsonObject o;
    o[u"id"] = block.id;
    auto spans = [&] {
        QJsonArray a;
        for (const Span &s : block.spans)
            a.append(spanToJson(s));
        return a;
    };
    switch (block.type) {
    case Block::Type::Paragraph:
        o[u"type"] = u"paragraph"_s;
        o[u"spans"] = spans();
        break;
    case Block::Type::Heading:
        o[u"type"] = u"heading"_s;
        o[u"level"] = block.level;
        o[u"spans"] = spans();
        break;
    case Block::Type::ListItem:
        o[u"type"] = u"list"_s;
        o[u"list"] = QLatin1StringView(listKindName(block.list));
        o[u"indent"] = block.indent;
        if (block.list == ListKind::Check)
            o[u"checked"] = block.checked;
        o[u"spans"] = spans();
        break;
    case Block::Type::Table: {
        o[u"type"] = u"table"_s;
        o[u"rows"] = block.rows;
        o[u"columns"] = block.columns;
        QJsonArray cells;
        for (const TableCell &c : block.cells) {
            QJsonObject co;
            co[u"row"] = c.row;
            co[u"col"] = c.column;
            if (c.rowSpan != 1) co[u"rowSpan"] = c.rowSpan;
            if (c.columnSpan != 1) co[u"colSpan"] = c.columnSpan;
            co[u"blocks"] = blocksToJson(c.blocks);
            cells.append(co);
        }
        o[u"cells"] = cells;
        break;
    }
    }
    return o;
}

QJsonArray blocksToJson(const QList<Block> &blocks)
{
    QJsonArray a;
    for (const Block &b : blocks)
        a.append(blockToJson(b));
    return a;
}

bool fail(QString *error, const QString &message)
{
    if (error)
        *error = message;
    return false;
}

bool spanFromJson(const QJsonObject &o, Span *out, QString *error)
{
    Span s;
    if (o.contains(u"text")) {
        s.kind = Span::Kind::Text;
        s.text = o[u"text"].toString();
        for (const QJsonValue &v : o[u"marks"].toArray()) {
            const QString name = v.toString();
            bool known = false;
            for (const auto &m : kMarkNames) {
                if (name == QLatin1StringView(m.name)) {
                    s.marks |= m.mark;
                    known = true;
                }
            }
            if (!known)
                return fail(error, u"unknown mark '%1'"_s.arg(name));
        }
        s.href = o[u"href"].toString();
    } else if (o.contains(u"image")) {
        s.kind = Span::Kind::Image;
        s.ref = o[u"image"].toString();
        if (!isValidBlobHash(s.ref))
            return fail(error, u"invalid image reference"_s);
        s.width = o[u"width"].toInt();
        s.height = o[u"height"].toInt();
        s.alt = o[u"alt"].toString();
    } else if (o.contains(u"attachment")) {
        s.kind = Span::Kind::Attachment;
        s.ref = o[u"attachment"].toString();
        if (QUuid::fromString(s.ref).isNull())
            return fail(error, u"invalid attachment reference"_s);
    } else {
        return fail(error, u"span has no content"_s);
    }
    *out = s;
    return true;
}

bool blocksFromJson(const QJsonArray &array, QList<Block> *out, QString *error, int depth);

bool blockFromJson(const QJsonObject &o, Block *out, QString *error, int depth)
{
    Block b;
    b.id = o[u"id"].toString();
    const QString type = o[u"type"].toString();

    auto readSpans = [&]() -> bool {
        for (const QJsonValue &v : o[u"spans"].toArray()) {
            Span s;
            if (!spanFromJson(v.toObject(), &s, error))
                return false;
            b.spans.append(s);
        }
        return true;
    };

    if (type == u"paragraph") {
        b.type = Block::Type::Paragraph;
        if (!readSpans()) return false;
    } else if (type == u"heading") {
        b.type = Block::Type::Heading;
        b.level = o[u"level"].toInt();
        if (b.level < 1 || b.level > 3)
            return fail(error, u"heading level out of range"_s);
        if (!readSpans()) return false;
    } else if (type == u"list") {
        b.type = Block::Type::ListItem;
        const auto kind = listKindFromName(o[u"list"].toString());
        if (!kind)
            return fail(error, u"unknown list kind"_s);
        b.list = *kind;
        b.indent = o[u"indent"].toInt();
        if (b.indent < 0 || b.indent > 16)
            return fail(error, u"list indent out of range"_s);
        b.checked = b.list == ListKind::Check && o[u"checked"].toBool();
        if (!readSpans()) return false;
    } else if (type == u"table") {
        b.type = Block::Type::Table;
        if (depth > 4)
            return fail(error, u"tables nested too deeply"_s);
        b.rows = o[u"rows"].toInt();
        b.columns = o[u"columns"].toInt();
        if (b.rows < 1 || b.columns < 1 || b.rows > 1000 || b.columns > 64)
            return fail(error, u"table dimensions out of range"_s);
        for (const QJsonValue &v : o[u"cells"].toArray()) {
            const QJsonObject co = v.toObject();
            TableCell c;
            c.row = co[u"row"].toInt();
            c.column = co[u"col"].toInt();
            c.rowSpan = co[u"rowSpan"].toInt(1);
            c.columnSpan = co[u"colSpan"].toInt(1);
            if (c.row < 0 || c.column < 0 || c.rowSpan < 1 || c.columnSpan < 1
                || c.row + c.rowSpan > b.rows || c.column + c.columnSpan > b.columns)
                return fail(error, u"table cell out of range"_s);
            if (!blocksFromJson(co[u"blocks"].toArray(), &c.blocks, error, depth + 1))
                return false;
            // A cell always holds at least one paragraph, as in the editor.
            if (c.blocks.isEmpty())
                c.blocks.append(Block::paragraph());
            b.cells.append(c);
        }
    } else {
        return fail(error, u"unknown block type '%1'"_s.arg(type));
    }
    *out = b;
    return true;
}

bool blocksFromJson(const QJsonArray &array, QList<Block> *out, QString *error, int depth)
{
    for (const QJsonValue &v : array) {
        Block b;
        if (!blockFromJson(v.toObject(), &b, error, depth))
            return false;
        out->append(b);
    }
    return true;
}

void appendText(const QList<Block> &blocks, QString *out)
{
    for (const Block &b : blocks) {
        if (b.type == Block::Type::Table) {
            for (const TableCell &c : b.cells) {
                appendText(c.blocks, out);
            }
            continue;
        }
        const QString text = b.plainText();
        if (text.isEmpty())
            continue;
        if (!out->isEmpty())
            out->append(u'\n');
        out->append(text);
    }
}

template<typename Fn>
void forEachSpan(const QList<Block> &blocks, Fn fn)
{
    for (const Block &b : blocks) {
        for (const Span &s : b.spans)
            fn(s);
        for (const TableCell &c : b.cells)
            forEachSpan(c.blocks, fn);
    }
}

void assignIds(QList<Block> &blocks, QSet<QString> &seen)
{
    for (Block &b : blocks) {
        if (b.id.isEmpty() || seen.contains(b.id))
            b.id = newId();
        seen.insert(b.id);
        for (TableCell &c : b.cells)
            assignIds(c.blocks, seen);
    }
}

} // namespace

Span Span::plain(const QString &text, quint32 marks, const QString &href)
{
    Span s;
    s.text = text;
    s.marks = marks;
    s.href = href;
    return s;
}

Span Span::image(const QString &blobHash, int width, int height, const QString &alt)
{
    Span s;
    s.kind = Kind::Image;
    s.ref = blobHash;
    s.width = width;
    s.height = height;
    s.alt = alt;
    return s;
}

Span Span::attachment(const QString &attachmentId)
{
    Span s;
    s.kind = Kind::Attachment;
    s.ref = attachmentId;
    return s;
}

bool TableCell::operator==(const TableCell &o) const
{
    return row == o.row && column == o.column && rowSpan == o.rowSpan
        && columnSpan == o.columnSpan && blocks == o.blocks;
}

bool Block::operator==(const Block &o) const
{
    return type == o.type && id == o.id && level == o.level && list == o.list
        && indent == o.indent && checked == o.checked && spans == o.spans
        && rows == o.rows && columns == o.columns && cells == o.cells;
}

Block Block::paragraph(const QList<Span> &spans)
{
    Block b;
    b.spans = spans;
    return b;
}

Block Block::heading(int level, const QList<Span> &spans)
{
    Block b;
    b.type = Type::Heading;
    b.level = level;
    b.spans = spans;
    return b;
}

Block Block::listItem(ListKind kind, int indent, const QList<Span> &spans, bool checked)
{
    Block b;
    b.type = Type::ListItem;
    b.list = kind;
    b.indent = indent;
    b.spans = spans;
    b.checked = kind == ListKind::Check && checked;
    return b;
}

QString Block::plainText() const
{
    QString text;
    for (const Span &s : spans) {
        if (s.kind == Span::Kind::Text)
            text += s.text;
    }
    return text;
}

bool Block::isEmptyParagraph() const
{
    return type == Type::Paragraph && spans.isEmpty();
}

QString RichDocument::title() const
{
    QString text;
    appendText(blocks, &text);
    for (const QString &line : text.split(u'\n')) {
        const QString trimmed = line.trimmed();
        if (!trimmed.isEmpty())
            return trimmed.left(200);
    }
    return {};
}

QString RichDocument::snippet(int maxLength) const
{
    QString text;
    appendText(blocks, &text);
    QStringList lines = text.split(u'\n', Qt::SkipEmptyParts);
    // Drop the title line.
    while (!lines.isEmpty() && lines.first().trimmed().isEmpty())
        lines.removeFirst();
    if (!lines.isEmpty())
        lines.removeFirst();
    QString joined = lines.join(u' ').simplified();
    if (joined.size() > maxLength)
        joined = joined.left(maxLength - 1) + u'…';
    return joined;
}

QString RichDocument::plainText() const
{
    QString text;
    appendText(blocks, &text);
    return text;
}

QStringList RichDocument::referencedBlobs() const
{
    QStringList out;
    forEachSpan(blocks, [&](const Span &s) {
        if (s.kind == Span::Kind::Image && !out.contains(s.ref))
            out.append(s.ref);
    });
    return out;
}

QStringList RichDocument::referencedAttachments() const
{
    QStringList out;
    forEachSpan(blocks, [&](const Span &s) {
        if (s.kind == Span::Kind::Attachment && !out.contains(s.ref))
            out.append(s.ref);
    });
    return out;
}

QStringList RichDocument::tags() const
{
    QStringList out;
    forEachSpan(blocks, [&](const Span &s) {
        if (s.kind != Span::Kind::Text || (s.marks & Code) || !s.href.isEmpty())
            return;
        for (const TagMatch &m : findTags(s.text)) {
            if (!out.contains(m.tag))
                out.append(m.tag);
        }
    });
    return out;
}

QString RichDocument::firstImage() const
{
    QString first;
    forEachSpan(blocks, [&](const Span &s) {
        if (first.isEmpty() && s.kind == Span::Kind::Image)
            first = s.ref;
    });
    return first;
}

void RichDocument::assignMissingIds()
{
    QSet<QString> seen;
    assignIds(blocks, seen);
}

QJsonObject RichDocument::toJson() const
{
    QJsonObject o;
    o[u"schema"] = SchemaVersion;
    o[u"blocks"] = blocksToJson(blocks);
    return o;
}

QByteArray RichDocument::toJsonBytes() const
{
    return QJsonDocument(toJson()).toJson(QJsonDocument::Compact);
}

std::optional<RichDocument> RichDocument::fromJson(const QJsonObject &json, QString *error)
{
    const int schema = json[u"schema"].toInt();
    if (schema != SchemaVersion) {
        fail(error, u"unsupported document schema %1"_s.arg(schema));
        return std::nullopt;
    }
    RichDocument doc;
    if (!blocksFromJson(json[u"blocks"].toArray(), &doc.blocks, error, 0))
        return std::nullopt;
    if (doc.blocks.isEmpty())
        doc.blocks.append(Block::paragraph());
    return doc;
}

std::optional<RichDocument> RichDocument::fromJsonBytes(const QByteArray &bytes, QString *error)
{
    QJsonParseError parseError;
    const QJsonDocument json = QJsonDocument::fromJson(bytes, &parseError);
    if (!json.isObject()) {
        fail(error, parseError.errorString());
        return std::nullopt;
    }
    return fromJson(json.object(), error);
}

QString newId()
{
    return QUuid::createUuid().toString(QUuid::WithoutBraces);
}

QList<TagMatch> findTags(const QString &text)
{
    static const QRegularExpression re(
        u"(?:^|(?<=[\\s(\\[{\"'\u201c\u2018]))#([\\p{L}\\p{N}_][\\p{L}\\p{N}_/\\-]*)"_s);
    QList<TagMatch> out;
    auto it = re.globalMatch(text);
    while (it.hasNext()) {
        const QRegularExpressionMatch m = it.next();
        QString tag = m.captured(1);
        // Trailing separators aren't part of the tag ("#todo-" -> "todo").
        while (tag.endsWith(u'-') || tag.endsWith(u'/'))
            tag.chop(1);
        const bool hasLetter = std::any_of(tag.cbegin(), tag.cend(), [](QChar c) { return c.isLetter(); });
        if (!hasLetter || tag.size() > 64)
            continue;
        out.append({m.capturedStart(0), tag.size() + 1, tag.toLower()});
    }
    return out;
}

bool isValidBlobHash(const QString &hash)
{
    static const QRegularExpression re(u"^[0-9a-f]{64}$"_s);
    return re.match(hash).hasMatch();
}

} // namespace onotes
