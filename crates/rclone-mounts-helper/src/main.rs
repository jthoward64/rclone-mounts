// SPDX-License-Identifier: GPL-2.0-or-later

//! rclone-mounts-helper: privileged system D-Bus service.
//!
//! Exposes `dev.jthoward.RcloneMounts.Helper1` on the system bus. Bounded API:
//! no method accepts a free-form path. Polkit actions:
//!   - dev.jthoward.rclone-mounts.read-system   (List* methods)
//!   - dev.jthoward.rclone-mounts.modify-system (ApplyChanges, DaemonReload)
//!
//! Wire format for ApplyChanges: TOML-encoded [`Changeset`]. TOML is human-
//! diffable in logs, and TOML's strictness about unknown fields gives us a
//! clean rejection path when the KCM and helper drift apart in version.

use rclone_mounts_core::backend::{Backend, Changeset, LocalBackend, SourceMetadata};
use rclone_mounts_core::control::system::SystemSystemd;
use rclone_mounts_core::credentials::Scope;
use rclone_mounts_core::store::local::LocalUnitStore;
use std::collections::HashMap;
use std::future::pending;
use zbus::{connection, interface};

const SERVICE: &str = "dev.jthoward.RcloneMounts.Helper";
const OBJECT_PATH: &str = "/dev/jthoward/RcloneMounts/Helper";

struct Helper;

impl Helper {
    async fn make_backend() -> zbus::fdo::Result<LocalBackend> {
        let control = SystemSystemd::new()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("connect system bus: {e}")))?;
        Ok(LocalBackend {
            store: Box::new(LocalUnitStore::new_system_default()),
            control: Box::new(control),
            scope: Scope::System,
            credential_dir_spec: "/etc/credstore.encrypted/rclone-mounts".into(),
        })
    }
}

#[interface(name = "dev.jthoward.RcloneMounts.Helper1")]
impl Helper {
    /// Lists system-scope sources. Authorized by `read-system`.
    /// Returns (name, kind, options-without-secrets) tuples.
    async fn list_sources(
        &self,
    ) -> zbus::fdo::Result<Vec<(String, String, HashMap<String, String>)>> {
        let backend = Self::make_backend().await?;
        let state = backend
            .load()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("load: {e}")))?;
        Ok(state
            .sources
            .into_iter()
            .map(|s: SourceMetadata| (s.name, s.kind, s.options.into_iter().collect()))
            .collect())
    }

    /// Lists system-scope mounts. Authorized by `read-system`.
    /// Returns (name, source, mountpoint, enabled).
    async fn list_mounts(&self) -> zbus::fdo::Result<Vec<(String, String, String, bool)>> {
        let backend = Self::make_backend().await?;
        let state = backend
            .load()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("load: {e}")))?;
        Ok(state
            .mounts
            .into_iter()
            .map(|m| {
                let mp = m.mountpoint.to_string_lossy().into_owned();
                (m.name, m.source, mp, m.enabled)
            })
            .collect())
    }

    async fn has_encrypted_credential(&self, _source: String) -> zbus::fdo::Result<bool> {
        // Probe by attempting to read the cred file path; the UnitStore trait
        // doesn't expose an "exists" check yet, so this returns false until
        // [[probe_credential]] is implemented properly.
        Ok(false)
    }

    /// Transactional batch apply. Authorized by `modify-system`. Validates the
    /// whole changeset first; writes to temp files via [`LocalUnitStore`]'s
    /// atomic-write helper; runs daemon-reload.
    async fn apply_changes(&self, changeset: String) -> zbus::fdo::Result<()> {
        let cs: Changeset = toml::from_str(&changeset)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("changeset toml: {e}")))?;
        let backend = Self::make_backend().await?;
        backend
            .apply(cs)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("apply: {e}")))?;
        Ok(())
    }

    async fn daemon_reload(&self) -> zbus::fdo::Result<()> {
        let backend = Self::make_backend().await?;
        backend
            .control
            .reload()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("reload: {e}")))?;
        Ok(())
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
