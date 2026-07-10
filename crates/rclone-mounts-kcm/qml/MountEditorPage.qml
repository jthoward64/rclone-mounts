// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import org.kde.kirigami as Kirigami

// Pushed onto Main.qml's PageRow as the right-hand edit pane for both
// adding and editing a mount — mounts never go through a dialog. No
// Save/Cancel here: MountEditorForm stages every edit into the pending
// changeset live as it's made, and the KCM's own Apply/Cancel/Defaults is
// what actually commits or discards it.
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

    MountEditorForm {
        id: form
        backend: root.backend
        helpers: root.helpers
        sources: root.sources
        editing: root.editing
        presetSource: root.presetSource
    }
}
