import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// Shown instead of the main window when the library can't be opened, so a
// launch from the app launcher never fails silently.
ApplicationWindow {
    id: window

    required property string heading
    required property string message
    required property string location

    width: 520
    height: 300
    minimumWidth: 360
    visible: true
    title: qsTr("Omarchy Notes")
    color: Theme.canvas
    palette {
        window: Theme.canvas
        windowText: Theme.text
        button: Theme.raised
        buttonText: Theme.text
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 12
        Label {
            Layout.fillWidth: true
            text: window.heading
            wrapMode: Text.Wrap
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: Theme.text
            Accessible.role: Accessible.Heading
        }
        Label {
            Layout.fillWidth: true
            text: window.message
            wrapMode: Text.Wrap
            color: Theme.text
            textFormat: Text.PlainText
            Accessible.role: Accessible.AlertMessage
        }
        Label {
            Layout.fillWidth: true
            visible: window.location.length > 0
            text: qsTr("Your notes are in %1. Nothing has been changed or deleted.").arg(window.location)
            wrapMode: Text.Wrap
            color: Theme.secondaryText
            font.pixelSize: 12
            textFormat: Text.PlainText
        }
        Item { Layout.fillHeight: true }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8
            Button {
                visible: window.location.length > 0
                text: qsTr("Open Library Folder")
                onClicked: Qt.openUrlExternally("file://" + window.location)
            }
            Button {
                text: qsTr("Quit")
                focus: true
                onClicked: Qt.quit()
            }
        }
    }
}
