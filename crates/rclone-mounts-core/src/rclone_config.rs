// SPDX-License-Identifier: GPL-2.0-or-later

//! rclone.conf format I/O. INI-ish; preserves unknown keys and comments on round-trip.

use crate::{Error, Result, Source};

pub fn parse(_text: &str) -> Result<Vec<Source>> {
    Err(Error::ConfigParse("not implemented".into()))
}

pub fn emit(_sources: &[Source]) -> String {
    String::new()
}
