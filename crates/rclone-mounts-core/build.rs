// SPDX-License-Identifier: GPL-2.0-or-later

//! Bakes an optional build-time default Google Drive OAuth client id/secret
//! into the binary via `option_env!`, when the packager/distro sets the
//! corresponding env vars at compile time. Absent env vars simply compile to
//! `None` (see `oauth_credentials::build_time_drive_credentials`) — this is
//! the lowest tier of the three-tier client credential precedence, never
//! required for the app to work (rclone falls back to its own shared
//! client), and never exposed to the UI.

fn main() {
    println!("cargo:rerun-if-env-changed=RCLONE_MOUNTS_DRIVE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=RCLONE_MOUNTS_DRIVE_CLIENT_SECRET");
    if let Ok(id) = std::env::var("RCLONE_MOUNTS_DRIVE_CLIENT_ID") {
        println!("cargo:rustc-env=RCLONE_MOUNTS_DRIVE_CLIENT_ID={id}");
    }
    if let Ok(secret) = std::env::var("RCLONE_MOUNTS_DRIVE_CLIENT_SECRET") {
        println!("cargo:rustc-env=RCLONE_MOUNTS_DRIVE_CLIENT_SECRET={secret}");
    }
}
