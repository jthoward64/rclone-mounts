// SPDX-License-Identifier: GPL-2.0-or-later

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Smb,
    Drive,
    WebDav,
}

impl SourceKind {
    /// The rclone `type=` tag for this kind. This is the canonical string form
    /// used both in rclone.conf and across the QML bridge.
    pub fn as_tag(&self) -> &'static str {
        match self {
            SourceKind::Smb => "smb",
            SourceKind::Drive => "drive",
            SourceKind::WebDav => "webdav",
        }
    }

    /// Parse a kind from its rclone `type=` tag. `None` for anything we don't
    /// model yet.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "smb" => Some(SourceKind::Smb),
            "drive" => Some(SourceKind::Drive),
            "webdav" => Some(SourceKind::WebDav),
            _ => None,
        }
    }
}

/// A configured rclone remote. The secret (if any) is never carried in this struct —
/// the KCM is write-only on secrets and the credential lives in a separate store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub kind: SourceKind,
    pub options: BTreeMap<String, String>,
    pub has_secret: bool,
}

/// Pending-state definition: how the UI describes a desired source. `new_secret` is
/// `Some(value)` only when the user is setting or rotating it; `None` means leave the
/// stored credential untouched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    pub name: String,
    pub kind: SourceKind,
    pub options: BTreeMap<String, String>,
    pub new_secret: Option<String>,
}
