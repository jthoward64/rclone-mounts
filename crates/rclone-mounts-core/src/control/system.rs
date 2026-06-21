// SPDX-License-Identifier: GPL-2.0-or-later

use super::proxy::{ManagerProxy, UnitProxy};
use super::SystemdControl;
use crate::{Error, Result};
use async_trait::async_trait;
use zbus::Connection;

/// [`SystemdControl`] over the system bus. Default Polkit policy on
/// `org.freedesktop.systemd1.manage-units` and `manage-unit-files` requires
/// admin; the agent prompts the user as needed when called from the KCM.
pub struct SystemSystemd {
    pub conn: Connection,
}

impl SystemSystemd {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::system().await?,
        })
    }

    async fn manager(&self) -> Result<ManagerProxy<'_>> {
        Ok(ManagerProxy::new(&self.conn).await?)
    }
}

#[async_trait]
impl SystemdControl for SystemSystemd {
    async fn reload(&self) -> Result<()> {
        self.manager().await?.reload().await?;
        Ok(())
    }

    async fn start(&self, unit: &str) -> Result<()> {
        self.manager().await?.start_unit(unit, "replace").await?;
        Ok(())
    }

    async fn stop(&self, unit: &str) -> Result<()> {
        self.manager().await?.stop_unit(unit, "replace").await?;
        Ok(())
    }

    async fn restart(&self, unit: &str) -> Result<()> {
        self.manager().await?.restart_unit(unit, "replace").await?;
        Ok(())
    }

    async fn enable(&self, unit: &str) -> Result<()> {
        self.manager()
            .await?
            .enable_unit_files(&[unit], false, true)
            .await?;
        Ok(())
    }

    async fn disable(&self, unit: &str) -> Result<()> {
        self.manager()
            .await?
            .disable_unit_files(&[unit], false)
            .await?;
        Ok(())
    }

    async fn active_state(&self, unit: &str) -> Result<String> {
        let mgr = self.manager().await?;
        let path = mgr.load_unit(unit).await?;
        let unit_proxy = UnitProxy::builder(&self.conn)
            .path(path)
            .map_err(|e| Error::Systemd(format!("bad object path: {e}")))?
            .build()
            .await?;
        Ok(unit_proxy.active_state().await?)
    }
}
