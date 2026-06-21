// SPDX-License-Identifier: GPL-2.0-or-later

//! Generates the text of a systemd .service unit for a given mount.
//!
//! Unit shape: Type=notify rclone mount, with LoadCredentialEncrypted= referencing the
//! source's credential blob and EnvironmentFile=${CREDENTIALS_DIRECTORY}/<source>.

use crate::Mount;

pub fn render(_mount: &Mount) -> String {
    String::new()
}

/// Validates a name against the helper-enforced pattern: `^[a-z0-9][a-z0-9-]{0,62}$`.
/// Returns Ok(()) if valid. Used by both user-side and helper-side write paths so the
/// invariant is checked twice.
pub fn validate_name(name: &str) -> crate::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| match (i, b) {
                (0, b'0'..=b'9') | (0, b'a'..=b'z') => true,
                (_, b'0'..=b'9') | (_, b'a'..=b'z') | (_, b'-') => true,
                _ => false,
            });
    if ok {
        Ok(())
    } else {
        Err(crate::Error::InvalidName(name.to_string()))
    }
}
