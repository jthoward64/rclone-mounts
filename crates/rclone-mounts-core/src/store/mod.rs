// SPDX-License-Identifier: GPL-2.0-or-later

pub mod helper;
pub mod local;

use crate::Result;

/// File-I/O boundary. User-mode impl writes directly; system-mode impl proxies to the
/// privileged helper. All path construction happens inside the impl — callers pass
/// validated *names*, never paths.
pub trait UnitStore: Send + Sync {
    fn write_unit(&self, name: &str, contents: &str) -> Result<()>;
    fn delete_unit(&self, name: &str) -> Result<()>;
    fn read_unit(&self, name: &str) -> Result<String>;
    fn list_units(&self) -> Result<Vec<String>>;

    fn write_credential(&self, name: &str, blob: &[u8]) -> Result<()>;
    fn delete_credential(&self, name: &str) -> Result<()>;

    fn read_sources_conf(&self) -> Result<String>;
    fn write_sources_conf(&self, contents: &str) -> Result<()>;
}
