use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use crate::backend::open_platform_backend;
use crate::supervisor::{Command, Supervisor};
use crate::{AwakeError, AwakeStatus};

pub struct StayAwakeConfig {
    pub enabled: bool,
    /// Shown to the user by the OS power tools next to the hold.
    pub reason: String,
}

#[async_trait]
pub trait EnabledPreference: Send + Sync {
    async fn store(&self, enabled: bool);
}

#[derive(Clone)]
pub struct StayAwake {
    commands: mpsc::UnboundedSender<Command>,
    status: watch::Receiver<AwakeStatus>,
    preference: Arc<dyn EnabledPreference>,
}

impl StayAwake {
    /// Must be called inside a tokio runtime; the holds live on a task it spawns.
    pub fn spawn(config: StayAwakeConfig, preference: Arc<dyn EnabledPreference>) -> Self {
        let (status_tx, status_rx) = watch::channel(AwakeStatus::initial(config.enabled));
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let reason = config.reason;
        let enabled = config.enabled;
        tokio::spawn(async move {
            let (disruption_tx, disruption_rx) = mpsc::unbounded_channel();
            let backend = open_platform_backend(reason, disruption_tx).await;
            Supervisor {
                backend,
                enabled,
                status: status_tx,
                commands: command_rx,
                disruptions: disruption_rx,
            }
            .run()
            .await;
        });
        Self {
            commands: command_tx,
            status: status_rx,
            preference,
        }
    }

    /// Persists the choice, then applies it to the running holds.
    pub async fn set_enabled(&self, enabled: bool) {
        self.preference.store(enabled).await;
        let _ = self.commands.send(Command::SetEnabled(enabled));
    }

    pub fn status(&self) -> AwakeStatus {
        self.status.borrow().clone()
    }

    pub fn watch(&self) -> StatusWatch {
        StatusWatch(self.status.clone())
    }

    /// Releases every hold and stops the service without waiting; later calls do nothing.
    pub fn release(&self) {
        let _ = self.commands.send(Command::Release);
    }
}

pub struct StatusWatch(watch::Receiver<AwakeStatus>);

impl StatusWatch {
    pub fn current(&self) -> AwakeStatus {
        self.0.borrow().clone()
    }

    pub async fn changed(&mut self) -> Result<AwakeStatus, AwakeError> {
        self.0.changed().await.map_err(|_| AwakeError::Stopped)?;
        Ok(self.0.borrow_and_update().clone())
    }
}
