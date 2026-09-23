import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import OmarchyNotes

ApplicationWindow {
    id: window

    required property string initialNoteId
    // Wide: folders | notes | editor. Medium: notes | editor, folders in a
    // drawer. Narrow: one pane at a time.
    readonly property bool wide: width >= 1040
    readonly property bool narrow: width < 720
    property bool sidebarCollapsed: false
    readonly property bool sidebarInline: wide && !sidebarCollapsed
    property bool showingEditor: true
    readonly property var editor: editorPane.editor

    width: 1240
    height: 780
    minimumWidth: 360
    minimumHeight: 400
    visible: true
    title: qsTr("Omarchy Notes")
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

    // ------------------------------------------------------------ selection

    function openNote(noteId) {
        if (!noteId)
            return false
        if (editor.openNote(noteId)) {
            showingEditor = true
            return true
        }
        return false
    }

    // Opens the first note of the list, or nothing if the list is empty.
    function openFirstNote() {
        const first = Library.notes.idAt(0)
        if (first)
            openNote(first)
        else if (editor.flush())
            editor.closeNote()
    }

    function showView(key) {
        Library.showKey(key)
        if (sidebarDrawer.opened)
            sidebarDrawer.close()
        if (Library.notes.indexOf(editor.noteId) < 0)
            openFirstNote()
        if (narrow)
            showingEditor = false
    }

    function newNote() {
        const noteId = Library.createNote()
        if (noteId && openNote(noteId))
            editorPane.focusEditor()
    }

    // Runs `action` on a note that is about to leave the list, then selects
    // its neighbour, like Notes does.
    function removeFromList(noteId, action) {
        const row = Library.notes.indexOf(noteId)
        const wasOpen = noteId === editor.noteId
        if (wasOpen && !editor.flush())
            return false
        if (!action(noteId))
            return false
        if (wasOpen || !editor.hasNote) {
            const next = Library.notes.idAt(Math.min(Math.max(row, 0), Library.notes.count - 1))
            if (next)
                openNote(next)
            else
                editor.closeNote()
        }
        return true
    }

    function trashNote(noteId) {
        if (removeFromList(noteId, id => Library.trashNote(id)))
            toast.show(qsTr("Moved to Recently Deleted."))
    }

    function deleteForever(noteId) {
        const title = Library.notes.titleOf(noteId) || qsTr("This note")
        confirm.ask(qsTr("Delete Permanently?"),
                    qsTr("“%1” will be deleted permanently. This can't be undone.").arg(title),
                    qsTr("Delete Permanently"), true,
                    () => removeFromList(noteId, id => Library.deleteNotePermanently(id)))
    }

    function recoverNote(noteId) {
        if (removeFromList(noteId, id => Library.recoverNote(id)))
            toast.show(qsTr("Note recovered."))
    }

    function handleAction(action, noteId) {
        switch (action) {
        case "trash": trashNote(noteId); break
        case "deleteForever": deleteForever(noteId); break
        case "recover": recoverNote(noteId); break
        case "pin": Library.setPinned(noteId, true); break
        case "unpin": Library.setPinned(noteId, false); break
        case "move": moveDialog.openFor(noteId); break
        case "duplicate": {
            if (noteId === editor.noteId)
                editor.flush()
            const copy = Library.duplicateNote(noteId)
            if (copy)
                openNote(copy)
            break
        }
        }
    }

    function emptyTrash() {
        confirm.ask(qsTr("Empty Recently Deleted?"),
                    qsTr("%n note(s) will be deleted permanently. This can't be undone.", "", Library.trashCount),
                    qsTr("Delete Permanently"), true, () => {
                        const showingTrashedNote = editor.readOnly
                        Library.emptyTrash()
                        if (showingTrashedNote)
                            editor.closeNote()
                    })
    }

    function deleteFolder(id, name, count) {
        confirm.ask(qsTr("Delete “%1”?").arg(name),
                    count > 0
                        ? qsTr("The folder and any folders inside it will be removed. Their notes move to Recently Deleted, where you can recover them for 30 days.")
                        : qsTr("The folder and any folders inside it will be removed."),
                    qsTr("Delete Folder"), true, () => {
                        Library.deleteFolder(id)
                        if (!editor.hasNote || editor.readOnly || Library.notes.indexOf(editor.noteId) < 0)
                            window.openFirstNote()
                    })
    }

    Component.onCompleted: {
        const remembered = Library.lastNoteId
        const first = window.initialNoteId
            || (Library.notes.indexOf(remembered) >= 0 ? remembered : Library.notes.idAt(0))
        if (first)
            openNote(first)
        else if (Library.notes.count === 0 && Library.currentKey !== "trash")
            newNote() // an empty library opens straight into a note
        editorPane.focusEditor()
    }

    onClosing: close => {
        if (!editor.flush()) {
            close.accepted = false
            unsavedDialog.open()
        }
    }

    Connections {
        target: Library
        function onErrorOccurred(message) { toast.show(message) }
        function onLibraryReplaced() { window.openFirstNote() }
    }

    // ------------------------------------------------------------ layout

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            id: inlineSidebar
            objectName: "sidebar"
            visible: window.sidebarInline
            Layout.fillHeight: true
            Layout.preferredWidth: 220
            onViewChosen: key => window.showView(key)
            onNewFolderRequested: parentId => folderDialog.openForCreate(parentId)
            onRenameFolderRequested: (id, name) => folderDialog.openForRename(id, name)
            onDeleteFolderRequested: (id, name, count) => window.deleteFolder(id, name, count)
            onEmptyTrashRequested: window.emptyTrash()
            onBackupRequested: backupFolderDialog.open()
            onRestoreRequested: restoreFolderDialog.open()
        }
        Rectangle {
            visible: window.sidebarInline
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            color: Theme.divider
        }
        NoteList {
            id: noteList
            objectName: "noteList"
            Layout.fillHeight: true
            Layout.fillWidth: window.narrow
            Layout.preferredWidth: window.narrow ? window.width : 300
            visible: !window.narrow || !window.showingEditor
            currentNoteId: window.editor.noteId
            showSidebarButton: !window.sidebarInline
            onNoteChosen: noteId => window.openNote(noteId)
            onNewNoteRequested: window.newNote()
            onSidebarToggled: window.wide ? (window.sidebarCollapsed = false) : sidebarDrawer.open()
            onActionRequested: (action, noteId) => window.handleAction(action, noteId)
            onEmptyTrashRequested: window.emptyTrash()
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
            onNewNoteRequested: window.newNote()
            onDeleteRequested: window.trashNote(window.editor.noteId)
            onRecoverRequested: window.recoverNote(window.editor.noteId)
            onDeleteForeverRequested: window.deleteForever(window.editor.noteId)
            onNotice: message => toast.show(message)
        }
    }

    Drawer {
        id: sidebarDrawer
        width: Math.min(280, window.width * 0.8)
        height: window.height
        edge: Qt.LeftEdge
        background: Rectangle { color: Theme.sidebar }
        onOpened: drawerSidebar.focusList()
        Sidebar {
            id: drawerSidebar
            anchors.fill: parent
            onViewChosen: key => window.showView(key)
            onNewFolderRequested: parentId => { sidebarDrawer.close(); folderDialog.openForCreate(parentId) }
            onRenameFolderRequested: (id, name) => { sidebarDrawer.close(); folderDialog.openForRename(id, name) }
            onDeleteFolderRequested: (id, name, count) => { sidebarDrawer.close(); window.deleteFolder(id, name, count) }
            onEmptyTrashRequested: { sidebarDrawer.close(); window.emptyTrash() }
            onBackupRequested: { sidebarDrawer.close(); backupFolderDialog.open() }
            onRestoreRequested: { sidebarDrawer.close(); restoreFolderDialog.open() }
        }
    }

    // ------------------------------------------------------------ dialogs

    Toast {
        id: toast
        objectName: "toast"
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 44
    }
    ConfirmDialog { id: confirm; objectName: "confirmDialog" }
    FolderNameDialog {
        id: folderDialog
        objectName: "folderDialog"
        onCreated: folderId => window.showView(folderId)
    }
    MoveNoteDialog {
        id: moveDialog
        onMoved: (noteId, folderId) => {
            if (Library.moveNote(noteId, folderId))
                toast.show(qsTr("Moved to “%1”.").arg(Library.folderName(folderId)))
        }
    }

    FolderDialog {
        id: backupFolderDialog
        title: qsTr("Choose Where to Save the Backup")
        onAccepted: {
            window.editor.flush()
            const result = Library.backupTo(selectedFolder, Library.defaultBackupName())
            if (result.ok)
                toast.show(qsTr("Backed up %n note(s) (%1) to %2", "", result.notes).arg(result.size).arg(result.path))
            else
                toast.show(result.error)
        }
    }
    FolderDialog {
        id: restoreFolderDialog
        title: qsTr("Choose a Backup to Restore")
        onAccepted: {
            const info = Library.inspectBackup(selectedFolder)
            if (!info.ok) {
                toast.show(info.error)
                return
            }
            const folder = selectedFolder
            confirm.ask(qsTr("Restore This Backup?"),
                        qsTr("Your notes will be replaced with the backup from %1 (%n note(s), %2).\n\nYour current notes aren't deleted: they're set aside in a folder next to your library, so you can go back.", "", info.notes)
                            .arg(info.created).arg(info.size),
                        qsTr("Restore"), false, () => {
                            if (!window.editor.flush()) {
                                toast.show(qsTr("Save or export your current changes before restoring."))
                                return
                            }
                            window.editor.closeNote()
                            const result = Library.restoreFrom(folder)
                            toast.show(result.ok ? qsTr("Restored. Your previous notes are in %1").arg(result.previous)
                                                 : result.error)
                        })
        }
    }

    // ------------------------------------------------------------ shortcuts

    Shortcut { sequences: [StandardKey.New]; onActivated: window.newNote() }
    Shortcut { sequence: "Ctrl+Shift+N"; onActivated: folderDialog.openForCreate("") }
    Shortcut {
        sequence: "Ctrl+Shift+F"
        onActivated: {
            if (window.narrow)
                window.showingEditor = false
            noteList.focusSearch()
        }
    }
    Shortcut {
        sequence: "Ctrl+D"
        enabled: window.editor.hasNote && !window.editor.readOnly
        onActivated: window.handleAction("duplicate", window.editor.noteId)
    }
    Shortcut {
        sequence: "Ctrl+Shift+S"
        onActivated: window.wide ? (window.sidebarCollapsed = !window.sidebarCollapsed) : sidebarDrawer.open()
    }
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
                  .arg(window.editor.saveError)
        }
        footer: DialogButtonBox {
            Button {
                text: qsTr("Retry")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: {
                    window.editor.retrySave()
                    if (window.editor.flush())
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
            if (window.editor.exportDraft(selectedFile))
                unsavedDialog.close()
        }
    }
}
