// SPDX-License-Identifier: GPL-2.0-or-later

pub mod backend;
pub mod control;
pub mod credentials;
pub mod error;
pub mod mount;
pub mod rclone_cli;
pub mod rclone_config;
pub mod source;
pub mod store;
pub mod unit_writer;

pub use backend::{Backend, Changeset, HelperBackend, LocalBackend, SourceMetadata, State};
pub use error::{Error, Result};
pub use mount::{Mount, MountDef, MountOptions};
pub use source::{Source, SourceDef, SourceKind};
