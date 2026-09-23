#include "NoteExchange.h"

#include "DocumentConverter.h"
#include "LibraryService.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QLocale>
#include <QMimeDatabase>
#include <QPageLayout>
#include <QPdfWriter>
#include <QRegularExpression>
#include <QSaveFile>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QUrl>

#include <functional>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

bool fail(QString *error, const QString &message)
{
    if (error)
        *error = message;
    return false;
}

bool writeText(const QString &path, const QString &text, QString *error)
{
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        return fail(error, u"Couldn't write %1: %2"_s.arg(QFileInfo(path).fileName(), file.errorString()));
    }
    file.write(text.toUtf8());
    if (!file.commit())
        return fail(error, u"Couldn't write %1: %2"_s.arg(QFileInfo(path).fileName(), file.errorString()));
    return true;
}

// Markdown front matter (Obsidian, Jekyll...) isn't note content.
QString stripFrontMatter(const QString &text)
{
    if (!text.startsWith(u"---\n"_s) && !text.startsWith(u"---\r\n"_s))
        return text;
    static const QRegularExpression end(u"\\r?\\n---\\r?\\n"_s);
    const QRegularExpressionMatch m = end.match(text, 3);
    return m.hasMatch() ? text.mid(m.capturedEnd()) : text;
}

// Swaps images that point at files on disk for images stored in the library.
void storeLocalImages(QTextDocument &doc, const QString &baseDir, LibraryService &library, const QString &fileName,
                      QStringList *warnings)
{
    struct Found {
        int position;
        QTextImageFormat format;
    };
    QList<Found> images;
    for (QTextBlock block = doc.begin(); block.isValid(); block = block.next()) {
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            if (fragment.charFormat().isImageFormat())
                images.append({fragment.position(), fragment.charFormat().toImageFormat()});
        }
    }
    bool warnedWeb = false;
    QList<int> unusable;
    for (Found &image : images) {
        const QUrl url(image.format.name());
        const QString path = url.isLocalFile() ? url.toLocalFile()
            : url.scheme().isEmpty() ? QDir(baseDir).absoluteFilePath(QUrl::fromPercentEncoding(image.format.name().toUtf8()))
                                     : QString();
        if (path.isEmpty()) {
            if (!warnedWeb)
                warnings->append(u"%1: images from the web weren't imported."_s.arg(fileName));
            warnedWeb = true;
            unusable << image.position;
            continue;
        }
        QImageReader reader(path);
        const QSize size = reader.size();
        QString error;
        const auto hash = size.isValid() ? library.addImageFile(path, &error) : std::nullopt;
        if (!hash) {
            warnings->append(u"%1: couldn't import the image %2."_s.arg(fileName, QFileInfo(path).fileName()));
            unusable << image.position;
            continue;
        }
        QTextImageFormat stored = image.format;
        stored.setName(DocumentConverter::imageResourceName(*hash));
        stored.setWidth(size.width());
        stored.setHeight(size.height());
        stored.setProperty(TextProperty::ImageLogicalSize, size);
        QTextCursor cursor(&doc);
        cursor.setPosition(image.position);
        cursor.setPosition(image.position + 1, QTextCursor::KeepAnchor);
        cursor.setCharFormat(stored);
    }
    // Already reported above; remove them so they aren't reported again.
    for (auto it = unusable.crbegin(); it != unusable.crend(); ++it) {
        QTextCursor cursor(&doc);
        cursor.setPosition(*it);
        cursor.setPosition(*it + 1, QTextCursor::KeepAnchor);
        cursor.removeSelectedText();
    }
}

// What an exported note needs from the library, resolved up front.
struct ExportContext {
    LibraryService &library;
    QString filesDirName;     // "<name> files", relative to the note file
    QString filesDir;         // absolute
    QStringList used;         // file names already written into filesDir
    std::function<QString(const QString &noteId)> linkForNote; // empty: drop note links

