// SPDX-License-Identifier: GPL-2.0-or-later

pub mod control;
pub mod credentials;
pub mod error;
pub mod mount;
pub mod rclone_config;
pub mod source;
pub mod store;
pub mod unit_writer;

pub use error::{Error, Result};
pub use mount::{Mount, MountDef, MountOptions};
pub use source::{Source, SourceDef, SourceKind};

use control::SystemdControl;
use store::UnitStore;

/// The backend ties one [`UnitStore`] (file I/O) and one [`SystemdControl`] (lifecycle)
/// to a single scope (user or system). All business logic operates on `Backend` and is
/// mode-agnostic; the only divergence is which trait impls were injected here.
pub struct Backend {
    pub store: Box<dyn UnitStore>,
    pub control: Box<dyn SystemdControl>,
}

impl Backend {
    pub fn new(store: Box<dyn UnitStore>, control: Box<dyn SystemdControl>) -> Self {
        Self { store, control }
    }
}
