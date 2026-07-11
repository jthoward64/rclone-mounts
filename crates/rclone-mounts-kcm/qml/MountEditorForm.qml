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
    Layout.fillWidth: true

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
            if (root.sources[i].name === root.sourceId)
                return root.sources[i];
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
    readonly property bool cacheTooLowForSource: !readOnlyBox.checked && !root.sourceCanStream && (cacheModeBox.currentValue === "off" || cacheModeBox.currentValue === "minimal")
    // Whether this source's kind notices remote changes on its own via
    // rclone's --poll-interval (Drive today; see KindSchema::supports_polling
    // for the rest of the list rclone itself supports). Gates both the
    // longer dir-cache default below and the poll-interval controls further
    // down — meaningless, so hidden, on a kind that can't use them.
    readonly property bool sourceSupportsPolling: root.helpers.kindSupportsPolling(root.sourceKind)

    // Common, human-meaningful umask presets — the octal form isn't
    // something most people can reason about at a glance. -1 is a sentinel
    // for "Custom…", which reveals customUmaskField below instead of
    // mapping to a fixed value.
    readonly property var umaskPresets: [
        {
            value: 63,
            label: i18n("Private")
        },
        {
            value: 18,
            label: i18n("Anyone can read")
        },
        {
            value: 0,
            label: i18n("Anyone can edit")
        },
        {
            value: -1,
            label: i18n("Custom…")
        }
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
        vfsRefreshBox.checked = !!o.vfs_refresh;
        cacheSizeSlider.value = o.cache_max_size_mb ?? 0;
        // A brand-new mount of a polling-capable kind starts with a long
        // dir-cache time (the backend notices changes itself via polling,
        // so there's no staleness cost); anything else — a new mount of any
        // other kind, or an existing mount's own stored value — keeps
        // whatever `o.dir_cache_time_secs` already says (null means "rclone
        // default" for a plain new mount).
        let defaultDirCacheSecs = (!root.editing && root.sourceSupportsPolling) ? root.helpers.dirCachePollingDefaultSecs : (o.dir_cache_time_secs ?? null);
        dirCacheSlider.value = root.helpers.durationIndexFor(root.helpers.dirCacheSteps, defaultDirCacheSecs);
        let pollSecs = o.poll_interval_secs ?? null;
        pollEnabledBox.checked = pollSecs !== 0;
        pollIntervalSlider.value = root.helpers.durationIndexFor(root.helpers.pollIntervalSteps, pollSecs === 0 ? null : pollSecs);
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
            vfs_refresh: vfsRefreshBox.checked,
            cache_max_size_mb: cacheSizeSlider.value === 0 ? null : cacheSizeSlider.value,
            dir_cache_time_secs: root.helpers.dirCacheSteps[dirCacheSlider.value].seconds,
            // Not applicable at all on a non-polling source — leave the
            // stored value alone (null == "no opinion", same as never having
            // set it) rather than writing a 0 that would just be confusing
            // if the source's kind ever changed.
            poll_interval_secs: root.sourceSupportsPolling ? (pollEnabledBox.checked ? root.helpers.pollIntervalSteps[pollIntervalSlider.value].seconds : 0) : null,
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
                to: root.helpers.dirCacheSteps.length - 1
                stepSize: 1
                snapMode: QQC2.Slider.SnapAlways
                onMoved: root.commitLive()
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                text: root.helpers.dirCacheSteps[dirCacheSlider.value].label
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("How long a listing of a folder's contents is trusted before rclone re-checks the source. Longer means fewer requests but slower to notice changes made elsewhere. A source that supports polling for changes (below) can safely use a much longer value.")
            }
        }
        RowLayout {
            Kirigami.FormData.label: i18n("Poll for changes:")
            visible: cacheModeBox.currentValue !== "off" && root.sourceSupportsPolling
            Layout.fillWidth: true
            QQC2.Switch {
                id: pollEnabledBox
                onToggled: root.commitLive()
                Layout.fillWidth: true
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("This source can tell rclone about changes made elsewhere as they happen, instead of waiting for the directory cache above to expire. Turning this off relies on the directory cache alone — lower it if you do.")
            }
        }
        RowLayout {
            Kirigami.FormData.label: i18n("Poll interval:")
            visible: cacheModeBox.currentValue !== "off" && root.sourceSupportsPolling && pollEnabledBox.checked
            Layout.fillWidth: true
            QQC2.Slider {
                id: pollIntervalSlider
                Layout.fillWidth: true
                from: 0
                to: root.helpers.pollIntervalSteps.length - 1
                stepSize: 1
                snapMode: QQC2.Slider.SnapAlways
                onMoved: root.commitLive()
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                text: root.helpers.pollIntervalSteps[pollIntervalSlider.value].label
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("How often rclone checks this source for changes. Must stay well under the directory cache time above — the default is a safe choice for nearly every value on that slider.")
            }
        }
        RowLayout {
            Kirigami.FormData.label: i18n("On mount start:")
            visible: cacheModeBox.currentValue !== "off"
            ColumnLayout {
                Layout.fillWidth: true
                QQC2.CheckBox {
                    id: vfsRefreshBox
                    Layout.fillWidth: true
                    text: i18n("Refresh the directory cache in the background")
                    onToggled: root.commitLive()
                }
                QQC2.Label {
                    Kirigami.FormData.isSection: true
                    visible: cacheModeBox.currentValue !== "off"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    opacity: 0.7
                    text: i18n("Turning this off sends fewer requests to the server and can reduce RAM use, but folders may freeze or load slowly the first time you browse into them.")
                }
            }
            Kirigami.ContextualHelpButton {
                toolTipText: i18n("Walks the whole remote right when the mount starts so folders show up instantly once you browse in — otherwise the first listing of each folder is fetched on demand. Best left off for very large remotes, since it means a burst of requests at every mount start.")
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
            validator: RegularExpressionValidator {
                regularExpression: /[0-7]{0,4}/
            }
            onEditingFinished: root.commitLive()
        }
    }
}
