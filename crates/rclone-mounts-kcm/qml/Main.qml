// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigami.layouts as KirigamiLayouts
import org.kde.kcmutils as KCM
import dev.jthoward.RcloneMounts

// Root: wires the backend, the scope switcher, and the local PageRow that
// hosts every page. The list stays pinned as the first column and an editor
// pane (source edit, mount add/edit, credentials) is pushed as the second —
// see the `columnView.columnResizeMode` override below for why that second
// column never collapses the list out of view. `AbstractKCM` (not
// `SimpleKCM`) because PageRow's pages manage their own scrolling; a second,
// outer ScrollablePage would just fight it. Adding a source is the one
// exception to the pushed-pane pattern: it's always the `SourceAddWizardDialog`
// below, since that flow is inherently interactive (kind/name, then either
// flat fields or an OAuth/2FA handoff to `SourceWizard`).
//
// Nothing here forwards actions to the host's own title bar — `AbstractKCM.actions`
// is left at its default (empty). Editing a source/mount has no Save/Cancel
// of its own either: SourceEditorForm/MountEditorForm stage every field
// edit straight into the pending changeset live (see their commitLive()),
// so the KCM's own built-in Apply/Cancel/Defaults is the only commit/discard
// control anywhere. MainListPage and CredentialsPage do still carry their
// own in-content footers, but only for things that aren't a save/cancel
// gesture (Add source…, Credentials…, Close).
//
// Note on ids below: none of them are named after the properties they get
// bound to elsewhere (e.g. the BackendController's id is `backendController`,
// not `backend`) — components like MainListPage declare `required property
// var backend`, and writing `backend: backend` inside such an instantiation
// would resolve the RHS to the new object's own (unset) property rather than
// this outer id, silently breaking every binding that depends on it.
KCM.AbstractKCM {
    id: root

    // No "Use Defaults" button: there's no hardcoded default changeset to
    // restore to (only the on-disk applied state and whatever's pending),
    // so a Defaults button here would have nothing meaningful to do. No
    // Help button either: there's no X-DocPath/handbook behind it.
    KCM.ConfigModule.buttons: KCM.ConfigModule.Apply

    // AbstractKCM's default `framedView: true` reserves a 6px margin around
    // the whole content, expecting an inner scrollview to draw its own frame
    // there instead. Nothing here does that, so it just reads as a stray
    // border around the KCM's edge — disable it to let the tab bar/PageRow
    // reach the host window's edges directly.
    framedView: false

    // Best-effort floor on the host window's width: with the list column
    // pinned via `columnView.columnResizeMode` below, the pane pushed next
    // to it (source/mount editor, credentials) has nowhere to go if the
    // window shrinks past both columns' combined width other than getting
    // clipped. Hinting a minimum here asks the host (System Settings /
    // kcmshell6) not to let that happen instead. Scales with `depth` so a
    // list-only view (nothing pushed yet) doesn't force a wide window before
    // there's a second column to make room for.
    Layout.minimumWidth: pageRowItem.defaultColumnWidth * Math.max(pageRowItem.depth, 1)

    BackendController {
        id: backendController
    }

    // Shared pure display logic + static config tables (kind lookups, status
    // mapping, cache-mode table, …), passed down explicitly to whichever
    // page/delegate needs it.
    Helpers {
        id: uiHelpers
        sourceKinds: JSON.parse(backendController.kindSchemasJson || "[]")
    }

    // SourceWizard.qml predates this file's split and refers to `backend`
    // unqualified (it has no `backend` property of its own to shadow it
    // with), so this alias is what makes those references still resolve —
    // everything added since qualifies through `backendController`/`uiHelpers`
    // directly instead of relying on this.
    property alias backend: backendController

    // Set when a scope switch (My mounts ↔ System mounts) was refused because
    // there are unsaved edits; clears once the user Applies or Resets.
    property bool scopeSwitchBlocked: false

    function trySwitchScope(system) {
        if (system === backendController.systemScope)
            return;
        // Switching scope reloads and drops the pending changeset, so refuse
        // while dirty: leave the tab bar's `checked` bindings as they are
        // (they mirror backendController.systemScope directly, so there's
        // nothing to manually revert) and prompt the user to Apply or Reset
        // first.
        if (backendController.dirty) {
            root.scopeSwitchBlocked = true;
            return;
        }
        root.scopeSwitchBlocked = false;
        // The pushed editor/credentials pane (if any) refers to state from
        // the scope being left — pop it back to the list so switching scopes
        // doesn't leave a stale right panel open.
        if (pageRowItem.depth > 1)
            pageRowItem.pop(mainListPage);
        backendController.setScope(system);
    }

    // Editing an existing source always pushes SourceEditorPage as the
    // second PageRow column (see the `columnView.columnResizeMode` override
    // below for why that never hides the list); adding a new source is a
    // fully separate flow, always the wizard dialog. Pages are pushed by URL
    // (not by an inline `Component { ... }` block) so required properties
    // are set the ordinary way via push()'s properties argument — the same
    // mechanism PageRow's own push()/getPageComponent() uses for
    // string/url pages.
    function openSourceEditor(source) {
        if (!source) {
            sourceAddWizard.openFor();
            return;
        }
        // `currentItem` tracks column focus, not "the page you just pushed"
        // — PageRow can show the list and a detail page side by side, and
        // currentItem stayed on the list unless focus was moved explicitly.
        // `lastItem` is the rightmost/most-recently-pushed page, which is
        // what "is this source's editor already open?" actually means.
        let cur = pageRowItem.lastItem;
        if (cur && cur.pageKind === "sourceEditor" && cur.editing && cur.editing.name === source.name)
            return;
        let props = { backend: backendController, helpers: uiHelpers, pageRow: pageRowItem, editing: source };
        // Switching from editing one source to another should swap out the
        // detail page, not stack a second one next to it. `replace()` acts
        // on `currentIndex` (column focus), which the list column can hold
        // even while an editor is open — using it here would pop the list
        // itself off the stack instead of the editor. Pop back to the list
        // explicitly first so the target is unambiguous.
        if (pageRowItem.depth > 1)
            pageRowItem.pop(mainListPage);
        let page = pageRowItem.push(Qt.resolvedUrl("SourceEditorPage.qml"), props);
        page.wizardHandoff.connect((kind, editing) => sourceWizard.openFor(kind, editing));
    }

    function openMountEditor(mount, presetSource) {
        let cur = pageRowItem.lastItem;
        if (mount && cur && cur.pageKind === "mountEditor" && cur.editing && cur.editing.name === mount.name)
            return;
        let props = {
            backend: backendController,
            helpers: uiHelpers,
            pageRow: pageRowItem,
            editing: mount,
            sources: JSON.parse(backendController.sourcesJson || "[]"),
            presetSource: presetSource
        };
        if (pageRowItem.depth > 1)
            pageRowItem.pop(mainListPage);
        pageRowItem.push(Qt.resolvedUrl("MountEditorPage.qml"), props);
    }

    // Poll live mount status whenever there are applied mounts on screen, in
    // either scope. The query runs on a worker thread (see refreshStatus); in
    // system scope it's a non-interactive helper call, so it never prompts.
    Timer {
        interval: 3000
        repeat: true
        running: JSON.parse(backendController.mountsJson || "[]").length > 0
        onTriggered: backendController.refreshStatus()
    }

    // The C++ shim emits these in response to the KCM's Apply/Reset/load.
    // The framework has no separate "Reset was clicked" signal — the KCM's
    // load() override fires loadRequested both right after construction and
    // on every Reset/Cancel click — so `backendController.loaded` (true once
    // the first load has actually hydrated state) is what tells the two
    // apart: not yet loaded means this is the construction-time call.
    Connections {
        target: kcm
        function onLoadRequested() {
            if (backendController.loaded) {
                // Reset click: just drop the pending changeset back to the
                // already-applied state, without re-reading from disk.
                backendController.reset();
            } else {
                backendController.load();
            }
        }
        function onSaveRequested() { backendController.commit() }
    }

    // Dirty state drives the framework's Apply/Reset button enablement.
    Binding {
        target: kcm
        property: "needsSave"
        value: backendController.dirty
    }

    // No Component.onCompleted load: the KCM framework calls the C++ load()
    // after construction, which emits loadRequested → backendController.load()
    // above.

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: backendController.errorString
            visible: backendController.errorString.length > 0
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: i18n("Apply or discard your changes before switching between your mounts and system mounts.")
            visible: root.scopeSwitchBlocked && backendController.dirty
        }

        // No helper installed/activatable → system mounts aren't an option at
        // all; don't offer a tab that would just error out. Falls back to
        // showing only the user's own mounts.
        Kirigami.NavigationTabBar {
            Layout.fillWidth: true
            visible: backendController.systemScopeAvailable
            // Lock scope switching while a load/commit is in flight so a
            // second click can't spawn an overlapping load (or a second
            // Polkit prompt) before the first finishes.
            enabled: !backendController.busy
            actions: [
                Kirigami.Action {
                    text: i18n("My mounts")
                    icon.name: "user-symbolic"
                    checked: !backendController.systemScope
                    onTriggered: root.trySwitchScope(false)
                },
                Kirigami.Action {
                    text: i18n("System mounts")
                    icon.name: "system-users-symbolic"
                    checked: backendController.systemScope
                    onTriggered: root.trySwitchScope(true)
                }
            ]
        }

        Kirigami.PageRow {
            id: pageRowItem
            Layout.fillWidth: true
            Layout.fillHeight: true
            // Prevent poking at another mount/source while a commit from a
            // prior Apply is still running in the background (see the busy
            // overlay below, which is what actually blocks input here).
            enabled: !backendController.busy
            // A plain sibling instance, not a Component — this is the
            // pattern Kirigami's own PageRow docs use for `initialPage`
            // (a Page declared and referenced by id). Wrapping it in a
            // Component instead (as pushed pages are) produced a "Created
            // graphical object was not placed in the graphics scene"
            // warning and a blank page.
            initialPage: mainListPage

            // PageRow normally drops to SingleColumn mode (hiding earlier
            // columns behind whatever's on top) once its width falls below
            // `defaultColumnWidth * 2` — see PageRow.qml's `wideMode`/
            // `columnResizeMode` binding. The list must stay visible at all
            // times, including in a narrow window, so pin this instead:
            // the columns become horizontally scrollable rather than
            // collapsing.
            columnView.columnResizeMode: KirigamiLayouts.ColumnView.FixedColumns
        }
    }

    MainListPage {
        id: mainListPage
        backend: backendController
        helpers: uiHelpers
        onAddSourceRequested: root.openSourceEditor(null)
        onEditSourceRequested: source => root.openSourceEditor(source)
        onAddMountRequested: source => root.openMountEditor(null, source)
        onEditMountRequested: mount => root.openMountEditor(mount, null)
        onOpenCredentialsRequested: pageRowItem.push(Qt.resolvedUrl("CredentialsPage.qml"), { backend: backendController, pageRow: pageRowItem })
    }

    // --- Add-source wizard dialog (the only way a new source is created) --
    SourceAddWizardDialog {
        id: sourceAddWizard
        backend: backendController
        helpers: uiHelpers
        onWizardHandoff: (kind, editing, name) => sourceWizard.openFor(kind, editing, name)
    }

    // --- Interactive source wizard (Google Drive OAuth, iCloud 2FA) -------
    SourceWizard {
        id: sourceWizard
    }

    // Apply/OK gives no feedback of its own — `save()` (cpp/kcm_rclone_mounts.cpp)
    // returns as soon as it emits saveRequested, before backendController.commit()'s
    // background thread actually finishes, so the host considers the KCM done
    // saving while it's still writing/reloading. This overlay is what makes
    // that in-flight work visible and, via its MouseArea eating every click,
    // is what actually stops you from opening another mount/source mid-save
    // (pageRowItem.enabled above is just belt-and-braces for keyboard focus).
    Item {
        anchors.fill: parent
        visible: backendController.busy
        z: 1000

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.AllButtons
        }

        Rectangle {
            anchors.fill: parent
            color: Kirigami.Theme.backgroundColor
            opacity: 0.6
        }

        ColumnLayout {
            anchors.centerIn: parent
            QQC2.BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: backendController.busy
            }
            QQC2.Label {
                Layout.alignment: Qt.AlignHCenter
                text: i18n("Please wait…")
            }
        }
    }
}