    QString uniqueName(const QString &wanted)
    {
        const QFileInfo info(wanted);
        QString name = wanted;
        for (int n = 2; used.contains(name, Qt::CaseInsensitive); ++n)
            name = info.suffix().isEmpty() ? u"%1 %2"_s.arg(info.completeBaseName()).arg(n)
                                           : u"%1 %2.%3"_s.arg(info.completeBaseName()).arg(n).arg(info.suffix());
        used << name;
        return name;
    }

    // Copies a blob into the files folder; returns its relative path.
    QString copyBlob(const QString &hash, const QString &wantedName)
    {
        QDir().mkpath(filesDir);
        const QString name = uniqueName(wantedName);
        QFile::copy(library.blobPath(hash), filesDir + u'/' + name);
        return filesDirName + u'/' + name;
    }
};

QString encodePath(const QString &relative)
{
    QStringList parts;
    for (const QString &part : relative.split(u'/'))
        parts << QString::fromUtf8(QUrl::toPercentEncoding(part));
    return parts.join(u'/');
}

// Rewrites a note for export: attachments become links to copied files,
// note links point at exported files (or become plain text).
RichDocument prepareForExport(const RichDocument &source, ExportContext &ctx, bool copyImages,
                              QHash<QString, QString> *imagePaths)
{
    RichDocument doc = source;
    int imageNumber = 0;
    std::function<void(QList<Block> &)> walk = [&](QList<Block> &blocks) {
        for (Block &b : blocks) {
            for (Span &span : b.spans) {
                if (span.kind == Span::Kind::Attachment) {
                    const auto info = ctx.library.attachment(span.ref);
                    const QString name = info ? info->fileName : u"attachment"_s;
                    const QString path = info ? ctx.copyBlob(info->blobHash, name) : QString();
                    span = Span::plain(u"\U0001F4CE "_s + name, 0, path.isEmpty() ? QString() : encodePath(path));
                } else if (span.kind == Span::Kind::Image && copyImages && !imagePaths->contains(span.ref)) {
                    const QString suffix = QMimeDatabase().mimeTypeForFile(ctx.library.blobPath(span.ref)).preferredSuffix();
                    const QString name = u"image-%1.%2"_s.arg(++imageNumber).arg(suffix.isEmpty() ? u"png"_s : suffix);
                    imagePaths->insert(span.ref, encodePath(ctx.copyBlob(span.ref, name)));
                } else if (span.kind == Span::Kind::Text) {
                    if (span.href.startsWith(u"note://"))
                        span.href = ctx.linkForNote ? ctx.linkForNote(span.href.mid(7)) : QString();
                    // Markdown has no underline; Qt would write it as
                    // emphasis, which reads back as italic.
                    if (copyImages)
                        span.marks &= ~quint32(Underline);
                }
            }
            for (TableCell &cell : b.cells)
                walk(cell.blocks);
        }
    };
    walk(doc.blocks);
    return doc;
}

QString toMarkdown(const RichDocument &doc, const QHash<QString, QString> &imagePaths)
{
    QTextDocument text;
    DocumentStyle style;
    DocumentConverter::load(doc, &text, style);
    QString markdown = text.toMarkdown(QTextDocument::MarkdownDialectGitHub);
    for (auto it = imagePaths.cbegin(); it != imagePaths.cend(); ++it)
        markdown.replace(DocumentConverter::imageResourceName(it.key()), it.value());
    return markdown;
}

QString uniqueFile(const QString &dir, const QString &base, const QString &suffix, QStringList *taken)
{
    QString name = base + suffix;
    for (int n = 2; taken->contains(name, Qt::CaseInsensitive) || QFileInfo::exists(dir + u'/' + name); ++n)
        name = u"%1 %2%3"_s.arg(base).arg(n).arg(suffix);
    taken->append(name);
    return name;
}

} // namespace

QString fileNameForTitle(const QString &title)
{
    QString name = title;
    static const QRegularExpression reserved(u"[/\\\\:*?\"<>|\\x00-\\x1f]"_s);
    name.replace(reserved, u" "_s);
    name = name.simplified();
    while (name.startsWith(u'.'))
        name.remove(0, 1);
    if (name.size() > 120)
        name = name.left(120).trimmed();
    return name.isEmpty() ? u"Untitled note"_s : name;
}

