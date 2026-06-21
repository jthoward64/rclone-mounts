// SPDX-License-Identifier: GPL-2.0-or-later

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("dbus: {0}")]
    DBus(#[from] zbus::Error),

    #[error("rclone.conf parse: {0}")]
    ConfigParse(String),

    #[error("invalid name {0:?}: must match ^[a-z0-9][a-z0-9-]{{0,62}}$")]
    InvalidName(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("credentials: {0}")]
    Credentials(String),

    #[error("systemd: {0}")]
    Systemd(String),
}

pub type Result<T> = std::result::Result<T, Error>;
