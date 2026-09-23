import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import QtQuick.Layouts
import OmarchyNotes

// Every image and file across your notes. Click shows the note it's in;
// double-click opens the file itself.
Rectangle {
    id: root

    property bool showSidebarButton: false
    signal noteChosen(string noteId)
    signal sidebarToggled()

    function focusGrid() { grid.forceActiveFocus() }

    color: Theme.sidebar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            Layout.leftMargin: root.showSidebarButton ? 8 : 16
            Layout.rightMargin: 12
            spacing: 8
            ToolIcon {
                visible: root.showSidebarButton
                iconName: "sidebar"
                text: qsTr("Show folders")
                onClicked: root.sidebarToggled()
            }
            Label {
                text: qsTr("Attachments")
                color: Theme.text
                font.pixelSize: 17
                font.weight: Font.DemiBold
                Accessible.role: Accessible.Heading
            }
            Label {
                text: Library.attachments.count
                color: Theme.secondaryText
            }
            Item { Layout.fillWidth: true }
            Repeater {
                model: [qsTr("All"), qsTr("Images"), qsTr("Files")]
                delegate: ToolIcon {
                    required property int index
                    required property string modelData
                    text: modelData
                    active: Library.attachments.filter === index
                    onClicked: Library.attachments.filter = index
                }
            }
        }

        GridView {
            id: grid
            objectName: "attachmentGrid"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: 10
            clip: true
            model: Library.attachments
            keyNavigationEnabled: true
            activeFocusOnTab: true
            boundsBehavior: Flickable.StopAtBounds
            readonly property int columns: Math.max(1, Math.floor(width / 200))
            cellWidth: Math.floor(width / columns)
            cellHeight: 212
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Attachments")
            ScrollBar.vertical: ScrollBar {}
            Keys.onReturnPressed: if (currentItem) currentItem.open()
            Keys.onSpacePressed: if (currentItem) root.noteChosen(currentItem.noteId)

            delegate: Item {
                id: tile

                required property int index
                required property bool isImage
                required property string blobHash
                required property string attachmentId
                required property string fileName
                required property string detail
                required property string extension
                required property string noteId
                required property string noteTitle

                function open() {
                    if (isImage)
                        Library.openImage(blobHash)
                    else
                        Library.openAttachment(attachmentId)
                }

                width: GridView.view.cellWidth
                height: GridView.view.cellHeight
                Accessible.role: Accessible.ListItem
                Accessible.name: fileName + ", " + qsTr("in %1").arg(noteTitle)
                Accessible.description: detail

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 7
                    radius: 10
                    color: Theme.canvas
                    clip: true
                    border.width: tile.GridView.isCurrentItem && tile.GridView.view.activeFocus ? 2 : 1
                    border.color: tile.GridView.isCurrentItem && tile.GridView.view.activeFocus
                                  ? Theme.focus : (hover.hovered ? Theme.tableBorder : Theme.divider)

                    Item {
                        id: preview
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 1
                        height: 120
                        Image {
                            anchors.fill: parent
                            visible: tile.isImage
                            source: tile.isImage ? "image://blob/" + tile.blobHash : ""
                            sourceSize: Qt.size(400, 240)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                        }
                        Rectangle {
                            visible: !tile.isImage
                            anchors.centerIn: parent
                            width: 64
                            height: 76
                            radius: 8
                            color: Theme.accent
                            Label {
                                anchors.centerIn: parent
                                text: tile.extension || qsTr("FILE")
                                color: Theme.accentText
                                font.bold: true
                            }
                        }
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: 1
                            color: Theme.divider
                        }
                    }
                    ColumnLayout {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: preview.bottom
                        anchors.margins: 10
                        spacing: 1
                        Label {
                            Layout.fillWidth: true
                            text: tile.fileName
                            elide: Text.ElideMiddle
                            font.weight: Font.DemiBold
                            color: Theme.text
                        }
                        Label {
                            Layout.fillWidth: true
                            text: tile.detail
                            elide: Text.ElideRight
                            font.pixelSize: 12
                            color: Theme.secondaryText
                        }
                        RowLayout {
                            spacing: 4
                            IconImage {
                                source: "qrc:/qt/qml/OmarchyNotes/icons/note.svg"
                                sourceSize: Qt.size(12, 12)
                                color: Theme.secondaryText
                            }
                            Label {
                                Layout.fillWidth: true
                                text: tile.noteTitle
                                elide: Text.ElideRight
                                font.pixelSize: 11
                                color: Theme.secondaryText
                            }
                        }
                    }
                }
                HoverHandler { id: hover }
                TapHandler {
                    onTapped: {
                        tile.GridView.view.currentIndex = tile.index
                        root.noteChosen(tile.noteId)
                    }
                    onDoubleTapped: tile.open()
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                width: parent.width - 48
                visible: grid.count === 0
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("No attachments yet")
                    font.weight: Font.DemiBold
                    color: Theme.text
                }
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: qsTr("Images and files you add to notes appear here.")
                    font.pixelSize: 12
                    color: Theme.secondaryText
                }
            }
        }
    }
}
