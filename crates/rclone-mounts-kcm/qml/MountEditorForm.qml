// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Just the fields for adding/editing a mount, hosted by MountEditorPage (the
// right-hand pane for both flows — mounts never go through a dialog). There
// is no Save/Cancel here: every field commits straight into the pending
// changeset as it's edited (see commitLive() below), and the KCM's own
// Apply/Cancel/Defaults — not anything in this pane — is what actually
// writes it to disk or discards it.
ColumnLayout {
    id: root

    required property var helpers
    required property var backend
    // Used only to resolve the source id (below) to a display name — mounts
    // are always created for a specific source now (see MainListPage's
    // per-source "Add mount" row), so there's no source picker here.
    required property var sources
    // null → create; otherwise editing an existing row (name is the key).
    property var editing: null
    // The source a new mount is being created for. Ignored when `editing`
    // is set (an existing mount keeps its own source).
    property var presetSource: null

    // The id this pane's edits are staged under. Empty until the first
    // valid commit for a new mount — after that, every subsequent commit
    // (including a mount added under this same pane visit) reuses it, so
    // renaming doesn't fork off a second pending mount with a freshly
    // re-derived slug (backend.upsertMount only derives a fresh id from
    // display_name when `id` is empty).
    property string committedId: ""

    readonly property string sourceId: root.editing ? root.editing.source : (root.presetSource ? root.presetSource.name : "")
    readonly property string sourceDisplayName: root.helpers.sourceDisplay(root.sources, root.sourceId)
    readonly property var sourceObj: {
        for (let i = 0; i < root.sources.length; i++)
            if (root.sources[i].name === root.sourceId) return root.sources[i];
        return null;
    }
    readonly property string sourceKind: root.sourceObj ? root.sourceObj.kind : ""
    // WebDAV's PutStream is vendor-dependent (true only when talking to
    // another rclone instance), not a fixed fact of the "webdav" kind, so
    // the schema's put_stream is a fallback and this reads the source's
    // actual vendor choice instead — see backend_features::webdav_put_stream.
    readonly property bool sourceCanStream: {
        if (root.sourceKind === "webdav")
            return (root.sourceObj && root.sourceObj.options && root.sourceObj.options.vendor) === "rclone";
        let schema = root.helpers.kindSchema(root.sourceKind);
        return schema ? !!schema.put_stream : true;
    }
    // Mirrors rclone's own check (vfs/vfs.go): it warns when the mount is
    // writable, the cache mode is below Writes, and the backend lacks
    // PutStream (can't stream an upload without buffering it first) — see
    // the "can't stream" log line at mount time.
    readonly property bool cacheTooLowForSource: !readOnlyBox.checked
        && !root.sourceCanStream
        && (cacheModeBox.currentValue === "off" || cacheModeBox.currentValue === "minimal")

    // Common, human-meaningful umask presets — the octal form isn't
    // something most people can reason about at a glance. -1 is a sentinel
    // for "Custom…", which reveals customUmaskField below instead of
    // mapping to a fixed value.
    readonly property var umaskPresets: [
        { value: 63, label: i18n("Private") },
        { value: 18, label: i18n("Anyone can read") },
        { value: 0, label: i18n("Anyone can edit") },
        { value: -1, label: i18n("Custom…") }
    ]
    readonly property bool customUmaskSelected: umaskBox.currentValue === -1
    readonly property bool customUmaskValid: /^[0-7]{1,4}$/.test(customUmaskField.text)

    readonly property bool acceptable: nameField.text.trim().length > 0 && (!customUmaskSelected || customUmaskValid)

    function reset() {
        root.committedId = root.editing ? root.editing.name : "";
        nameField.text = root.editing ? root.editing.display_name : "";
        subpathField.text = root.editing ? (root.editing.subpath || "") : "";
        mountpointField.text = root.editing ? root.editing.mountpoint : "";
        enabledBox.checked = root.editing ? root.editing.enabled : false;
        // Tuning options: an existing mount carries its own; a new one gets
        // the defaults (so the fields aren't blank and behavior is unchanged).
        let o = (root.editing && root.editing.options) ? root.editing.options : root.helpers.defaultMountOptions;
        cacheModeBox.currentIndex = root.helpers.cacheModeIndex(o.cache_mode);
        readOnlyBox.checked = !!o.read_only;
        cacheSizeSlider.value = o.cache_max_size_mb ?? 0;
        dirCacheSlider.value = o.dir_cache_time_secs ?? 0;
        let umaskIdx = root.umaskPresets.findIndex(p => p.value === o.umask);
        if (umaskIdx >= 0) {
            umaskBox.currentIndex = umaskIdx;
            customUmaskField.text = "";
        } else {
            umaskBox.currentIndex = root.umaskPresets.length - 1; // "Custom…"
            customUmaskField.text = (o.umask ?? 0).toString(8).padStart(3, "0");
        }
    }

    // Users very plausibly paste a subfolder path straight out of a
    // browser's address bar (e.g. a Nextcloud/ownCloud "Spaces" URL), which
    // comes URL-encoded (`%24` for `$`, etc). rclone does its own encoding
    // when it builds the WebDAV request, so a pre-encoded path gets encoded
    // *again* (`%24` → `%2524`) and the server 404s. Decode defensively; if
    // it's not validly encoded, a literal `%` was probably intended, so
    // fall back to the raw text rather than fail the whole save.
    function decodedSubpath(raw) {
        try {
            return decodeURIComponent(raw);
        } catch (e) {
            return raw;
        }
    }

    // Collects the current field values into the shape `backend.upsertMount`
    // expects.
    function collect() {
        let opts = {
            cache_mode: cacheModeBox.currentValue,
            read_only: readOnlyBox.checked,
            cache_max_size_mb: cacheSizeSlider.value === 0 ? null : cacheSizeSlider.value,
            dir_cache_time_secs: dirCacheSlider.value === 0 ? null : dirCacheSlider.value,
            umask: root.customUmaskSelected ? parseInt(customUmaskField.text, 8) : umaskBox.currentValue
        };
        return {
            id: root.committedId,
            displayName: nameField.text.trim(),
            source: root.sourceId,
            subpath: root.decodedSubpath(subpathField.text.trim().replace(/^\/+/, "")),
            mountpoint: mountpointField.text.trim(),
            optionsJson: JSON.stringify(opts),
            enabled: enabledBox.checked
        };
    }

    // Stages the current field values. A no-op while required fields aren't
    // filled in yet, so opening "Add mount" and clicking away without typing
    // anything never stages a blank draft.
    function commitLive() {
        if (!root.acceptable)
            return;
        let v = root.collect();
        let resolvedId = root.backend.upsertMount(v.id, v.displayName, v.source, v.subpath, v.mountpoint, v.optionsJson, v.enabled);
        if (resolvedId.length > 0)
            root.committedId = resolvedId;
    }

    Component.onCompleted: root.reset()

    Kirigami.FormLayout {
        Layout.fillWidth: true

        QQC2.TextField {
            id: nameField
            Kirigami.FormData.label: i18n("Name:")
            placeholderText: i18n("e.g. Work files")
            onEditingFinished: root.commitLive()
        }
        QQC2.Label {
            Kirigami.FormData.label: i18n("Source:")
            text: root.sourceDisplayName
            opacity: 0.7
        }
        RowLayout {
            Kirigami.FormData.label: i18n("Subfolder:")
            QQC2.TextField {
                id: subpathField
                Layout.fillWidth: true
                placeholderText: i18n("optional — leave blank to mount the whole source")
                onEditingFinished: root.commitLive()
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("Mount just a folder within the source instead of its root — e.g. “Documents” or “Photos/2026”. Works the same for every source type.")
            }
        }
        QQC2.TextField {
            id: mountpointField
            Kirigami.FormData.label: i18n("Mount point:")
            placeholderText: i18n("e.g. ~/Mounts/work")
            onEditingFinished: root.commitLive()
        }
        QQC2.CheckBox {
            id: enabledBox
            Kirigami.FormData.label: i18n("Mount automatically:")
            text: i18n("Mount when you log in")
            onToggled: root.commitLive()
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: i18n("Performance")
        }

        RowLayout {
            Kirigami.FormData.label: i18n("Cache mode:")
            QQC2.ComboBox {
                id: cacheModeBox
                Layout.fillWidth: true
                model: root.helpers.cacheModes
                textRole: "label"
                valueRole: "value"
                onActivated: root.commitLive()
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("Off never caches file data locally. Minimal caches only what's needed for reads. Writes also caches data being written. Full keeps a persistent local copy of everything you access.")
            }
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Kirigami.FormData.isSection: true
            type: Kirigami.MessageType.Warning
            visible: root.cacheTooLowForSource
            text: i18n("This source doesn't support streaming uploads — file access may be slow with this cache mode. Writes or Full is usually a better fit.")
        }
        QQC2.CheckBox {
            id: readOnlyBox
            Layout.fillWidth: true
            Kirigami.FormData.label: i18n("Read-only:")
            text: i18n("Mount read-only")
            onToggled: root.commitLive()
        }
        // Cache size/time only mean anything once caching is on at all.
        RowLayout {
            Kirigami.FormData.label: i18n("Max cache size:")
            visible: cacheModeBox.currentValue !== "off"
            Layout.fillWidth: true
            QQC2.Slider {
                id: cacheSizeSlider
                Layout.fillWidth: true
                from: 0
                to: 20480
                stepSize: 256
                snapMode: QQC2.Slider.SnapAlways
                onMoved: root.commitLive()
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                text: cacheSizeSlider.value === 0 ? i18n("Unlimited") : i18n("%1 MB", cacheSizeSlider.value)
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("The most local disk space the cache is allowed to use. Unlimited lets it grow as large as needed.")
            }
        }
        RowLayout {
            Kirigami.FormData.label: i18n("Directory cache:")
            visible: cacheModeBox.currentValue !== "off"
            Layout.fillWidth: true
            QQC2.Slider {
                id: dirCacheSlider
                Layout.fillWidth: true
                from: 0
                to: 3600
                stepSize: 30
                snapMode: QQC2.Slider.SnapAlways
                onMoved: root.commitLive()
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                text: dirCacheSlider.value === 0 ? i18n("rclone default") : i18n("%1 s", dirCacheSlider.value)
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("How long a listing of a folder's contents is trusted before rclone re-checks the source. Longer means fewer requests but slower to notice changes made elsewhere.")
            }
        }
        QQC2.ComboBox {
            id: umaskBox
            Kirigami.FormData.label: i18n("File permissions:")
            Layout.fillWidth: true
            model: root.umaskPresets
            textRole: "label"
            valueRole: "value"
            onActivated: root.commitLive()
        }
        QQC2.TextField {
            id: customUmaskField
            Kirigami.FormData.label: i18n("Custom umask (octal) *:")
            Layout.fillWidth: true
            visible: root.customUmaskSelected
            placeholderText: "022"
            inputMethodHints: Qt.ImhDigitsOnly
            validator: RegularExpressionValidator { regularExpression: /[0-7]{0,4}/ }
            onEditingFinished: root.commitLive()
        }
    }
}
