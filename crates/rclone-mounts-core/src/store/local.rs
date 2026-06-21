// SPDX-License-Identifier: GPL-2.0-or-later

use super::UnitStore;
use crate::Result;
use std::path::PathBuf;

/// Direct filesystem `UnitStore` for user-scope. `config_dir` is e.g.
/// `~/.config/rclone-mounts`; `unit_dir` is e.g. `~/.config/systemd/user`.
pub struct LocalUnitStore {
    pub config_dir: PathBuf,
    pub unit_dir: PathBuf,
}

impl UnitStore for LocalUnitStore {
    fn write_unit(&self, _name: &str, _contents: &str) -> Result<()> {
        unimplemented!()
    }
    fn delete_unit(&self, _name: &str) -> Result<()> {
        unimplemented!()
    }
    fn read_unit(&self, _name: &str) -> Result<String> {
        unimplemented!()
    }
    fn list_units(&self) -> Result<Vec<String>> {
        unimplemented!()
    }
    fn write_credential(&self, _name: &str, _blob: &[u8]) -> Result<()> {
        unimplemented!()
    }
    fn delete_credential(&self, _name: &str) -> Result<()> {
        unimplemented!()
    }
    fn read_sources_conf(&self) -> Result<String> {
        unimplemented!()
    }
    fn write_sources_conf(&self, _contents: &str) -> Result<()> {
        unimplemented!()
    }
}
