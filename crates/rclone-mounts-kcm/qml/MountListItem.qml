// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One mount's row, nested under its source's SourceSectionHeader in the
// unified list. Clicking the row opens it for editing; a plain ItemDelegate
// rather than Kirigami.SwipeListItem — see SourceSectionHeader.qml's
// comment for why.
QQC2.ItemDelegate {
    id: root

    required property var mount
    required property var helpers
    required property var sources

    signal startRequested()
    signal stopRequested()
    signal restartRequested()
    signal editRequested()
    signal removeRequested()

    width: ListView.view ? ListView.view.width : implicitWidth
    onClicked: root.editRequested()

    contentItem: RowLayout {
        // Indented so the row visually reads as belonging to the source
        // section above it.
        Item { implicitWidth: Kirigami.Units.gridUnit }
        Kirigami.Icon {
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: Kirigami.Units.iconSizes.small
            source: root.helpers.statusIcon(root.mount.active)
            color: root.helpers.statusColor(root.mount.active)
            isMask: true
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            QQC2.Label {
                Layout.fillWidth: true
                text: root.mount.display_name
                elide: Text.ElideRight
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                QQC2.Label {
                    text: root.helpers.statusText(root.mount.active)
                    color: root.helpers.statusColor(root.mount.active)
                    font: Kirigami.Theme.smallFont
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: i18n("· %1 → %2", root.mount.subpath
                        ? i18n("%1/%2", root.helpers.sourceDisplay(root.sources, root.mount.source), root.mount.subpath)
                        : root.helpers.sourceDisplay(root.sources, root.mount.source), root.mount.mountpoint)
                    elide: Text.ElideMiddle
                    opacity: 0.7
                    font: Kirigami.Theme.smallFont
                }
            }
        }
        QQC2.ToolButton {
            icon.name: "media-playback-start-symbolic"
            text: i18n("Start")
            display: QQC2.ToolButton.IconOnly
            visible: root.mount.applied && !root.helpers.isRunning(root.mount.active)
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            onClicked: root.startRequested()
        }
        QQC2.ToolButton {
            icon.name: "media-playback-stop-symbolic"
            text: i18n("Stop")
            display: QQC2.ToolButton.IconOnly
            visible: root.helpers.isRunning(root.mount.active)
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            onClicked: root.stopRequested()
        }
        QQC2.ToolButton {
            icon.name: "view-refresh-symbolic"
            text: i18n("Refresh directory cache")
            display: QQC2.ToolButton.IconOnly
            visible: root.helpers.isRunning(root.mount.active)
            QQC2.ToolTip.text: i18n("Restart the mount to drop its cached directory listing and re-fetch it from the remote")
            QQC2.ToolTip.visible: hovered
            onClicked: root.restartRequested()
        }
        QQC2.ToolButton {
            icon.name: "edit-delete-symbolic"
            text: i18n("Remove")
            display: QQC2.ToolButton.IconOnly
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            onClicked: root.removeRequested()
        }
    }
}
