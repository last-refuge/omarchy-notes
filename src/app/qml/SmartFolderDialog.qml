import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import OmarchyNotes

// Creates or edits a Smart Folder: a saved set of filters.
Dialog {
    id: dialog

    property string smartId // set when editing
    property var selectedTags: []
    signal created(string key)

    readonly property var dayChoices: [0, 1, 7, 30, 365]
    readonly property var dayLabels: [qsTr("Any time"), qsTr("Today"), qsTr("In the last 7 days"),
                                      qsTr("In the last 30 days"), qsTr("In the last year")]

    function openForCreate() {
        smartId = ""
        nameField.text = ""
        load({})
        open()
    }
    function openForEdit(id) {
        smartId = id
        const current = Library.smartFolder(id)
        nameField.text = current.name || ""
        load(current)
        open()
    }
    function load(c) {
        selectedTags = (c.tags || []).slice()
        matchAll.checked = !!c.matchAllTags
        checklist.checked = !!c.hasChecklist
        attachments.checked = !!c.hasAttachments
        pinned.checked = !!c.pinnedOnly
        edited.currentIndex = Math.max(0, dayChoices.indexOf(c.editedWithinDays || 0))
        created.currentIndex = Math.max(0, dayChoices.indexOf(c.createdWithinDays || 0))
        folders.model = [{id: "", name: qsTr("Any folder"), depth: 0}]
            .concat(Library.folderChoices().filter(f => f.id !== ""))
        folders.currentIndex = Math.max(0, folders.model.findIndex(f => f.id === (c.folderId || "")))
        error.text = ""
    }
    function toggleTag(tag) {
        const next = selectedTags.slice()
        const at = next.indexOf(tag)
        if (at >= 0)
            next.splice(at, 1)
        else
            next.push(tag)
        selectedTags = next
    }
    function criteria() {
        const c = {}
        if (selectedTags.length) c.tags = selectedTags
        if (matchAll.checked) c.matchAllTags = true
        if (checklist.checked) c.hasChecklist = true
        if (attachments.checked) c.hasAttachments = true
        if (pinned.checked) c.pinnedOnly = true
        if (edited.currentIndex > 0) c.editedWithinDays = dayChoices[edited.currentIndex]
        if (created.currentIndex > 0) c.createdWithinDays = dayChoices[created.currentIndex]
        if (folders.currentIndex > 0) c.folderId = folders.model[folders.currentIndex].id
        return c
    }
    function submit() {
        const result = smartId ? Library.updateSmartFolder(smartId, nameField.text, criteria())
                               : Library.createSmartFolder(nameField.text, criteria())
        if (!result.ok) {
            error.text = result.error
            return
        }
        if (!smartId)
            created(result.id)
        close()
    }

    title: smartId ? qsTr("Edit Smart Folder") : qsTr("New Smart Folder")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(460, Overlay.overlay ? Overlay.overlay.width - 32 : 460)
    background: Rectangle { radius: 10; color: Theme.canvas; border.color: Theme.divider }
    onOpened: nameField.forceActiveFocus()

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        TextField {
            id: nameField
            Layout.fillWidth: true
            placeholderText: qsTr("Name")
            placeholderTextColor: Theme.secondaryText
            color: Theme.text
            Accessible.name: qsTr("Smart Folder name")
            onAccepted: dialog.submit()
            background: Rectangle {
                radius: 6
                color: Theme.raised
                border.width: nameField.activeFocus ? 2 : 1
                border.color: nameField.activeFocus ? Theme.focus : Theme.divider
            }
        }

        Label {
            text: qsTr("Notes with these tags")
            font.weight: Font.DemiBold
            color: Theme.text
        }
        Flow {
            Layout.fillWidth: true
            spacing: 6
            visible: Library.allTags.length > 0
            Repeater {
                model: Library.allTags
                delegate: Button {
                    required property string modelData
                    readonly property bool chosen: dialog.selectedTags.indexOf(modelData) >= 0
                    text: "#" + modelData
                    checkable: true
                    checked: chosen
                    onClicked: dialog.toggleTag(modelData)
                    Accessible.name: qsTr("Tag %1").arg(modelData)
                    background: Rectangle {
                        radius: 12
                        implicitHeight: 26
                        color: parent.chosen ? Theme.accent : Theme.raised
                        border.color: parent.visualFocus ? Theme.focus : Theme.divider
                    }
                    contentItem: Label {
                        text: parent.text
                        leftPadding: 4
                        rightPadding: 4
                        color: parent.chosen ? Theme.accentText : Theme.text
                    }
                }
            }
        }
        Label {
            visible: Library.allTags.length === 0
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Tags you add to notes (like #work) show up here.")
            color: Theme.secondaryText
            font.pixelSize: 12
        }
        CheckBox {
            id: matchAll
            visible: dialog.selectedTags.length > 1
            text: qsTr("Only notes with all of these tags")
        }

        Label {
            text: qsTr("And notes that…")
            font.weight: Font.DemiBold
            color: Theme.text
        }
        CheckBox { id: checklist; text: qsTr("Have a checklist") }
        CheckBox { id: attachments; text: qsTr("Have images or attachments") }
        CheckBox { id: pinned; text: qsTr("Are pinned") }

        GridLayout {
            columns: 2
            columnSpacing: 12
            rowSpacing: 8
            Label { text: qsTr("Edited"); color: Theme.text }
            ComboBox { id: edited; Layout.fillWidth: true; model: dialog.dayLabels }
            Label { text: qsTr("Created"); color: Theme.text }
            ComboBox { id: created; Layout.fillWidth: true; model: dialog.dayLabels }
            Label { text: qsTr("In"); color: Theme.text }
            ComboBox {
                id: folders
                Layout.fillWidth: true
                textRole: "name"
            }
        }

        Label {
            id: error
            Layout.fillWidth: true
            visible: text.length > 0
            wrapMode: Text.Wrap
            color: Theme.error
            font.pixelSize: 12
            Accessible.role: Accessible.AlertMessage
        }
    }

    footer: DialogButtonBox {
        background: null
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            text: dialog.smartId ? qsTr("Save") : qsTr("Create")
            enabled: nameField.text.trim().length > 0
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.submit()
        }
    }
}
