#pragma once

#include <QString>
#include <QStringList>

#include <optional>

namespace onotes {

class LibraryService;

// Moving notes in and out of the library as ordinary files. These run on the
// UI thread (they use QTextDocument) and reach storage through the service.

struct ImportReport {
    QStringList createdIds;
    QStringList warnings; // one line per problem, naming the file
};

// Imports Markdown (.md, .markdown) and plain-text files as notes in
// `folderId` ("" is Notes). Images a Markdown file references on disk are
// stored with the note; anything that can't be kept is listed in warnings.
ImportReport importFiles(LibraryService &library, const QStringList &paths, const QString &folderId);

// Imports a folder tree: the folder becomes a folder inside `parentFolderId`
// ("" is the top level), each subfolder with notes becomes a subfolder, and
// every Markdown/text file becomes a note. Asset-only folders (images next to
// the notes, as exporters write them) are not turned into folders; their
// images come in through the notes that use them.
ImportReport importFolder(LibraryService &library, const QString &directory, const QString &parentFolderId,
                          QString *createdFolderId = nullptr);

enum class ExportFormat { Markdown, Html, Pdf };

// Writes one note to `file`. Markdown and HTML put attachments (and, for
// Markdown, images) in a "<name> files" folder next to it; HTML embeds its
// images so the page is a single file.
bool exportNote(LibraryService &library, const QString &noteId, ExportFormat format, const QString &file,
                QString *error = nullptr);

// Writes every note (not deleted ones) as Markdown into a new folder inside
// `directory`, mirroring the folder tree. Links between notes become
// relative links. Returns the created folder.
std::optional<QString> exportLibraryAsMarkdown(LibraryService &library, const QString &directory,
                                               QString *error = nullptr);

// A file name for a note title: path separators and reserved characters
// removed, trimmed, and never empty.
QString fileNameForTitle(const QString &title);

} // namespace onotes
