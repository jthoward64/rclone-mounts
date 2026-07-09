// SPDX-License-Identifier: GPL-2.0-or-later

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Smb,
    Drive,
    WebDav,
    Sftp,
    Ftp,
    IcloudDrive,
}

impl SourceKind {
    /// The rclone `type=` tag for this kind. This is the canonical string form
    /// used both in rclone.conf and across the QML bridge.
    pub fn as_tag(&self) -> &'static str {
        match self {
            SourceKind::Smb => "smb",
            SourceKind::Drive => "drive",
            SourceKind::WebDav => "webdav",
            SourceKind::Sftp => "sftp",
            SourceKind::Ftp => "ftp",
            SourceKind::IcloudDrive => "iclouddrive",
        }
    }

    /// Parse a kind from its rclone `type=` tag. `None` for anything we don't
    /// model yet.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "smb" => Some(SourceKind::Smb),
            "drive" => Some(SourceKind::Drive),
            "webdav" => Some(SourceKind::WebDav),
            "sftp" => Some(SourceKind::Sftp),
            "ftp" => Some(SourceKind::Ftp),
            "iclouddrive" => Some(SourceKind::IcloudDrive),
            _ => None,
        }
    }

    /// True for kinds whose setup is a multi-step interactive flow (OAuth,
    /// 2FA) rather than a static field form.
    pub fn is_wizard_only(&self) -> bool {
        matches!(self, SourceKind::Drive | SourceKind::IcloudDrive)
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

/// One secret value bound for the credential blob. `obscure: true` means
/// `value` is plaintext that must be run through `rclone obscure` before
/// being written (e.g. `pass`); `false` means `value` is already in its final
/// on-disk form (an OAuth `token` JSON blob, an iCloud trust token) and must
/// be written byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretValue {
    pub value: String,
    pub obscure: bool,
}

/// Pending-state definition: how the UI describes a desired source.
/// `new_secrets` carries only the secret keys being set or rotated; a key
/// absent from the map means "leave that key's stored value untouched" (an
/// empty map means no secret changes at all).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    /// Internal id: rclone remote / file key. Slug, validated, immutable.
    pub name: String,
    /// Freeform name shown in the UI.
    pub display_name: String,
    pub kind: SourceKind,
    pub options: BTreeMap<String, String>,
    pub new_secrets: BTreeMap<String, SecretValue>,
}
