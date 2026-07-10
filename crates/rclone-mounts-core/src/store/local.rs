// SPDX-License-Identifier: GPL-2.0-or-later

use super::UnitStore;
use crate::unit_writer::validate_name;
use crate::{Error, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Filesystem-backed [`UnitStore`].
///
/// Path layout:
/// - `config_dir/sources.conf` — rclone.conf-shape source definitions
/// - `credential_dir/<name>` — encrypted credential blob, one file per source
/// - `unit_dir/rclone-mount-<name>.service` — generated mount units
///
/// Used directly by user-mode (paths under `$HOME`) and by the helper for
/// system-mode (paths under `/etc`). The helper-side wrapper [`HelperUnitStore`]
/// proxies these operations over D-Bus.
///
/// All writes are atomic via tempfile+rename in the destination directory; this
/// avoids leaving half-written units that systemd might pick up mid-write.
pub struct LocalUnitStore {
    pub config_dir: PathBuf,
    pub credential_dir: PathBuf,
    pub unit_dir: PathBuf,
    /// File mode for newly-created files. `0o600` for both user and system mode
    /// per the project's permissions model.
    pub file_mode: u32,
}

impl LocalUnitStore {
    pub fn new_user_default() -> Result<Self> {
        let home = std::env::var_os("HOME").ok_or_else(|| Error::Systemd("HOME not set".into()))?;
        let home = PathBuf::from(home);
        let cfg = home.join(".config/rclone-mounts");
        Ok(Self {
            credential_dir: cfg.join("credentials"),
            config_dir: cfg,
            unit_dir: home.join(".config/systemd/user"),
            file_mode: 0o600,
        })
    }

    pub fn new_system_default() -> Self {
        Self {
            config_dir: PathBuf::from("/etc/rclone-mounts"),
            credential_dir: PathBuf::from("/etc/credstore.encrypted/rclone-mounts"),
            unit_dir: PathBuf::from("/etc/systemd/system"),
            file_mode: 0o600,
        }
    }

    fn unit_path(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.unit_dir.join(crate::backend::mount_unit_name(name)))
    }

    fn credential_path(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.credential_dir.join(name))
    }

    fn sources_path(&self) -> PathBuf {
        self.config_dir.join("sources.conf")
    }

    fn mounts_state_path(&self) -> PathBuf {
        self.config_dir.join("mounts.toml")
    }

    fn ensure_dir(&self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            // Tighten directory permissions; tempfile creates files inside so
            // any race window is bounded by the dir's permissions.
            let perms = fs::Permissions::from_mode(0o700);
            fs::set_permissions(dir, perms)?;
        }
        Ok(())
    }

    fn atomic_write(&self, path: &Path, content: &[u8]) -> Result<()> {
        let dir = path
            .parent()
            .ok_or_else(|| Error::Systemd(format!("no parent dir for {path:?}")))?;
        self.ensure_dir(dir)?;
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        temp.as_file_mut().write_all(content)?;
        temp.as_file().sync_all()?;
        let perms = fs::Permissions::from_mode(self.file_mode);
        fs::set_permissions(temp.path(), perms)?;
        temp.persist(path).map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
}

impl UnitStore for LocalUnitStore {
    fn write_unit(&self, name: &str, contents: &str) -> Result<()> {
        let path = self.unit_path(name)?;
        self.atomic_write(&path, contents.as_bytes())
    }

