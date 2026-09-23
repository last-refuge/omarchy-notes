#pragma once

#include "AppLibrary.h"
#include "DocumentConverter.h"
#include "NoteStore.h"

#include <QObject>
#include <QPointer>
#include <QQuickTextDocument>
#include <QSet>
#include <QTextCursor>
#include <QTimer>
#include <QUrl>
#include <QtQml/qqmlregistration.h>

class QTextDocument;
class QTextList;

// Drives a TextEdit's QTextDocument: loading and saving notes, formatting
// commands, lists, tables, media and clipboard conversion. It only touches the
// QTextDocument and a cursor position, so the same controller could drive a
// QTextEdit if the QML editor fails the feasibility gate.
class EditorController : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(NoteEditor)

    Q_PROPERTY(AppLibrary *library READ library WRITE setLibrary NOTIFY libraryChanged)
    Q_PROPERTY(QQuickTextDocument *document READ document WRITE setDocument NOTIFY documentChanged)
    Q_PROPERTY(int cursorPosition READ cursorPosition WRITE setCursorPosition NOTIFY cursorPositionChanged)
    Q_PROPERTY(int selectionStart READ selectionStart WRITE setSelectionStart NOTIFY selectionChanged)
    Q_PROPERTY(int selectionEnd READ selectionEnd WRITE setSelectionEnd NOTIFY selectionChanged)
    Q_PROPERTY(qreal bodyPixelSize READ bodyPixelSize WRITE setBodyPixelSize NOTIFY styleChanged)
    Q_PROPERTY(qreal devicePixelRatio READ devicePixelRatio WRITE setDevicePixelRatio NOTIFY styleChanged)
    Q_PROPERTY(int maxImageWidth READ maxImageWidth WRITE setMaxImageWidth NOTIFY styleChanged)

    Q_PROPERTY(QString noteId READ noteId NOTIFY noteChanged)
    Q_PROPERTY(bool hasNote READ hasNote NOTIFY noteChanged)
    // Notes in Recently Deleted open read-only until recovered.
    Q_PROPERTY(bool readOnly READ readOnly NOTIFY noteMetaChanged)
    Q_PROPERTY(bool pinned READ pinned NOTIFY noteMetaChanged)
    Q_PROPERTY(QString editedText READ editedText NOTIFY savedChanged)
    Q_PROPERTY(SaveState saveState READ saveState NOTIFY saveStateChanged)
    Q_PROPERTY(QString saveError READ saveError NOTIFY saveStateChanged)

    Q_PROPERTY(bool bold READ bold NOTIFY formatChanged)
    Q_PROPERTY(bool italic READ italic NOTIFY formatChanged)
    Q_PROPERTY(bool underline READ underline NOTIFY formatChanged)
    Q_PROPERTY(bool strikethrough READ strikethrough NOTIFY formatChanged)
    Q_PROPERTY(bool highlight READ highlight NOTIFY formatChanged)
    Q_PROPERTY(bool code READ code NOTIFY formatChanged)
    Q_PROPERTY(int blockStyle READ blockStyle NOTIFY formatChanged)
    Q_PROPERTY(int listKind READ listKind NOTIFY formatChanged)
    Q_PROPERTY(bool inTable READ inTable NOTIFY formatChanged)
    Q_PROPERTY(bool inHeading READ inHeading NOTIFY formatChanged)
    Q_PROPERTY(bool hasPendingFormat READ hasPendingFormat NOTIFY formatChanged)

    // Find in note. Matches are highlighted; findIndex is 1-based (0: none).
    Q_PROPERTY(QString findText READ findText WRITE setFindText NOTIFY findChanged)
    Q_PROPERTY(int findCount READ findCount NOTIFY findChanged)
    Q_PROPERTY(int findIndex READ findIndex NOTIFY findChanged)

