// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Per-kind connection fields, driven by the kind's schema. A top-level
// Repeater (not wrapped in a ColumnLayout/Item) so its delegates flatten
// straight into whatever Kirigami.FormLayout instantiates it, exactly like
// an inline Repeater would — shared by SourceEditorForm (edit pane) and
// SourceAddWizardDialog (add dialog) so the field-type dispatch logic isn't
// duplicated between them.
//
// Each delegate carries both a text field and a checkbox, toggling which is
// visible by field_type — Kirigami.FormLayout needs a real Control as its
// direct child for its accessibility/label wiring, so a Loader delegate
// (whose `item` isn't ready synchronously) doesn't work here.
Repeater {
    id: root

    required property var helpers
    required property string kind
    // null → no prefill (adding); otherwise an existing source to read
    // option values from.
    property var editing: null

    // Fired on user interaction with any field (not on the programmatic
    // prefill in each delegate's Component.onCompleted below). SourceEditorForm
    // listens to live-stage the edit; SourceAddWizardDialog ignores it since
    // it only ever commits once, on its own "Add source" button.
    signal fieldEdited()

    model: (root.helpers.kindSchema(root.kind) || { fields: [] }).fields

    // Every `required` field (see source_schema.rs) needs a real value, not
    // just a non-empty display name — rclone can't connect without them.
    function allRequiredFieldsFilled() {
        for (let i = 0; i < root.count; i++) {
            let f = root.itemAt(i);
            if (f && f.modelData.required && f.fieldValueOrNull === null) return false;
        }
        return true;
    }

    delegate: RowLayout {
        id: fieldRow
        required property var modelData
        readonly property string fieldKey: modelData.key
        readonly property bool isBool: modelData.field_type === "bool"
        readonly property bool isSelect: modelData.field_type === "select"
        // null means "omit this field" (blank text, an unchecked bool, or a
        // select left on its blank/default choice).
        readonly property var fieldValueOrNull: {
            if (isBool) return boolControl.checked ? "true" : "false";
            if (isSelect) return (selectControl.currentValue || "").length > 0 ? selectControl.currentValue : null;
            return textControl.text.trim().length > 0 ? textControl.text.trim() : null;
        }
        Kirigami.FormData.label: modelData.required ? i18n("%1 *", modelData.label) : modelData.label

        QQC2.TextField {
            id: textControl
            Layout.fillWidth: true
            visible: !fieldRow.isBool && !fieldRow.isSelect
            placeholderText: fieldRow.modelData.placeholder || ""
            onEditingFinished: root.fieldEdited()
            Component.onCompleted: {
                if (root.editing && root.editing.options)
                    text = root.editing.options[fieldRow.fieldKey] || "";
            }
        }
        QQC2.CheckBox {
            id: boolControl
            visible: fieldRow.isBool
            onToggled: root.fieldEdited()
            Component.onCompleted: {
                if (root.editing && root.editing.options)
                    checked = root.editing.options[fieldRow.fieldKey] === "true";
            }
        }
        QQC2.ComboBox {
            id: selectControl
            Layout.fillWidth: true
            visible: fieldRow.isSelect
            model: fieldRow.modelData.options || []
            textRole: "label"
            valueRole: "value"
            onActivated: root.fieldEdited()
            Component.onCompleted: {
                if (root.editing && root.editing.options) {
                    let v = root.editing.options[fieldRow.fieldKey] || "";
                    for (let i = 0; i < model.length; i++) {
                        if (model[i].value === v) { currentIndex = i; break; }
                    }
                }
            }
        }
    }
}
