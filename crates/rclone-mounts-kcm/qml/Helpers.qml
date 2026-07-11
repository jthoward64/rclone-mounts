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
    function kindSupportsPolling(tag) {
        let k = helpers.kindSchema(tag);
        return k ? !!k.supports_polling : false;
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
        case "active": return "state-ok";
        case "activating":
        case "deactivating": return "state-sync";
        case "failed": return "state-error";
        case "inactive": return "state-offline";
        case "unsaved": return "state-information";
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
    // Defaults shown for a brand-new mount, matching MountOptions::default()
    // so a mount created without touching the tuning fields behaves the same
    // way. umask is the decimal form of 0o077 (== 63, "Private"), the shape
    // MountOptions wants. dir_cache_time_secs/poll_interval_secs are left out
    // here — see MountEditorForm.reset(), which picks a kind-aware default
    // for dir-cache (long on a polling-capable source, rclone's plain 5 min
    // default otherwise) and always starts poll interval unset (rclone's own
    // 1 min default).
    readonly property var defaultMountOptions: ({
        cache_mode: "writes",
        cache_max_size_mb: 2048,
        umask: 63,
        read_only: false,
        vfs_refresh: true
    })

    function cacheModeIndex(value) {
        for (let i = 0; i < helpers.cacheModes.length; i++)
            if (helpers.cacheModes[i].value === value) return i;
        return 2; // "writes"
    }

    // Duration pickers (directory cache / poll interval) use a slider whose
    // integer position indexes into one of these tables instead of scaling
    // linearly in seconds — a linear 0-3600s slider can't usefully reach "2
    // weeks" territory, and a linear scale that could would make every
    // sub-hour value indistinguishable at typical slider widths. Index 0 is
    // always the "rclone default" sentinel (`seconds: null` — omit the flag
    // entirely and let rclone's own default apply).
    readonly property var dirCacheSteps: helpers._buildDirCacheSteps()
    readonly property var pollIntervalSteps: helpers._buildPollIntervalSteps()

    function _buildDirCacheSteps() {
        let steps = [{ seconds: null, label: i18n("rclone default (5 min)") }];
        for (const m of [1, 2, 3, 4, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55])
            steps.push({ seconds: m * 60, label: i18n("%1 min", m) });
        for (let h = 1; h <= 48; h++)
            steps.push({ seconds: h * 3600, label: i18n("%1 h", h) });
        for (let d = 3; d <= 14; d++)
            steps.push({ seconds: d * 86400, label: i18n("%1 days", d) });
        // Nominal 30-day month — this is a cache lifetime, not a calendar
        // computation, so there's no benefit to real month lengths here.
        for (let mo = 1; mo <= 12; mo++)
            steps.push({ seconds: mo * 30 * 86400, label: i18n("%1 mo", mo) });
        return steps;
    }

    // A source whose kind supports polling notices remote changes on its own
    // (see kindSupportsPolling), so it's safe to default its dir-cache time
    // to something this long — MountEditorForm.reset() picks the last (and
    // longest) entry here for a brand-new mount of such a kind.
    readonly property int dirCachePollingDefaultSecs: 14 * 86400

    function _buildPollIntervalSteps() {
        let steps = [{ seconds: null, label: i18n("rclone default (1 min)") }];
        for (const s of [10, 30])
            steps.push({ seconds: s, label: i18n("%1 s", s) });
        for (const m of [1, 2, 5, 10, 15, 30, 45])
            steps.push({ seconds: m * 60, label: i18n("%1 min", m) });
        for (const h of [1, 2, 6, 12, 24])
            steps.push({ seconds: h * 3600, label: i18n("%1 h", h) });
        return steps;
    }

    // Index of the step whose `seconds` matches `secs` exactly, or (for a
    // value that predates this table, or was saved by a future version with
    // finer steps) the closest one — so an odd stored value still lands
    // somewhere sane on the slider instead of snapping to one extreme.
    // `null`/`undefined` always means index 0, the "rclone default" sentinel.
    function durationIndexFor(steps, secs) {
        if (secs === null || secs === undefined) return 0;
        let best = 1;
        let bestDiff = Math.abs(steps[1].seconds - secs);
        for (let i = 2; i < steps.length; i++) {
            let diff = Math.abs(steps[i].seconds - secs);
            if (diff < bestDiff) {
                best = i;
                bestDiff = diff;
            }
        }
        return best;
    }
}
