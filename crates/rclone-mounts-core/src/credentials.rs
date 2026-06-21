// SPDX-License-Identifier: GPL-2.0-or-later

//! Encrypt and decrypt rclone-mounts credentials via `systemd-creds`.
//!
//! systemd has a credential-encryption story (TPM2 / host-key) exposed only via
//! the `systemd-creds` CLI; there is no public C library entry point for
//! encryption (libsystemd's [`credentials`](https://docs.rs/libsystemd/latest/libsystemd/credentials/)
//! module covers reading credentials inside a unit, not authoring them).
//! Shelling out is therefore the supported interface and survives systemd
//! version changes more cleanly than reimplementing the wire format ourselves.
//!
//! Encryption is one-shot at save time, so the ~30–80 ms subprocess cost
//! doesn't matter for UX. Reading the cleartext back happens automatically at
//! unit start via `LoadCredentialEncrypted=` and never goes through this module.

use crate::{Error, Result};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    /// User-scope credential. `systemd-creds --user encrypt`. Key derives from
    /// the user's session key under `$XDG_RUNTIME_DIR`.
    User,
    /// System-scope credential. `systemd-creds encrypt`. Key derives from TPM2
    /// (preferred) or `/var/lib/systemd/credential.secret` (fallback).
    System,
}

/// Encrypt `plaintext` under the given credential `name`. The name is woven
/// into the encrypted blob and must match the `LoadCredentialEncrypted=` id at
/// unit start, otherwise systemd refuses to decrypt.
pub fn encrypt(scope: Scope, name: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    run(scope, "encrypt", name, plaintext)
}

/// Decrypt an `encrypted` blob produced by [`encrypt`] with the same `scope`
/// and `name`. Used for the "edit existing source" UI flow when the KCM needs
/// to read back the stored rclone.conf fragment.
pub fn decrypt(scope: Scope, name: &str, encrypted: &[u8]) -> Result<Vec<u8>> {
    run(scope, "decrypt", name, encrypted)
}

fn run(scope: Scope, op: &str, name: &str, input: &[u8]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("systemd-creds");
    if matches!(scope, Scope::User) {
        cmd.arg("--user");
    }
    cmd.arg(format!("--name={name}"))
        .arg(op)
        .arg("-")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Credentials(format!("spawn systemd-creds: {e}")))?;

    {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            Error::Credentials("failed to open stdin to systemd-creds".into())
        })?;
        stdin
            .write_all(input)
            .map_err(|e| Error::Credentials(format!("write to systemd-creds: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| Error::Credentials(format!("wait for systemd-creds: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(Error::Credentials(format!(
            "systemd-creds {op} failed ({}): {stderr}",
            output.status
        )));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn systemd_creds_available() -> bool {
        Command::new("systemd-creds")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Round-trip test gated on `systemd-creds` being present. The user-scope
    /// path additionally needs `$XDG_RUNTIME_DIR/systemd/credential.secret` to
    /// be writable, which is true in any normal login session but may not be
    /// in CI / sandboxes — so we skip with a printed message instead of failing.
    #[test]
    fn user_scope_round_trip() {
        if !systemd_creds_available() {
            eprintln!("skipping: systemd-creds not available");
            return;
        }
        let plaintext = b"hello secret world";
        let encrypted = match encrypt(Scope::User, "test-cred", plaintext) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("skipping: user-scope encrypt failed (likely no XDG_RUNTIME_DIR/systemd): {e}");
                return;
            }
        };
        assert_ne!(encrypted, plaintext, "output should differ from input");
        let decrypted = decrypt(Scope::User, "test-cred", &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_name_fails_decrypt() {
        if !systemd_creds_available() {
            eprintln!("skipping: systemd-creds not available");
            return;
        }
        let encrypted = match encrypt(Scope::User, "real-name", b"data") {
            Ok(e) => e,
            Err(e) => {
                eprintln!("skipping: encrypt setup failed: {e}");
                return;
            }
        };
        let result = decrypt(Scope::User, "wrong-name", &encrypted);
        assert!(result.is_err(), "decrypt with wrong name must fail");
    }
}
