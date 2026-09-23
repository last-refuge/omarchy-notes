import QtQuick
import QtQuick.Controls.Basic
import OmarchyNotes

// Toolbar button: a monochrome line icon (or short text label) that follows
// the theme, with a tooltip naming the action and its shortcut.
ToolButton {
    id: control

    property string iconName
    property bool active: false
    property string shortcutHint

    implicitWidth: iconName ? 32 : Math.max(32, implicitContentWidth + 14)
    implicitHeight: 32
    padding: 7
    focusPolicy: Qt.NoFocus
    display: iconName ? AbstractButton.IconOnly : AbstractButton.TextOnly
    icon.source: iconName ? "qrc:/qt/qml/OmarchyNotes/icons/" + iconName + ".svg" : ""
    icon.width: 18
    icon.height: 18
    icon.color: enabled ? Theme.text : Theme.secondaryText
    palette.buttonText: enabled ? Theme.text : Theme.secondaryText

    background: Rectangle {
        radius: 6
        color: control.active ? Theme.selection
             : control.down ? Theme.divider
             : control.hovered ? Theme.raised : "transparent"
        border.width: control.visualFocus ? 2 : 0
        border.color: Theme.focus
    }

    ToolTip.visible: hovered && text.length > 0
    ToolTip.delay: 600
    ToolTip.text: shortcutHint ? text + "  (" + shortcutHint + ")" : text
    Accessible.name: text
    Accessible.checkable: true
    Accessible.checked: active
}
