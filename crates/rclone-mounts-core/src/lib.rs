// SPDX-License-Identifier: GPL-2.0-or-later

pub mod backend;
pub mod backend_features;
pub mod control;
pub mod credentials;
pub mod error;
pub mod mount;
pub mod naming;
pub mod oauth_credentials;
pub mod rclone_cli;
pub mod rclone_config;
pub mod rclone_config_driver;
pub mod source;
pub mod source_schema;
pub mod store;
pub mod unit_writer;

pub use backend::{Backend, Changeset, HelperBackend, LocalBackend, SourceMetadata, State};
pub use error::{Error, Result};
pub use mount::{Mount, MountDef, MountOptions};
pub use source::{SecretValue, Source, SourceDef, SourceKind};
