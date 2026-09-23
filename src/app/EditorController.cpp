#include "EditorController.h"

#include "AppLibrary.h"
#include "LibraryPaths.h"
#include "ThemeController.h"

#include <QBuffer>
#include <QClipboard>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QLocale>
#include <QMimeData>
#include <QMimeDatabase>
#include <QPainter>
#include <QQuickTextDocument>
#include <QSaveFile>
#include <QTextBlock>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextList>
#include <QTextTable>

#include <algorithm>

using namespace Qt::StringLiterals;
using namespace onotes;

namespace {

constexpr auto kFragmentMime = "application/x-omarchy-notes-fragment+json";
constexpr char16_t kObjectReplacement = 0xFFFC;
constexpr int kMaxIndent = 8;

ListKind kindOfList(const QTextList *list)
{
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

int indentOfList(const QTextList *list)
{
    return std::max(0, list->format().indent() - 1);
}

// Blocks in different table cells (or a cell and the body) never share a list.
bool sameContainer(const QTextBlock &a, const QTextBlock &b)
{
    QTextCursor ca(a);
    QTextCursor cb(b);
    if (ca.currentFrame() != cb.currentFrame())
        return false;
    if (QTextTable *table = ca.currentTable())
        return table->cellAt(a.position()) == table->cellAt(b.position());
    return true;
}

// Removes one mark from a format without disturbing the others.
void clearMark(QTextCharFormat &fmt, quint32 mark)
{
    switch (mark) {
    case Bold: fmt.setFontWeight(QFont::Normal); break;
    case Italic: fmt.setFontItalic(false); break;
    case Underline: fmt.setFontUnderline(false); break;
    case Strikethrough: fmt.setFontStrikeOut(false); break;
    case Highlight:
        fmt.clearBackground();
        fmt.clearProperty(TextProperty::Highlight);
        break;
    case Code:
        fmt.clearProperty(QTextFormat::FontFamilies);
        fmt.clearProperty(QTextFormat::FontFixedPitch);
        fmt.clearProperty(TextProperty::Code);
        break;
    }
}

template<typename Fn>
void forEachFragment(QTextDocument *doc, int start, int end, Fn fn)
{
    for (QTextBlock block = doc->findBlock(start); block.isValid() && block.position() <= end;
         block = block.next()) {
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            const int fs = std::max(start, fragment.position());
            const int fe = std::min(end, fragment.position() + fragment.length());
            if (fs < fe)
                fn(fs, fe, fragment.charFormat());
        }
    }
}

} // namespace

EditorController::EditorController(QObject *parent) : QObject(parent)
{
    m_idleTimer.setSingleShot(true);
    m_idleTimer.setInterval(IdleSaveMs);
    connect(&m_idleTimer, &QTimer::timeout, this, &EditorController::saveNow);
    // Saves during continuous typing so edits never wait longer than the cap.
    m_capTimer.setSingleShot(true);
    m_capTimer.setInterval(MaxUnsavedMs);
    connect(&m_capTimer, &QTimer::timeout, this, &EditorController::saveNow);
    m_retryTimer.setSingleShot(true);
    connect(&m_retryTimer, &QTimer::timeout, this, &EditorController::saveNow);
    // Decorations follow edits on the next turn of the event loop, after
    // the text edit has finished its own update.
    m_decorateTimer.setSingleShot(true);
    m_decorateTimer.setInterval(0);
    connect(&m_decorateTimer, &QTimer::timeout, this, [this] {
        if (!m_findText.isEmpty())
            recountMatches();
        decorate(m_decorateFrom, m_decorateTo);
        m_decorateFrom = m_decorateTo = -1;
    });

    if (ThemeController *theme = ThemeController::instance())
        connect(theme, &ThemeController::changed, this, &EditorController::reloadPresentation);
}

EditorController::~EditorController()
{
    flush();
}

void EditorController::setLibrary(AppLibrary *library)
{
    if (m_library == library)
        return;
    if (m_library) {
        disconnect(m_library->service(), nullptr, this, nullptr);
        disconnect(m_library, nullptr, this, nullptr);
    }
    m_library = library;
    if (m_library) {
        connect(m_library->service(), &LibraryService::saveFinished, this, &EditorController::onSaveFinished);
        connect(m_library, &AppLibrary::noteMetaChanged, this, &EditorController::onLibraryNoteChanged);
    }
    emit libraryChanged();
    if (!m_pendingNoteId.isEmpty() && m_doc)
        openNote(std::exchange(m_pendingNoteId, {}));
}

void EditorController::setDocument(QQuickTextDocument *document)
{
    if (m_quickDocument == document)
        return;
    m_quickDocument = document;
    setTextDocument(document ? document->textDocument() : nullptr);
    emit documentChanged();
}

void EditorController::setTextDocument(QTextDocument *doc)
{
    disconnect(m_docConnection);
    m_doc = doc;
    m_loadedResources.clear();
    if (m_doc) {
        m_docConnection = connect(m_doc, &QTextDocument::contentsChange, this,
                                  &EditorController::onContentsChange);
        if (!m_pendingNoteId.isEmpty() && m_library)
            openNote(std::exchange(m_pendingNoteId, {}));
    }
}

void EditorController::setCursorPosition(int position)
{
    if (m_cursorPosition == position)
        return;
    m_cursorPosition = position;
    if (position != m_pendingPosition) {
        m_pendingPosition = -1; // moving the cursor drops a pending format
        m_pendingSet = m_pendingClear = 0;
    }
    emit cursorPositionChanged();
    updateFormatState();
}

void EditorController::setSelectionStart(int position)
{
    if (m_selectionStart == position)
        return;
    m_selectionStart = position;
    emit selectionChanged();
    updateFormatState();
}

void EditorController::setSelectionEnd(int position)
{
    if (m_selectionEnd == position)
        return;
    m_selectionEnd = position;
    emit selectionChanged();
    updateFormatState();
}

void EditorController::setBodyPixelSize(qreal size)
{
    if (qFuzzyCompare(m_bodyPixelSize, size) || size <= 0)
        return;
    m_bodyPixelSize = size;
    emit styleChanged();
    reloadPresentation();
}

void EditorController::setDevicePixelRatio(qreal ratio)
{
    if (qFuzzyCompare(m_devicePixelRatio, ratio) || ratio <= 0)
        return;
    m_devicePixelRatio = ratio;
    emit styleChanged();
    reloadPresentation();
}