public:
    enum SaveState { Saved, Edited, Saving, Failed, Conflict };
    Q_ENUM(SaveState)

    // Mirrors onotes::Mark so QML can pass NoteEditor.Bold etc.
    enum MarkType {
        Bold = onotes::Bold,
        Italic = onotes::Italic,
        Underline = onotes::Underline,
        Strikethrough = onotes::Strikethrough,
        Highlight = onotes::Highlight,
        Code = onotes::Code,
    };
    Q_ENUM(MarkType)

    enum ListType { NoList = 0, BulletList, OrderedList, CheckList };
    Q_ENUM(ListType)

    // Longest time typed text may stay unsaved while typing continues.
    static constexpr int MaxUnsavedMs = 3000;
    static constexpr int IdleSaveMs = 700;

    explicit EditorController(QObject *parent = nullptr);
    ~EditorController() override;

    AppLibrary *library() const { return m_library; }
    void setLibrary(AppLibrary *library);
    QQuickTextDocument *document() const { return m_quickDocument; }
    void setDocument(QQuickTextDocument *document);
    // Direct QTextDocument access, for tests and a possible Widgets frontend.
    QTextDocument *textDocument() const { return m_doc; }
    void setTextDocument(QTextDocument *doc);

    int cursorPosition() const { return m_cursorPosition; }
    void setCursorPosition(int position);
    int selectionStart() const { return m_selectionStart; }
    void setSelectionStart(int position);
    int selectionEnd() const { return m_selectionEnd; }
    void setSelectionEnd(int position);

    qreal bodyPixelSize() const { return m_bodyPixelSize; }
    void setBodyPixelSize(qreal size);
    qreal devicePixelRatio() const { return m_devicePixelRatio; }
    void setDevicePixelRatio(qreal ratio);
    int maxImageWidth() const { return m_maxImageWidth; }
    void setMaxImageWidth(int width);

    QString noteId() const { return m_noteId; }
    bool hasNote() const { return !m_noteId.isEmpty(); }
    bool readOnly() const { return m_deletedAt != 0; }
    bool pinned() const { return m_pinned; }
    QString editedText() const;
    SaveState saveState() const { return m_saveState; }
    QString saveError() const { return m_saveError; }

    bool bold() const { return m_marks & onotes::Bold; }
    bool italic() const { return m_marks & onotes::Italic; }
    bool underline() const { return m_marks & onotes::Underline; }
    bool strikethrough() const { return m_marks & onotes::Strikethrough; }
    bool highlight() const { return m_marks & onotes::Highlight; }
    bool code() const { return m_marks & onotes::Code; }
    int blockStyle() const { return m_blockStyle; }
    int listKind() const { return m_listKind; }
    bool inTable() const { return m_inTable; }
    bool inHeading() const { return m_blockStyle > 0; }
    bool hasPendingFormat() const { return m_pendingPosition >= 0 && m_pendingPosition == m_cursorPosition; }

    QString findText() const { return m_findText; }
    void setFindText(const QString &text);
    int findCount() const { return int(m_matchStarts.size()); }
    int findIndex() const { return m_findCurrent + 1; }

    onotes::DocumentStyle style() const;
    bool isDirty() const { return m_generation != m_savedGeneration; }

    // Notes
    Q_INVOKABLE bool openNote(const QString &id);
    // Leaves the editor empty. Call flush() first to keep changes.
    Q_INVOKABLE void closeNote();
    Q_INVOKABLE void setPinned(bool pinned);
    // Saves outstanding changes and waits for the result; false if they
    // could not be saved (the draft stays in the editor).
    Q_INVOKABLE bool flush();
    Q_INVOKABLE void retrySave();
    Q_INVOKABLE void keepMyVersion();
    Q_INVOKABLE bool exportDraft(const QUrl &file);

    // Formatting. With a selection (or inside a word) marks apply to that
    // text; at a bare insertion point they apply to what is typed next.
    Q_INVOKABLE bool toggleMark(int mark);
    // Inserts typed text in the pending format, so the rest of the run
    // continues in it and undoes as one step with it.
    Q_INVOKABLE bool typeWithPendingFormat(const QString &text);
    Q_INVOKABLE void setBlockStyle(int headingLevel);
    Q_INVOKABLE void toggleList(int kind);
    Q_INVOKABLE void indent();
    Q_INVOKABLE void outdent();

    // Keys the editor handles itself; each returns true if it consumed the key.
    Q_INVOKABLE bool handleReturn();
    Q_INVOKABLE bool handleTab(bool backward);
    Q_INVOKABLE bool handleBackspace();

    // Checklist items toggle natively on click; this is the keyboard path.
    Q_INVOKABLE void toggleCheckAtCursor();
    Q_INVOKABLE bool activateObjectAt(int position);
    Q_INVOKABLE void openLink(const QString &href);
    // The #tag at a document position, if any (for Ctrl+click).
    Q_INVOKABLE QString tagAt(int position) const;

    Q_INVOKABLE void findNext();
    Q_INVOKABLE void findPrevious();

    // Insertions
    Q_INVOKABLE void insertTable(int rows, int columns);
    Q_INVOKABLE void addTableRow();
    Q_INVOKABLE void addTableColumn();
    Q_INVOKABLE void removeTableRow();
    Q_INVOKABLE void removeTableColumn();
    Q_INVOKABLE bool insertImageFile(const QUrl &file);
    Q_INVOKABLE bool attachFile(const QUrl &file);
    Q_INVOKABLE void insertNoteLink(const QString &noteId);

    // Clipboard
    Q_INVOKABLE void copy();
    Q_INVOKABLE void cut();
    Q_INVOKABLE void paste();

