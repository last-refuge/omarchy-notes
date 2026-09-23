import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// Names a new folder or renames one. Errors (e.g. a duplicate name) show
// inline and keep the dialog open.
Dialog {
    id: dialog

    property string folderId   // set when renaming
    property string parentId   // set when creating inside a folder
    signal created(string folderId)

    function openForCreate(parent) {
        folderId = ""
        parentId = parent
        nameField.text = ""
        error.text = ""
        open()
    }
    function openForRename(id, name) {
        folderId = id
        parentId = ""
        nameField.text = name
        error.text = ""
        open()
    }
    function submit() {
        const result = folderId ? Library.renameFolder(folderId, nameField.text)
                                : Library.createFolder(nameField.text, parentId)
        if (!result.ok) {
            error.text = result.error
            nameField.forceActiveFocus()
            return
        }
        if (!folderId)
            created(result.id)
        close()
    }

    title: folderId ? qsTr("Rename Folder")
         : parentId ? qsTr("New Folder in “%1”").arg(Library.folderName(parentId))
         : qsTr("New Folder")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(380, Overlay.overlay ? Overlay.overlay.width - 32 : 380)
    onOpened: {
        nameField.forceActiveFocus()
        nameField.selectAll()
    }

    background: Rectangle { radius: 10; color: Theme.canvas; border.color: Theme.divider }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8
        TextField {
            id: nameField
            Layout.fillWidth: true
            placeholderText: qsTr("Folder name")
            placeholderTextColor: Theme.secondaryText
            color: Theme.text
            selectionColor: Theme.selection
            selectedTextColor: Theme.selectionText
            Accessible.name: qsTr("Folder name")
            onAccepted: dialog.submit()
            onTextEdited: error.text = ""
            background: Rectangle {
                radius: 6
                color: Theme.raised
                border.width: nameField.activeFocus ? 2 : 1
                border.color: error.text ? Theme.error : nameField.activeFocus ? Theme.focus : Theme.divider
            }
        }
        Label {
            id: error
            Layout.fillWidth: true
            visible: text.length > 0
            wrapMode: Text.Wrap
            color: Theme.error
            font.pixelSize: 12
            Accessible.role: Accessible.AlertMessage
        }
    }

    footer: DialogButtonBox {
        background: null
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            text: dialog.folderId ? qsTr("Rename") : qsTr("Create")
            enabled: nameField.text.trim().length > 0
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.submit()
        }
    }
}
