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

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.smallSpacing

                    Item { Layout.fillWidth: true }

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

                Kirigami.PlaceholderMessage {
                    Layout.alignment: Qt.AlignCenter
                    Layout.fillWidth: true
                    visible: root.mounts.length === 0
                    icon.name: "folder-cloud-symbolic"
                    text: i18n("No mounts yet")
                    explanation: root.sources.length > 0
                        ? i18n("Add a mount that uses one of your sources.")
                        : i18n("Create an rclone source, then a mount that uses it.")
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
                                source: item.modelData.enabled
                                    ? "media-playback-start-symbolic"
                                    : "media-playback-stopped-symbolic"
                                opacity: item.modelData.enabled ? 1.0 : 0.5
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: item.modelData.name
                                    elide: Text.ElideRight
                                }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: i18n("%1 → %2", item.modelData.source, item.modelData.mountpoint)
                                    elide: Text.ElideMiddle
                                    opacity: 0.7
                                    font: Kirigami.Theme.smallFont
                                }
                            }
                        }

                        actions: [
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

        Kirigami.FormLayout {
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
