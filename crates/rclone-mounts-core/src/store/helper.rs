// SPDX-License-Identifier: GPL-2.0-or-later

use super::UnitStore;
use crate::Result;
use zbus::Connection;

/// `UnitStore` impl that proxies every operation to the privileged D-Bus helper.
/// Constructed once per system-mode backend; holds a system-bus connection.
pub struct HelperUnitStore {
    pub conn: Connection,
}

impl HelperUnitStore {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::system().await?,
        })
    }
}

impl UnitStore for HelperUnitStore {
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