void EditorController::setMaxImageWidth(int width)
{
    width = std::max(120, width);
    if (m_maxImageWidth == width)
        return;
    m_maxImageWidth = width;
    emit styleChanged();
    reloadPresentation();
}

QString EditorController::editedText() const
{
    if (!m_updatedAt)
        return {};
    const QDateTime when = QDateTime::fromMSecsSinceEpoch(m_updatedAt);
    const QLocale locale;
    return tr("%1 at %2").arg(locale.toString(when.date(), QLocale::LongFormat),
                              locale.toString(when.time(), QLocale::ShortFormat));
}

DocumentStyle EditorController::style() const
{
    DocumentStyle s;
    s.bodyPixelSize = m_bodyPixelSize;
    s.maxImageWidth = m_maxImageWidth;
    if (ThemeController *theme = ThemeController::instance()) {
        s.link = theme->link();
        s.highlight = theme->highlight();
        s.tableBorder = theme->tableBorder();
    }
    return s;
}

// ---------------------------------------------------------------- notes

bool EditorController::openNote(const QString &id)
{
    if (!m_doc || !m_library) {
        m_pendingNoteId = id;
        return true;
    }
    if (id == m_noteId)
        return true;
    if (!m_noteId.isEmpty() && !flush()) {
        emit notice(tr("This note has changes that couldn't be saved. Retry or export them first."));
        return false;
    }

    QString error;
    const auto record = m_library->service()->loadNote(id, &error);
    if (!record) {
        emit notice(tr("Couldn't open the note: %1").arg(error));
        return false;
    }
    m_noteId = id;
    m_revision = record->revision;
    m_updatedAt = record->updatedAt;
    m_deletedAt = record->deletedAt;
    m_pinned = record->pinned;
    m_retryTimer.stop();
    m_retryCount = 0;
    loadBody(record->body);
    setSaveState(Saved);
    emit noteChanged();
    emit noteMetaChanged();
    emit savedChanged();
    emit cursorRequested(m_doc->characterCount() - 1);
    m_library->setLastNoteId(id);
    return true;
}

void EditorController::closeNote()
{
    m_idleTimer.stop();
    m_capTimer.stop();
    m_retryTimer.stop();
    m_ticket = 0; // a late acknowledgement belongs to a note no longer shown
    m_noteId.clear();
    m_revision = m_updatedAt = m_deletedAt = 0;
    m_pinned = false;
    if (m_doc)
        loadBody(RichDocument{{Block::paragraph()}});
    setSaveState(Saved);
    emit noteChanged();
    emit noteMetaChanged();
    emit savedChanged();
}

void EditorController::setPinned(bool pinned)
{
    if (m_library && !m_noteId.isEmpty() && pinned != m_pinned)
        m_library->setPinned(m_noteId, pinned);
}

// Pinning, moving, deleting or recovering happened in the library; pick up
// the note's new state, or let go of it if it's gone.
void EditorController::onLibraryNoteChanged(const QString &noteId)
{
    if (noteId != m_noteId || !m_library)
        return;
    const auto record = m_library->service()->loadNote(noteId);
    if (!record) {
        closeNote();
        return;
    }
    m_revision = std::max(m_revision, record->revision);
    m_pinned = record->pinned;
    m_deletedAt = record->deletedAt;
    emit noteMetaChanged();
}

void EditorController::loadBody(const RichDocument &body)
{
    m_pendingPosition = -1;
    m_pendingSet = m_pendingClear = 0;
    m_loading = true;
    m_loadedResources.clear();
    DocumentConverter::load(body, m_doc, style());
    ensureResources();
    m_loading = false;
    m_savedGeneration = m_generation;
    if (!m_findText.isEmpty())
        recountMatches();
    decorateAll();
    updateFormatState();
}

// Rebuilds the document with the current theme/font. Content is unchanged;
// undo history is reset because formats are recreated.
void EditorController::reloadPresentation()
{
    if (!m_doc || m_noteId.isEmpty())
        return;
    const RichDocument body = DocumentConverter::read(m_doc);
    const int position = m_cursorPosition;
    const quint64 generation = m_generation;
    const quint64 saved = m_savedGeneration;
    loadBody(body);
    m_generation = generation;
    m_savedGeneration = saved;
    emit cursorRequested(std::clamp(position, 0, m_doc->characterCount() - 1));
}

void EditorController::onContentsChange(int position, int removed, int added)
{
    if (m_decorating)
        return; // our own re-layout, not an edit
    if (m_loading || m_noteId.isEmpty() || readOnly() || (removed == 0 && added == 0))
        return;
    if (m_applyingPending)
        return; // counted with the insertion it formats
    if (m_pendingPosition >= 0 && position == m_pendingPosition && removed == 0 && added > 0)
        applyPendingFormat(position, position + added);
    ++m_generation;
    if (m_saveState == Saved)
        setSaveState(Edited);
    scheduleDecorate(position, position + std::max(added, 1));
    // Save once typing pauses, and at the latest when the cap expires.
    m_idleTimer.start();
    if (!m_capTimer.isActive())
        m_capTimer.start();
    QMetaObject::invokeMethod(this, &EditorController::updateFormatState, Qt::QueuedConnection);
}

QTextCharFormat EditorController::pendingFormatAt(const QTextCursor &cursor) const
{
    QTextCharFormat fmt = cursor.charFormat();
    if (fmt.isImageFormat())
        fmt = baseFormatFor(cursor.block());
    for (quint32 mark = 1; mark <= Code; mark <<= 1) {
        if (m_pendingClear & mark)
            clearMark(fmt, mark);
    }
    DocumentConverter::applyMarks(fmt, m_pendingSet, {}, style());
    return fmt;
}

bool EditorController::typeWithPendingFormat(const QString &text)
{
    if (!m_doc || !hasPendingFormat())
        return false;
    QTextCursor cursor = textCursor();
    const QTextCharFormat fmt = pendingFormatAt(cursor);
    m_applyingPending = true; // the insertion already carries the format
    cursor.insertText(text, fmt);
    m_applyingPending = false;
    m_pendingPosition = -1;
    m_pendingSet = m_pendingClear = 0;
    // The edit still counts as a change for autosave.
    onContentsChange(cursor.position() - int(text.size()), 0, int(text.size()));
    emit cursorRequested(cursor.position());
    updateFormatState();
    return true;
}

