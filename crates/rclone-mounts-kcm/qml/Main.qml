// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM
import dev.jthoward.RcloneMounts

KCM.SimpleKCM {
    id: root

    BackendController {
        id: backend
    }

    // Parsed views of the controller's JSON model properties. Re-parsed
    // whenever the controller re-serializes (i.e. on every edit/load).
    readonly property var mounts: JSON.parse(backend.mountsJson || "[]")
    readonly property var sources: JSON.parse(backend.sourcesJson || "[]")

    // Source kinds we offer, and the per-kind connection fields the editor
    // renders. Keeping this declarative means a new rclone backend is just a
    // new entry — the editor and the options payload follow automatically.
    readonly property var sourceKinds: [
        { tag: "smb", label: i18n("SMB / Windows share"), icon: "folder-network-symbolic" },
        { tag: "webdav", label: i18n("WebDAV"), icon: "folder-cloud-symbolic" },
        { tag: "drive", label: i18n("Google Drive"), icon: "folder-google-drive" }
    ]
    readonly property var sourceSchemas: ({
        "smb": [
            { key: "host", label: i18n("Host:"), placeholder: "files.example.com" },
            { key: "user", label: i18n("User:"), placeholder: "alice" },
            { key: "domain", label: i18n("Domain:"), placeholder: i18n("optional") },
            { key: "port", label: i18n("Port:"), placeholder: "445" }
        ],
        "webdav": [
            { key: "url", label: i18n("URL:"), placeholder: "https://dav.example.com/remote.php/dav/files/alice" },
            { key: "vendor", label: i18n("Vendor:"), placeholder: "nextcloud / owncloud / other" },
            { key: "user", label: i18n("User:"), placeholder: "alice" }
        ],
        "drive": []
    })

    function kindLabel(tag) {
        for (let i = 0; i < sourceKinds.length; i++)
            if (sourceKinds[i].tag === tag) return sourceKinds[i].label;
        return tag;
    }
    function kindIcon(tag) {
        for (let i = 0; i < sourceKinds.length; i++)
            if (sourceKinds[i].tag === tag) return sourceKinds[i].icon;
        return "folder-cloud-symbolic";
    }

    // Map a systemd ActiveState (plus our "unsaved" sentinel) to UI bits.
    function statusIcon(active) {
        switch (active) {
        case "active": return "emblem-success-symbolic";
        case "activating":
        case "deactivating": return "view-refresh-symbolic";
        case "failed": return "emblem-error-symbolic";
        case "inactive": return "media-playback-stopped-symbolic";
        case "unsaved": return "document-save-symbolic";
        default: return "dialog-question-symbolic";
        }
    }
    function statusText(active) {
        switch (active) {
        case "active": return i18n("Mounted");
        case "activating": return i18n("Mounting…");
        case "deactivating": return i18n("Unmounting…");
        case "failed": return i18n("Failed");
        case "inactive": return i18n("Stopped");
        case "unsaved": return i18n("Not applied");
        default: return i18n("Unknown");
        }
    }
    function statusColor(active) {
        switch (active) {
        case "active": return Kirigami.Theme.positiveTextColor;
        case "failed": return Kirigami.Theme.negativeTextColor;
        default: return Kirigami.Theme.disabledTextColor;
        }
    }
    function isRunning(active) {
        return active === "active" || active === "activating";
    }

    // Poll live mount status while the My mounts tab is showing applied mounts.
    Timer {
        interval: 3000
        repeat: true
        running: tabs.currentIndex === 0 && root.mounts.length > 0
        onTriggered: backend.refreshStatus()
    }

    // The C++ shim emits these in response to the KCM's Apply/Reset/load.
    Connections {
        target: kcm
        function onLoadRequested() { backend.load() }
        function onSaveRequested() { backend.commit() }
        function onDefaultsRequested() { backend.reset() }
    }

    // Dirty state drives the framework's Apply/Reset button enablement.
    Binding {
        target: kcm
        property: "needsSave"
        value: backend.dirty
    }

    // No Component.onCompleted load: the KCM framework calls the C++ load()
    // after construction, which emits loadRequested → backend.load() above.

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: backend.errorString
            visible: backend.errorString.length > 0
        }

        QQC2.TabBar {
            id: tabs
            Layout.fillWidth: true
            QQC2.TabButton { text: i18n("My mounts") }
            QQC2.TabButton { text: i18n("System mounts") }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            // --- My mounts -------------------------------------------------
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                // Sources section.
                RowLayout {
                    Layout.fillWidth: true
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: i18n("Sources")
                    }
                    QQC2.Button {
                        icon.name: "list-add-symbolic"
                        text: i18n("Add source…")
                        onClicked: sourceEditor.openFor(null)
                    }
                }

                QQC2.Label {
                    Layout.fillWidth: true
                    visible: root.sources.length === 0
                    text: i18n("No sources yet. A source is an rclone remote (an SMB share, WebDAV server, …) that mounts point at.")
                    wrapMode: Text.WordWrap
                    opacity: 0.7
                }

                ListView {
                    id: sourcesView
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: root.sources.length > 0
                    clip: true
                    model: root.sources

                    delegate: Kirigami.SwipeListItem {
                        id: srcItem
                        required property var modelData

                        contentItem: RowLayout {
                            Kirigami.Icon {
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: Kirigami.Units.iconSizes.small
                                source: root.kindIcon(srcItem.modelData.kind)
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: srcItem.modelData.name
                                    elide: Text.ElideRight
                                }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: {
                                        let o = srcItem.modelData.options || {};
                                        let target = o.host || o.url || "";
                                        return target.length > 0
                                            ? i18n("%1 — %2", root.kindLabel(srcItem.modelData.kind), target)
                                            : root.kindLabel(srcItem.modelData.kind);
                                    }
                                    elide: Text.ElideMiddle
                                    opacity: 0.7
                                    font: Kirigami.Theme.smallFont
                                }
                            }
                            Kirigami.Icon {
                                visible: srcItem.modelData.has_secret
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: Kirigami.Units.iconSizes.small
                                source: "lock-symbolic"
                                opacity: 0.6
                                QQC2.ToolTip.text: i18n("A password is stored for this source")
                                QQC2.ToolTip.visible: hovered ?? false
                            }
                        }

                        actions: [
                            Kirigami.Action {
                                icon.name: "document-edit-symbolic"
                                text: i18n("Edit")
                                onTriggered: sourceEditor.openFor(srcItem.modelData)
                            },
                            Kirigami.Action {
                                icon.name: "edit-delete-symbolic"
                                text: i18n("Remove")
                                onTriggered: backend.removeSource(srcItem.modelData.name)
                            }
                        ]
                    }
                }

                Kirigami.Separator { Layout.fillWidth: true }

                // Mounts section.
                RowLayout {
                    Layout.fillWidth: true
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: i18n("Mounts")
                    }
                    QQC2.Button {
                        icon.name: "list-add-symbolic"
                        text: i18n("Add mount…")
                        enabled: root.sources.length > 0
                        QQC2.ToolTip.text: i18n("Define a source first")
                        QQC2.ToolTip.visible: hovered && root.sources.length === 0
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        onClicked: mountEditor.openFor(null)
                    }
                }

                QQC2.Label {
                    Layout.fillWidth: true
                    visible: root.mounts.length === 0
                    text: root.sources.length > 0
                        ? i18n("No mounts yet. Add a mount that uses one of your sources.")
                        : i18n("No mounts yet. Create a source first, then a mount that uses it.")
                    wrapMode: Text.WordWrap
                    opacity: 0.7
                }

                ListView {
                    id: mountsView
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: root.mounts.length > 0
                    clip: true
                    model: root.mounts

                    delegate: Kirigami.SwipeListItem {
                        id: item
                        required property var modelData

                        contentItem: RowLayout {
                            Kirigami.Icon {
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: Kirigami.Units.iconSizes.small
                                source: root.statusIcon(item.modelData.active)
                                color: root.statusColor(item.modelData.active)
                                isMask: true
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: item.modelData.name
                                    elide: Text.ElideRight
                                }
                                // Status word (colored) + connection, on the
                                // left so it never collides with the swipe
                                // action icons on the right.
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Kirigami.Units.smallSpacing
                                    QQC2.Label {
                                        text: root.statusText(item.modelData.active)
                                        color: root.statusColor(item.modelData.active)
                                        font: Kirigami.Theme.smallFont
                                    }
                                    QQC2.Label {
                                        Layout.fillWidth: true
                                        text: i18n("· %1 → %2", item.modelData.source, item.modelData.mountpoint)
                                        elide: Text.ElideMiddle
                                        opacity: 0.7
                                        font: Kirigami.Theme.smallFont
                                    }
                                }
                            }
                        }

                        actions: [
                            Kirigami.Action {
                                icon.name: "media-playback-start-symbolic"
                                text: i18n("Start")
                                visible: item.modelData.applied && !root.isRunning(item.modelData.active)
                                onTriggered: backend.startMount(item.modelData.name)
                            },
                            Kirigami.Action {
                                icon.name: "media-playback-stop-symbolic"
                                text: i18n("Stop")
                                visible: root.isRunning(item.modelData.active)
                                onTriggered: backend.stopMount(item.modelData.name)
                            },
                            Kirigami.Action {
                                icon.name: "document-edit-symbolic"
                                text: i18n("Edit")
                                onTriggered: mountEditor.openFor(item.modelData)
                            },
                            Kirigami.Action {
                                icon.name: "edit-delete-symbolic"
                                text: i18n("Remove")
                                onTriggered: backend.removeMount(item.modelData.name)
                            }
                        ]
                    }
                }
            }

            // --- System mounts ---------------------------------------------
            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignCenter
                icon.name: "system-lock-screen-symbolic"
                text: i18n("Administrator access required")
                explanation: i18n("Authenticate to view and manage system-wide mounts.")
            }
        }
    }

    // --- Source editor ----------------------------------------------------
    Kirigami.OverlaySheet {
        id: sourceEditor

        // null → create; otherwise editing an existing source (name is the key).
        property var editing: null
        readonly property string currentKind: kindBox.currentValue ?? "smb"
        readonly property bool kindSupported: currentKind !== "drive"

        function openFor(source) {
            editing = source;
            srcNameField.text = source ? source.name : "";
            secretField.text = "";
            // Kind is locked when editing; select it (or default to SMB).
            let kindIdx = 0;
            if (source) {
                for (let i = 0; i < root.sourceKinds.length; i++)
                    if (root.sourceKinds[i].tag === source.kind) { kindIdx = i; break; }
            }
            kindBox.currentIndex = kindIdx;
            open();
        }

        title: editing ? i18n("Edit source") : i18n("Add source")

        // Bound width: an OverlaySheet sizes to its content, so a fillWidth
        // child (the InlineMessage) would otherwise collapse the sheet to 0.
        ColumnLayout {
            implicitWidth: Kirigami.Units.gridUnit * 26

            Kirigami.FormLayout {
                id: sourceForm
                Layout.fillWidth: true

                QQC2.TextField {
                    id: srcNameField
                    Kirigami.FormData.label: i18n("Name:")
                    enabled: sourceEditor.editing === null
                    placeholderText: i18n("e.g. work-share")
                }
                QQC2.ComboBox {
                    id: kindBox
                    Kirigami.FormData.label: i18n("Type:")
                    // Changing the type of an existing source rewrites its whole
                    // section; lock it on edit to avoid silent data loss.
                    enabled: sourceEditor.editing === null
                    model: root.sourceKinds
                    textRole: "label"
                    valueRole: "tag"
                }

                Kirigami.InlineMessage {
                    Kirigami.FormData.isSection: true
                    Layout.fillWidth: true
                    visible: !sourceEditor.kindSupported
                    type: Kirigami.MessageType.Information
                    text: i18n("Google Drive needs an OAuth sign-in flow that isn't wired up yet.")
                }

                // Per-kind connection fields, driven by sourceSchemas.
                Repeater {
                    id: fieldsRepeater
                    model: root.sourceSchemas[sourceEditor.currentKind] || []
                    delegate: QQC2.TextField {
                        required property var modelData
                        property string fieldKey: modelData.key
                        Kirigami.FormData.label: modelData.label
                        placeholderText: modelData.placeholder || ""
                        Component.onCompleted: {
                            if (sourceEditor.editing && sourceEditor.editing.options)
                                text = sourceEditor.editing.options[fieldKey] || "";
                        }
                    }
                }

                QQC2.TextField {
                    id: secretField
                    visible: sourceEditor.kindSupported
                    Kirigami.FormData.label: i18n("Password:")
                    echoMode: TextInput.Password
                    placeholderText: (sourceEditor.editing && sourceEditor.editing.has_secret)
                        ? i18n("•••• (leave blank to keep)")
                        : i18n("required")
                }
            }
        }

        footer: QQC2.DialogButtonBox {
            standardButtons: QQC2.DialogButtonBox.Ok | QQC2.DialogButtonBox.Cancel
            // Disable OK until the source is valid enough to stage.
            Component.onCompleted: {
                let ok = standardButton(QQC2.DialogButtonBox.Ok);
                ok.enabled = Qt.binding(() =>
                    srcNameField.text.trim().length > 0 && sourceEditor.kindSupported);
            }
            onAccepted: {
                let opts = {};
                for (let i = 0; i < fieldsRepeater.count; i++) {
                    let f = fieldsRepeater.itemAt(i);
                    if (f && f.text.trim().length > 0) opts[f.fieldKey] = f.text.trim();
                }
                backend.upsertSource(srcNameField.text.trim(),
                                     sourceEditor.currentKind,
                                     JSON.stringify(opts),
                                     secretField.text);
                sourceEditor.close();
            }
            onRejected: sourceEditor.close()
        }
    }

    // --- Mount editor -----------------------------------------------------
    Kirigami.OverlaySheet {
        id: mountEditor

        // null → create; otherwise editing an existing row (name is the key).
        property var editing: null

        function openFor(mount) {
            editing = mount;
            nameField.text = mount ? mount.name : "";
            mountpointField.text = mount ? mount.mountpoint : "";
            enabledBox.checked = mount ? mount.enabled : false;
            // Preselect the mount's source, else the first available.
            let idx = 0;
            if (mount) {
                for (let i = 0; i < root.sources.length; i++) {
                    if (root.sources[i].name === mount.source) { idx = i; break; }
                }
            }
            sourceBox.currentIndex = idx;
            open();
        }

        title: editing ? i18n("Edit mount") : i18n("Add mount")

        ColumnLayout {
            implicitWidth: Kirigami.Units.gridUnit * 26

            Kirigami.FormLayout {
                Layout.fillWidth: true

                QQC2.TextField {
                    id: nameField
                    Kirigami.FormData.label: i18n("Name:")
                    // The name is the unit key; lock it when editing.
                    enabled: mountEditor.editing === null
                    placeholderText: i18n("e.g. work-files")
                }
                QQC2.ComboBox {
                    id: sourceBox
                    Kirigami.FormData.label: i18n("Source:")
                    model: root.sources
                    textRole: "name"
                }
                QQC2.TextField {
                    id: mountpointField
                    Kirigami.FormData.label: i18n("Mount point:")
                    placeholderText: i18n("e.g. ~/Mounts/work")
                }
                QQC2.CheckBox {
                    id: enabledBox
                    Kirigami.FormData.label: i18n("Start at login:")
                }
            }
        }

        footer: QQC2.DialogButtonBox {
            standardButtons: QQC2.DialogButtonBox.Ok | QQC2.DialogButtonBox.Cancel
            onAccepted: {
                let src = root.sources[sourceBox.currentIndex];
                backend.upsertMount(nameField.text.trim(),
                                    src ? src.name : "",
                                    mountpointField.text.trim(),
                                    enabledBox.checked);
                mountEditor.close();
            }
            onRejected: mountEditor.close()
        }
    }
}
