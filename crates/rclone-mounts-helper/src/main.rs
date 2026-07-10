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

use enumflags2::BitFlags;
use rclone_mounts_core::backend::{Backend, Changeset, LocalBackend, SourceMetadata};
use rclone_mounts_core::control::system::SystemSystemd;
use rclone_mounts_core::credentials::Scope;
use rclone_mounts_core::source::SourceKind;
use rclone_mounts_core::store::local::LocalUnitStore;
use std::collections::HashMap;
use std::future::pending;
use zbus::message::Header;
use zbus::{connection, interface, Connection};
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

const SERVICE: &str = "dev.jthoward.RcloneMounts.Helper";
const OBJECT_PATH: &str = "/dev/jthoward/RcloneMounts/Helper";

/// Polkit action for read-only methods (the `List*`/status getters).
const ACTION_READ: &str = "dev.jthoward.rclone-mounts.read-system";
/// Polkit action for state-changing methods (apply, reload, start/stop).
const ACTION_MODIFY: &str = "dev.jthoward.rclone-mounts.modify-system";

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

    /// Gate a method on a Polkit action. Builds the subject from the calling
    /// message's sender (so a client can't impersonate another), then asks the
    /// system authority. Maps a negative result to `AccessDenied` so the client
    /// sees a clean denial.
    ///
    /// `interactive` controls whether polkit may raise the admin auth prompt:
    /// user-initiated methods pass `true` (the call blocks until the user
    /// answers or declines); the background status poll passes `false` so a
    /// timer tick can never pop an unexpected dialog once a kept auth lapses —
    /// it just reports the unit as unknown instead.
    async fn authorize(
        conn: &Connection,
        hdr: &Header<'_>,
        action: &str,
        interactive: bool,
    ) -> zbus::fdo::Result<()> {
        let subject = Subject::new_for_message_header(hdr)
            .map_err(|e| zbus::fdo::Error::Failed(format!("polkit subject: {e}")))?;
        let authority = AuthorityProxy::new(conn)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("polkit authority: {e}")))?;
        let flags: BitFlags<CheckAuthorizationFlags> = if interactive {
            CheckAuthorizationFlags::AllowUserInteraction.into()
        } else {
            BitFlags::empty()
        };
        let result = authority
            .check_authorization(&subject, action, &HashMap::new(), flags, "")
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("polkit check: {e}")))?;
        if result.is_authorized {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied(format!(
                "not authorized for {action}"
            )))
        }
    }
}

#[interface(name = "dev.jthoward.RcloneMounts.Helper1")]
impl Helper {
    /// Lists system-scope sources. Authorized by `read-system`.
    /// Returns (name, display_name, kind, options-without-secrets, has_secret).
    async fn list_sources(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<Vec<(String, String, String, HashMap<String, String>, bool)>> {
        Self::authorize(conn, &hdr, ACTION_READ, true).await?;
        let backend = Self::make_backend().await?;
        let state = backend
            .load()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("load: {e}")))?;
        Ok(state
            .sources
            .into_iter()
            .map(|s: SourceMetadata| {
                (
                    s.name,
                    s.display_name,
                    s.kind,
                    s.options.into_iter().collect(),
                    s.has_secret,
                )
            })
            .collect())
    }

    /// Lists system-scope mounts. Authorized by `read-system`.
    /// Returns (name, display_name, source, subpath, mountpoint, options-json, enabled).
    /// Mount tuning options travel as a JSON blob; see [`HelperBackend::load`].
    async fn list_mounts(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<Vec<(String, String, String, String, String, String, bool)>> {
        Self::authorize(conn, &hdr, ACTION_READ, true).await?;
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
                let options = serde_json::to_string(&m.options).unwrap_or_else(|_| "{}".into());
                (
                    m.name,
                    m.display_name,
                    m.source,
                    m.subpath,
                    mp,
                    options,
                    m.enabled,
                )
            })
            .collect())
    }

    /// Transactional batch apply. Authorized by `modify-system`. Validates the
    /// whole changeset first; writes to temp files via [`LocalUnitStore`]'s
    /// atomic-write helper; runs daemon-reload.
    async fn apply_changes(
        &self,
        changeset: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        Self::authorize(conn, &hdr, ACTION_MODIFY, true).await?;
        let cs: Changeset = toml::from_str(&changeset)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("changeset toml: {e}")))?;
        let backend = Self::make_backend().await?;
        backend
            .apply(cs)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("apply: {e}")))?;
        Ok(())
    }

    async fn daemon_reload(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        Self::authorize(conn, &hdr, ACTION_MODIFY, true).await?;
        let backend = Self::make_backend().await?;
        backend
            .control
            .reload()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("reload: {e}")))?;
        Ok(())
    }

    /// Start a mount's unit now. Authorized by `modify-system`. Live action,
    /// mirrors [`Backend::start_mount`]; the helper owns the system-bus systemd.
    async fn start_mount(
        &self,
        name: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        Self::authorize(conn, &hdr, ACTION_MODIFY, true).await?;
        let backend = Self::make_backend().await?;
        backend
            .start_mount(&name)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("start {name}: {e}")))?;
        Ok(())
    }

    /// Stop a mount's unit now. Authorized by `modify-system`.
    async fn stop_mount(
        &self,
        name: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        Self::authorize(conn, &hdr, ACTION_MODIFY, true).await?;
        let backend = Self::make_backend().await?;
        backend
            .stop_mount(&name)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("stop {name}: {e}")))?;
        Ok(())
    }

    /// systemd `ActiveState` of a mount's unit. Authorized by `read-system`.
    async fn mount_status(
        &self,
        name: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<String> {
        // Non-interactive: this is the path the UI polls on a timer, so a
        // background tick must never raise an auth dialog (see `authorize`).
        Self::authorize(conn, &hdr, ACTION_READ, false).await?;
        let backend = Self::make_backend().await?;
        backend
            .mount_status(&name)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("status {name}: {e}")))
    }

    /// Set (or, if both empty, clear) the system-wide admin override for a
    /// kind's OAuth client id/secret. Authorized by `modify-system`.
    async fn set_provider_override(
        &self,
        kind: String,
        client_id: String,
        client_secret: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        Self::authorize(conn, &hdr, ACTION_MODIFY, true).await?;
        let kind = SourceKind::from_tag(&kind)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown kind: {kind}")))?;
        let backend = Self::make_backend().await?;
        backend
            .set_provider_override(kind, &client_id, &client_secret)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("set provider override: {e}")))?;
        Ok(())
    }

    /// The stored admin override for a kind, if any. Authorized by
    /// `read-system`. Returns (has_value, client_id, client_secret); the
    /// caller only surfaces a has-value boolean to the UI (mirrors
    /// `has_secret` — the raw secret never round-trips into QML).
    async fn provider_override(
        &self,
        kind: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<(bool, String, String)> {
        Self::authorize(conn, &hdr, ACTION_READ, true).await?;
        let kind = SourceKind::from_tag(&kind)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown kind: {kind}")))?;
        let backend = Self::make_backend().await?;
        let pair = backend
            .provider_override(kind)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("provider override: {e}")))?;
        Ok(match pair {
            Some((id, secret)) => (true, id, secret),
            None => (false, String::new(), String::new()),
        })
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
