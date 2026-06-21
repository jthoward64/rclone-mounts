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

    Component.onCompleted: backend.load()

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

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

            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignCenter
                icon.name: "folder-cloud-symbolic"
                text: i18n("No mounts yet")
                explanation: i18n("Create an rclone source, then a mount that uses it.")
            }

            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignCenter
                icon.name: "system-lock-screen-symbolic"
                text: i18n("Administrator access required")
                explanation: i18n("Authenticate to view and manage system-wide mounts.")
            }
        }
    }
}
