// SPDX-License-Identifier: GPL-2.0-or-later

import QtQuick
import org.kde.kirigami as Kirigami

// Shared pure display logic + static config tables, instantiated once in
// Main.qml and passed down explicitly (as a plain property, not a
// singleton) to whichever page/delegate needs it. Kept stateless aside from
// `sourceKinds`, which Main.qml binds once to the backend's schema JSON.
QtObject {
    id: helpers

    // Parsed view of the controller's kind-schema JSON (see backend_controller.rs's
    // `kind_schemas_json`). Bound once from Main.qml; constant for the process
    // lifetime in practice, since the backend only sets it at construction.
    property var sourceKinds: []

    function kindSchema(tag) {
        for (let i = 0; i < helpers.sourceKinds.length; i++)
            if (helpers.sourceKinds[i].tag === tag) return helpers.sourceKinds[i];
        return null;
    }
    function kindLabel(tag) {
        let k = helpers.kindSchema(tag);
        return k ? k.label : tag;
    }
    function kindIcon(tag) {
        let k = helpers.kindSchema(tag);
        return k ? k.icon : "folder-cloud-symbolic";
    }
    function kindIsWizardOnly(tag) {
        let k = helpers.kindSchema(tag);
        return k ? !!k.wizard_only : false;
    }

    // iCloud's trust token is valid ~30 days; warn a few days before it's
    // likely to lapse. `_trust_token_stamped_at` is Unix seconds, stamped by
    // the wizard when the sign-in completes.
    function needsReconnectSoon(source) {
        if (!source || source.kind !== "iclouddrive") return false;
        let stampedAt = (source.options || {})._trust_token_stamped_at;
        if (!stampedAt) return false;
        let ageDays = (Date.now() / 1000 - Number(stampedAt)) / 86400;
        return ageDays > 25;
    }

    // Map a source id to its display name for the mount list.
    function sourceDisplay(sources, id) {
        for (let i = 0; i < sources.length; i++)
            if (sources[i].name === id) return sources[i].display_name;
        return id;
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

    // rclone VFS cache modes, in increasing-cache order. Value is the wire
    // token that MountOptions.cache_mode deserializes from.
    readonly property var cacheModes: [
        { value: "off", label: i18n("Off (direct, no cache)") },
        { value: "minimal", label: i18n("Minimal") },
        { value: "writes", label: i18n("Writes") },
        { value: "full", label: i18n("Full") }
    ]
    // Defaults shown for a brand-new mount, matching MountOptions::default() so
    // a mount created without touching the tuning fields behaves as before.
    // umask is the decimal form of 0o077 (== 63, "Private"), the shape
    // MountOptions wants.
    readonly property var defaultMountOptions: ({
        cache_mode: "writes",
        cache_max_size_mb: 2048,
        dir_cache_time_secs: 30,
        umask: 63,
        read_only: false
    })

    function cacheModeIndex(value) {
        for (let i = 0; i < helpers.cacheModes.length; i++)
            if (helpers.cacheModes[i].value === value) return i;
        return 2; // "writes"
    }
}
