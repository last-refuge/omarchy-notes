import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import QtQuick.Layouts
import OmarchyNotes

// Picks the folder a note moves to.
Dialog {
    id: dialog

    property string noteId
    signal moved(string noteId, string folderId)

    function openFor(id) {
        noteId = id
        choices.model = Library.folderChoices()
        choices.currentIndex = 0
        open()
    }

    title: qsTr("Move “%1” to…").arg(Library.notes.titleOf(noteId))
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(360, Overlay.overlay ? Overlay.overlay.width - 32 : 360)
    height: Math.min(460, Overlay.overlay ? Overlay.overlay.height - 64 : 460)
    background: Rectangle { radius: 10; color: Theme.canvas; border.color: Theme.divider }
    onOpened: choices.forceActiveFocus()

    ListView {
        id: choices
        anchors.fill: parent
        clip: true
        keyNavigationEnabled: true
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Folders")
        Keys.onReturnPressed: if (currentItem) currentItem.clicked()

        delegate: ItemDelegate {
            required property int index
            required property var modelData
            width: ListView.view.width
            height: 34
            leftPadding: 12 + modelData.depth * 16
            focusPolicy: Qt.NoFocus
            onClicked: {
                dialog.close()
                dialog.moved(dialog.noteId, modelData.id)
            }
            Accessible.name: modelData.name
            background: Rectangle {
                radius: 6
                color: parent.ListView.isCurrentItem ? Theme.selection
                     : parent.hovered ? Theme.raised : "transparent"
            }
            contentItem: RowLayout {
                spacing: 8
                IconImage {
                    source: "qrc:/qt/qml/OmarchyNotes/icons/" + (modelData.id ? "folder" : "note") + ".svg"
                    sourceSize: Qt.size(16, 16)
                    color: Theme.secondaryText
                }
                Label {
                    Layout.fillWidth: true
                    text: modelData.name
                    elide: Text.ElideRight
                    color: Theme.text
                }
            }
        }
    }

    footer: DialogButtonBox {
        background: null
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
}
