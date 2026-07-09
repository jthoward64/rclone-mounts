// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The unified sources+mounts list: one section (with a SourceSectionHeader)
// per source, its mounts nested underneath. Pushed as the initial page of
// Main.qml's PageRow.
//
// Kirigami.ScrollablePage auto-detects a single Flickable child and
// reparents it into its real contentItem, while any other plain-Item
// siblings are left behind in a separate internal container that overlaps
// at (0,0) — mixing a ListView with sibling Items here silently made the
// siblings invisible (covered by the ListView). So PlaceholderMessage is
// nested *inside* the ListView (its child, not its sibling) rather than
// alongside it, keeping ScrollablePage's single-Flickable-child assumption
// intact.
Kirigami.ScrollablePage {
    id: root

    required property var backend
    required property var helpers

    signal addSourceRequested()
    signal editSourceRequested(var source)
    signal addMountRequested(var source)
    signal editMountRequested(var mount)
    signal openCredentialsRequested()

    readonly property var sources: JSON.parse(backend.sourcesJson || "[]")
    readonly property var mounts: JSON.parse(backend.mountsJson || "[]")

    // Flattens sources+mounts into section-header/row entries for a single
    // ListView: one "source" row per source, that source's "mount" rows,
    // then an "addMount" affordance row to add another mount to it. Mounts
    // whose source was deleted out from under them (an edge case, not a
    // normal flow) still get a group of their own rather than silently
    // vanishing from view.
    function buildListModel() {
        let rows = [];
        for (const src of root.sources) {
            rows.push({ rowType: "source", source: src, mount: null });
            for (const m of root.mounts.filter(mm => mm.source === src.name))
                rows.push({ rowType: "mount", source: null, mount: m });
            rows.push({ rowType: "addMount", source: src, mount: null });
        }
        const knownSourceNames = new Set(root.sources.map(s => s.name));
        const orphaned = root.mounts.filter(m => !knownSourceNames.has(m.source));
        if (orphaned.length > 0) {
            rows.push({ rowType: "source", source: null, mount: null });
            for (const m of orphaned)
                rows.push({ rowType: "mount", source: null, mount: m });
        }
        return rows;
    }

    title: i18n("Mounts")

    // Rendered by the host (System Settings / kcmshell6) in the page's own
    // title bar, matching how every other KCM ("Online Accounts", etc.)
    // puts its primary action next to the title. Adding a mount is always
    // done from a specific source's row (below), not globally here, since
    // a mount always belongs to exactly one source.
    actions: [
        Kirigami.Action {
            icon.name: "list-add-symbolic"
            text: i18n("Add source…")
            onTriggered: root.addSourceRequested()
        },
        Kirigami.Action {
            icon.name: "settings-configure-symbolic"
            text: i18n("Credentials…")
            visible: root.backend.systemScope
            onTriggered: root.openCredentialsRequested()
        }
    ]

    ListView {
        id: listView
        model: root.buildListModel()

        delegate: Loader {
            id: rowLoader
            required property var modelData
            width: listView.width
            sourceComponent: {
                switch (rowLoader.modelData.rowType) {
                case "source": return sourceHeaderComponent;
                case "addMount": return addMountComponent;
                default: return mountItemComponent;
                }
            }

            Component {
                id: sourceHeaderComponent
                SourceSectionHeader {
                    source: rowLoader.modelData.source
                    helpers: root.helpers
                    onEditRequested: root.editSourceRequested(source)
                    onRemoveRequested: root.backend.removeSource(source.name)
                }
            }
            Component {
                id: mountItemComponent
                MountListItem {
                    mount: rowLoader.modelData.mount
                    helpers: root.helpers
                    sources: root.sources
                    onStartRequested: root.backend.startMount(mount.name)
                    onStopRequested: root.backend.stopMount(mount.name)
                    onEditRequested: root.editMountRequested(mount)
                    onRemoveRequested: root.backend.removeMount(mount.name)
                }
            }
            Component {
                id: addMountComponent
                QQC2.ItemDelegate {
                    // No local `modelData` property — unlike `rowLoader`
                    // (the actual ListView delegate root), objects created
                    // via Loader.sourceComponent don't get one supplied
                    // automatically, so declaring it `required` here made
                    // this component fail to instantiate every time (no row
                    // ever appeared, silently).
                    width: rowLoader.width
                    text: i18n("Add mount…")
                    icon.name: "list-add-symbolic"
                    onClicked: root.addMountRequested(rowLoader.modelData.source)

                    // Indented under its source, like MountListItem's rows.
                    leftPadding: Kirigami.Units.gridUnit + Kirigami.Units.smallSpacing
                }
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: root.backend.busy && root.sources.length === 0
            icon.name: "view-refresh-symbolic"
            text: i18n("Loading…")

            QQC2.BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: true
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: !root.backend.busy && root.sources.length === 0
            icon.name: "folder-cloud-symbolic"
            text: i18n("No sources yet")
            explanation: i18n("A source is an rclone remote (an SMB share, WebDAV server, …) that mounts point at. Add one to get started.")

            helpfulAction: Kirigami.Action {
                icon.name: "list-add-symbolic"
                text: i18n("Add source…")
                onTriggered: root.addSourceRequested()
            }
        }
    }
}
