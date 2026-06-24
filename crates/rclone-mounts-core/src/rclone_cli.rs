// SPDX-License-Identifier: GPL-2.0-or-later

//! Thin wrappers around the `rclone` binary for the few things only rclone can
//! do correctly. Currently just password obscuring.

use crate::{Error, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Obscure a plaintext password into rclone's reversible-obscured form, which
/// is what backends like `smb`/`webdav` expect in the `pass` config field.
///
/// The plaintext is fed via stdin (never argv) so it can't leak through `ps` or
/// `/proc/<pid>/cmdline`. Note that obscuring is *not* encryption — it's
/// trivially reversible; the at-rest protection comes from systemd-creds
/// encrypting the whole blob. We obscure only because rclone refuses a
/// cleartext `pass`.
pub fn obscure(plaintext: &str) -> Result<String> {
    let mut child = Command::new("rclone")
        .arg("obscure")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Systemd(format!("spawn `rclone obscure`: {e}")))?;

    child
        .stdin
        .take()
        .ok_or_else(|| Error::Systemd("rclone obscure: no stdin".into()))?
        .write_all(plaintext.as_bytes())
        .map_err(|e| Error::Systemd(format!("write to `rclone obscure`: {e}")))?;

    let out = child
        .wait_with_output()
        .map_err(|e| Error::Systemd(format!("wait for `rclone obscure`: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Systemd(format!(
            "`rclone obscure` failed: {}",
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