ImportReport importFiles(LibraryService &library, const QStringList &paths, const QString &folderId)
{
    ImportReport report;
    for (const QString &path : paths) {
        const QFileInfo info(path);
        QFile file(path);
        if (!file.open(QIODevice::ReadOnly)) {
            report.warnings << u"%1: %2"_s.arg(info.fileName(), file.errorString());
            continue;
        }
        const QString text = QString::fromUtf8(file.readAll());
        const QString suffix = info.suffix().toLower();

        RichDocument body;
        if (suffix == u"md" || suffix == u"markdown" || suffix == u"mdown") {
            QTextDocument doc;
            doc.setMarkdown(stripFrontMatter(text), QTextDocument::MarkdownDialectGitHub);
            storeLocalImages(doc, info.absolutePath(), library, info.fileName(), &report.warnings);
            ConversionReport conversion;
            body = DocumentConverter::read(&doc, &conversion);
            for (const QString &warning : conversion.warnings)
                report.warnings << u"%1: %2"_s.arg(info.fileName(), warning);
        } else {
            body = DocumentConverter::fromPlainText(text);
        }
        // A file without a first line gets its name as the title.
        if (body.title().isEmpty() || body.blocks.value(0).plainText().trimmed().isEmpty())
            body.blocks.prepend(Block::heading(1, {Span::plain(info.completeBaseName())}));

        QString error;
        const auto note = library.createNote(body, &error, folderId);
        if (note)
            report.createdIds << note->id;
        else
            report.warnings << u"%1: %2"_s.arg(info.fileName(), error);
    }
    return report;
}

bool exportNote(LibraryService &library, const QString &noteId, ExportFormat format, const QString &file,
                QString *error)
{
    QString loadError;
    const auto note = library.loadNote(noteId, &loadError);
    if (!note)
        return fail(error, loadError);

    const QFileInfo target(file);
    ExportContext ctx{library, target.completeBaseName() + u" files"_s,
                      target.absolutePath() + u'/' + target.completeBaseName() + u" files"_s, {}, {}};

    switch (format) {
    case ExportFormat::Markdown: {
        QHash<QString, QString> images;
        const RichDocument doc = prepareForExport(note->body, ctx, true, &images);
        return writeText(file, toMarkdown(doc, images), error);
    }
    case ExportFormat::Html: {
        QHash<QString, QString> unused;
        const RichDocument doc = prepareForExport(note->body, ctx, false, &unused);
        QTextDocument text;
        DocumentConverter::load(doc, &text, DocumentStyle{});
        text.setMetaInformation(QTextDocument::DocumentTitle, note->body.title());
        QString html = text.toHtml();
        // Embed images so the page is one self-contained file.
        for (const QString &hash : doc.referencedBlobs()) {
            QFile blob(library.blobPath(hash));
            if (!blob.open(QIODevice::ReadOnly))
                continue;
            const QString mime = QMimeDatabase().mimeTypeForFile(blob.fileName()).name();
            html.replace(DocumentConverter::imageResourceName(hash),
                         u"data:%1;base64,%2"_s.arg(mime, QString::fromLatin1(blob.readAll().toBase64())));
        }
        return writeText(file, html, error);
    }
    case ExportFormat::Pdf: {
        // Attachments can't live inside a PDF; they're listed by name.
        RichDocument doc = note->body;
        std::function<void(QList<Block> &)> strip = [&](QList<Block> &blocks) {
            for (Block &b : blocks) {
                for (Span &span : b.spans) {
                    if (span.kind == Span::Kind::Attachment) {
                        const auto info = library.attachment(span.ref);
                        span = Span::plain(u"\U0001F4CE "_s + (info ? info->fileName : u"attachment"_s));
                    } else if (span.kind == Span::Kind::Text && span.href.startsWith(u"note://")) {
                        span.href.clear();
                    }
                }
                for (TableCell &cell : b.cells)
                    strip(cell.blocks);
            }
        };
        strip(doc.blocks);

        DocumentStyle style;
        style.pointSizes = true;
        style.bodyPixelSize = 11 / 0.75; // 11 pt body text
        style.maxImageWidth = 460;       // points, within A4/Letter margins
        QTextDocument text;
        QFont font = QGuiApplication::font();
        font.setPointSizeF(11);
        text.setDefaultFont(font);
        DocumentConverter::load(doc, &text, style);
        for (const QString &hash : doc.referencedBlobs()) {
            QImage image(library.blobPath(hash));
            if (!image.isNull())
                text.addResource(QTextDocument::ImageResource, QUrl(DocumentConverter::imageResourceName(hash)), image);
        }

        QPdfWriter writer(file);
        writer.setTitle(note->body.title());
        writer.setCreator(u"Omarchy Notes"_s);
        const bool metric = QLocale().measurementSystem() == QLocale::MetricSystem;
        writer.setPageLayout(QPageLayout(QPageSize(metric ? QPageSize::A4 : QPageSize::Letter),
                                         QPageLayout::Portrait, QMarginsF(18, 18, 18, 18), QPageLayout::Millimeter));
        text.print(&writer);
        if (!QFileInfo::exists(file) || QFileInfo(file).size() == 0)
            return fail(error, u"Couldn't write the PDF."_s);
        return true;
    }
    }
    return false;
}

