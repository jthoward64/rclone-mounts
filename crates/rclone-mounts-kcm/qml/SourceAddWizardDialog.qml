// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigami.dialogs as KirigamiDialogs

// Adding a source is always this wizard-style dialog: step 0 picks the kind
// and a display name, step 1 collects that kind's fields (skipped for
// wizard-only kinds, which hand off to SourceWizard's interactive sign-in
// instead). Editing an existing source is a fully separate flow
// (SourceEditorPage, the right-hand pane) — this dialog only ever creates.
KirigamiDialogs.Dialog {
    id: root

    required property var backend
    required property var helpers

    signal wizardHandoff(string kind, var editing)

    property int step: 0
    readonly property string chosenKind: kindBox.currentValue ?? ""
    readonly property bool kindSupported: !root.helpers.kindIsWizardOnly(root.chosenKind)

    function openFor() {
        root.step = 0;
        kindBox.currentIndex = 0;
        nameField.text = "";
        secretField.text = "";
        open();
    }

    function advance() {
        if (root.step === 0) {
            if (!root.kindSupported) {
                let kind = root.chosenKind;
                root.close();
                root.wizardHandoff(kind, null);
                return;
            }
            root.step = 1;
            return;
        }
        let opts = {};
        for (let i = 0; i < fieldsRepeater.count; i++) {
            let f = fieldsRepeater.itemAt(i);
            if (f && f.fieldValueOrNull !== null) opts[f.fieldKey] = f.fieldValueOrNull;
        }
        root.backend.upsertSource("", nameField.text.trim(), root.chosenKind, JSON.stringify(opts), secretField.text);
        root.close();
    }

    title: root.step === 0 ? i18n("Add source") : i18n("Add source — %1", root.helpers.kindLabel(root.chosenKind))
    preferredWidth: Kirigami.Units.gridUnit * 34

    standardButtons: Kirigami.Dialog.Cancel
    customFooterActions: [
        Kirigami.Action {
            text: i18n("Back")
            visible: root.step === 1
            onTriggered: root.step = 0
        },
        Kirigami.Action {
            text: {
                if (root.step === 0) return root.kindSupported ? i18n("Next") : i18n("Continue");
                return i18n("Add source");
            }
            enabled: root.step === 0
                ? nameField.text.trim().length > 0
                : (nameField.text.trim().length > 0 && fieldsRepeater.allRequiredFieldsFilled())
            onTriggered: root.advance()
        }
    ]

    onRejected: root.close()

    Kirigami.FormLayout {
        Layout.fillWidth: true
        visible: root.step === 0

        QQC2.ComboBox {
            id: kindBox
            Kirigami.FormData.label: i18n("Type:")
            model: root.helpers.sourceKinds
            textRole: "label"
            valueRole: "tag"
        }
        QQC2.TextField {
            id: nameField
            Kirigami.FormData.label: i18n("Name:")
            placeholderText: i18n("e.g. Work share")
        }
    }

    Kirigami.FormLayout {
        Layout.fillWidth: true
        visible: root.step === 1

        SourceKindFieldsRepeater {
            id: fieldsRepeater
            helpers: root.helpers
            // Only reachable when step === 1, which already implies
            // kindSupported — see advance() — so chosenKind is always a
            // flat-schema kind here.
            kind: root.step === 1 ? root.chosenKind : ""
        }
        QQC2.TextField {
            id: secretField
            Kirigami.FormData.label: i18n("Password:")
            echoMode: TextInput.Password
            placeholderText: i18n("password")
        }
    }
}
