import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// Chooses a note to link to from the current one.
Popup {
    id: picker

    property string excludeNoteId
    signal picked(string noteId)

    modal: true
    focus: true
    width: 360
    height: Math.min(420, list.contentHeight + header.height + 24)
    padding: 8
    anchors.centerIn: Overlay.overlay

    background: Rectangle {
        radius: 10
        color: Theme.canvas
        border.color: Theme.divider
    }

    onOpened: list.forceActiveFocus()

    contentItem: ColumnLayout {
        spacing: 4
        Label {
            id: header
            Layout.margins: 8
            text: qsTr("Link to note")
            font.weight: Font.DemiBold
            color: Theme.text
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: Library.notes
            keyNavigationEnabled: true
            currentIndex: 0
            Keys.onReturnPressed: if (currentItem && currentItem.enabled) picker.choose(currentItem.noteId)
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Notes")

            delegate: ItemDelegate {
                required property int index
                required property string noteId
                required property string title
                width: ListView.view.width
                enabled: noteId !== picker.excludeNoteId
                text: title
                palette.text: Theme.text
                palette.windowText: Theme.text
                background: Rectangle {
                    radius: 6
                    color: parent.ListView.isCurrentItem ? Theme.selection
                         : parent.hovered ? Theme.raised : "transparent"
                }
                onClicked: picker.choose(noteId)
            }
        }
    }

    function choose(noteId) {
        close()
        picked(noteId)
    }
}
