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
    pub dir_cache_time_secs: Option<u64>,
    pub umask: Option<u32>,
    pub read_only: bool,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            cache_mode: CacheMode::Writes,
            cache_max_size_mb: Some(2048),
            dir_cache_time_secs: Some(30),
            umask: Some(0o022),
            read_only: false,
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
