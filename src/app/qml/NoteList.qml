import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import QtQuick.Layouts
import OmarchyNotes

Rectangle {
    id: root

    property string currentNoteId
    property bool showSidebarButton: false
    signal noteChosen(string noteId)
    signal newNoteRequested()
    signal sidebarToggled()
    // "trash", "recover", "deleteForever", "pin", "unpin", "move", "duplicate"
    signal actionRequested(string action, string noteId)
    signal emptyTrashRequested()

    function focusSearch() {
        search.forceActiveFocus()
        search.selectAll()
    }
    function focusList() { list.forceActiveFocus() }

    color: Theme.sidebar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            Layout.leftMargin: root.showSidebarButton ? 8 : 16
            Layout.rightMargin: 8
            spacing: 6

            ToolIcon {
                visible: root.showSidebarButton
                iconName: "sidebar"
                text: qsTr("Show folders")
                onClicked: root.sidebarToggled()
            }
            Label {
                Layout.fillWidth: true
                text: Library.viewTitle
                elide: Text.ElideRight
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
            ToolIcon {
                id: sortButton
                iconName: "more"
                text: qsTr("Sort notes")
                visible: !Library.inTrash && !Library.searching
                onClicked: sortMenu.popup(sortButton, 0, sortButton.height)
                Menu {
                    id: sortMenu
                    MenuItem { text: qsTr("Sort by Date Edited"); checkable: true; checked: Library.sortOrder === Library.SortEdited; onTriggered: Library.sortOrder = Library.SortEdited }
                    MenuItem { text: qsTr("Sort by Date Created"); checkable: true; checked: Library.sortOrder === Library.SortCreated; onTriggered: Library.sortOrder = Library.SortCreated }
                    MenuItem { text: qsTr("Sort by Title"); checkable: true; checked: Library.sortOrder === Library.SortTitle; onTriggered: Library.sortOrder = Library.SortTitle }
                }
            }
            ToolIcon {
                iconName: "compose"
                text: qsTr("New note")
                shortcutHint: "Ctrl+N"
                onClicked: root.newNoteRequested()
            }
        }

        TextField {
            id: search
            objectName: "searchField"
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.bottomMargin: 8
            leftPadding: 32
            rightPadding: clearSearch.visible ? 30 : 10
            placeholderText: qsTr("Search all notes")
            placeholderTextColor: Theme.secondaryText
            color: Theme.text
            selectionColor: Theme.selection
            selectedTextColor: Theme.selectionText
            text: Library.searchText
            onTextEdited: Library.searchText = text
            Accessible.name: qsTr("Search all notes")
            background: Rectangle {
                radius: 7
                color: Theme.raised
                border.width: search.activeFocus ? 2 : 1
                border.color: search.activeFocus ? Theme.focus : Theme.divider
            }
            IconImage {
                anchors.left: parent.left
                anchors.leftMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                source: "qrc:/qt/qml/OmarchyNotes/icons/search.svg"
                sourceSize: Qt.size(15, 15)
                color: Theme.secondaryText
            }
            ToolButton {
                id: clearSearch
                visible: search.text.length > 0
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 28
                height: 28
                focusPolicy: Qt.NoFocus
                icon.source: "qrc:/qt/qml/OmarchyNotes/icons/close.svg"
                icon.width: 12
                icon.height: 12
                icon.color: Theme.secondaryText
                background: null
                onClicked: Library.searchText = ""
                Accessible.name: qsTr("Clear search")
            }
            Keys.onEscapePressed: Library.searchText = ""
            Keys.onDownPressed: list.forceActiveFocus()
            // Enter opens the best match.
            onAccepted: if (list.count > 0) root.noteChosen(Library.notes.idAt(0))
        }

        // Recently Deleted explains itself and offers to empty.
        RowLayout {
            visible: Library.inTrash && list.count > 0
            Layout.fillWidth: true
            Layout.leftMargin: 16
            Layout.rightMargin: 12
            Layout.bottomMargin: 6
            Label {
                Layout.fillWidth: true
                text: qsTr("Notes are deleted for good after 30 days.")
                wrapMode: Text.Wrap
                font.pixelSize: 12
                color: Theme.secondaryText
            }
            ToolIcon {
                text: qsTr("Empty")
                onClicked: root.emptyTrashRequested()
            }
        }

        ListView {
            id: list
            objectName: "noteListView"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: Library.notes
            keyNavigationEnabled: true
            activeFocusOnTab: true
            boundsBehavior: Flickable.StopAtBounds
            spacing: 2
            bottomMargin: 8
            currentIndex: Library.notes.indexOf(root.currentNoteId)
            Accessible.role: Accessible.List
            Accessible.name: Library.viewTitle
            ScrollBar.vertical: ScrollBar {}

            section.property: "section"
            section.delegate: Label {
                required property string section
                width: ListView.view.width
                visible: section.length > 0
                height: visible ? 30 : 0
                leftPadding: 18
                verticalAlignment: Text.AlignBottom
                bottomPadding: 4
                text: section
                font.pixelSize: 12
                font.weight: Font.DemiBold
                color: Theme.secondaryText
                Accessible.role: Accessible.Heading
            }

            // Moving through the list opens notes, like Notes does.
            onCurrentIndexChanged: {
                if (activeFocus && currentItem && currentItem.noteId !== root.currentNoteId)
                    root.noteChosen(currentItem.noteId)
            }
            Keys.onReturnPressed: if (currentItem) root.noteChosen(currentItem.noteId)
            Keys.onDeletePressed: if (currentItem) root.actionRequested(Library.inTrash ? "deleteForever" : "trash", currentItem.noteId)
            Keys.onMenuPressed: if (currentItem) currentItem.openMenu()

            delegate: ItemDelegate {
                id: row

                required property int index
                required property string noteId
                required property string title
                required property string snippet
                required property string date
                required property string folderName
                required property bool pinned
                required property bool hasAttachments
                required property bool hasChecklist
                readonly property bool selected: noteId === root.currentNoteId
                // Where a note lives matters when the list mixes folders.
                readonly property bool showFolder: (Library.searching || Library.currentKey === "all")
                                                   && folderName.length > 0

                function openMenu() { contextMenu.popup(row, 24, row.height) }

                width: ListView.view.width
                height: showFolder ? 78 : 62
                leftPadding: 16
                rightPadding: 14
                focusPolicy: Qt.NoFocus
                onClicked: {
                    ListView.view.currentIndex = index
                    root.noteChosen(noteId)
                }
                Accessible.name: title + ", " + date
                Accessible.description: snippet
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
                        IconImage {
                            visible: row.pinned && !Library.inTrash
                            source: "qrc:/qt/qml/OmarchyNotes/icons/pin.svg"
                            sourceSize: Qt.size(13, 13)
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                            Accessible.name: qsTr("Pinned")
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
                            textFormat: Text.StyledText
                            elide: Text.ElideRight
                            maximumLineCount: 1
                            font.pixelSize: 12
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                        IconImage {
                            visible: row.hasChecklist
                            source: "qrc:/qt/qml/OmarchyNotes/icons/checklist.svg"
                            sourceSize: Qt.size(14, 14)
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                            Accessible.name: qsTr("Has a checklist")
                        }
                        IconImage {
                            visible: row.hasAttachments
                            source: "qrc:/qt/qml/OmarchyNotes/icons/attach.svg"
                            sourceSize: Qt.size(14, 14)
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                            Accessible.name: qsTr("Has attachments")
                        }
                    }
                    RowLayout {
                        visible: row.showFolder
                        spacing: 5
                        IconImage {
                            source: "qrc:/qt/qml/OmarchyNotes/icons/folder.svg"
                            sourceSize: Qt.size(12, 12)
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                        Label {
                            Layout.fillWidth: true
                            text: row.folderName
                            elide: Text.ElideRight
                            font.pixelSize: 11
                            color: row.selected ? Theme.selectionText : Theme.secondaryText
                        }
                    }
                }

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: row.openMenu()
                }

                Menu {
                    id: contextMenu
                    MenuItem {
                        text: row.pinned ? qsTr("Unpin Note") : qsTr("Pin Note")
                        visible: !Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested(row.pinned ? "unpin" : "pin", row.noteId)
                    }
                    MenuItem {
                        text: qsTr("Move to Folder…")
                        visible: !Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested("move", row.noteId)
                    }
                    MenuItem {
                        text: qsTr("Duplicate")
                        visible: !Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested("duplicate", row.noteId)
                    }
                    MenuItem {
                        text: qsTr("Delete")
                        visible: !Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested("trash", row.noteId)
                    }
                    MenuItem {
                        text: qsTr("Recover")
                        visible: Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested("recover", row.noteId)
                    }
                    MenuItem {
                        text: qsTr("Delete Permanently…")
                        visible: Library.inTrash
                        height: visible ? implicitHeight : 0
                        onTriggered: root.actionRequested("deleteForever", row.noteId)
                    }
                }
            }

            // Empty states say what's going on and what to do next.
            ColumnLayout {
                anchors.centerIn: parent
                width: parent.width - 48
                visible: list.count === 0
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.weight: Font.DemiBold
                    color: Theme.text
                    text: Library.searching ? qsTr("No notes match “%1”").arg(Library.searchText.trim())
                        : Library.inTrash ? qsTr("Recently Deleted is empty")
                        : qsTr("No notes here yet")
                }
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: 12
                    color: Theme.secondaryText
                    text: Library.searching ? qsTr("Try fewer or shorter words.")
                        : Library.inTrash ? qsTr("Notes you delete stay here for 30 days.")
                        : qsTr("Press Ctrl+N to start one.")
                }
            }
        }
    }
}