// Fallback for text that arrives without a key event (input method
// commits): format it after insertion, as a separate undo step.
void EditorController::applyPendingFormat(int start, int end)
{
    m_applyingPending = true;
    QTextCursor range(m_doc);
    range.setPosition(start);
    range.setPosition(end, QTextCursor::KeepAnchor);
    QTextCursor before(m_doc);
    before.setPosition(start);
    range.setCharFormat(pendingFormatAt(before));
    m_applyingPending = false;
    // Following text inherits the format from the characters before it.
    m_pendingPosition = -1;
    m_pendingSet = m_pendingClear = 0;
}

void EditorController::saveNow()
{
    m_idleTimer.stop();
    if (!m_library || !m_doc || m_noteId.isEmpty() || !isDirty())
        return;
    if (m_ticket || m_saveState == Conflict)
        return; // a save is in flight; its completion schedules the next one

    m_inFlightBody = DocumentConverter::read(m_doc);
    m_inFlightGeneration = m_generation;
    m_capTimer.stop();
    m_ticket = m_library->service()->saveNote({m_noteId, m_revision, m_inFlightBody});
    if (m_saveState != Failed)
        setSaveState(Saving);
}

void EditorController::onSaveFinished(quint64 ticket, const QString &noteId, const SaveResult &result)
{
    if (ticket != m_ticket)
        return;
    m_ticket = 0;

    switch (result.status) {
    case SaveResult::Status::Saved:
        m_retryCount = 0;
        if (m_library)
            m_library->noteSaved(noteId, m_inFlightBody, result.updatedAt);
        if (noteId != m_noteId)
            return;
        m_revision = result.revision;
        m_updatedAt = result.updatedAt;
        m_savedGeneration = m_inFlightGeneration;
        emit savedChanged();
        if (isDirty()) {
            setSaveState(Edited);
            m_idleTimer.start();
        } else {
            setSaveState(Saved);
        }
        return;
    case SaveResult::Status::Conflict:
        m_conflictRevision = result.revision;
        setSaveState(Conflict, result.error);
        return;
    case SaveResult::Status::Failed: {
        // Keep the draft in the editor and retry with backoff.
        setSaveState(Failed, result.error);
        static constexpr int backoff[] = {2000, 5000, 15000, 30000};
        m_retryTimer.start(backoff[std::min<int>(m_retryCount++, std::size(backoff) - 1)]);
        return;
    }
    }
}

bool EditorController::flush()
{
    if (!m_library || m_noteId.isEmpty())
        return true;
    m_retryTimer.stop();
    // Each round queues a save if needed and waits for its acknowledgement;
    // edits made while a save was in flight need a second round.
    for (int round = 0; round < 3; ++round) {
        if (m_saveState == Conflict)
            return false;
        if (!m_ticket) {
            if (!isDirty())
                return true;
            saveNow();
        }
        m_library->service()->deliverPendingResults();
        if (m_saveState == Failed)
            return false;
    }
    return !isDirty() && !m_ticket;
}

void EditorController::retrySave()
{
    m_retryTimer.stop();
    m_retryCount = 0;
    saveNow();
}

void EditorController::keepMyVersion()
{
    if (m_saveState != Conflict)
        return;
    m_revision = m_conflictRevision;
    setSaveState(Edited);
    saveNow();
}

bool EditorController::exportDraft(const QUrl &file)
{
    if (!m_doc)
        return false;
    QSaveFile out(file.toLocalFile());
    if (!out.open(QIODevice::WriteOnly)) {
        emit notice(tr("Couldn't export: %1").arg(out.errorString()));
        return false;
    }
    out.write(m_doc->toMarkdown().toUtf8());
    if (!out.commit()) {
        emit notice(tr("Couldn't export: %1").arg(out.errorString()));
        return false;
    }
    return true;
}

void EditorController::setSaveState(SaveState state, const QString &error)
{
    if (m_saveState == state && m_saveError == error)
        return;
    m_saveState = state;
    m_saveError = error;
    emit saveStateChanged();
}

// ---------------------------------------------------------------- resources

void EditorController::ensureResources()
{
    if (!m_doc || !m_library)
        return;
    bool added = false;
    for (QTextBlock block = m_doc->begin(); block.isValid(); block = block.next()) {
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const QTextCharFormat cf = it.fragment().charFormat();
            if (!cf.isImageFormat())
                continue;
            const QTextImageFormat fmt = cf.toImageFormat();
            const QString name = fmt.name();
            if (m_loadedResources.contains(name))
                continue;
            m_loadedResources.insert(name);

            QImage image;
            if (const QString hash = DocumentConverter::blobHashFromResource(name); !hash.isEmpty()) {
                const QSize logical(qRound(fmt.width()), qRound(fmt.height()));
                QImageReader reader(m_library->service()->blobPath(hash));
                reader.setAutoTransform(true);
                const QSize source = reader.size();
                const QSize target = logical.isValid() && !logical.isEmpty()
                    ? (logical * m_devicePixelRatio).boundedTo(source.isValid() ? source : QSize(8192, 8192))
                    : QSize();
                if (target.isValid() && source.isValid() && target != source)
                    reader.setScaledSize(source.scaled(target, Qt::KeepAspectRatio));
                image = reader.read();
                if (image.isNull()) {
                    image = renderMissingImage(logical.isEmpty() ? QSize(240, 120) : logical);
                } else {
                    image.setDevicePixelRatio(double(image.width()) / std::max(1, logical.width()));
                }
            } else if (const QString id = DocumentConverter::attachmentIdFromResource(name); !id.isEmpty()) {
                image = renderAttachmentChip(id);
            } else {
                continue;
            }
            m_doc->addResource(QTextDocument::ImageResource, QUrl(name), image);
            added = true;
        }
    }
    if (added) {
        const bool wasLoading = std::exchange(m_loading, true);
        m_doc->markContentsDirty(0, m_doc->characterCount());
        m_loading = wasLoading;
    }
}

