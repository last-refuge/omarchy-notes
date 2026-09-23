import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// Asks before something that can't be undone. The action button names
// exactly what happens ("Delete Permanently", not "OK").
Dialog {
    id: dialog

    property string message
    property string confirmText: qsTr("OK")
    property bool destructive: false
    property var callback: null

    function ask(titleText, messageText, confirmLabel, isDestructive, done) {
        title = titleText
        message = messageText
        confirmText = confirmLabel
        destructive = isDestructive
        dialog.callback = done
        open()
    }

    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(420, Overlay.overlay ? Overlay.overlay.width - 32 : 420)
    background: Rectangle { radius: 10; color: Theme.canvas; border.color: Theme.divider }
    onOpened: confirmButton.forceActiveFocus()

    Label {
        width: parent.width
        text: dialog.message
        wrapMode: Text.Wrap
        color: Theme.text
    }

    footer: DialogButtonBox {
        background: null
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            id: confirmButton
            text: dialog.confirmText
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            palette.buttonText: dialog.destructive ? Theme.error : Theme.text
            Keys.onReturnPressed: clicked()
            onClicked: {
                const callback = dialog.callback
                dialog.close()
                if (callback)
                    callback()
            }
        }
    }
}
