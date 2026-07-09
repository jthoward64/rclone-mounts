// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Just the fields for adding/editing a source — no dialog/page chrome, so
// the same form can be hosted in either SourceEditorDialog (Kirigami.Dialog)
// or SourceEditorPage (a pushed Kirigami.ScrollablePage). See Main.qml's
// `editorPresentation` for which one is actually used.
ColumnLayout {
    id: root

    required property var helpers
    // null → create; otherwise editing an existing source (name is the key).
    property var editing: null

    // Reads straight from `editing` when editing, rather than
    // `kindBox.currentValue`: QQC2.ComboBox's `currentValue` lags one tick
    // behind `currentIndex` when the index is set programmatically during
    // construction (confirmed via logging — currentIndex was already 1 while
    // currentValue/fieldsRepeater still saw the previous kind), so deriving
    // currentKind from it caused fieldsRepeater's model to briefly build the
    // wrong kind's fields, then immediately rebuild with the right ones —
    // the exact child-churn-mid-construction pattern that trips a real bug
    // in Kirigami.FormLayout (relayout() dereferences a transiently null
    // `item`; see FormLayout.qml:348/397/468). `root.editing.kind` is
    // available synchronously with no such lag.
    readonly property string currentKind: root.editing ? root.editing.kind : (kindBox.currentValue ?? "smb")
    // Wizard-only kinds (OAuth/2FA backends) are configured through
    // SourceWizard, not this flat-field form; the wrapper checks this to
    // decide whether accepting hands off to the wizard instead of upserting.
    readonly property bool kindSupported: !root.helpers.kindIsWizardOnly(currentKind)
    // Every `required` field (see source_schema.rs) needs a real value, not
    // just a non-empty display name — rclone can't connect without them.
    function allRequiredFieldsFilled() {
        for (let i = 0; i < fieldsRepeater.count; i++) {
            let f = fieldsRepeater.itemAt(i);
            if (f && f.modelData.required && f.fieldValueOrNull === null) return false;
        }
        return true;
    }
    // Wizard-only kinds collect the name in SourceWizard instead — this
    // form's name field is hidden for them, so it shouldn't gate acceptance.
    readonly property bool acceptable: root.kindSupported
        ? (srcNameField.text.trim().length > 0 && allRequiredFieldsFilled())
        : true

    function reset() {
        srcNameField.text = root.editing ? root.editing.display_name : "";
        secretField.text = "";
        // kindBox.currentIndex is NOT set here — see its own binding for why.
    }

    // Collects the current field values into the shape `backend.upsertSource`
    // expects. Only meaningful when `kindSupported` — wizard-only kinds are
    // handed off to SourceWizard instead.
    function collect() {
        let opts = {};
        for (let i = 0; i < fieldsRepeater.count; i++) {
            let f = fieldsRepeater.itemAt(i);
            if (f && f.fieldValueOrNull !== null) opts[f.fieldKey] = f.fieldValueOrNull;
        }
        return {
            id: root.editing ? root.editing.name : "",
            displayName: srcNameField.text.trim(),
            kind: root.currentKind,
            optionsJson: JSON.stringify(opts),
            secret: secretField.text
        };
    }

    Component.onCompleted: root.reset()

    Kirigami.FormLayout {
        id: sourceForm
        Layout.fillWidth: true

        QQC2.TextField {
            id: srcNameField
            visible: root.kindSupported
            Kirigami.FormData.label: i18n("Name:")
            placeholderText: i18n("e.g. Work share")
        }
        QQC2.ComboBox {
            id: kindBox
            Kirigami.FormData.label: i18n("Type:")
            // Changing the type of an existing source rewrites its whole
            // section; lock it on edit to avoid silent data loss.
            enabled: root.editing === null
            model: root.helpers.sourceKinds
            textRole: "label"
            valueRole: "tag"
            // A real binding evaluated as part of construction — not set
            // imperatively in reset()/Component.onCompleted. This ComboBox
            // drives fieldsRepeater's model (via currentKind below); setting
            // it *after* the FormLayout's first layout pass made the
            // Repeater tear down its "smb"-fallback delegates and build the
            // real kind's the moment reset() ran, which is exactly the
            // child-churn-mid-construction pattern that trips a real bug in
            // Kirigami.FormLayout (relayout() dereferences a transiently
            // null `item` — see FormLayout.qml:348/397/468). Getting this
            // right on the very first evaluation means the Repeater's model
            // is correct from the start and never has to swap.
            currentIndex: {
                if (!root.editing) return 0;
                for (let i = 0; i < root.helpers.sourceKinds.length; i++)
                    if (root.helpers.sourceKinds[i].tag === root.editing.kind) return i;
                return 0;
            }
        }

        // Per-kind connection fields, driven by the kind's schema. Each
        // delegate carries both a text field and a checkbox, toggling which
        // is visible by field_type — Kirigami.FormLayout needs a real
        // Control as its direct child for its accessibility/label wiring, so
        // a Loader delegate (whose `item` isn't ready synchronously) doesn't
        // work here.
        Repeater {
            id: fieldsRepeater
            model: (root.helpers.kindSchema(root.currentKind) || { fields: [] }).fields
            delegate: RowLayout {
                id: fieldRow
                required property var modelData
                readonly property string fieldKey: modelData.key
                readonly property bool isBool: modelData.field_type === "bool"
                readonly property bool isSelect: modelData.field_type === "select"
                // null means "omit this field" (blank text, an unchecked
                // bool, or a select left on its blank/default choice).
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
                    Component.onCompleted: {
                        if (root.editing && root.editing.options)
                            text = root.editing.options[fieldRow.fieldKey] || "";
                    }
                }
                QQC2.CheckBox {
                    id: boolControl
                    visible: fieldRow.isBool
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

        QQC2.TextField {
            id: secretField
            visible: root.kindSupported
            Kirigami.FormData.label: i18n("Password:")
            echoMode: TextInput.Password
            placeholderText: (root.editing && root.editing.has_secret)
                ? i18n("•••• (leave blank to keep)")
                : i18n("password")
        }
    }
}
