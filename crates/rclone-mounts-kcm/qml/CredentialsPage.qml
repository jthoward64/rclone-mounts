// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// System-scope-only settings page: which source kinds have a shared/admin
// credential override, and controls to set/clear it. Pushed from
// MainListPage's "Credentials…" action.
Kirigami.ScrollablePage {
    id: root

    required property var backend
    required property var pageRow

    title: i18n("Credentials")

    // No top-bar actions — every pushed page dismisses itself via its own
    // footer instead.
    footer: QQC2.DialogButtonBox {
        standardButtons: QQC2.DialogButtonBox.Close
        onRejected: root.pageRow.pop()
        onAccepted: root.pageRow.pop()
    }

    readonly property var kinds: JSON.parse(backend.credentialKindsJson || "[]")

    // Re-evaluate status text/actions whenever the page becomes visible again
    // (e.g. after closing CredentialEditDialog) — providerOverrideConfigured
    // isn't a qproperty, just a plain invokable, so nothing re-binds it
    // automatically.
    function statusFor(tag) {
        if (root.backend.providerOverrideConfigured(tag)) return "custom";
        if (root.backend.providerDefaultAvailable(tag)) return "default";
        return "unset";
    }

    ColumnLayout {
        width: root.width

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.gridUnit * 4
            visible: root.kinds.length === 0
            icon.name: "dialog-password-symbolic"
            text: i18n("Nothing needs credentials yet")
        }

        Repeater {
            model: root.kinds
            delegate: ColumnLayout {
                id: row
                required property var modelData
                // `providerOverrideConfigured`/`providerDefaultAvailable` are
                // plain invokables, not qproperties, so referencing
                // `backend.busy` here is what makes this binding re-evaluate
                // once `setProviderOverride` finishes (busy flips false) —
                // otherwise nothing would ever re-trigger it.
                readonly property string status: { void root.backend.busy; return root.statusFor(modelData.tag); }
                Layout.fillWidth: true

                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing

                    Kirigami.Icon {
                        implicitWidth: Kirigami.Units.iconSizes.medium
                        implicitHeight: Kirigami.Units.iconSizes.medium
                        source: row.modelData.icon
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        QQC2.Label {
                            text: row.modelData.label
                        }
                        QQC2.Label {
                            text: {
                                switch (row.status) {
                                case "custom": return i18n("Custom credentials configured");
                                case "default": return i18n("Using the built-in default");
                                default: return i18n("Not set — sign-in will use rclone's own shared client");
                                }
                            }
                            opacity: 0.7
                            font: Kirigami.Theme.smallFont
                        }
                    }
                    QQC2.Button {
                        text: row.status === "custom" ? i18n("Clear") : (row.status === "default" ? i18n("Set custom…") : i18n("Set…"))
                        onClicked: {
                            if (row.status === "custom")
                                root.backend.setProviderOverride(row.modelData.tag, "", "");
                            else
                                credentialEditDialog.openFor(row.modelData.tag, row.modelData.label);
                        }
                    }
                }
                Kirigami.Separator { Layout.fillWidth: true }
            }
        }
    }

    CredentialEditDialog {
        id: credentialEditDialog
        backend: root.backend
    }
}
