// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import org.kde.kirigami as Kirigami

// Pushed-page presentation of MountEditorForm. See Main.qml's
// `editorPresentation` for how this is chosen over MountEditorDialog.
Kirigami.ScrollablePage {
    id: root

    required property var backend
    required property var helpers
    required property var pageRow
    property var sources: []
    // null → create; otherwise editing an existing row (name is the key).
    property var editing: null
    // Preselect a specific source when adding a mount from a source's
    // section header, instead of always defaulting to the first one.
    property var presetSource: null
    // Lets Main.qml's openMountEditor() tell this page apart from
    // SourceEditorPage/MainListPage when checking "is this mount's editor
    // already the page on screen?".
    readonly property string pageKind: "mountEditor"

    title: root.editing ? i18n("Edit mount") : i18n("Add mount")

    // Rendered by the host in the page's own title bar; the automatic back
    // button PageRow gives pushed pages covers Cancel. Labelled by what it
    // actually does (stage the mount into the pending changeset) rather
    // than a generic "OK", since the KCM's own Apply button — which is what
    // actually writes it to disk — sits right below and a bare "OK" here
    // reads as a duplicate of it.
    actions: [
        Kirigami.Action {
            text: root.editing ? i18n("Save changes") : i18n("Add mount")
            icon.name: "dialog-ok"
            enabled: form.acceptable
            onTriggered: root.accept()
        }
    ]

    function accept() {
        let v = form.collect();
        root.backend.upsertMount(v.id, v.displayName, v.source, v.subpath, v.mountpoint, v.optionsJson, v.enabled);
        root.pageRow.pop();
    }

    MountEditorForm {
        id: form
        helpers: root.helpers
        sources: root.sources
        editing: root.editing
        presetSource: root.presetSource
    }
}
