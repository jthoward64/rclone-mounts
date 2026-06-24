// SPDX-License-Identifier: GPL-2.0-or-later

use thiserror::Error;

/// Error messages are written to be shown directly to the user (the KCM
/// surfaces `Display` in an inline banner), so they avoid jargon and internal
/// detail. The trailing `{0}` carries the underlying cause for the cases where
/// it's a readable system message; purely internal failures keep their detail
/// in logs via `Debug`.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("Couldn't reach the systemd service. {0}")]
    DBus(#[from] zbus::Error),

    #[error("The rclone configuration couldn't be read. {0}")]
    ConfigParse(String),

    #[error("\"{0}\" can't be used as a name here. Use lowercase letters, numbers, and dashes — for example \"work-share\".")]
    InvalidName(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0} already exists.")]
    AlreadyExists(String),

    #[error("You don't have permission to do that. {0}")]
    PermissionDenied(String),

    #[error("Couldn't store the password securely. {0}")]
    Credentials(String),

    #[error("{0}")]
    Systemd(String),
}

pub type Result<T> = std::result::Result<T, Error>;
