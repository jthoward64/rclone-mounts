// SPDX-License-Identifier: GPL-2.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM
import dev.jthoward.RcloneMounts

// Root: wires the backend, the scope switcher, and the local PageRow that
// hosts every page (the unified list, Credentials, and — when
// `editorPresentation === "page"` — the source/mount editors). `AbstractKCM`
// (not `SimpleKCM`) because PageRow's pages manage their own scrolling; a
// second, outer ScrollablePage would just fight it. See the design notes in
// MainListPage.qml and Source/MountEditorPage.qml for how pages get pushed.
//
// Note on ids below: none of them are named after the properties they get
// bound to elsewhere (e.g. the BackendController's id is `backendController`,
// not `backend`) — components like MainListPage declare `required property
// var backend`, and writing `backend: backend` inside such an instantiation
// would resolve the RHS to the new object's own (unset) property rather than
// this outer id, silently breaking every binding that depends on it.
KCM.AbstractKCM {
    id: root

    // Flip this to "dialog" to feel out Kirigami.Dialog instead of a pushed
    // page for adding/editing sources and mounts — everything else about the
    // editors (SourceEditorForm/MountEditorForm) is shared between both.
    readonly property string editorPresentation: "page"

    // The host (System Settings / kcmshell6) renders title-bar actions from
    // *this* root item, not from whichever page is currently active inside
    // our nested PageRow — so the currently-visible inner page's actions
    // have to be forwarded up here, or they simply never appear anywhere.
    actions: pageRowItem.currentItem ? pageRowItem.currentItem.actions : []

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
        backendController.setScope(system);
    }

    // Pages pushed by URL (not by an inline `Component { ... }` block) so
    // required properties are set the ordinary way via push()'s properties
    // argument — the same mechanism PageRow's own push()/getPageComponent()
    // uses for string/url pages.
    function openSourceEditor(source) {
        // `currentItem` tracks column focus, not "the page you just pushed"
        // — PageRow can show the list and a detail page side by side, and
        // currentItem stayed on the list unless focus was moved explicitly.
        // `lastItem` is the rightmost/most-recently-pushed page, which is
        // what "is this source's editor already open?" actually means.
        let cur = pageRowItem.lastItem;
        if (source && cur && cur.pageKind === "sourceEditor" && cur.editing && cur.editing.name === source.name)
            return;
        if (root.editorPresentation === "dialog") {
            sourceEditorDialog.openFor(source);
        } else {
            let props = { backend: backendController, helpers: uiHelpers, pageRow: pageRowItem, editing: source };
            // Switching from editing one source to another should swap out
            // the detail page, not stack a second one next to it.
            let page = pageRowItem.depth > 1
                ? pageRowItem.replace(Qt.resolvedUrl("SourceEditorPage.qml"), props)
                : pageRowItem.push(Qt.resolvedUrl("SourceEditorPage.qml"), props);
            page.wizardHandoff.connect((kind, editing) => sourceWizard.openFor(kind, editing));
        }
    }

    function openMountEditor(mount, presetSource) {
        let cur = pageRowItem.lastItem;
        if (mount && cur && cur.pageKind === "mountEditor" && cur.editing && cur.editing.name === mount.name)
            return;
        if (root.editorPresentation === "dialog") {
            mountEditorDialog.sources = JSON.parse(backendController.sourcesJson || "[]");
            mountEditorDialog.openFor(mount, presetSource);
        } else {
            let props = {
                backend: backendController,
                helpers: uiHelpers,
                pageRow: pageRowItem,
                editing: mount,
                sources: JSON.parse(backendController.sourcesJson || "[]"),
                presetSource: presetSource
            };
            if (pageRowItem.depth > 1)
                pageRowItem.replace(Qt.resolvedUrl("MountEditorPage.qml"), props);
            else
                pageRowItem.push(Qt.resolvedUrl("MountEditorPage.qml"), props);
        }
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
    Connections {
        target: kcm
        function onLoadRequested() { backendController.load() }
        function onSaveRequested() { backendController.commit() }
        function onDefaultsRequested() { backendController.reset() }
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
            // A plain sibling instance, not a Component — this is the
            // pattern Kirigami's own PageRow docs use for `initialPage`
            // (a Page declared and referenced by id). Wrapping it in a
            // Component instead (as pushed pages are) produced a "Created
            // graphical object was not placed in the graphics scene"
            // warning and a blank page.
            initialPage: mainListPage
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
        onOpenCredentialsRequested: pageRowItem.push(Qt.resolvedUrl("CredentialsPage.qml"), { backend: backendController })
    }

    // --- Source/mount editors: dialog variants (editorPresentation === "dialog") ---
    SourceEditorDialog {
        id: sourceEditorDialog
        backend: backendController
        helpers: uiHelpers
        onWizardHandoff: (kind, editing) => sourceWizard.openFor(kind, editing)
    }
    MountEditorDialog {
        id: mountEditorDialog
        backend: backendController
        helpers: uiHelpers
    }

    // --- Interactive source wizard (Google Drive OAuth, iCloud 2FA) -------
    SourceWizard {
        id: sourceWizard
    }
}
