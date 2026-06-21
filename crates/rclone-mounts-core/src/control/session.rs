// SPDX-License-Identifier: GPL-2.0-or-later

use super::SystemdControl;
use crate::Result;
use async_trait::async_trait;
use zbus::Connection;

/// `SystemdControl` over the session bus for user-mode units. No Polkit prompts.
pub struct SessionSystemd {
    pub conn: Connection,
}

impl SessionSystemd {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::session().await?,
        })
    }
}

#[async_trait]
impl SystemdControl for SessionSystemd {
    async fn reload(&self) -> Result<()> {
        unimplemented!()
    }
    async fn start(&self, _unit: &str) -> Result<()> {
        unimplemented!()
    }
    async fn stop(&self, _unit: &str) -> Result<()> {
        unimplemented!()
    }
    async fn restart(&self, _unit: &str) -> Result<()> {
        unimplemented!()
    }
    async fn enable(&self, _unit: &str) -> Result<()> {
        unimplemented!()
    }
    async fn disable(&self, _unit: &str) -> Result<()> {
        unimplemented!()
    }
    async fn active_state(&self, _unit: &str) -> Result<String> {
        unimplemented!()
    }
}
