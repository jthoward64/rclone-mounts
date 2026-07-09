// SPDX-License-Identifier: GPL-2.0-or-later

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
    /// Read the encrypted credential blob for `name`, or `None` if there isn't
    /// one. Used to reuse a stored password when editing a source's other
    /// fields, and (decrypted) to answer "does this source have a password?".
    fn read_credential(&self, name: &str) -> Result<Option<Vec<u8>>>;

    fn read_sources_conf(&self) -> Result<String>;
    fn write_sources_conf(&self, contents: &str) -> Result<()>;

    /// Read the KCM's mount-state file. Returns empty string if the file
    /// doesn't exist yet (first run). The format is TOML; deserialization is
    /// the caller's job so the trait stays serde-free.
    fn read_mounts_state(&self) -> Result<String>;
    fn write_mounts_state(&self, contents: &str) -> Result<()>;
}
