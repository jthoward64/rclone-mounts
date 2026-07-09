// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigami.dialogs as KirigamiDialogs

// Dialog presentation of MountEditorForm. See Main.qml's
// `editorPresentation` for how this is chosen over MountEditorPage.
KirigamiDialogs.Dialog {
    id: root

    required property var backend
    required property var helpers
    property var sources: []
    property var editing: null

    // Preselect a specific source when adding a mount from a source's
    // section header, instead of always defaulting to the first one.
    function openFor(mount, presetSource) {
        editing = mount;
        form.editing = mount;
        form.presetSource = presetSource;
        form.reset();
        open();
    }

    title: form.editing ? i18n("Edit mount") : i18n("Add mount")
    preferredWidth: Kirigami.Units.gridUnit * 34

    standardButtons: Kirigami.Dialog.Cancel
    customFooterActions: [
        Kirigami.Action {
            text: i18n("OK")
            enabled: form.acceptable
            onTriggered: root.accept()
        }
    ]

    onRejected: root.close()

    function accept() {
        let v = form.collect();
        root.backend.upsertMount(v.id, v.displayName, v.source, v.subpath, v.mountpoint, v.optionsJson, v.enabled);
        root.close();
    }

    MountEditorForm {
        id: form
        Layout.preferredWidth: Kirigami.Units.gridUnit * 34
        helpers: root.helpers
        sources: root.sources
    }
}
