import QtQuick
import QtQuick.Controls.Basic
import OmarchyNotes

// Actions on one note, shared by the list and the gallery.
Menu {
    id: menu

    property string noteId
    property bool pinned
    // "trash", "recover", "deleteForever", "pin", "unpin", "move", "duplicate"
    signal action(string action, string noteId)

    MenuItem {
        text: menu.pinned ? qsTr("Unpin Note") : qsTr("Pin Note")
        visible: !Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action(menu.pinned ? "unpin" : "pin", menu.noteId)
    }
    MenuItem {
        text: qsTr("Move to Folder…")
        visible: !Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action("move", menu.noteId)
    }
    MenuItem {
        text: qsTr("Duplicate")
        visible: !Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action("duplicate", menu.noteId)
    }
    MenuItem {
        text: qsTr("Delete")
        visible: !Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action("trash", menu.noteId)
    }
    MenuItem {
        text: qsTr("Recover")
        visible: Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action("recover", menu.noteId)
    }
    MenuItem {
        text: qsTr("Delete Permanently…")
        visible: Library.inTrash
        height: visible ? implicitHeight : 0
        onTriggered: menu.action("deleteForever", menu.noteId)
    }
}
