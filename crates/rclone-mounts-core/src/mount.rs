// SPDX-License-Identifier: GPL-2.0-or-later

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheMode {
    Off,
    Minimal,
    Writes,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountOptions {
    pub cache_mode: CacheMode,
    pub cache_max_size_mb: Option<u64>,
    /// `--dir-cache-time`. `None` omits the flag (rclone's own default: 5
    /// minutes). The KCM defaults new mounts to something much longer than
    /// that on backends whose `SourceKind::supports_polling()` is true — see
    /// `MountEditorForm.qml` — since those backends notice remote changes via
    /// `--poll-interval` rather than relying on this expiring.
    pub dir_cache_time_secs: Option<u64>,
    pub umask: Option<u32>,
    pub read_only: bool,
    /// `--vfs-refresh`: recursively walk the whole remote in the background
    /// right when the mount starts, to warm the directory cache before the
    /// user's first `ls`. Off by default — on a large remote this means a
    /// burst of listing requests at every login/mount-start that most people
    /// won't want by default.
    #[serde(default)]
    pub vfs_refresh: bool,
    /// `--poll-interval`. Only meaningful on a `supports_polling` backend.
    /// `None` omits the flag (rclone's own default: 1 minute). `Some(0)`
    /// explicitly disables polling (`--poll-interval=0s`) — the KCM's
    /// "poll for changes" switch being off. `Some(n)` for `n > 0` is a
    /// user-chosen interval, always well under any `dir_cache_time_secs`
    /// this app offers (the flag's one hard requirement: it must be smaller
    /// than `--dir-cache-time`).
    #[serde(default)]
    pub poll_interval_secs: Option<u64>,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            cache_mode: CacheMode::Writes,
            cache_max_size_mb: Some(2048),
            // rclone's own default (5 minutes) — this app previously shipped
            // a much shorter 30s here, which was simply wrong.
            dir_cache_time_secs: Some(300),
            umask: Some(0o077),
            read_only: false,
            vfs_refresh: false,
            poll_interval_secs: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mount {
    /// Internal id: the systemd unit / file key. Slug, validated, immutable.
    pub name: String,
    /// Freeform name shown in the UI. Defaults to `name` for data written
    /// before display names existed.
    #[serde(default)]
    pub display_name: String,
    /// Id of the source this mount uses.
    pub source: String,
    /// Path within the source's remote to mount, instead of its root (rclone's
    /// `remote:path` addressing — works the same for every source kind).
    /// Empty means "mount the whole remote", matching mounts saved before
    /// this field existed.
    #[serde(default)]
    pub subpath: String,
    pub mountpoint: PathBuf,
    pub options: MountOptions,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct MountDef {
    pub name: String,
    pub source: String,
    pub mountpoint: PathBuf,
    pub options: MountOptions,
}
