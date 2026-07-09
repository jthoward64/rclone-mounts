// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigami.dialogs as KirigamiDialogs

// Dialog presentation of SourceEditorForm. See Main.qml's
// `editorPresentation` for how this is chosen over SourceEditorPage.
KirigamiDialogs.Dialog {
    id: root

    required property var backend
    required property var helpers
    property var editing: null

    signal wizardHandoff(string kind, var editing)

    function openFor(source) {
        editing = source;
        form.editing = source;
        form.reset();
        open();
    }

    title: form.editing ? i18n("Edit source") : i18n("Add source")
    preferredWidth: Kirigami.Units.gridUnit * 34

    standardButtons: Kirigami.Dialog.Cancel
    customFooterActions: [
        Kirigami.Action {
            text: form.kindSupported ? i18n("OK") : i18n("Continue")
            enabled: form.acceptable
            onTriggered: root.accept()
        }
    ]

    onRejected: root.close()

    function accept() {
        if (!form.kindSupported) {
            let kind = form.currentKind;
            let editingSource = root.editing;
            root.close();
            root.wizardHandoff(kind, editingSource);
            return;
        }
        let v = form.collect();
        root.backend.upsertSource(v.id, v.displayName, v.kind, v.optionsJson, v.secret);
        root.close();
    }

    SourceEditorForm {
        id: form
        Layout.preferredWidth: Kirigami.Units.gridUnit * 34
        helpers: root.helpers
    }
}
