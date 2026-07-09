// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigami.dialogs as KirigamiDialogs

// Set/clear the system-scope admin override for a kind's OAuth client
// id/secret. Generalized over `kind` so any future credential-capable kind
// (see `backend.credentialKindsJson`) reuses this same dialog.
KirigamiDialogs.Dialog {
    id: root

    required property var backend

    property string kind: ""
    property string kindLabel: ""

    function openFor(newKind, newKindLabel) {
        kind = newKind;
        kindLabel = newKindLabel;
        idField.text = "";
        secretField.text = "";
        open();
    }

    title: i18n("%1 credentials for all system mounts", root.kindLabel)
    preferredWidth: Kirigami.Units.gridUnit * 30

    standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
    onAccepted: {
        root.backend.setProviderOverride(root.kind, idField.text.trim(), secretField.text.trim());
        root.close();
    }
    onRejected: root.close()

    ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 30

        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            opacity: 0.7
            text: root.backend.providerOverrideConfigured(root.kind)
                ? i18n("A shared client ID/secret is currently configured. Leave both fields blank and click Save to remove it.")
                : i18n("Optional. Sets the default %1 client ID/secret for every system mount that doesn't provide its own.", root.kindLabel)
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            QQC2.TextField {
                id: idField
                Kirigami.FormData.label: i18n("Client ID:")
            }
            QQC2.TextField {
                id: secretField
                Kirigami.FormData.label: i18n("Client secret:")
                echoMode: TextInput.Password
            }
        }
    }
}
