import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import QtQuick.Layouts
import OmarchyNotes

// Folder list: All Notes, Notes, your folders, Recently Deleted.
Rectangle {
    id: root

    signal viewChosen(string key)
    signal newFolderRequested(string parentId)
    signal renameFolderRequested(string folderId, string name)
    signal deleteFolderRequested(string folderId, string name, int count)
    signal emptyTrashRequested()
    signal backupRequested()
    signal restoreRequested()
    signal newSmartFolderRequested()
    signal editSmartFolderRequested(string smartId)
    signal deleteSmartFolderRequested(string smartId, string name)
    signal importRequested()
    signal importFolderRequested()
    signal exportAllRequested()

    function focusList() { list.forceActiveFocus() }

    color: Theme.sidebar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Label {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            leftPadding: 18
            verticalAlignment: Text.AlignVCenter
            text: qsTr("Folders")
            color: Theme.secondaryText
            font.pixelSize: 12
            font.weight: Font.DemiBold
            font.letterSpacing: 0.4
            Accessible.role: Accessible.Heading
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: Library.folders
            activeFocusOnTab: true
            keyNavigationEnabled: true
            boundsBehavior: Flickable.StopAtBounds
            spacing: 1
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Folders")
            ScrollBar.vertical: ScrollBar {}

            section.property: "section"
            section.delegate: Label {
                required property string section
                width: ListView.view.width
                visible: section.length > 0
                height: visible ? 34 : 0
                leftPadding: 18
                verticalAlignment: Text.AlignBottom
                bottomPadding: 5
                text: section
                color: Theme.secondaryText
                font.pixelSize: 12
                font.weight: Font.DemiBold
                font.letterSpacing: 0.4
                Accessible.role: Accessible.Heading
            }

            currentIndex: Library.folders.indexOfKey(Library.currentKey)
            Connections {
                target: Library
                function onViewChanged() { list.currentIndex = Library.folders.indexOfKey(Library.currentKey) }
            }
            // Arrow keys move through folders and show them.
            onCurrentIndexChanged: {
                if (activeFocus && currentItem && currentItem.key !== Library.currentKey)
                    root.viewChosen(currentItem.key)
            }
            Keys.onMenuPressed: if (currentItem) currentItem.openMenu()

            delegate: ItemDelegate {
                id: row

                required property int index
                required property string key
                required property string name
                required property string kind
                required property int depth
                required property int count
                required property string folderId
                readonly property bool selected: key === Library.currentKey && !Library.searching

                function openMenu() {
                    if (kind !== "tag" && kind !== "attachments")
                        contextMenu.popup(row, 24, row.height)
                }

                width: ListView.view.width
                height: 34
                leftPadding: 18 + depth * 16
                rightPadding: 14
                focusPolicy: Qt.NoFocus
                onClicked: root.viewChosen(key)
                Accessible.name: name + ", " + qsTr("%n notes", "", count)
                Accessible.selected: selected

                background: Rectangle {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    radius: 6
                    color: row.selected ? Theme.selection
                         : row.hovered ? Theme.raised : "transparent"
                    border.width: row.ListView.isCurrentItem && row.ListView.view.activeFocus ? 2 : 0
                    border.color: Theme.focus
                }

                contentItem: RowLayout {
                    spacing: 9
                    IconImage {
                        source: "qrc:/qt/qml/OmarchyNotes/icons/"
                                + ({all: "all-notes", notes: "note", folder: "folder", trash: "trash",
                                    attachments: "attach", smart: "smart-folder", tag: "tag"})[row.kind] + ".svg"
                        sourceSize: Qt.size(16, 16)
                        color: row.selected ? Theme.selectionText : Theme.secondaryText
                    }
                    Label {
                        Layout.fillWidth: true
                        text: row.name
                        elide: Text.ElideRight
                        color: row.selected ? Theme.selectionText : Theme.text
                    }
                    Label {
                        visible: row.count > 0
                        text: row.count
                        font.pixelSize: 12
                        color: row.selected ? Theme.selectionText : Theme.secondaryText
                    }
                }

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: row.openMenu()
                }

                Menu {
                    id: contextMenu
                    MenuItem {
                        text: qsTr("Edit Smart Folder…")
                        visible: row.kind === "smart"
                        height: visible ? implicitHeight : 0
                        onTriggered: root.editSmartFolderRequested(row.folderId)
                    }
                    MenuItem {
                        text: qsTr("Delete Smart Folder…")
                        visible: row.kind === "smart"
                        height: visible ? implicitHeight : 0
                        onTriggered: root.deleteSmartFolderRequested(row.folderId, row.name)
                    }
                    MenuItem {
                        text: row.kind === "folder" ? qsTr("New Folder Inside") : qsTr("New Folder")
                        visible: row.kind === "folder" || row.kind === "notes" || row.kind === "all"
                        height: visible ? implicitHeight : 0
                        onTriggered: root.newFolderRequested(row.kind === "folder" ? row.folderId : "")
                    }
                    MenuItem {
                        text: qsTr("Rename…")
                        visible: row.kind === "folder"
                        height: visible ? implicitHeight : 0
                        onTriggered: root.renameFolderRequested(row.folderId, row.name)
                    }
                    MenuItem {
                        text: qsTr("Delete Folder…")
                        visible: row.kind === "folder"
                        height: visible ? implicitHeight : 0
                        onTriggered: root.deleteFolderRequested(row.folderId, row.name, row.count)
                    }
                    MenuItem {
                        text: qsTr("Empty Recently Deleted…")
                        visible: row.kind === "trash"
                        height: visible ? implicitHeight : 0
                        enabled: row.count > 0
                        onTriggered: root.emptyTrashRequested()
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.divider
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 6
            spacing: 2
            ToolIcon {
                iconName: "folder-new"
                text: qsTr("New folder")
                shortcutHint: "Ctrl+Shift+N"
                onClicked: root.newFolderRequested("")
            }
            Item { Layout.fillWidth: true }
            ToolIcon {
                id: libraryButton
                iconName: "more"
                text: qsTr("Library options")
                onClicked: libraryMenu.popup(libraryButton, 0, -libraryMenu.implicitHeight)
                Menu {
                    id: libraryMenu
                    MenuItem { text: qsTr("New Smart Folder…"); onTriggered: root.newSmartFolderRequested() }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Import Markdown or Text…"); onTriggered: root.importRequested() }
                    MenuItem { text: qsTr("Import a Folder of Notes…"); onTriggered: root.importFolderRequested() }
                    MenuItem { text: qsTr("Export All Notes as Markdown…"); onTriggered: root.exportAllRequested() }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Back Up Library…"); onTriggered: root.backupRequested() }
                    MenuItem { text: qsTr("Restore from Backup…"); onTriggered: root.restoreRequested() }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Show Library Folder"); onTriggered: Library.openLibraryFolder() }
                }
            }
        }
    }
}
