import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import QtCore
import OmarchyNotes

Rectangle {
    id: root
    objectName: "editorPane"

    property alias editor: editor
    property alias textArea: textArea
    property bool narrow: false
    signal backRequested()
    signal openNoteRequested(string noteId)
    signal newNoteRequested()
    signal deleteRequested()
    signal recoverRequested()
    signal deleteForeverRequested()
    signal notice(string message)
    signal tagRequested(string tag)

    function openFind() {
        findBar.visible = true
        findField.forceActiveFocus()
        findField.selectAll()
        if (findField.text)
            editor.findText = findField.text
    }
    function closeFind() {
        findBar.visible = false
        editor.findText = ""
        focusEditor()
    }

    function focusEditor() { textArea.forceActiveFocus() }

    function exportAs(format) {
        noteExportDialog.format = format
        noteExportDialog.nameFilters = [format === "pdf" ? qsTr("PDF (*.pdf)")
                                       : format === "html" ? qsTr("Web page (*.html)")
                                       : qsTr("Markdown (*.md)")]
        noteExportDialog.defaultSuffix = format
        noteExportDialog.selectedFile = noteExportDialog.currentFolder + "/"
            + encodeURIComponent(Library.exportFileName(editor.noteId, format))
        noteExportDialog.open()
    }

    function toggleMark(mark) {
        editor.toggleMark(mark)
    }

    color: Theme.canvas

    NoteEditor {
        id: editor
        objectName: "noteEditor"
        library: Library
        document: textArea.textDocument
        cursorPosition: textArea.cursorPosition
        selectionStart: textArea.selectionStart
        selectionEnd: textArea.selectionEnd
        bodyPixelSize: 15
        devicePixelRatio: Screen.devicePixelRatio
        maxImageWidth: Math.min(640, Math.max(160, textArea.width - textArea.leftPadding - textArea.rightPadding - 8))

        onCursorRequested: position => textArea.cursorPosition = position
        onSelectionRequested: (start, end) => textArea.select(start, end)
        onOpenNoteRequested: noteId => root.openNoteRequested(noteId)
        onNotice: message => root.notice(message)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ------------------------------------------------------------ toolbar
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.topMargin: 8
            Layout.bottomMargin: 8
            spacing: 2

            ToolIcon {
                Layout.alignment: Qt.AlignTop
                visible: root.narrow
                iconName: "back"
                text: qsTr("Back to notes")
                onClicked: root.backRequested()
            }

            Flow {
                id: toolbar
                readonly property bool editable: editor.hasNote && !editor.readOnly
                Layout.fillWidth: true
                spacing: 2
                enabled: editable
                opacity: editable ? 1 : 0.35
                Accessible.role: Accessible.ToolBar
                Accessible.name: qsTr("Formatting")

                ToolIcon {
                    id: styleButton
                    text: [qsTr("Body"), qsTr("Title"), qsTr("Heading"), qsTr("Subheading")][editor.blockStyle] + " ▾"
                    font.pixelSize: 13
                    width: 104
                    onClicked: styleMenu.popup(styleButton, 0, styleButton.height)
                    Menu {
                        id: styleMenu
                        MenuItem { text: qsTr("Title"); checkable: true; checked: editor.blockStyle === 1; onTriggered: editor.setBlockStyle(1) }
                        MenuItem { text: qsTr("Heading"); checkable: true; checked: editor.blockStyle === 2; onTriggered: editor.setBlockStyle(2) }
                        MenuItem { text: qsTr("Subheading"); checkable: true; checked: editor.blockStyle === 3; onTriggered: editor.setBlockStyle(3) }
                        MenuItem { text: qsTr("Body"); checkable: true; checked: editor.blockStyle === 0; onTriggered: editor.setBlockStyle(0) }
                    }
                }

                ToolSeparator {}

                ToolIcon {
                    text: qsTr("Bold"); shortcutHint: "Ctrl+B"
                    font.bold: true; font.pixelSize: 15
                    active: editor.bold
                    display: AbstractButton.TextOnly
                    contentItem: Text { text: "B"; font.bold: true; font.pixelSize: 15; color: Theme.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                    onClicked: root.toggleMark(NoteEditor.Bold)
                }
                ToolIcon {
                    text: qsTr("Italic"); shortcutHint: "Ctrl+I"
                    active: editor.italic
                    contentItem: Text { text: "I"; font.italic: true; font.pixelSize: 15; font.family: "serif"; color: Theme.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                    onClicked: root.toggleMark(NoteEditor.Italic)
                }
                ToolIcon {
                    text: qsTr("Underline"); shortcutHint: "Ctrl+U"
                    active: editor.underline
                    contentItem: Text { text: "U"; font.underline: true; font.pixelSize: 15; color: Theme.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                    onClicked: root.toggleMark(NoteEditor.Underline)
                }
                ToolIcon {
                    text: qsTr("Strikethrough"); shortcutHint: "Ctrl+Shift+X"
                    active: editor.strikethrough
                    contentItem: Text { text: "S"; font.strikeout: true; font.pixelSize: 15; color: Theme.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                    onClicked: root.toggleMark(NoteEditor.Strikethrough)
                }
                ToolIcon {
                    iconName: "highlight"; text: qsTr("Highlight"); shortcutHint: "Ctrl+Shift+Y"
                    active: editor.highlight
                    onClicked: root.toggleMark(NoteEditor.Highlight)
                }

                ToolSeparator {}

                ToolIcon {
                    iconName: "bullets"; text: qsTr("Bulleted list"); shortcutHint: "Ctrl+Shift+8"
                    active: editor.listKind === NoteEditor.BulletList
                    onClicked: editor.toggleList(NoteEditor.BulletList)
                }
                ToolIcon {
                    iconName: "numbers"; text: qsTr("Numbered list"); shortcutHint: "Ctrl+Shift+7"
                    active: editor.listKind === NoteEditor.OrderedList
                    onClicked: editor.toggleList(NoteEditor.OrderedList)
                }
                ToolIcon {
                    iconName: "checklist"; text: qsTr("Checklist"); shortcutHint: "Ctrl+Shift+L"
                    active: editor.listKind === NoteEditor.CheckList
                    onClicked: editor.toggleList(NoteEditor.CheckList)
                }

                ToolSeparator {}

                ToolIcon {
                    id: tableButton
                    iconName: "table"
                    text: editor.inTable ? qsTr("Table options") : qsTr("Insert table")
                    shortcutHint: editor.inTable ? "" : "Ctrl+Alt+T"
                    active: editor.inTable
                    onClicked: editor.inTable ? tableMenu.popup(tableButton, 0, tableButton.height)
                                              : editor.insertTable(2, 2)
                    Menu {
                        id: tableMenu
                        MenuItem { text: qsTr("Add Row Below"); onTriggered: editor.addTableRow() }
                        MenuItem { text: qsTr("Add Column After"); onTriggered: editor.addTableColumn() }
                        MenuSeparator {}
                        MenuItem { text: qsTr("Delete Row"); onTriggered: editor.removeTableRow() }
                        MenuItem { text: qsTr("Delete Column"); onTriggered: editor.removeTableColumn() }
                    }
                }
                ToolIcon {
                    iconName: "image"; text: qsTr("Insert image")
                    onClicked: imageDialog.open()
                }
                ToolIcon {
                    iconName: "attach"; text: qsTr("Attach file"); shortcutHint: "Ctrl+Shift+A"
                    onClicked: attachDialog.open()
                }
                ToolIcon {
                    iconName: "link"; text: qsTr("Link to note"); shortcutHint: "Ctrl+L"
                    onClicked: linkPicker.open()
                }

                ToolSeparator {}

                ToolIcon {
                    iconName: "pin"
                    text: editor.pinned ? qsTr("Unpin note") : qsTr("Pin note")
                    active: editor.pinned
                    onClicked: editor.setPinned(!editor.pinned)
                }
                ToolIcon {
                    iconName: "trash"
                    text: qsTr("Delete note")
                    onClicked: root.deleteRequested()
                }
                ToolIcon {
                    id: exportButton
                    iconName: "share"
                    text: qsTr("Export note")
                    onClicked: exportMenu.popup(exportButton, 0, exportButton.height)
                    Menu {
                        id: exportMenu
                        MenuItem { text: qsTr("Export as Markdown…"); onTriggered: root.exportAs("md") }
                        MenuItem { text: qsTr("Export as HTML…"); onTriggered: root.exportAs("html") }
                        MenuItem { text: qsTr("Export as PDF…"); onTriggered: root.exportAs("pdf") }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.divider
        }

        // Find in note.
        Rectangle {
            id: findBar
            objectName: "findBar"
            visible: false
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.raised
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 6
                TextField {
                    id: findField
                    objectName: "findField"
                    Layout.fillWidth: true
                    Layout.maximumWidth: 360
                    placeholderText: qsTr("Find in note")
                    placeholderTextColor: Theme.secondaryText
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.selectionText
                    Accessible.name: qsTr("Find in note")
                    onTextEdited: editor.findText = text
                    Keys.onReturnPressed: event => {
                        if (event.modifiers & Qt.ShiftModifier)
                            editor.findPrevious()
                        else
                            editor.findNext()
                    }
                    Keys.onEscapePressed: root.closeFind()
                    background: Rectangle {
                        radius: 6
                        color: Theme.canvas
                        border.width: findField.activeFocus ? 2 : 1
                        border.color: findField.text && editor.findCount === 0 ? Theme.error
                                     : findField.activeFocus ? Theme.focus : Theme.divider
                    }
                }
                Label {
                    text: !findField.text ? ""
                        : editor.findCount === 0 ? qsTr("No matches")
                        : qsTr("%1 of %2").arg(editor.findIndex).arg(editor.findCount)
                    color: editor.findCount === 0 && findField.text ? Theme.error : Theme.secondaryText
                    font.pixelSize: 12
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
                ToolIcon { text: "\u2191"; Accessible.name: qsTr("Previous match"); enabled: editor.findCount > 0; onClicked: editor.findPrevious() }
                ToolIcon { text: "\u2193"; Accessible.name: qsTr("Next match"); enabled: editor.findCount > 0; onClicked: editor.findNext() }
                Item { Layout.fillWidth: true }
                ToolIcon { text: qsTr("Done"); onClicked: root.closeFind() }
            }
        }

        // Notes in Recently Deleted open read-only.
        Rectangle {
            visible: editor.readOnly
            Layout.fillWidth: true
            Layout.preferredHeight: banner.implicitHeight + 16
            color: Theme.raised
            RowLayout {
                id: banner
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 10
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    text: qsTr("This note is in Recently Deleted. Recover it to make changes.")
                    wrapMode: Text.Wrap
                    color: Theme.text
                }
                ToolIcon { iconName: "recover"; text: qsTr("Recover"); display: AbstractButton.TextBesideIcon; implicitWidth: implicitContentWidth + 20; onClicked: root.recoverRequested() }
                ToolIcon { text: qsTr("Delete Permanently…"); onClicked: root.deleteForeverRequested() }
            }
        }

        // ------------------------------------------------------------ document
        ScrollView {
            id: scroller
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            visible: editor.hasNote

            TextArea {
                id: textArea
                objectName: "noteText"

                readonly property int column: 720

                textFormat: TextEdit.RichText
                wrapMode: TextEdit.Wrap
                readOnly: editor.readOnly || !editor.hasNote
                selectByMouse: true
                persistentSelection: true
                focus: true
                color: Theme.text
                selectionColor: Theme.selection
                selectedTextColor: Theme.selectionText
                placeholderTextColor: Theme.secondaryText
                font.pixelSize: editor.bodyPixelSize
                topPadding: 52
                bottomPadding: 96
                leftPadding: Math.max(24, (width - column) / 2)
                rightPadding: leftPadding
                background: null
                Accessible.name: qsTr("Note")
                Accessible.description: qsTr("Rich text note editor")

                Label {
                    id: editedLabel
                    y: 20
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: editor.editedText
                    color: Theme.secondaryText
                    font.pixelSize: 12
                }

                Keys.onPressed: event => {
                    const plain = !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
                    if (event.matches(StandardKey.Copy)) {
                        editor.copy(); event.accepted = true
                    } else if (event.matches(StandardKey.Cut)) {
                        editor.cut(); event.accepted = true
                    } else if (event.matches(StandardKey.Paste)) {
                        editor.paste(); event.accepted = true
                    } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && plain
                               && !(event.modifiers & Qt.ShiftModifier)) {
                        event.accepted = editor.handleReturn()
                    } else if (event.key === Qt.Key_Tab && plain) {
                        event.accepted = editor.handleTab(false)
                    } else if (event.key === Qt.Key_Backtab) {
                        editor.handleTab(true); event.accepted = true
                    } else if (event.key === Qt.Key_Backspace && plain) {
                        event.accepted = editor.handleBackspace()
                    } else if (editor.hasPendingFormat && plain && event.text.length > 0
                               && event.text.charCodeAt(0) >= 32) {
                        event.accepted = editor.typeWithPendingFormat(event.text)
                    }
                }

                // Checkbox markers toggle natively on click (and undo).
                // Double-click opens attachments and images.
                TapHandler {
                    acceptedModifiers: Qt.NoModifier
                    onDoubleTapped: eventPoint => {
                        const p = eventPoint.position
                        editor.activateObjectAt(textArea.positionAt(p.x, p.y))
                    }
                }
                // Ctrl+click follows links and opens #tags.
                TapHandler {
                    acceptedModifiers: Qt.ControlModifier
                    onTapped: eventPoint => {
                        const p = eventPoint.position
                        const link = textArea.linkAt(p.x, p.y)
                        if (link) {
                            editor.openLink(link)
                            return
                        }
                        const tag = editor.tagAt(textArea.positionAt(p.x, p.y))
                        if (tag)
                            root.tagRequested(tag)
                    }
                }
                HoverHandler {
                    cursorShape: textArea.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor
                }
            }
        }

        // Nothing open (empty folder, or the note was deleted).
        ColumnLayout {
            visible: !editor.hasNote
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 12
            Item { Layout.fillHeight: true }
            Label {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("No note selected")
                font.pixelSize: 16
                font.weight: Font.DemiBold
                color: Theme.secondaryText
            }
            Button {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("New Note")
                onClicked: root.newNoteRequested()
            }
            Item { Layout.fillHeight: true }
        }

        // ------------------------------------------------------------ status
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.divider
        }
        RowLayout {
            visible: editor.hasNote && !editor.readOnly
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            Layout.leftMargin: 14
            Layout.rightMargin: 8
            spacing: 8

            Label {
                id: status
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.pixelSize: 12
                color: editor.saveState === NoteEditor.Failed || editor.saveState === NoteEditor.Conflict
                       ? Theme.error : Theme.secondaryText
                text: {
                    switch (editor.saveState) {
                    case NoteEditor.Saved: return qsTr("Saved")
                    case NoteEditor.Edited: return qsTr("Edited")
                    case NoteEditor.Saving: return qsTr("Saving…")
                    case NoteEditor.Failed: return qsTr("Couldn't save: %1 Your changes are kept here.").arg(editor.saveError)
                    case NoteEditor.Conflict: return qsTr("This note was changed elsewhere.")
                    }
                    return ""
                }
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            ToolIcon {
                visible: editor.saveState === NoteEditor.Failed
                text: qsTr("Retry")
                onClicked: editor.retrySave()
            }
            ToolIcon {
                visible: editor.saveState === NoteEditor.Failed
                text: qsTr("Export…")
                onClicked: exportDialog.open()
            }
            ToolIcon {
                visible: editor.saveState === NoteEditor.Conflict
                text: qsTr("Keep this version")
                onClicked: editor.keepMyVersion()
            }
        }
    }

    NoteLinkPicker {
        id: linkPicker
        excludeNoteId: editor.noteId
        onPicked: noteId => {
            editor.insertNoteLink(noteId)
            root.focusEditor()
        }
        onClosed: root.focusEditor()
    }

    FileDialog {
        id: imageDialog
        title: qsTr("Insert Image")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.webp *.svg *.bmp)")]
        onAccepted: { editor.insertImageFile(selectedFile); root.focusEditor() }
    }
    FileDialog {
        id: attachDialog
        title: qsTr("Attach File")
        onAccepted: { editor.attachFile(selectedFile); root.focusEditor() }
    }
    FileDialog {
        id: noteExportDialog
        property string format: "md"
        title: qsTr("Export Note")
        fileMode: FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            editor.flush()
            const result = Library.exportNote(editor.noteId, format, selectedFile)
            root.notice(result.ok ? qsTr("Exported to %1").arg(result.path) : result.error)
        }
    }
    FileDialog {
        id: exportDialog
        title: qsTr("Export Note")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "md"
        nameFilters: [qsTr("Markdown (*.md)")]
        onAccepted: editor.exportDraft(selectedFile)
    }

    // ------------------------------------------------------------ shortcuts
    Shortcut { sequences: [StandardKey.Bold]; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Bold) }
    Shortcut { sequences: [StandardKey.Italic]; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Italic) }
    Shortcut { sequences: [StandardKey.Underline]; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Underline) }
    Shortcut { sequence: "Ctrl+Shift+X"; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Strikethrough) }
    Shortcut { sequence: "Ctrl+Shift+Y"; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Highlight) }
    Shortcut { sequence: "Ctrl+E"; enabled: textArea.activeFocus; onActivated: root.toggleMark(NoteEditor.Code) }
    Shortcut { sequence: "Ctrl+Shift+T"; enabled: textArea.activeFocus; onActivated: editor.setBlockStyle(1) }
    Shortcut { sequence: "Ctrl+Shift+H"; enabled: textArea.activeFocus; onActivated: editor.setBlockStyle(2) }
    Shortcut { sequence: "Ctrl+Shift+J"; enabled: textArea.activeFocus; onActivated: editor.setBlockStyle(3) }
    Shortcut { sequence: "Ctrl+Shift+B"; enabled: textArea.activeFocus; onActivated: editor.setBlockStyle(0) }
    Shortcut { sequence: "Ctrl+Shift+L"; enabled: textArea.activeFocus; onActivated: editor.toggleList(NoteEditor.CheckList) }
    Shortcut { sequence: "Ctrl+Shift+U"; enabled: textArea.activeFocus; onActivated: editor.toggleCheckAtCursor() }
    Shortcut { sequences: ["Ctrl+Shift+8", "Ctrl+*"]; enabled: textArea.activeFocus; onActivated: editor.toggleList(NoteEditor.BulletList) }
    Shortcut { sequences: ["Ctrl+Shift+7", "Ctrl+&"]; enabled: textArea.activeFocus; onActivated: editor.toggleList(NoteEditor.OrderedList) }
    Shortcut { sequence: "Ctrl+]"; enabled: textArea.activeFocus; onActivated: editor.indent() }
    Shortcut { sequence: "Ctrl+["; enabled: textArea.activeFocus; onActivated: editor.outdent() }
    Shortcut { sequence: "Ctrl+Alt+T"; enabled: textArea.activeFocus; onActivated: editor.insertTable(2, 2) }
    Shortcut { sequence: "Ctrl+Shift+A"; enabled: toolbar.editable; onActivated: attachDialog.open() }
    Shortcut { sequence: "Ctrl+L"; enabled: toolbar.editable; onActivated: linkPicker.open() }
    Shortcut { sequences: [StandardKey.Find]; enabled: editor.hasNote; onActivated: root.openFind() }
    Shortcut { sequences: [StandardKey.FindNext]; enabled: findBar.visible; onActivated: editor.findNext() }
    Shortcut { sequences: [StandardKey.FindPrevious]; enabled: findBar.visible; onActivated: editor.findPrevious() }
}