    fn delete_unit(&self, name: &str) -> Result<()> {
        let path = self.unit_path(name)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn read_unit(&self, name: &str) -> Result<String> {
        let path = self.unit_path(name)?;
        Ok(fs::read_to_string(&path)?)
    }

    fn list_units(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.unit_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if let Some(fname) = entry.file_name().to_str() {
                if let Some(stem) = fname
                    .strip_prefix("rclone-mount-")
                    .and_then(|s| s.strip_suffix(".service"))
                {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    fn write_credential(&self, name: &str, blob: &[u8]) -> Result<()> {
        let path = self.credential_path(name)?;
        self.atomic_write(&path, blob)
    }

    fn delete_credential(&self, name: &str) -> Result<()> {
        let path = self.credential_path(name)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn read_credential(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let path = self.credential_path(name)?;
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_sources_conf(&self) -> Result<String> {
        match fs::read_to_string(self.sources_path()) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write_sources_conf(&self, contents: &str) -> Result<()> {
        self.atomic_write(&self.sources_path(), contents.as_bytes())
    }

    fn read_mounts_state(&self) -> Result<String> {
        match fs::read_to_string(self.mounts_state_path()) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write_mounts_state(&self, contents: &str) -> Result<()> {
        self.atomic_write(&self.mounts_state_path(), contents.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, LocalUnitStore) {
        let dir = TempDir::new().unwrap();
        let store = LocalUnitStore {
            config_dir: dir.path().join("config"),
            credential_dir: dir.path().join("creds"),
            unit_dir: dir.path().join("units"),
            file_mode: 0o600,
        };
        (dir, store)
    }

    #[test]
    fn write_and_read_unit_round_trips() {
        let (_dir, s) = fixture();
        s.write_unit("foo", "[Unit]\nDescription=Test\n").unwrap();
        let back = s.read_unit("foo").unwrap();
        assert_eq!(back, "[Unit]\nDescription=Test\n");
    }

    #[test]
    fn list_units_returns_bare_names_sorted() {
        let (_dir, s) = fixture();
        s.write_unit("zeta", "x").unwrap();
        s.write_unit("alpha", "x").unwrap();
        s.write_unit("middle", "x").unwrap();
        assert_eq!(s.list_units().unwrap(), vec!["alpha", "middle", "zeta"]);
    }

    #[test]
    fn list_units_ignores_unrelated_files() {
        let (_dir, s) = fixture();
        s.write_unit("real", "x").unwrap();
        fs::create_dir_all(&s.unit_dir).unwrap();
        fs::write(s.unit_dir.join("unrelated.service"), "x").unwrap();
        fs::write(s.unit_dir.join("rclone-mount-bad"), "x").unwrap(); // no .service
        assert_eq!(s.list_units().unwrap(), vec!["real"]);
    }

    #[test]
    fn delete_unit_is_idempotent() {
        let (_dir, s) = fixture();
        s.write_unit("gone", "x").unwrap();
        s.delete_unit("gone").unwrap();
        s.delete_unit("gone").unwrap(); // second call must not error
        assert!(s.read_unit("gone").is_err());
    }

    #[test]
    fn credentials_round_trip_with_binary_data() {
        let (_dir, s) = fixture();
        let blob: Vec<u8> = (0u8..=255).collect();
        s.write_credential("src", &blob).unwrap();
        let path = s.credential_dir.join("src");
        let back = fs::read(&path).unwrap();
        assert_eq!(back, blob);
    }

    #[test]
    fn write_creates_files_with_requested_mode() {
        let (_dir, s) = fixture();
        s.write_unit("foo", "x").unwrap();
        let meta = fs::metadata(s.unit_dir.join("rclone-mount-foo.service")).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn invalid_names_rejected_before_filesystem_touched() {
        let (_dir, s) = fixture();
        assert!(s.write_unit("../escape", "x").is_err());
        assert!(s.write_credential("../escape", b"x").is_err());
        assert!(s.delete_unit("../escape").is_err());
        // Confirm nothing was actually written.
        assert!(!s.unit_dir.exists() || fs::read_dir(&s.unit_dir).unwrap().count() == 0);
    }

    #[test]
    fn read_sources_returns_empty_for_missing_file() {
        let (_dir, s) = fixture();
        assert_eq!(s.read_sources_conf().unwrap(), "");
    }

    #[test]
    fn write_then_read_sources_conf() {
        let (_dir, s) = fixture();
        let conf = "[work]\ntype = smb\n";
        s.write_sources_conf(conf).unwrap();
        assert_eq!(s.read_sources_conf().unwrap(), conf);
    }
}
