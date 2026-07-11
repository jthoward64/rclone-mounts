// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Multi-step interactive source setup: Google Drive OAuth (local-browser
// sign-in) and iCloud Drive (Apple ID + password + 2FA code). Driven by
// backend.beginInteractiveSource/submitWizardInput/cancelWizard and the
// wizardState/wizardPromptJson/wizardError properties — there's no push
// signal, so this just binds to those the same way the rest of the UI binds
// to busy/errorString.
Kirigami.OverlaySheet {
    id: wizard

    property string kind: ""
    // null → create; otherwise reconnecting an existing source.
    property var editing: null
    readonly property var prompt: {
        try {
            return backend.wizardPromptJson ? JSON.parse(backend.wizardPromptJson) : null;
        } catch (e) {
            return null;
        }
    }
    // No build-time or admin default client id/secret is available for
    // Drive — the user must supply their own or sign-in has nothing to
    // authenticate with.
    readonly property bool driveNeedsOwnCreds: kind === "drive" && !backend.providerDefaultAvailable("drive")

    // `prefillName` carries over whatever the user already typed into the
    // add-source dialog's own Name field before handing off to this wizard
    // — without it, the field would just reappear empty and make them type
    // the name a second time.
    function openFor(kindTag, source, prefillName) {
        kind = kindTag;
        editing = source;
        nameField.text = source ? source.display_name : (prefillName || "");
        appleIdField.text = "";
        applePasswordField.text = "";
        driveClientIdField.text = "";
        driveClientSecretField.text = "";
        codeField.text = "";
        backend.cancelWizard();
        open();
    }

    function startSignIn() {
        let seed = {};
        if (kind === "iclouddrive") {
            seed.apple_id = appleIdField.text.trim();
            seed.password = applePasswordField.text;
        } else if (kind === "drive") {
            if (driveClientIdField.text.trim().length > 0) seed.client_id = driveClientIdField.text.trim();
            if (driveClientSecretField.text.trim().length > 0) seed.client_secret = driveClientSecretField.text.trim();
        }
        backend.beginInteractiveSource(editing ? editing.name : "", nameField.text.trim(), kind, JSON.stringify(seed));
    }

    // OverlaySheet is a QtQuick.Templates.Popup under the hood, which has
    // `opened()`/`closed()` signals (no `closing()`) — `closed` fires after
    // every way the sheet can go away: Esc, clicking outside, or the
    // footer's Cancel/Close.
    onClosed: backend.cancelWizard()

    title: kind === "drive" ? i18n("Sign in to Google Drive") : i18n("Sign in to iCloud Drive")

    // NOT overriding implicitHeight (tried, reverted — see Main.qml's
    // sourceEditor for the full explanation): Kirigami's own self-referential
    // clamp (`Math.min(h, parent.height - y)`) is what keeps the sheet from
    // rendering past the true usable area; replacing it to silence a cosmetic
    // binding-loop warning let the sheet overflow past kcmshell's button bar.

    ColumnLayout {
        id: wizardContent
        // Layout.preferredWidth (not implicitWidth): OverlaySheet checks
        // Layout.preferredWidth before implicitWidth when sizing itself, and
        // a plain constant here avoids a width binding loop that
        // implicitWidth caused.
        Layout.preferredWidth: Kirigami.Units.gridUnit * 34
        spacing: Kirigami.Units.smallSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: backend.wizardError
            visible: backend.wizardState === "error" && backend.wizardError.length > 0
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: backend.wizardState === "idle" || backend.wizardState === "error"

            QQC2.TextField {
                id: nameField
                Kirigami.FormData.label: i18n("Name:")
                placeholderText: i18n("e.g. Work Drive")
            }

            // --- Google Drive ---
            Kirigami.InlineMessage {
                Kirigami.FormData.isSection: true
                Layout.fillWidth: true
                visible: wizard.driveNeedsOwnCreds
                type: Kirigami.MessageType.Information
                text: i18n("No default Google Drive credentials are configured. Enter your own OAuth client ID and secret to continue.")
            }
            Kirigami.Separator {
                visible: wizard.kind === "drive" && !wizard.driveNeedsOwnCreds
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: i18n("Advanced (optional)")
            }
            QQC2.TextField {
                id: driveClientIdField
                visible: wizard.kind === "drive"
                Kirigami.FormData.label: i18n("Client ID:")
                placeholderText: wizard.driveNeedsOwnCreds ? i18n("required") : i18n("leave blank to use the default")
            }
            QQC2.TextField {
                id: driveClientSecretField
                visible: wizard.kind === "drive"
                Kirigami.FormData.label: i18n("Client secret:")
                echoMode: TextInput.Password
                placeholderText: wizard.driveNeedsOwnCreds ? i18n("required") : i18n("leave blank to use the default")
            }

            // --- iCloud Drive ---
            QQC2.TextField {
                id: appleIdField
                visible: wizard.kind === "iclouddrive"
                Kirigami.FormData.label: i18n("Apple ID:")
                placeholderText: "you@icloud.com"
            }
            QQC2.TextField {
                id: applePasswordField
                visible: wizard.kind === "iclouddrive"
                Kirigami.FormData.label: i18n("Password:")
                echoMode: TextInput.Password
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: backend.wizardState === "running"
            spacing: Kirigami.Units.smallSpacing
            QQC2.BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: true
            }
            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: wizard.kind === "drive"
                    ? i18n("A browser window has opened — finish signing in there, then come back.")
                    : i18n("Contacting Apple…")
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: backend.wizardState === "need_input" && wizard.prompt !== null
            RowLayout {
                Kirigami.FormData.label: wizard.prompt ? wizard.prompt.help : ""
                QQC2.TextField {
                    id: codeField
                    Layout.fillWidth: true
                    echoMode: (wizard.prompt && wizard.prompt.is_password) ? TextInput.Password : TextInput.Normal
                    placeholderText: i18n("code")
                    onAccepted: verifyButton.clicked()
                }
                QQC2.Button {
                    id: verifyButton
                    text: i18n("Verify")
                    enabled: codeField.text.trim().length > 0
                    onClicked: {
                        backend.submitWizardInput(codeField.text.trim());
                        codeField.text = "";
                    }
                }
            }
            QQC2.Button {
                text: i18n("Send code by SMS instead")
                visible: wizard.prompt && wizard.prompt.sms_available === true
                onClicked: backend.submitWizardInput("sms")
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: backend.wizardState === "done"
            text: i18n("Signed in. Click Close, then Apply to save.")
            wrapMode: Text.WordWrap
        }

        // OverlaySheet auto-margins its content's top/left/right edges but
        // never binds a bottom anchor (see templates/OverlaySheet.qml's
        // onContentItemChanged), so content otherwise sits flush against the
        // footer separator with no breathing room at all.
        Item { Layout.preferredHeight: Kirigami.Units.largeSpacing }
    }

    footer: QQC2.DialogButtonBox {
        standardButtons: backend.wizardState === "done" ? QQC2.DialogButtonBox.Close : QQC2.DialogButtonBox.Cancel
        QQC2.Button {
            text: wizard.kind === "drive" ? i18n("Sign in with Google") : i18n("Continue")
            visible: backend.wizardState === "idle" || backend.wizardState === "error"
            enabled: nameField.text.trim().length > 0
                && (wizard.kind !== "iclouddrive" || (appleIdField.text.trim().length > 0 && applePasswordField.text.length > 0))
                && (!wizard.driveNeedsOwnCreds || (driveClientIdField.text.trim().length > 0 && driveClientSecretField.text.trim().length > 0))
            // Not AcceptRole: DialogButtonBox emits accepted() for AcceptRole
            // buttons, and this sheet's onAccepted closes it — which would
            // dismiss the wizard the instant sign-in starts, before it can
            // reach the "need_input"/"done" states. ActionRole just runs
            // onClicked with no auto-close side effect.
            QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
            onClicked: wizard.startSignIn()
        }
        onRejected: wizard.close()
        onAccepted: wizard.close()
    }
}
