import QtQuick
import QtQuick.Controls.Basic
import OmarchyNotes

// Transient message at the bottom of the window, e.g. content that couldn't
// be pasted. Announced to screen readers as an alert.
Rectangle {
    id: toast

    function show(message) {
        label.text = message
        visible = true
        hideTimer.restart()
    }

    visible: false
    width: Math.min(label.implicitWidth + 32, parent.width - 32)
    height: label.implicitHeight + 20
    radius: 8
    color: Theme.raised
    border.color: Theme.divider
    Accessible.role: Accessible.AlertMessage
    Accessible.name: label.text

    Label {
        id: label
        anchors.fill: parent
        anchors.margins: 10
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        color: Theme.text
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
    }

    Timer {
        id: hideTimer
        interval: 5000
        onTriggered: toast.visible = false
    }

    TapHandler {
        onTapped: toast.visible = false
    }
}
