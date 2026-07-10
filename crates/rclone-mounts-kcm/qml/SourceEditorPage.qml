// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import org.kde.kirigami as Kirigami

// Pushed onto Main.qml's PageRow as the right-hand edit pane whenever an
// existing source is opened for editing. Adding a new source is a fully
// separate flow (SourceAddWizardDialog) — this page is edit-only. No
// Save/Cancel here: SourceEditorForm stages every edit into the pending
// changeset live as it's made, and the KCM's own Apply/Cancel/Defaults is
// what actually writes it to disk or discards it.
Kirigami.ScrollablePage {
    id: root

    required property var backend
    required property var helpers
    required property var pageRow
    required property var editing
    // Lets Main.qml's openSourceEditor() tell this page apart from
    // MountEditorPage/MainListPage when checking "is this source's editor
    // already the page on screen?".
    readonly property string pageKind: "sourceEditor"

    // Reconnecting a wizard-only source (Drive/iCloud) isn't a field edit —
    // it launches an interactive OAuth/2FA flow, so it can't be live-staged
    // like everything else here. SourceEditorForm's "Reconnect…" action
    // fires this straight through to Main.qml, which opens SourceWizard.
    signal wizardHandoff(string kind, var editing)

    title: i18n("Edit source")

    SourceEditorForm {
        id: form
        backend: root.backend
        helpers: root.helpers
        editing: root.editing
        onReconnectRequested: root.wizardHandoff(currentKind, root.editing)
    }
}