QImage EditorController::renderAttachmentChip(const QString &attachmentId) const
{
    const DocumentStyle s = style();
    const QSize logical = s.attachmentChipSize;
    QImage image(logical * m_devicePixelRatio, QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(m_devicePixelRatio);
    image.fill(Qt::transparent);

    const auto info = m_library ? m_library->service()->attachment(attachmentId) : std::nullopt;
    const bool available = info && QFileInfo::exists(m_library->service()->blobPath(info->blobHash));
    ThemeController *theme = ThemeController::instance();
    const QColor surface = theme ? theme->raised() : QColor(0xee, 0xee, 0xee);
    const QColor border = theme ? theme->divider() : QColor(0xcc, 0xcc, 0xcc);
    const QColor text = theme ? theme->text() : Qt::black;
    const QColor secondary = theme ? theme->secondaryText() : Qt::darkGray;
    const QColor badge = available ? (theme ? theme->accent() : Qt::blue) : (theme ? theme->error() : Qt::red);
    const QColor badgeText = theme ? theme->accentText() : Qt::white;

    QPainter p(&image);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(QPen(border, 1));
    p.setBrush(surface);
    p.drawRoundedRect(QRectF(0.5, 0.5, logical.width() - 1, logical.height() - 1), 8, 8);

    const QString extension = info ? QFileInfo(info->fileName).suffix().toUpper().left(4) : u"?"_s;
    const QRectF badgeRect(8, 8, 40, logical.height() - 16);
    p.setPen(Qt::NoPen);
    p.setBrush(badge);
    p.drawRoundedRect(badgeRect, 5, 5);
    QFont font = QGuiApplication::font();
    font.setPixelSize(10);
    font.setBold(true);
    p.setFont(font);
    p.setPen(badgeText);
    p.drawText(badgeRect, Qt::AlignCenter, extension.isEmpty() ? u"FILE"_s : extension);

    const qreal textX = badgeRect.right() + 10;
    const qreal textWidth = logical.width() - textX - 10;
    font.setPixelSize(13);
    font.setBold(false);
    font.setWeight(QFont::DemiBold);
    p.setFont(font);
    p.setPen(text);
    const QString title = info ? info->fileName : tr("Missing attachment");
    p.drawText(QRectF(textX, 8, textWidth, 18), Qt::AlignLeft | Qt::AlignVCenter,
               QFontMetrics(font).elidedText(title, Qt::ElideMiddle, int(textWidth)));

    font.setWeight(QFont::Normal);
    font.setPixelSize(12);
    p.setFont(font);
    p.setPen(available ? secondary : badge);
    QString subtitle;
    if (!info)
        subtitle = tr("Not found in this library");
    else if (!available)
        subtitle = tr("File data is missing");
    else
        subtitle = u"%1 · %2"_s.arg(QMimeDatabase().mimeTypeForName(info->mimeType).comment(),
                                    QLocale().formattedDataSize(info->size));
    p.drawText(QRectF(textX, 27, textWidth, 16), Qt::AlignLeft | Qt::AlignVCenter,
               QFontMetrics(font).elidedText(subtitle, Qt::ElideRight, int(textWidth)));
    return image;
}

QImage EditorController::renderMissingImage(QSize logical) const
{
    QImage image(logical * m_devicePixelRatio, QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(m_devicePixelRatio);
    image.fill(Qt::transparent);
    ThemeController *theme = ThemeController::instance();
    QPainter p(&image);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(QPen(theme ? theme->divider() : Qt::gray, 1, Qt::DashLine));
    p.drawRoundedRect(QRectF(0.5, 0.5, logical.width() - 1, logical.height() - 1), 8, 8);
    p.setPen(theme ? theme->secondaryText() : Qt::darkGray);
    p.drawText(QRectF(QPointF(0, 0), logical), Qt::AlignCenter, tr("Image unavailable"));
    return image;
}

// ---------------------------------------------------------------- state

QTextCursor EditorController::textCursor() const
{
    QTextCursor cursor(m_doc);
    const int last = m_doc->characterCount() - 1;
    if (m_selectionStart != m_selectionEnd) {
        cursor.setPosition(std::clamp(m_selectionStart, 0, last));
        cursor.setPosition(std::clamp(m_selectionEnd, 0, last), QTextCursor::KeepAnchor);
    } else {
        cursor.setPosition(std::clamp(m_cursorPosition, 0, last));
    }
    return cursor;
}

QList<QTextBlock> EditorController::selectedBlocks() const
{
    const QTextCursor cursor = textCursor();
    QList<QTextBlock> blocks;
    for (QTextBlock b = m_doc->findBlock(cursor.selectionStart());
         b.isValid() && b.position() <= cursor.selectionEnd(); b = b.next()) {
        blocks.append(b);
    }
    return blocks;
}

void EditorController::updateFormatState()
{
    if (!m_doc)
        return;
    QTextCursor cursor = textCursor();
    // With a selection, report the format of its first character.
    if (cursor.hasSelection())
        cursor.setPosition(std::min(cursor.selectionStart() + 1, m_doc->characterCount() - 1));
    const QTextBlock block = cursor.block();
    const int heading = std::min(block.blockFormat().headingLevel(), 3);

    quint32 marks = DocumentConverter::marksOf(cursor.charFormat(), heading > 0);
    if (!cursor.hasSelection() && cursor.position() == m_pendingPosition)
        marks = (marks | m_pendingSet) & ~m_pendingClear;
    const int list = int(DocumentConverter::listKindOf(block));
    const bool table = cursor.currentTable() != nullptr;
    if (marks == m_marks && heading == m_blockStyle && list == m_listKind && table == m_inTable)
        return;
    m_marks = marks;
    m_blockStyle = heading;
    m_listKind = list;
    m_inTable = table;
    emit formatChanged();
}

// ---------------------------------------------------------------- formatting

bool EditorController::toggleMark(int mark)
{
    if (!m_doc)
        return false;
    QTextCursor cursor = textCursor();
    if (!cursor.hasSelection()) {
        // Inside a word, act on the word; at a boundary, set the format for
        // the text typed next.
        QTextCursor word = cursor;
        word.select(QTextCursor::WordUnderCursor);
        if (!word.hasSelection() || word.selectionStart() == cursor.position()
            || word.selectionEnd() == cursor.position()) {
            if (m_pendingPosition != cursor.position()) {
                m_pendingSet = m_pendingClear = 0;
                m_pendingPosition = cursor.position();
            }
            const bool on = m_marks & quint32(mark);
            if (on) {
                m_pendingClear |= quint32(mark);
                m_pendingSet &= ~quint32(mark);
            } else {
                m_pendingSet |= quint32(mark);
                m_pendingClear &= ~quint32(mark);
            }
            updateFormatState();
            return true;
        }
        cursor = word;
    }
    const int start = cursor.selectionStart();
    const int end = cursor.selectionEnd();
    const bool inHeading = cursor.block().blockFormat().headingLevel() > 0;

    bool allHave = true;
    forEachFragment(m_doc, start, end, [&](int, int, const QTextCharFormat &fmt) {
        if (!fmt.isImageFormat() && !(DocumentConverter::marksOf(fmt, inHeading) & quint32(mark)))
            allHave = false;
    });

    cursor.beginEditBlock();
    if (allHave) {
        forEachFragment(m_doc, start, end, [&](int fs, int fe, QTextCharFormat fmt) {
            if (fmt.isImageFormat())
                return;
            clearMark(fmt, quint32(mark));
            QTextCursor range(m_doc);
            range.setPosition(fs);
            range.setPosition(fe, QTextCursor::KeepAnchor);
            range.setCharFormat(fmt);
        });
    } else {
        QTextCharFormat add;
        DocumentConverter::applyMarks(add, quint32(mark), {}, style());
        cursor.mergeCharFormat(add);
    }
    cursor.endEditBlock();
    updateFormatState();
    return true;
}

QTextCharFormat EditorController::baseFormatFor(const QTextBlock &block) const
{
    Block shape;
    const int heading = block.blockFormat().headingLevel();
    if (heading > 0) {
        shape.type = Block::Type::Heading;
        shape.level = std::min(heading, 3);
    }
    return DocumentConverter::charFormatForBlock(shape, style());
}

void EditorController::applyBlockStyle(QTextBlock block, int level)
{
    if (level > 0 && block.textList())
        removeFromList(block);

    QTextCursor cursor(block);
    QTextBlockFormat bf = block.blockFormat();
    bf.setHeadingLevel(level);
    bf.setTopMargin(level == 0 ? 0 : (level == 1 ? 4 : 10));
    bf.setBottomMargin(level == 0 ? 0 : 4);
    cursor.setBlockFormat(bf);

    const qreal size = style().headingPixelSize(level);
    auto restyle = [&](QTextCharFormat fmt) {
        if (level > 0) {
            fmt.setProperty(QTextFormat::FontPixelSize, int(size));
            fmt.setFontWeight(QFont::Bold);
        } else {
            fmt.clearProperty(QTextFormat::FontPixelSize);
            fmt.clearProperty(QTextFormat::FontWeight);
        }
        return fmt;
    };
    for (auto it = block.begin(); !it.atEnd(); ++it) {
        const QTextFragment fragment = it.fragment();
        QTextCursor range(m_doc);
        range.setPosition(fragment.position());
        range.setPosition(fragment.position() + fragment.length(), QTextCursor::KeepAnchor);
        range.setCharFormat(restyle(fragment.charFormat()));
    }
    cursor.setBlockCharFormat(restyle(cursor.blockCharFormat()));
}

void EditorController::setBlockStyle(int headingLevel)
{
    if (!m_doc)
        return;
    headingLevel = std::clamp(headingLevel, 0, 3);
    QTextCursor cursor = textCursor();
    cursor.beginEditBlock();
    for (const QTextBlock &block : selectedBlocks())
        applyBlockStyle(block, headingLevel);
    cursor.endEditBlock();
    updateFormatState();
}

void EditorController::removeFromList(QTextBlock block)
{
    if (QTextList *list = block.textList())
        list->remove(block);
    QTextBlockFormat bf = block.blockFormat();
    bf.setIndent(0);
    bf.setMarker(QTextBlockFormat::MarkerType::NoMarker);
    bf.setObjectIndex(-1);
    QTextCursor(block).setBlockFormat(bf);
}

void EditorController::attachToList(QTextBlock block, ListKind kind, int indent)
{
    if (QTextList *current = block.textList()) {
        if (kindOfList(current) == kind && indentOfList(current) == indent)
            return;
        current->remove(block);
    }

    // Join the nearest list at the same depth and kind, looking past deeper
    // nested items, so numbering continues across sub-lists.
    auto search = [&](bool forward) -> QTextList * {
        for (QTextBlock b = forward ? block.next() : block.previous(); b.isValid();
             b = forward ? b.next() : b.previous()) {
            QTextList *list = b.textList();
            if (!list || !sameContainer(b, block))
                return nullptr;
            const int depth = indentOfList(list);
            if (depth == indent && kindOfList(list) == kind)
                return list;
            if (depth < indent)
                return nullptr;
        }
        return nullptr;
    };
    if (QTextList *list = search(false))
        list->add(block);
    else if (QTextList *list = search(true))
        list->add(block);
    else
        QTextCursor(block).createList(DocumentConverter::listFormat(kind, indent));
}

void EditorController::makeListItem(QTextBlock block, ListKind kind, int indent)
{
    if (block.blockFormat().headingLevel() > 0)
        applyBlockStyle(block, 0);
    block = m_doc->findBlock(block.position());
    QTextBlockFormat bf = block.blockFormat();
    const bool wasChecked = bf.marker() == QTextBlockFormat::MarkerType::Checked;
    bf.setMarker(kind == ListKind::Check ? (wasChecked ? QTextBlockFormat::MarkerType::Checked
                                                       : QTextBlockFormat::MarkerType::Unchecked)
                                         : QTextBlockFormat::MarkerType::NoMarker);
    QTextCursor(block).setBlockFormat(bf);
    attachToList(block, kind, indent);
}

void EditorController::toggleList(int kindValue)
{
    if (!m_doc)
        return;
    const auto kind = static_cast<ListKind>(std::clamp(kindValue, 1, 3));
    const QList<QTextBlock> blocks = selectedBlocks();
    const bool all = std::all_of(blocks.begin(), blocks.end(), [&](const QTextBlock &b) {
        return DocumentConverter::listKindOf(b) == kind;
    });

    QTextCursor cursor = textCursor();
    cursor.beginEditBlock();
    for (const QTextBlock &block : blocks) {
        if (all) {
            removeFromList(block);
        } else {
            const int indent = block.textList() ? indentOfList(block.textList()) : 0;
            makeListItem(block, kind, indent);
        }
    }
    cursor.endEditBlock();
    updateFormatState();
}

void EditorController::setListIndent(int delta)
{
    QTextCursor cursor = textCursor();
    cursor.beginEditBlock();
    for (const QTextBlock &block : selectedBlocks()) {
        QTextList *list = block.textList();
        if (!list)
            continue;
        const int depth = indentOfList(list) + delta;
        if (depth < 0)
            removeFromList(block);
        else
            attachToList(block, DocumentConverter::listKindOf(block), std::min(depth, kMaxIndent));
    }
    cursor.endEditBlock();
    updateFormatState();
}

void EditorController::indent()
{
    if (m_doc)
        setListIndent(+1);
}

void EditorController::outdent()
{
    if (m_doc)
        setListIndent(-1);
}

// ---------------------------------------------------------------- keys

bool EditorController::handleReturn()
{
    if (!m_doc)
        return false;
    QTextCursor cursor = textCursor();
    if (cursor.hasSelection())
        return false;
    const QTextBlock block = cursor.block();

    if (QTextList *list = block.textList()) {
        if (block.length() <= 1) {
            // Return on an empty item steps out one level, then out of the list.
            cursor.beginEditBlock();
            if (indentOfList(list) > 0)
                attachToList(block, DocumentConverter::listKindOf(block), indentOfList(list) - 1);
            else
                removeFromList(block);
            cursor.endEditBlock();
            updateFormatState();
            return true;
        }
        cursor.beginEditBlock();
        cursor.insertBlock();
        // New checklist items start unchecked.
        QTextBlockFormat bf = cursor.blockFormat();
        if (bf.marker() == QTextBlockFormat::MarkerType::Checked) {
            bf.setMarker(QTextBlockFormat::MarkerType::Unchecked);
            cursor.setBlockFormat(bf);
        }
        cursor.endEditBlock();
        emit cursorRequested(cursor.position());
        return true;
    }

    if (block.blockFormat().headingLevel() > 0 && cursor.atBlockEnd()) {
        // Headings are followed by body text, like Notes' title.
        const QTextBlockFormat body = style().bodyBlockFormat();
        cursor.beginEditBlock();
        cursor.insertBlock(body, baseFormatFor(QTextBlock()));
        cursor.endEditBlock();
        emit cursorRequested(cursor.position());
        return true;
    }
    return false;
}

bool EditorController::handleTab(bool backward)
{
    if (!m_doc)
        return false;
    QTextCursor cursor = textCursor();
    if (QTextTable *table = cursor.currentTable()) {
        const QTextTableCell cell = table->cellAt(cursor);
        int row = cell.row();
        int column = cell.column() + (backward ? -1 : cell.columnSpan());
        if (column >= table->columns()) {
            column = 0;
            ++row;
        } else if (column < 0) {
            column = table->columns() - 1;
            --row;
        }
        if (row < 0)
            return true;
        if (row >= table->rows()) {
            // Tab from the last cell adds a row.
            table->appendRows(1);
        }
        const QTextTableCell next = table->cellAt(row, column);
        emit selectionRequested(next.firstCursorPosition().position(), next.lastCursorPosition().position());
        return true;
    }
    if (cursor.block().textList()) {
        setListIndent(backward ? -1 : +1);
        return true;
    }
    return false;
}

bool EditorController::handleBackspace()
{
    if (!m_doc)
        return false;
    const QTextCursor cursor = textCursor();
    if (cursor.hasSelection() || !cursor.atBlockStart())
        return false;
    const QTextBlock block = cursor.block();
    if (QTextList *list = block.textList()) {
        QTextCursor edit = cursor;
        edit.beginEditBlock();
        if (indentOfList(list) > 0)
            attachToList(block, DocumentConverter::listKindOf(block), indentOfList(list) - 1);
        else
            removeFromList(block);
        edit.endEditBlock();
        updateFormatState();
        return true;
    }
    if (block.blockFormat().headingLevel() > 0 && block.length() <= 1) {
        setBlockStyle(0);
        return true;
    }
    return false;
}

// ---------------------------------------------------------------- objects

void EditorController::toggleCheckAtCursor()
{
    if (!m_doc)
        return;
    const QTextBlock block = textCursor().block();
    QTextBlockFormat bf = block.blockFormat();
    if (bf.marker() == QTextBlockFormat::MarkerType::NoMarker)
        return;
    bf.setMarker(bf.marker() == QTextBlockFormat::MarkerType::Checked
                     ? QTextBlockFormat::MarkerType::Unchecked
                     : QTextBlockFormat::MarkerType::Checked);
    QTextCursor(block).setBlockFormat(bf);
    updateFormatState();
}

bool EditorController::activateObjectAt(int position)
{
    if (!m_doc)
        return false;
    // The object may be just before or after the hit position.
    for (int p : {position, position - 1}) {
        if (p < 0 || p >= m_doc->characterCount() - 1 || m_doc->characterAt(p) != QChar(kObjectReplacement))
            continue;
        QTextCursor c(m_doc);
        c.setPosition(p + 1);
        const QTextCharFormat cf = c.charFormat();
        if (!cf.isImageFormat())
            continue;
        const QString name = cf.toImageFormat().name();
        if (const QString id = DocumentConverter::attachmentIdFromResource(name); !id.isEmpty())
            return m_library->openAttachment(id);
        if (const QString hash = DocumentConverter::blobHashFromResource(name); !hash.isEmpty())
            return m_library->openImage(hash);
    }
    return false;
}

void EditorController::openLink(const QString &href)
{
    if (href.startsWith(u"note://")) {
        emit openNoteRequested(href.mid(7));
        return;
    }
    if (DocumentConverter::isAllowedLink(href))
        QDesktopServices::openUrl(QUrl(href));
}

// ---------------------------------------------------------------- insertions

void EditorController::insertTable(int rows, int columns)
{
    if (!m_doc)
        return;
    QTextCursor cursor = textCursor();
    cursor.clearSelection();
    cursor.beginEditBlock();
    if (cursor.block().length() > 1)
        cursor.movePosition(QTextCursor::EndOfBlock);
    QTextTable *table = cursor.insertTable(std::max(1, rows), std::max(1, columns),
                                           DocumentConverter::tableFormat(style()));
    cursor.endEditBlock();
    emit cursorRequested(table->cellAt(0, 0).firstCursorPosition().position());
}

void EditorController::addTableRow()
{
    QTextCursor cursor = textCursor();
    if (QTextTable *table = cursor.currentTable()) {
        const QTextTableCell cell = table->cellAt(cursor);
        table->insertRows(cell.row() + cell.rowSpan(), 1);
    }
}

void EditorController::addTableColumn()
{
    QTextCursor cursor = textCursor();
    if (QTextTable *table = cursor.currentTable()) {
        const QTextTableCell cell = table->cellAt(cursor);
        table->insertColumns(cell.column() + cell.columnSpan(), 1);
    }
}

void EditorController::removeTableRow()
{
    QTextCursor cursor = textCursor();
    if (QTextTable *table = cursor.currentTable()) {
        if (table->rows() > 1)
            table->removeRows(table->cellAt(cursor).row(), 1);
    }
}

void EditorController::removeTableColumn()
{
    QTextCursor cursor = textCursor();
    if (QTextTable *table = cursor.currentTable()) {
        if (table->columns() > 1)
            table->removeColumns(table->cellAt(cursor).column(), 1);
    }
}

void EditorController::insertImage(QTextCursor &cursor, const QString &hash, QSize logicalSize)
{
    QTextImageFormat fmt;
    fmt.setName(DocumentConverter::imageResourceName(hash));
    QSizeF display = logicalSize;
    if (display.width() > m_maxImageWidth)
        display = display.scaled(m_maxImageWidth, 1e6, Qt::KeepAspectRatio);
    fmt.setWidth(display.width());
    fmt.setHeight(display.height());
    fmt.setProperty(TextProperty::ImageLogicalSize, logicalSize);
    cursor.insertImage(fmt);
}

bool EditorController::insertImageFile(const QUrl &file)
{
    if (!m_doc || !m_library)
        return false;
    const QString path = file.toLocalFile();
    QImageReader reader(path);
    reader.setAutoTransform(true);
    const QSize size = reader.size();
    if (!size.isValid()) {
        emit notice(tr("%1 isn't an image this app can show.").arg(QFileInfo(path).fileName()));
        return false;
    }
    QString error;
    const auto hash = m_library->service()->addImageFile(path, &error);
    if (!hash) {
        emit notice(error);
        return false;
    }
    QTextCursor cursor = textCursor();
    insertImage(cursor, *hash, size);
    ensureResources();
    return true;
}

bool EditorController::attachFile(const QUrl &file)
{
    if (!m_doc || !m_library || m_noteId.isEmpty())
        return false;
    QString error;
    const auto info = m_library->service()->addAttachment(m_noteId, file.toLocalFile(), &error);
    if (!info) {
        emit notice(error);
        return false;
    }
    QTextImageFormat fmt;
    fmt.setName(DocumentConverter::attachmentResourceName(info->id));
    const QSize chip = style().attachmentChipSize;
    fmt.setWidth(chip.width());
    fmt.setHeight(chip.height());
    fmt.setVerticalAlignment(QTextCharFormat::AlignMiddle);
    QTextCursor cursor = textCursor();
    cursor.insertImage(fmt);
    ensureResources();
    return true;
}

void EditorController::insertNoteLink(const QString &noteId)
{
    if (!m_doc || !m_library)
        return;
    QString title = m_library->notes()->titleOf(noteId);
    if (title.isEmpty())
        title = tr("Untitled note");
    QTextCursor cursor = textCursor();
    const QTextCharFormat base = baseFormatFor(cursor.block());
    QTextCharFormat link = base;
    DocumentConverter::applyMarks(link, 0, u"note://"_s + noteId, style());
    cursor.beginEditBlock();
    cursor.insertText(title, link);
    // End the link so typing continues as plain text.
    cursor.insertText(u" "_s, base);
    cursor.endEditBlock();
    emit cursorRequested(cursor.position());
}

// ---------------------------------------------------------------- decorations and find

void EditorController::scheduleDecorate(int from, int to)
{
    m_decorateFrom = m_decorateFrom < 0 ? from : std::min(m_decorateFrom, from);
    m_decorateTo = std::max(m_decorateTo, to);
    m_decorateTimer.start();
}

void EditorController::decorateAll()
{
    if (m_doc)
        decorate(0, m_doc->characterCount());
}

void EditorController::decorate(int from, int to)
{
    if (!m_doc || from < 0)
        return;
    ThemeController *theme = ThemeController::instance();
    QTextCharFormat tagFormat;
    tagFormat.setForeground(theme ? theme->accent() : QColor(0x2f, 0x6f, 0xd0));
    QTextCharFormat matchFormat;
    QColor highlight = theme ? theme->highlight() : QColor(255, 204, 0, 110);
    matchFormat.setBackground(highlight);
    QTextCharFormat currentFormat;
    QColor current = theme ? theme->accent() : QColor(0x2f, 0x6f, 0xd0);
    current.setAlphaF(0.45f);
    currentFormat.setBackground(current);
    const int currentStart = m_findCurrent >= 0 ? m_matchStarts.value(m_findCurrent, -1) : -1;

    to = std::min(to, m_doc->characterCount());
    for (QTextBlock block = m_doc->findBlock(from); block.isValid() && block.position() <= to;
         block = block.next()) {
        QList<QTextLayout::FormatRange> ranges;
        const QString text = block.text();
        for (const TagMatch &m : findTags(text)) {
            // Tags aren't tags inside code or links (see RichDocument::tags).
            QTextCursor at(block);
            at.setPosition(block.position() + int(m.start) + 1);
            const QTextCharFormat cf = at.charFormat();
            if (cf.isAnchor() || cf.boolProperty(TextProperty::Code))
                continue;
            ranges.append({int(m.start), int(m.length), tagFormat});
        }
        if (!m_findText.isEmpty()) {
            for (qsizetype i = text.indexOf(m_findText, 0, Qt::CaseInsensitive); i >= 0;
                 i = text.indexOf(m_findText, i + m_findText.size(), Qt::CaseInsensitive)) {
                const bool isCurrent = block.position() + int(i) == currentStart;
                ranges.append({int(i), int(m_findText.size()), isCurrent ? currentFormat : matchFormat});
            }
        }
        if (ranges.isEmpty() && block.layout()->formats().isEmpty())
            continue;
        block.layout()->setFormats(ranges);
        m_decorating = true;
        m_doc->markContentsDirty(block.position(), block.length());
        m_decorating = false;
    }
}

void EditorController::setFindText(const QString &text)
{
    if (text == m_findText)
        return;
    m_findText = text;
    recountMatches();
    // Jump to the first match from the cursor, like typing into a find bar.
    m_findCurrent = -1;
    for (int i = 0; i < m_matchStarts.size(); ++i) {
        if (m_matchStarts[i] >= m_cursorPosition - int(text.size())) {
            m_findCurrent = i;
            break;
        }
    }
    if (m_findCurrent < 0 && !m_matchStarts.isEmpty())
        m_findCurrent = 0;
    decorateAll();
    if (m_findCurrent >= 0)
        selectMatch(m_findCurrent);
    emit findChanged();
}

void EditorController::recountMatches()
{
    const int previous = m_findCurrent >= 0 ? m_matchStarts.value(m_findCurrent, -1) : -1;
    m_matchStarts.clear();
    if (m_doc && !m_findText.isEmpty()) {
        for (QTextBlock block = m_doc->begin(); block.isValid(); block = block.next()) {
            const QString text = block.text();
            for (qsizetype i = text.indexOf(m_findText, 0, Qt::CaseInsensitive); i >= 0;
                 i = text.indexOf(m_findText, i + m_findText.size(), Qt::CaseInsensitive))
                m_matchStarts.append(block.position() + int(i));
        }
    }
    // Keep the current match if it still exists.
    m_findCurrent = previous >= 0 ? int(m_matchStarts.indexOf(previous)) : -1;
    if (m_findCurrent < 0 && !m_matchStarts.isEmpty() && previous >= 0)
        m_findCurrent = 0;
    emit findChanged();
}

void EditorController::selectMatch(int index)
{
    if (index < 0 || index >= m_matchStarts.size())
        return;
    const int old = m_findCurrent >= 0 ? m_matchStarts.value(m_findCurrent, -1) : -1;
    m_findCurrent = index;
    const int start = m_matchStarts[index];
    if (old >= 0)
        decorate(old, old + 1);
    decorate(start, start + 1);
    emit selectionRequested(start, start + int(m_findText.size()));
    emit findChanged();
}

void EditorController::findNext()
{
    if (!m_matchStarts.isEmpty())
        selectMatch((m_findCurrent + 1) % int(m_matchStarts.size()));
}

void EditorController::findPrevious()
{
    if (!m_matchStarts.isEmpty())
        selectMatch((m_findCurrent - 1 + int(m_matchStarts.size())) % int(m_matchStarts.size()));
}

QString EditorController::tagAt(int position) const
{
    if (!m_doc)
        return {};
    const QTextBlock block = m_doc->findBlock(position);
    const int offset = position - block.position();
    for (const TagMatch &m : findTags(block.text())) {
        if (offset >= m.start && offset <= m.start + m.length)
            return m.tag;
    }
    return {};
}

// ---------------------------------------------------------------- clipboard

void EditorController::copy()
{
    if (!m_doc)
        return;
    const QTextCursor cursor = textCursor();
    if (!cursor.hasSelection())
        return;
    const RichDocument part = DocumentConverter::readSelection(cursor);
    const QTextDocumentFragment fragment = cursor.selection();
    auto *mime = new QMimeData;
    mime->setData(QString::fromLatin1(kFragmentMime), part.toJsonBytes());
    QString plain = fragment.toPlainText();
    plain.remove(QChar(kObjectReplacement));
    mime->setText(plain);
    mime->setHtml(fragment.toHtml());
    QGuiApplication::clipboard()->setMimeData(mime);
}

void EditorController::cut()
{
    if (!m_doc)
        return;
    QTextCursor cursor = textCursor();
    if (!cursor.hasSelection())
        return;
    copy();
    cursor.removeSelectedText();
    emit cursorRequested(cursor.position());
}

void EditorController::insertRich(const RichDocument &doc, QTextCursor &cursor)
{
    const Block &first = doc.blocks.first();
    const bool inline_ = doc.blocks.size() == 1 && first.type == Block::Type::Paragraph;
    if (!inline_) {
        DocumentConverter::insert(doc, cursor, style());
        return;
    }
    // A single paragraph flows into the current block and takes its style
    // (e.g. pasting a word into a heading keeps the heading look).
    const QTextCharFormat base = baseFormatFor(cursor.block());
    for (const Span &span : first.spans) {
        switch (span.kind) {
        case Span::Kind::Text: {
            QTextCharFormat fmt = base;
            DocumentConverter::applyMarks(fmt, span.marks, span.href, style());
            QString text = span.text;
            text.replace(u'\n', QChar::LineSeparator);
            cursor.insertText(text, fmt);
            break;
        }
        case Span::Kind::Image:
            insertImage(cursor, span.ref, QSize(span.width, span.height));
            break;
        case Span::Kind::Attachment: {
            QTextImageFormat fmt;
            fmt.setName(DocumentConverter::attachmentResourceName(span.ref));
            fmt.setWidth(style().attachmentChipSize.width());
            fmt.setHeight(style().attachmentChipSize.height());
            fmt.setVerticalAlignment(QTextCharFormat::AlignMiddle);
            cursor.insertImage(fmt);
            break;
        }
        }
    }
}

void EditorController::paste()
{
    if (!m_doc || !m_library)
        return;
    const QMimeData *mime = QGuiApplication::clipboard()->mimeData();
    if (!mime)
        return;

    QTextCursor cursor = textCursor();
    ConversionReport report;
    std::optional<RichDocument> content;

    if (mime->hasFormat(QString::fromLatin1(kFragmentMime))) {
        content = RichDocument::fromJsonBytes(mime->data(QString::fromLatin1(kFragmentMime)));
    }
    if (!content && mime->hasUrls()) {
        // Files copied from a file manager: images inline, others attached.
        const QList<QUrl> urls = mime->urls();
        if (std::all_of(urls.begin(), urls.end(), [](const QUrl &u) { return u.isLocalFile(); })) {
            cursor.beginEditBlock();
            for (const QUrl &url : urls) {
                if (QImageReader(url.toLocalFile()).canRead())
                    insertImageFile(url);
                else
                    attachFile(url);
            }
            cursor.endEditBlock();
            return;
        }
    }
    if (!content && mime->hasImage()) {
        const QImage image = qvariant_cast<QImage>(mime->imageData());
        QByteArray png;
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        image.save(&buffer, "PNG");
        QString error;
        const auto hash = m_library->service()->addImage(png, &error);
        if (!hash) {
            emit notice(error);
            return;
        }
        cursor.beginEditBlock();
        cursor.removeSelectedText();
        insertImage(cursor, *hash, (QSizeF(image.size()) / image.devicePixelRatio()).toSize());
        cursor.endEditBlock();
        ensureResources();
        return;
    }
    if (!content && mime->hasHtml())
        content = DocumentConverter::fromHtml(mime->html(), &report);
    if (!content && mime->hasText())
        content = DocumentConverter::fromPlainText(mime->text());
    if (!content || content->blocks.isEmpty())
        return;

    cursor.beginEditBlock();
    cursor.removeSelectedText();
    insertRich(*content, cursor);
    cursor.endEditBlock();
    ensureResources();
    emit cursorRequested(cursor.position());
    if (!report.warnings.isEmpty())
        emit notice(report.warnings.join(u' '));
}
