// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One source's row in the unified list, acting as the section header for the
// mounts nested underneath it. `source` is null for the defensive "orphaned
// mounts" group (a mount whose source was deleted out from under it).
// Clicking the row opens it for editing (or reconnecting, for wizard-only
// kinds). A plain ItemDelegate rather than Kirigami.SwipeListItem — the
// latter's swipe/hover-reveal actions and press animation are built for a
// "reveal actions on hover" pattern, not a "click the row to navigate" one,
// and fought with it (a visible flash on click).
QQC2.ItemDelegate {
    id: root

    required property var source
    required property var helpers

    signal editRequested()
    signal removeRequested()

    width: ListView.view ? ListView.view.width : implicitWidth
    onClicked: if (root.source !== null) root.editRequested()

    contentItem: RowLayout {
        Kirigami.Icon {
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: Kirigami.Units.iconSizes.small
            source: root.source ? root.helpers.kindIcon(root.source.kind) : "dialog-question-symbolic"
        }
        Kirigami.Icon {
            // iCloud's trust token is only valid ~30 days; this is a passive
            // reminder computed client-side from the stamp left in options by
            // the wizard (see Main.qml's Rust counterpart,
            // source_def_from_remote_conf). No backend plumbing needed for a
            // glanceable warning icon.
            visible: root.helpers.needsReconnectSoon(root.source)
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: Kirigami.Units.iconSizes.small
            source: "emblem-warning-symbolic"
            QQC2.ToolTip.text: i18n("This sign-in may expire soon. Use Reconnect to refresh it.")
            // Bound to this icon's own hover, not the row's.
            QQC2.ToolTip.visible: reconnectHover.hovered
            HoverHandler { id: reconnectHover }
        }
        Kirigami.Icon {
            visible: !!(root.source && root.source.has_secret)
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: Kirigami.Units.iconSizes.small
            source: "lock-symbolic"
            opacity: 0.6
            QQC2.ToolTip.text: i18n("A password is stored for this source")
            QQC2.ToolTip.visible: secretHover.hovered
            HoverHandler { id: secretHover }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            QQC2.Label {
                Layout.fillWidth: true
                font.bold: true
                text: root.source ? root.source.display_name : i18n("Unknown source")
                elide: Text.ElideRight
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: {
                    if (!root.source) return "";
                    let o = root.source.options || {};
                    let target = o.host || o.url || "";
                    return target.length > 0
                        ? i18n("%1 — %2", root.helpers.kindLabel(root.source.kind), target)
                        : root.helpers.kindLabel(root.source.kind);
                }
                elide: Text.ElideMiddle
                opacity: 0.7
                font: Kirigami.Theme.smallFont
            }
        }
        QQC2.ToolButton {
            icon.name: "edit-delete-symbolic"
            text: i18n("Remove")
            display: QQC2.ToolButton.IconOnly
            visible: root.source !== null
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            onClicked: root.removeRequested()
        }
    }
}