signals:
    void libraryChanged();
    void documentChanged();
    void cursorPositionChanged();
    void selectionChanged();
    void styleChanged();
    void noteChanged();
    void noteMetaChanged();
    void savedChanged();
    void saveStateChanged();
    void formatChanged();
    void findChanged();

    // Requests for the view, which owns the visible cursor.
    void cursorRequested(int position);
    void selectionRequested(int start, int end);
    void openNoteRequested(const QString &noteId);
    // Short user-facing message (e.g. content that could not be pasted).
    void notice(const QString &message);

private:
    void onContentsChange(int position, int removed, int added);
    void onSaveFinished(quint64 ticket, const QString &noteId, const onotes::SaveResult &result);
    void onLibraryNoteChanged(const QString &noteId);
    void saveNow();
    void applyPendingFormat(int start, int end);
    QTextCharFormat pendingFormatAt(const QTextCursor &cursor) const;
    void setSaveState(SaveState state, const QString &error = {});
    void loadBody(const onotes::RichDocument &body);
    void reloadPresentation();
    void ensureResources();
    void updateFormatState();

    // Presentation-only formats on the text layout (tag colour, find
    // highlights): never part of the document, its undo history or saves.
    void decorate(int from, int to);
    void decorateAll();
    void scheduleDecorate(int from, int to);
    void recountMatches();
    void selectMatch(int index);

    QTextCursor textCursor() const;
    QList<QTextBlock> selectedBlocks() const;
    void applyBlockStyle(QTextBlock block, int level);
    void makeListItem(QTextBlock block, onotes::ListKind kind, int indent);
    void attachToList(QTextBlock block, onotes::ListKind kind, int indent);
    void removeFromList(QTextBlock block);
    void setListIndent(int delta);
    QTextCharFormat baseFormatFor(const QTextBlock &block) const;
    void insertRich(const onotes::RichDocument &doc, QTextCursor &cursor);
    void insertImage(QTextCursor &cursor, const QString &hash, QSize logicalSize);
    QImage renderAttachmentChip(const QString &attachmentId) const;
    QImage renderMissingImage(QSize logicalSize) const;

    QPointer<AppLibrary> m_library;
    QPointer<QQuickTextDocument> m_quickDocument;
    QPointer<QTextDocument> m_doc;
    QMetaObject::Connection m_docConnection;

    int m_cursorPosition = 0;
    int m_selectionStart = 0;
    int m_selectionEnd = 0;
    qreal m_bodyPixelSize = 15;
    qreal m_devicePixelRatio = 1;
    int m_maxImageWidth = 640;

    QString m_noteId;
    QString m_pendingNoteId; // requested before the document was attached
    qint64 m_revision = 0;
    qint64 m_updatedAt = 0;
    qint64 m_deletedAt = 0;
    bool m_pinned = false;
    bool m_loading = false;

    // Autosave bookkeeping: every edit bumps m_generation; a save snapshots
    // it, and the note is clean once an acknowledged save matches it.
    quint64 m_generation = 0;
    quint64 m_savedGeneration = 0;
    quint64 m_inFlightGeneration = 0;
    quint64 m_ticket = 0;
    onotes::RichDocument m_inFlightBody;
    qint64 m_conflictRevision = 0;
    QTimer m_idleTimer;
    QTimer m_capTimer;
    QTimer m_retryTimer;
    int m_retryCount = 0;
    SaveState m_saveState = Saved;
    QString m_saveError;

    QSet<QString> m_loadedResources;

    bool m_decorating = false;
    QTimer m_decorateTimer;
    int m_decorateFrom = -1;
    int m_decorateTo = -1;
    QString m_findText;
    QList<int> m_matchStarts;
    int m_findCurrent = -1;

    // Marks to set/clear on the next text typed at m_pendingPosition.
    int m_pendingPosition = -1;
    quint32 m_pendingSet = 0;
    quint32 m_pendingClear = 0;
    bool m_applyingPending = false;

    quint32 m_marks = 0;
    int m_blockStyle = 0;
    int m_listKind = 0;
    bool m_inTable = false;
};
