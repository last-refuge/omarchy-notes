import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import QtQuick.Layouts
import OmarchyNotes

Rectangle {
    id: root

    property string currentNoteId
    signal noteChosen(string noteId)
    signal newNoteRequested()

    color: Theme.sidebar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            Layout.leftMargin: 16
            Layout.rightMargin: 8
            spacing: 6

            Label {
                text: qsTr("Notes")
                color: Theme.text
                font.pixelSize: 17
                font.weight: Font.DemiBold
                Accessible.role: Accessible.Heading
            }
            Label {
                text: list.count
                color: Theme.secondaryText
                Accessible.name: qsTr("%n notes", "", list.count)
            }
            Item { Layout.fillWidth: true }
            ToolIcon {
                iconName: "compose"
                text: qsTr("New note")
                shortcutHint: "Ctrl+N"
                onClicked: root.newNoteRequested()
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: Library.notes
            keyNavigationEnabled: true
            activeFocusOnTab: true
            boundsBehavior: Flickable.StopAtBounds
            spacing: 2
            topMargin: 2
            bottomMargin: 8
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Notes")
            ScrollBar.vertical: ScrollBar {}

            Keys.onReturnPressed: if (currentItem) root.noteChosen(currentItem.noteId)

            delegate: ItemDelegate {
                id: row

                required property int index
                required property string noteId
                required property string title
                required property string snippet
                required property string date
                required property bool pinned
                required property bool hasAttachments
                required property bool hasChecklist
                readonly property bool selected: noteId === root.currentNoteId

                width: ListView.view.width
                height: 62
                leftPadding: 16
                rightPadding: 14
                focusPolicy: Qt.NoFocus
                onClicked: {
                    ListView.view.currentIndex = index
                    root.noteChosen(noteId)
                }
                Accessible.name: title + ", " + date + ", " + snippet
                Accessible.selected: selected

                background: Rectangle {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    radius: 7
                    color: row.selected ? Theme.selection
                         : row.hovered ? Theme.raised : "transparent"
                    border.width: row.ListView.isCurrentItem && row.ListView.view.activeFocus ? 2 : 0
                    border.color: Theme.focus
                }

                contentItem: ColumnLayout {
                    spacing: 2
                    RowLayout {
                        spacing: 6
                        Label {
                            Layout.fillWidth: true
                            text: row.title
                            elide: Text.ElideRight
                            font.weight: Font.DemiBold
                            color: row.selected ? Theme.selectionText : Theme.text
                        }
                        Label {
                            visible: row.pinned
                            text: qsTr("Pinned")
                            font.pixelSize: 11
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                    }
                    RowLayout {
                        spacing: 8
                        Label {
                            text: row.date
                            font.pixelSize: 12
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                        Label {
                            Layout.fillWidth: true
                            text: row.snippet
                            elide: Text.ElideRight
                            font.pixelSize: 12
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                        IconImage {
                            visible: row.hasChecklist
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                            source: "qrc:/qt/qml/OmarchyNotes/icons/checklist.svg"
                            sourceSize: Qt.size(14, 14)
                            Accessible.name: qsTr("Has a checklist")
                        }
                        IconImage {
                            visible: row.hasAttachments
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                            source: "qrc:/qt/qml/OmarchyNotes/icons/attach.svg"
                            sourceSize: Qt.size(14, 14)
                            Accessible.name: qsTr("Has attachments")
                        }
                    }
                }
            }
        }
    }
}
