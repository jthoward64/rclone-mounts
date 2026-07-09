// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import org.kde.kirigami as Kirigami

// Pushed-page presentation of SourceEditorForm. See Main.qml's
// `editorPresentation` for how this is chosen over SourceEditorDialog.
Kirigami.ScrollablePage {
    id: root

    required property var backend
    required property var helpers
    required property var pageRow
    // null → create; otherwise editing an existing source (name is the key).
    property var editing: null
    // Lets Main.qml's openSourceEditor() tell this page apart from
    // MountEditorPage/MainListPage when checking "is this source's editor
    // already the page on screen?".
    readonly property string pageKind: "sourceEditor"

    signal wizardHandoff(string kind, var editing)

    title: root.editing ? i18n("Edit source") : i18n("Add source")

    // Rendered by the host in the page's own title bar; the automatic back
    // button PageRow gives pushed pages covers Cancel. Labelled by what it
    // actually does (stage the source into the pending changeset) rather
    // than a generic "OK", since the KCM's own Apply button — which is what
    // actually writes it to disk — sits right below and a bare "OK" here
    // reads as a duplicate of it.
    actions: [
        Kirigami.Action {
            text: {
                if (!form.kindSupported) return i18n("Continue");
                return root.editing ? i18n("Save changes") : i18n("Add source");
            }
            icon.name: "dialog-ok"
            enabled: form.acceptable
            onTriggered: root.accept()
        }
    ]

    function accept() {
        if (!form.kindSupported) {
            let kind = form.currentKind;
            let editingSource = root.editing;
            root.pageRow.pop();
            root.wizardHandoff(kind, editingSource);
            return;
        }
        let v = form.collect();
        root.backend.upsertSource(v.id, v.displayName, v.kind, v.optionsJson, v.secret);
        root.pageRow.pop();
    }

    SourceEditorForm {
        id: form
        helpers: root.helpers
        editing: root.editing
    }
}
