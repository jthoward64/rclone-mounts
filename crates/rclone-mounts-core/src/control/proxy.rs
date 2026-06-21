// SPDX-License-Identifier: GPL-2.0-or-later

//! zbus proxies for `org.freedesktop.systemd1`. Same XML on the session and
//! system bus, so [`session`] and [`system`] both reuse these.
//!
//! Only the subset of the Manager API that our backend exercises is bound; the
//! full interface is large and we'd rather grow this on demand than carry
//! unused signatures.

use zbus::{proxy, zvariant::OwnedObjectPath};

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    /// `daemon-reload`. Re-reads unit files from disk.
    fn reload(&self) -> zbus::Result<()>;

    /// `mode` is typically `"replace"` for our use; returns the job object path.
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// `files` is a list of unit filenames (e.g. `["rclone-mount-foo.service"]`)
    /// or absolute paths. `runtime` writes to `/run` instead of `/etc`; we always
    /// pass `false`. `force` overwrites existing symlinks; `true` matches
    /// `systemctl enable`'s default behavior.
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<(String, String, String)>)>;

    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<(String, String, String)>>;

    /// Returns the object path of the loaded unit. `LoadUnit` loads on demand
    /// without starting; `GetUnit` returns NoSuchUnit if not yet loaded.
    fn load_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Unit {
    /// Combined active state: `active`, `inactive`, `failed`, `activating`, etc.
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    /// Finer sub-state, varies by unit type.
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;

    /// `loaded`, `not-found`, `error`, `masked`.
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;
}
