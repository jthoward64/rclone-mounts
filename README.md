# Rclone Mounts

A KDE Configuration Module (KCM) for managing rclone mounts.

This KCM allows securely connecting to, and automatically mounting, Rclone remotes as both user and system mounts.

Currently supported remotes:

- SMB
- FTP(S)
- SFTP
- WebDAV
- Google Drive (bring your own client ID)
- iCloud Drive

The KCM manages Systemd unit files and rclone configuration files per mount and per remote, securely storing credentials using Systemd.

## Project structure

- `crates/rclone-mounts-kcm` - The KCM itself, written in Rust and QML
- `crates/rclone-mounts-core` - The core library for managing rclone mounts
- `crates/rclone-mounts-helper` - An elevated helper binary for managing system mounts
- `cpp` - Thin C++ shim that handles some of the kcm boilerplate that the existing bindings don't
- `systemd` and `data` - Systemd unit, polkit rules, dbus service

