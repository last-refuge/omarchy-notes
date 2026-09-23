import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import OmarchyNotes

ApplicationWindow {
    id: window

    required property string initialNoteId
    readonly property bool narrow: width < 720
    // In a narrow tile only one pane shows at a time.
    property bool showingEditor: true

    width: 1120
    height: 760
    minimumWidth: 360
    minimumHeight: 400
    visible: true
    title: qsTr("Notes")
    color: Theme.canvas

    palette {
        window: Theme.canvas
        windowText: Theme.text
        base: Theme.canvas
        text: Theme.text
        button: Theme.raised
        buttonText: Theme.text
        highlight: Theme.selection
        highlightedText: Theme.selectionText
        mid: Theme.divider
        light: Theme.raised
        dark: Theme.divider
        placeholderText: Theme.secondaryText
        toolTipBase: Theme.raised
        toolTipText: Theme.text
    }

    function openNote(noteId) {
        if (editorPane.editor.openNote(noteId)) {
            showingEditor = true
            return true
        }
        return false
    }

    function newNote() {
        const noteId = Library.createNote()
        if (noteId && openNote(noteId))
            editorPane.focusEditor()
    }

    Component.onCompleted: {
        const first = window.initialNoteId || Library.notes.idAt(0)
        if (first)
            openNote(first)
        else
            newNote() // an empty library opens straight into a note
        editorPane.focusEditor()
    }

    onClosing: close => {
        if (!editorPane.editor.flush()) {
            close.accepted = false
            unsavedDialog.open()
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        NoteList {
            objectName: "noteList"
            Layout.fillHeight: true
            Layout.fillWidth: window.narrow
            Layout.preferredWidth: window.narrow ? window.width : 300
            visible: !window.narrow || !window.showingEditor
            currentNoteId: editorPane.editor.noteId
            onNoteChosen: noteId => window.openNote(noteId)
            onNewNoteRequested: window.newNote()
        }
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            color: Theme.divider
            visible: !window.narrow
        }
        EditorPane {
            id: editorPane
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !window.narrow || window.showingEditor
            narrow: window.narrow
            onBackRequested: window.showingEditor = false
            onOpenNoteRequested: noteId => window.openNote(noteId)
        }
    }

    Shortcut { sequences: [StandardKey.New]; onActivated: window.newNote() }
    // Developer switch for exercising the save-failure path.
    Shortcut {
        sequence: "Ctrl+Alt+Shift+F"
        onActivated: Library.simulateSaveFailure = !Library.simulateSaveFailure
    }

    Dialog {
        id: unsavedDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Your latest changes aren't saved")
        Label {
            width: 380
            wrapMode: Text.Wrap
            color: Theme.text
            text: qsTr("%1\n\nRetry saving, export the note as Markdown, or quit and lose the unsaved changes.")
                  .arg(editorPane.editor.saveError)
        }
        footer: DialogButtonBox {
            Button {
                text: qsTr("Retry")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: {
                    editorPane.editor.retrySave()
                    if (editorPane.editor.flush())
                        Qt.quit()
                }
            }
            Button {
                text: qsTr("Export…")
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: quitExportDialog.open()
            }
            Button {
                text: qsTr("Quit Without Saving")
                DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
                onClicked: Qt.exit(0)
            }
            Button {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: unsavedDialog.close()
            }
        }
    }
    FileDialog {
        id: quitExportDialog
        fileMode: FileDialog.SaveFile
        defaultSuffix: "md"
        nameFilters: [qsTr("Markdown (*.md)")]
        onAccepted: {
            if (editorPane.editor.exportDraft(selectedFile))
                unsavedDialog.close()
        }
    }
}