std::optional<QString> exportLibraryAsMarkdown(LibraryService &library, const QString &directory,
                                               QString *error)
{
    const QString root = QDir(directory).filePath(
        u"Omarchy Notes Export "_s + QDateTime::currentDateTime().toString(u"yyyy-MM-dd HHmm"_s));
    if (QFileInfo::exists(root) || !QDir().mkpath(root)) {
        fail(error, u"Couldn't create %1."_s.arg(root));
        return std::nullopt;
    }

    // Folder paths, parents before children.
    QHash<QString, QString> folderDirs; // folder id -> relative dir ("" is the root)
    const QList<FolderInfo> folders = library.listFolders();
    QHash<QString, QStringList> takenNames;
    std::function<QString(const QString &)> dirFor = [&](const QString &id) -> QString {
        if (id.isEmpty())
            return {};
        if (folderDirs.contains(id))
            return folderDirs.value(id);
        for (const FolderInfo &f : folders) {
            if (f.id == id) {
                const QString parent = dirFor(f.parentId);
                const QString name = uniqueFile(root + u'/' + parent, fileNameForTitle(f.name), {},
                                                &takenNames[parent]);
                const QString dir = parent.isEmpty() ? name : parent + u'/' + name;
                QDir().mkpath(root + u'/' + dir);
                folderDirs.insert(id, dir);
                return dir;
            }
        }
        return {};
    };

    // Decide every note's file first so links can point at them.
    const QList<NoteSummary> notes = library.listNotes(NoteQuery::all(NoteSort::Title));
    QHash<QString, QString> noteFiles; // note id -> relative path of its .md
    for (const NoteSummary &n : notes) {
        const QString dir = dirFor(n.folderId);
        const QString name = uniqueFile(root + u'/' + dir, fileNameForTitle(n.title), u".md"_s, &takenNames[dir]);
        noteFiles.insert(n.id, dir.isEmpty() ? name : dir + u'/' + name);
    }

    for (const NoteSummary &n : notes) {
        const auto note = library.loadNote(n.id);
        if (!note)
            continue;
        const QString relative = noteFiles.value(n.id);
        const QFileInfo info(root + u'/' + relative);
        ExportContext ctx{library, info.completeBaseName() + u" files"_s,
                          info.absolutePath() + u'/' + info.completeBaseName() + u" files"_s, {}, {}};
        const QDir from(info.absolutePath());
        ctx.linkForNote = [&](const QString &id) {
            return noteFiles.contains(id) ? encodePath(from.relativeFilePath(root + u'/' + noteFiles.value(id)))
                                          : QString();
        };
        QHash<QString, QString> images;
        const RichDocument doc = prepareForExport(note->body, ctx, true, &images);
        if (!writeText(info.absoluteFilePath(), toMarkdown(doc, images), error))
            return std::nullopt;
    }
    return root;
}

} // namespace onotes
