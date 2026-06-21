// SPDX-License-Identifier: GPL-2.0-or-later

pub mod session;
pub mod system;

use crate::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SystemdControl: Send + Sync {
    async fn reload(&self) -> Result<()>;
    async fn start(&self, unit: &str) -> Result<()>;
    async fn stop(&self, unit: &str) -> Result<()>;
    async fn restart(&self, unit: &str) -> Result<()>;
    async fn enable(&self, unit: &str) -> Result<()>;
    async fn disable(&self, unit: &str) -> Result<()>;
    async fn active_state(&self, unit: &str) -> Result<String>;
}
