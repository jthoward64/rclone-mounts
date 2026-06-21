// SPDX-License-Identifier: GPL-2.0-or-later

//! systemd-creds wrapper. Subprocess for now; could move to a libsystemd binding later.

use crate::Result;

pub enum Scope {
    User,
    System,
}

pub fn encrypt(_scope: Scope, _name: &str, _plaintext: &[u8]) -> Result<Vec<u8>> {
    unimplemented!("systemd-creds encrypt wrapper")
}
