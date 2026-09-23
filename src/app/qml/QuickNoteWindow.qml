import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// A small window for jotting something down without the full app. It uses
// the same editor and storage; notes land in Notes. A Quick Note you never
// write in is discarded when the window closes.
Window {
    id: quick

    property string noteId
    signal openInNotes(string noteId)

    function start(token) {
        if (!visible || !noteId) {
            noteId = Library.createQuickNote()
            pane.editor.openNote(noteId)
        }
        App.activate(quick, token)
        pane.focusEditor()
    }

    function finish() {
        const id = noteId
        if (!pane.editor.flush())
            return false
        pane.editor.closeNote()
        if (id)
            Library.discardIfEmpty(id)
        noteId = ""
        return true
    }

    title: qsTr("Quick Note")
    width: 520
    height: 440
    minimumWidth: 360
    minimumHeight: 260
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

    onClosing: close => {
        if (!finish()) {
            close.accepted = false
            pane.notice(qsTr("This note has changes that couldn't be saved. Retry or export them first."))
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        EditorPane {
            id: pane
            objectName: "quickNotePane"
            Layout.fillWidth: true
            Layout.fillHeight: true
            onDeleteRequested: quick.close()
            onNotice: message => toast.show(message)
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.divider
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 8
            spacing: 8
            Item { Layout.fillWidth: true }
            Button {
                text: qsTr("Open in Omarchy Notes")
                onClicked: {
                    const id = quick.noteId
                    if (!pane.editor.flush())
                        return
                    pane.editor.closeNote()
                    quick.noteId = ""
                    quick.hide()
                    quick.openInNotes(id)
                }
            }
            Button {
                text: qsTr("Done")
                onClicked: quick.close()
            }
        }
    }

    Toast {
        id: toast
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 56
    }
    Shortcut { sequences: [StandardKey.Close]; onActivated: quick.close() }
}
