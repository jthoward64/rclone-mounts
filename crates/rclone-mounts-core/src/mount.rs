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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mount {
    pub name: String,
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
