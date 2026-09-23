#pragma once

#include "RichDocument.h"

#include <QColor>
#include <QSize>
#include <QStringList>
#include <QTextFormat>

class QTextBlock;
class QTextCharFormat;
class QTextCursor;
class QTextDocument;
class QTextTable;

namespace onotes {

// Presentation parameters applied when a RichDocument is laid into a
// QTextDocument. None of these are persisted; they come from the theme and
// user font settings, and changing them means reloading the document.
struct DocumentStyle {
    qreal bodyPixelSize = 15;
    // Extra space between lines, as a fraction of the body size. A fixed
    // distance (not a proportional height) keeps lines holding images tight.
    qreal lineSpacing = 0.35;
    QColor link = QColor(0x2f, 0x6f, 0xd0);
    QColor highlight = QColor(255, 204, 0, 110);
    QColor tableBorder = QColor(0x90, 0x90, 0x90);
    QString codeFamily = QStringLiteral("monospace");
    QSize attachmentChipSize = QSize(320, 52);
    int maxImageWidth = 640;

    qreal headingPixelSize(int level) const;
    QTextBlockFormat bodyBlockFormat() const;
};

// Semantic markers stored on QTextFormats so that reading a document back
// does not have to guess from presentation (colors, sizes).
namespace TextProperty {
enum : int {
    Highlight = QTextFormat::UserProperty + 1,
    Code,
    ListKind,
    // Image size as stored in the schema, before display scaling.
    ImageLogicalSize,
};
}

struct ConversionReport {
    QStringList warnings;
    void warn(const QString &message);
};

class DocumentConverter
{
public:
    // Replaces the contents of `doc` and clears its undo history.
    static void load(const RichDocument &source, QTextDocument *doc, const DocumentStyle &style);

    // Inserts `source` at `cursor` as a single fragment (used for paste).
    static void insert(const RichDocument &source, QTextCursor &cursor, const DocumentStyle &style);

    // Reads `doc` back into the schema. Blocks without an identity (new or
    // pasted) are given one, stored as block user data so it is stable across
    // subsequent reads but never duplicated by copy/paste or block splits.
    static RichDocument read(QTextDocument *doc, ConversionReport *report = nullptr);
    static RichDocument readSelection(const QTextCursor &cursor, ConversionReport *report = nullptr);
    static RichDocument fromHtml(const QString &html, ConversionReport *report = nullptr);
    static RichDocument fromPlainText(const QString &text);

    // Formats shared with the editing commands.
    static QTextCharFormat charFormatForBlock(const Block &block, const DocumentStyle &style);
    static void applyMarks(QTextCharFormat &format, quint32 marks, const QString &href,
                           const DocumentStyle &style);
    static quint32 marksOf(const QTextCharFormat &format, bool inHeading);
    static ListKind listKindOf(const QTextBlock &block);
    static QTextListFormat listFormat(ListKind kind, int indent);
    static QTextTableFormat tableFormat(const DocumentStyle &style);

    static QString imageResourceName(const QString &blobHash);
    static QString attachmentResourceName(const QString &attachmentId);
    // Returns the blob hash / attachment id for a resource name, or empty.
    static QString blobHashFromResource(const QString &name);
    static QString attachmentIdFromResource(const QString &name);

    static QString blockId(const QTextBlock &block);
    static void setBlockId(QTextBlock block, const QString &id);
    static bool isAllowedLink(const QString &href);
};

} // namespace onotes
