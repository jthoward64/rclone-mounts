// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Just the fields for editing an existing source, hosted by SourceEditorPage
// (the right-hand pane). Adding a new source is a fully separate flow
// (SourceAddWizardDialog), so this form only ever has `editing` set and
// never picks a kind. There is no Save/Cancel here: every field commits
// straight into the pending changeset as it's edited (see commitLive()
// below), and the KCM's own Apply/Cancel/Defaults is what actually writes
// it to disk or discards it.
ColumnLayout {
    id: root

    required property var helpers
    required property var backend
    // Always set — this form is edit-only.
    required property var editing

    signal reconnectRequested()

    readonly property string currentKind: root.editing.kind
    // Wizard-only kinds (OAuth/2FA backends) are reconfigured through
    // SourceWizard, not this flat-field form; the "Reconnect…" message below
    // is the only affordance shown for them.
    readonly property bool kindSupported: !root.helpers.kindIsWizardOnly(currentKind)
    readonly property bool acceptable: srcNameField.text.trim().length > 0 && fieldsRepeater.allRequiredFieldsFilled()

    function reset() {
        srcNameField.text = root.editing.display_name;
        secretField.text = "";
    }

    // Collects the current field values into the shape `backend.upsertSource`
    // expects.
    function collect() {
        let opts = {};
        for (let i = 0; i < fieldsRepeater.count; i++) {
            let f = fieldsRepeater.itemAt(i);
            if (f && f.fieldValueOrNull !== null) opts[f.fieldKey] = f.fieldValueOrNull;
        }
        return {
            id: root.editing.name,
            displayName: srcNameField.text.trim(),
            kind: root.currentKind,
            optionsJson: JSON.stringify(opts),
            secret: secretField.text
        };
    }

    // Stages the current field values. A no-op while required fields aren't
    // filled in yet.
    function commitLive() {
        if (!root.kindSupported || !root.acceptable)
            return;
        let v = root.collect();
        root.backend.upsertSource(v.id, v.displayName, v.kind, v.optionsJson, v.secret);
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
            onEditingFinished: root.commitLive()
        }
        QQC2.Label {
            // A source's type can't be changed after creation — rewriting
            // its whole section risks silent data loss — so this is a plain
            // label, not the enabled ComboBox the add wizard uses.
            Kirigami.FormData.label: i18n("Type:")
            text: root.helpers.kindLabel(root.currentKind)
        }

        RowLayout {
            Kirigami.FormData.isSection: true
            visible: !root.kindSupported
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: i18n("This source's sign-in is managed by its setup wizard.")
            }
            QQC2.Button {
                text: i18n("Reconnect…")
                onClicked: root.reconnectRequested()
            }
        }

        SourceKindFieldsRepeater {
            id: fieldsRepeater
            helpers: root.helpers
            // Wizard-only kinds have nothing to show here — reconnecting is
            // handled by SourceWizard instead of this flat form.
            kind: root.kindSupported ? root.currentKind : ""
            editing: root.editing
            onFieldEdited: root.commitLive()
        }

        QQC2.TextField {
            id: secretField
            visible: root.kindSupported
            Kirigami.FormData.label: i18n("Password:")
            echoMode: TextInput.Password
            placeholderText: root.editing.has_secret
                ? i18n("•••• (leave blank to keep)")
                : i18n("password")
            onEditingFinished: root.commitLive()
        }
    }
}
