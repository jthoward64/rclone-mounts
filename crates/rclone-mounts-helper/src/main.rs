// SPDX-License-Identifier: GPL-2.0-or-later

//! rclone-mounts-helper: privileged system D-Bus service.
//!
//! Exposes `dev.jthoward.RcloneMounts.Helper1` on the system bus. Bounded API: no
//! method accepts a free-form path. Polkit actions:
//!   - dev.jthoward.rclone-mounts.read-system   (List* methods)
//!   - dev.jthoward.rclone-mounts.modify-system (ApplyChanges, DaemonReload)

use std::future::pending;
use zbus::{connection, interface};

const SERVICE: &str = "dev.jthoward.RcloneMounts.Helper";
const OBJECT_PATH: &str = "/dev/jthoward/RcloneMounts/Helper";

struct Helper;

#[interface(name = "dev.jthoward.RcloneMounts.Helper1")]
impl Helper {
    /// Lists system-scope sources. Authorized by `read-system`.
    /// Returns (name, kind, options-without-secrets) tuples.
    async fn list_sources(&self) -> zbus::fdo::Result<Vec<(String, String, std::collections::HashMap<String, String>)>> {
        Err(zbus::fdo::Error::NotSupported("not implemented".into()))
    }

    /// Lists system-scope mounts. Authorized by `read-system`.
    /// Returns (name, source, mountpoint, has_encrypted_source).
    async fn list_mounts(&self) -> zbus::fdo::Result<Vec<(String, String, String, bool)>> {
        Err(zbus::fdo::Error::NotSupported("not implemented".into()))
    }

    async fn has_encrypted_credential(&self, _source: String) -> zbus::fdo::Result<bool> {
        Err(zbus::fdo::Error::NotSupported("not implemented".into()))
    }

    /// Transactional batch apply. Authorized by `modify-system`. Validates the whole
    /// changeset first; writes to temp files; atomic-renames; runs daemon-reload.
    /// Changeset is a TOML-encoded string for now (until the wire format settles).
    async fn apply_changes(&self, _changeset: String) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported("not implemented".into()))
    }

    async fn daemon_reload(&self) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported("not implemented".into()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    async_io::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _conn = connection::Builder::system()?
        .name(SERVICE)?
        .serve_at(OBJECT_PATH, Helper)?
        .build()
        .await?;

    tracing::info!("rclone-mounts-helper listening on {SERVICE}{OBJECT_PATH}");
    pending::<()>().await;
    Ok(())
}
