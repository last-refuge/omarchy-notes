#pragma once

#include "RichDocument.h"

#include <QByteArray>
#include <QString>

namespace onotes {

class LibraryService;

// Realistic content used to seed an empty library and as the round-trip
// fixture: headings, marks, nested and checked lists, a table, an image, a
// file attachment, a note link, emoji and right-to-left text.
namespace SampleLibrary {

RichDocument mixedContentNote(const QString &imageHash, const QString &attachmentId,
                              const QString &linkedNoteId);
RichDocument vendorResearchNote();
RichDocument shoppingNote();

// A small PNG chart and a one-page PDF, generated so the fixture has real
// media without shipping binary files.
QByteArray chartPng();
bool writeSamplePdf(const QString &path);

// Adds the sample notes to `library`. Returns the id of the mixed-content note.
QString seed(LibraryService &library, QString *error = nullptr);

} // namespace SampleLibrary
} // namespace onotes
